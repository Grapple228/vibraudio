use std::marker::PhantomData;
use tracing::info;
use vibraudio_core::BackendAsync;
use vibraudio_core::{
    AudioConfig, AudioFormatInfo, Backend, Channels, Error, Result, StreamDirection, sample::Sample,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioContextLatencyCategory, AudioContextOptions, AudioContextState,
    AudioWorkletNode, AudioWorkletNodeOptions, MediaStream, MediaStreamAudioSourceNode,
    MediaStreamConstraints,
};

pub const FRAMES: usize = 128;

const MAX_FRAMES: usize = 128;
const RING_CAP: usize = 4096;

static mut INPUT: [f32; MAX_FRAMES] = [0.0; MAX_FRAMES];
static mut OUTPUT: [f32; MAX_FRAMES] = [0.0; MAX_FRAMES];
static mut RING: [f32; RING_CAP] = [0.0; RING_CAP];
static mut RING_WR: usize = 0;
static mut RING_RD: usize = 0;

#[wasm_bindgen]
pub fn input_ptr() -> u32 {
    std::ptr::addr_of_mut!(INPUT) as u32
}

#[wasm_bindgen]
pub fn output_ptr() -> u32 {
    std::ptr::addr_of_mut!(OUTPUT) as u32
}

#[wasm_bindgen]
pub fn capture_process(frames: u32) {
    let n = (frames as usize).min(MAX_FRAMES);
    unsafe {
        let input = &*std::ptr::addr_of!(INPUT);
        let ring = &mut *std::ptr::addr_of_mut!(RING);
        let mut wr = RING_WR;
        for i in 0..n {
            ring[wr] = input[i];
            wr = (wr + 1) % RING_CAP;
        }
        RING_WR = wr;
    }
}

#[wasm_bindgen]
pub fn playback_process(frames: u32) {
    let n = (frames as usize).min(MAX_FRAMES);
    unsafe {
        let ring = &*std::ptr::addr_of!(RING);
        let output = &mut *std::ptr::addr_of_mut!(OUTPUT);
        let mut rd = RING_RD;
        let wr = RING_WR;
        for i in 0..n {
            if rd != wr {
                output[i] = ring[rd];
                rd = (rd + 1) % RING_CAP;
            } else {
                output[i] = 0.0;
            }
        }
        RING_RD = rd;
    }
}

pub struct WebBackend<S: Sample> {
    direction: StreamDirection,
    inner: Option<Inner>,
    _phantom: PhantomData<S>,
}

struct Inner {
    ctx: AudioContext,
    _mic_stream: Option<MediaStream>,
    _mic_source: Option<MediaStreamAudioSourceNode>,
}

