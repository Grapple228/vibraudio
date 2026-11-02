use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    MediaStreamConstraints, MediaTrackConstraints,
};

pub const FRAMES: usize = 128;
const MAX_FRAMES: usize = 128;

// Стерео-буферы, ёмкость в сэмплах (L,R,L,R,...).
// CAPTURE: ~42 мс @ 48k stereo — Mic дренирует быстро.
// PLAYBACK: ~42 мс @ 48k stereo — перекрывает тик MediaClock 20 мс.
const CAPTURE_CAP: usize = 8192;
const PLAYBACK_CAP: usize = 8192;

// INPUT/OUTPUT — стерео interleaved, с ними работает worklet.js (256 сэмплов на квант).
static mut INPUT: [f32; MAX_FRAMES * 2] = [0.0; MAX_FRAMES * 2];
static mut OUTPUT: [f32; MAX_FRAMES * 2] = [0.0; MAX_FRAMES * 2];

static mut CAPTURE_BUF: [f32; CAPTURE_CAP] = [0.0; CAPTURE_CAP];
static CAPTURE_WR: AtomicUsize = AtomicUsize::new(0);
static CAPTURE_RD: AtomicUsize = AtomicUsize::new(0);

static mut PLAYBACK_BUF: [f32; PLAYBACK_CAP] = [0.0; PLAYBACK_CAP];
static PLAYBACK_WR: AtomicUsize = AtomicUsize::new(0);
static PLAYBACK_RD: AtomicUsize = AtomicUsize::new(0);

#[wasm_bindgen]
pub fn input_ptr() -> u32 {
    std::ptr::addr_of_mut!(INPUT) as u32
}

#[wasm_bindgen]
pub fn output_ptr() -> u32 {
    std::ptr::addr_of_mut!(OUTPUT) as u32
}

/// CaptureProcessor пишет в INPUT (стерео interleaved). Копируем INPUT → CAPTURE_BUF.
#[wasm_bindgen]
pub fn capture_process(frames: u32) {
    let n = (frames as usize).min(MAX_FRAMES) * 2;
    let input = unsafe { &*std::ptr::addr_of!(INPUT) };
    let buf = unsafe { &mut *std::ptr::addr_of_mut!(CAPTURE_BUF) };
    let mut wr = CAPTURE_WR.load(Ordering::Acquire);
    for i in 0..n {
        buf[wr] = input[i];
        wr = (wr + 1) % CAPTURE_CAP;
    }
    CAPTURE_WR.store(wr, Ordering::Release);
}

/// PlaybackProcessor читает из OUTPUT (стерео interleaved). Копируем PLAYBACK_BUF → OUTPUT.
#[wasm_bindgen]
pub fn playback_process(frames: u32) {
    let n = (frames as usize).min(MAX_FRAMES) * 2;
    let buf = unsafe { &*std::ptr::addr_of!(PLAYBACK_BUF) };
    let output = unsafe { &mut *std::ptr::addr_of_mut!(OUTPUT) };
    let mut rd = PLAYBACK_RD.load(Ordering::Acquire);
    let wr = PLAYBACK_WR.load(Ordering::Acquire);
    for i in 0..n {
        if rd != wr {
            output[i] = buf[rd];
            rd = (rd + 1) % PLAYBACK_CAP;
        } else {
            output[i] = 0.0;
        }
    }
    PLAYBACK_RD.store(rd, Ordering::Release);
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
        co.set_sample_rate(48_000.0);
        let ctx = AudioContext::new_with_context_options(&co).map_err(|_| Error::DeviceNotFound)?;

        info!("AudioContext sample_rate = {}", ctx.sample_rate());

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
                opts.set_number_of_inputs(0);
                opts.set_number_of_outputs(1);

                // Явно стерео-выход (иначе браузер может отдать моно)
                let oc = js_sys::Array::new();
                oc.push(&JsValue::from(2));
                opts.set_output_channel_count(&oc);

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

                let audio_constraints = MediaTrackConstraints::new();
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
        Ok(vec![AudioFormatInfo::new(Channels::Stereo, 48_000, 32, 1)])
    }

    fn read_frames(&self, buffer: &mut [S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let want_frames = buffer.len() / ch;
        let rd = CAPTURE_RD.load(Ordering::Acquire);
        let wr = CAPTURE_WR.load(Ordering::Acquire);
        let avail_samples = (wr + CAPTURE_CAP - rd) % CAPTURE_CAP;
        let avail_frames = avail_samples / 2;
        let take = want_frames.min(avail_frames);
        if take == 0 {
            return Ok(0);
        }

        let buf = unsafe { &*std::ptr::addr_of!(CAPTURE_BUF) };
        let mut rd_new = rd;
        for i in 0..take {
            let l = buf[rd_new];
            let r = buf[(rd_new + 1) % CAPTURE_CAP];
            rd_new = (rd_new + 2) % CAPTURE_CAP;
            if ch == 1 {
                buffer[i] = S::from_f32((l + r) * 0.5);
            } else {
                buffer[i * ch] = S::from_f32(l);
                buffer[i * ch + 1] = S::from_f32(r);
            }
        }
        CAPTURE_RD.store(rd_new, Ordering::Release);
        Ok(take)
    }

    fn write_frames(&mut self, buffer: &[S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let frames = buffer.len() / ch;
        let wr = PLAYBACK_WR.load(Ordering::Acquire);
        let rd = PLAYBACK_RD.load(Ordering::Acquire);
        let used_samples = (wr + PLAYBACK_CAP - rd) % PLAYBACK_CAP;
        // Резервируем один стерео-фрейм свободным для различия «полный/пустой»
        let free_frames = (PLAYBACK_CAP - 2 - used_samples) / 2;
        let take = frames.min(free_frames);
        if take == 0 {
            return Ok(0);
        }

        let buf = unsafe { &mut *std::ptr::addr_of_mut!(PLAYBACK_BUF) };
        let mut wr_new = wr;
        for i in 0..take {
            let (l, r) = if ch == 1 {
                let s = buffer[i].to_f32();
                (s, s)
            } else {
                (buffer[i * ch].to_f32(), buffer[i * ch + 1].to_f32())
            };
            buf[wr_new] = l;
            buf[(wr_new + 1) % PLAYBACK_CAP] = r;
            wr_new = (wr_new + 2) % PLAYBACK_CAP;
        }
        PLAYBACK_WR.store(wr_new, Ordering::Release);
        Ok(take)
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