impl<S: Sample> WebBackend<S> {
    async fn build(direction: StreamDirection) -> Result<Inner> {
        let co = AudioContextOptions::new();
        co.set_latency_hint_audio_context_latency_category(
            AudioContextLatencyCategory::Interactive,
        );
        let ctx = AudioContext::new_with_context_options(&co).map_err(|_| Error::DeviceNotFound)?;

        if ctx.state() == AudioContextState::Suspended {
            if let Ok(p) = ctx.resume() {
                let _ = p.await;
            }
        }

        let worklet = ctx.audio_worklet().map_err(|_| Error::DeviceNotFound)?;
        let p = worklet
            .add_module("worklet.js")
            .map_err(|_| Error::DeviceNotFound)?;
        p.await.map_err(|_| Error::DeviceNotFound)?;

        let module = wasm_bindgen::module();
        let memory = wasm_bindgen::memory();

        let po = js_sys::Object::new();
        js_sys::Reflect::set(&po, &"module".into(), &module).ok();
        js_sys::Reflect::set(&po, &"memory".into(), &memory).ok();

        let opts = AudioWorkletNodeOptions::new();
        opts.set_processor_options(Some(&po));

        let (mic_stream, mic_source) = match direction {
            StreamDirection::Playback => {
                opts.set_number_of_inputs(1);
                opts.set_number_of_outputs(1);

                let node = AudioWorkletNode::new_with_options(&ctx, "vibraudio-playback", &opts)
                    .map_err(|_| Error::InvalidConfig)?;
                node.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (None, None)
            }
            StreamDirection::Capture => {
                opts.set_number_of_inputs(1);
                opts.set_number_of_outputs(1);

                let win = web_sys::window().ok_or(Error::DeviceNotFound)?;
                let md = win
                    .navigator()
                    .media_devices()
                    .map_err(|_| Error::DeviceNotFound)?;

                // configure audio constraints
                let audio_constraints = web_sys::MediaTrackConstraints::new();
                audio_constraints.set_auto_gain_control_bool(false);
                audio_constraints.set_echo_cancellation_bool(false);
                audio_constraints.set_noise_suppression_bool(false);

                let c = MediaStreamConstraints::new();
                c.set_audio_media_track_constraints(&audio_constraints);
                c.set_video(&JsValue::FALSE);
                let p = md
                    .get_user_media_with_constraints(&c)
                    .map_err(|_| Error::DeviceNotFound)?;
                let s = p
                    .await
                    .map_err(|_| Error::DeviceNotFound)?
                    .dyn_into::<MediaStream>()
                    .map_err(|_| Error::DeviceNotFound)?;
                let src = ctx
                    .create_media_stream_source(&s)
                    .map_err(|_| Error::InvalidConfig)?;
                let node = AudioWorkletNode::new_with_options(&ctx, "vibraudio-capture", &opts)
                    .map_err(|_| Error::InvalidConfig)?;
                src.connect_with_audio_node(&node)
                    .map_err(|_| Error::InvalidConfig)?;
                // Микрофон → capture node → gain(0) → destination.
                // gain(0) = захватываем, но НЕ воспроизводим напрямую.
                let gain = ctx.create_gain().map_err(|_| Error::InvalidConfig)?;
                gain.gain().set_value(0.0);
                node.connect_with_audio_node(&gain)
                    .map_err(|_| Error::InvalidConfig)?;
                gain.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (Some(s), Some(src))
            }
        };

        Ok(Inner {
            ctx,
            _mic_stream: mic_stream,
            _mic_source: mic_source,
        })
    }
}

impl<S: Sample> BackendAsync<S> for WebBackend<S> {
    async fn open_async(_name: &str, direction: StreamDirection) -> Result<Self> {
        Ok(Self {
            direction,
            inner: None,
            _phantom: PhantomData,
        })
    }

    async fn configure_async(&mut self, _config: &AudioConfig) -> Result<()> {
        let inner = Self::build(self.direction).await?;
        self.inner = Some(inner);
        info!("✅ WebAudio device opened for {:?}", self.direction);
        Ok(())
    }
}

impl<S: Sample> Backend<S> for WebBackend<S> {
    fn open(_: &str, _: StreamDirection) -> Result<Self> {
        Err(Error::AsyncRequired)
    }
    fn configure(&mut self, _: &AudioConfig) -> Result<()> {
        Ok(())
    }

    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        Ok(vec![AudioFormatInfo::new(Channels::Mono, 48_000, 32, 1)])
    }

    // Loopback идёт напрямую worklet-to-worklet через RING.
    // Трейт-методы для loopback не используются.
    fn write_frames(&mut self, _buffer: &[S], _channels: Channels) -> Result<usize> {
        Ok(0)
    }
    fn read_frames(&self, _buffer: &mut [S], _channels: Channels) -> Result<usize> {
        Ok(0)
    }

    fn reset(&mut self) -> Result<()> {
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        if let Some(inner) = self.inner.take() {
            let _ = inner.ctx.close();
        }
        Ok(())
    }
}

impl<S: Sample> Drop for WebBackend<S> {
    fn drop(&mut self) {
        _ = self.close();
    }
}

unsafe impl<S: Sample> Send for WebBackend<S> {}
unsafe impl<S: Sample> Sync for WebBackend<S> {}
