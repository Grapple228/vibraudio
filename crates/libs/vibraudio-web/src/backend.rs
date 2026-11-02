use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::OnceLock;
use tracing::info;
use vibraudio_core::BackendAsync;
use vibraudio_core::{
    AudioConfig, AudioFormatInfo, Backend, Channels, Error, Result, StreamDirection, sample::Sample,
};
use vibraudio_ringbuffer::{BufferReader, BufferWriter};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioContextLatencyCategory, AudioContextOptions, AudioContextState,
    AudioWorkletNode, AudioWorkletNodeOptions, GainNode, MediaStream, MediaStreamAudioSourceNode,
    MediaStreamConstraints, MediaTrackConstraints,
};

pub const FRAMES: usize = 128;
const MAX_FRAMES: usize = 256;

// Ring buffer capacities in stereo frames.
const CAPTURE_FRAMES: usize = 4096;
const PLAYBACK_FRAMES: usize = 4096;

// ============================================================================
// StereoFrame — Sample for a single [L, R] pair of f32.
// Because of this, vibraudio_ringbuffer operates in FRAMES, not samples.
// Frame atomicity is guaranteed by the type instead of manual (rd + 1) % CAP.
// ============================================================================

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq)]
pub struct StereoFrame(pub [f32; 2]);

impl Sample for StereoFrame {
    const ZERO: Self = StereoFrame([0.0; 2]);

    fn from_f32(v: f32) -> Self {
        StereoFrame([v, v])
    }

    fn to_f32(&self) -> f32 {
        (self.0[0] + self.0[1]) * 0.5
    }

    fn sample_format() -> vibraudio_core::SampleFormat {
        vibraudio_core::SampleFormat::FloatLe
    }

    fn add(&self, other: Self) -> Self {
        StereoFrame([self.0[0] + other.0[0], self.0[1] + other.0[1]])
    }

    fn mul(&self, other: Self) -> Self {
        StereoFrame([self.0[0] * other.0[0], self.0[1] * other.0[1]])
    }
}

type CapWriter = BufferWriter<CAPTURE_FRAMES, StereoFrame>;
type CapReader = BufferReader<CAPTURE_FRAMES, StereoFrame>;
type PlayWriter = BufferWriter<PLAYBACK_FRAMES, StereoFrame>;
type PlayReader = BufferReader<PLAYBACK_FRAMES, StereoFrame>;

static CAPTURE_PAIR: OnceLock<(CapWriter, CapReader)> = OnceLock::new();
static PLAYBACK_PAIR: OnceLock<(PlayWriter, PlayReader)> = OnceLock::new();

fn capture_pair() -> &'static (CapWriter, CapReader) {
    CAPTURE_PAIR.get_or_init(|| vibraudio_ringbuffer::create_pair())
}

fn playback_pair() -> &'static (PlayWriter, PlayReader) {
    PLAYBACK_PAIR.get_or_init(|| vibraudio_ringbuffer::create_pair())
}

// Buffers for exchanging with worklet.js — interleaved [L, R, L, R, ...].
static mut INPUT: [f32; MAX_FRAMES * 2] = [0.0; MAX_FRAMES * 2];
static mut OUTPUT: [f32; MAX_FRAMES * 2] = [0.0; MAX_FRAMES * 2];

// One AudioContext for the whole app — one hardware clock, no ppm drift.
thread_local! {
    static SHARED_CTX: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
    static WORKLET_LOADED: RefCell<bool> = const { RefCell::new(false) };
}

#[wasm_bindgen]
pub fn input_ptr() -> u32 {
    std::ptr::addr_of_mut!(INPUT) as u32
}

#[wasm_bindgen]
pub fn output_ptr() -> u32 {
    std::ptr::addr_of_mut!(OUTPUT) as u32
}

/// Worklet writes INPUT → ring.
/// `write_data` inside vibraudio_ringbuffer is atomic per frame.
#[wasm_bindgen]
pub fn capture_process(frames: u32) {
    let frames = (frames as usize).min(MAX_FRAMES);
    if frames == 0 {
        return;
    }
    let input_frames: &[StereoFrame] = unsafe {
        std::slice::from_raw_parts(std::ptr::addr_of!(INPUT) as *const StereoFrame, frames)
    };
    capture_pair().0.write(input_frames);
}

/// Worklet reads ring → OUTPUT. On underrun — fade out.
#[wasm_bindgen]
pub fn playback_process(frames: u32) {
    let frames = (frames as usize).min(MAX_FRAMES);
    if frames == 0 {
        return;
    }
    let output_frames: &mut [StereoFrame] = unsafe {
        std::slice::from_raw_parts_mut(std::ptr::addr_of_mut!(OUTPUT) as *mut StereoFrame, frames)
    };
    let got = playback_pair().1.read(output_frames);
    if got < frames {
        let (l, r) = if got > 0 {
            (output_frames[got - 1].0[0], output_frames[got - 1].0[1])
        } else {
            (0.0, 0.0)
        };
        let rem = frames - got;
        for j in 0..rem {
            let t = 1.0 - (j as f32 / rem as f32);
            output_frames[got + j].0 = [l * t, r * t];
        }
    }
}

pub struct WebBackend<S: Sample> {
    direction: StreamDirection,
    inner: Option<Inner>,
    _phantom: PhantomData<S>,
}

struct Inner {
    ctx: AudioContext,
    // Kept alive explicitly: even though the JS graph holds the node,
    // AudioWorkletNode retirement rules for sourceless nodes are fuzzy.
    // Holding the handle guarantees the node is dropped deterministically
    // together with Inner (and disconnected from destination).
    _node: AudioWorkletNode,
    // Only for capture path: gain(0) so mic is not echoed to speakers.
    _gain: Option<GainNode>,
    _mic_stream: Option<MediaStream>,
    _mic_source: Option<MediaStreamAudioSourceNode>,
}

async fn get_shared_ctx() -> Result<AudioContext> {
    if let Some(ctx) = SHARED_CTX.with(|c| c.borrow().clone()) {
        if ctx.state() == AudioContextState::Suspended {
            if let Ok(p) = ctx.resume() {
                let _ = p.await;
            }
        }
        return Ok(ctx);
    }

    let co = AudioContextOptions::new();
    co.set_latency_hint_audio_context_latency_category(AudioContextLatencyCategory::Interactive);
    co.set_sample_rate(48_000.0);
    let ctx = AudioContext::new_with_context_options(&co).map_err(|_| Error::DeviceNotFound)?;

    info!("AudioContext created, sample_rate = {}", ctx.sample_rate());

    if ctx.state() == AudioContextState::Suspended {
        if let Ok(p) = ctx.resume() {
            let _ = p.await;
        }
    }

    let loaded = WORKLET_LOADED.with(|c| *c.borrow());
    if !loaded {
        ctx.audio_worklet()
            .map_err(|_| Error::DeviceNotFound)?
            .add_module("worklet.js")
            .map_err(|_| Error::DeviceNotFound)?
            .await
            .map_err(|_| Error::DeviceNotFound)?;
        WORKLET_LOADED.with(|c| *c.borrow_mut() = true);
    }

    SHARED_CTX.with(|c| *c.borrow_mut() = Some(ctx.clone()));
    Ok(ctx)
}

impl<S: Sample> WebBackend<S> {
    pub fn playback_free_frames(&self) -> usize {
        playback_pair().0.available_space()
    }

    pub fn capture_available_frames(&self) -> usize {
        capture_pair().1.available_data()
    }

    async fn build(direction: StreamDirection) -> Result<Inner> {
        let ctx = get_shared_ctx().await?;

        let module = wasm_bindgen::module();
        let memory = wasm_bindgen::memory();

        let po = js_sys::Object::new();
        js_sys::Reflect::set(&po, &"module".into(), &module).ok();
        js_sys::Reflect::set(&po, &"memory".into(), &memory).ok();

        let opts = AudioWorkletNodeOptions::new();
        opts.set_processor_options(Some(&po));

        let (mic_stream, mic_source, node, gain) = match direction {
            StreamDirection::Playback => {
                opts.set_number_of_inputs(0);
                opts.set_number_of_outputs(1);

                let oc = js_sys::Array::new();
                oc.push(&JsValue::from(2));
                opts.set_output_channel_count(&oc);

                let node = AudioWorkletNode::new_with_options(&ctx, "vibraudio-playback", &opts)
                    .map_err(|_| Error::InvalidConfig)?;
                node.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (None, None, node, None)
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
                let gain = ctx.create_gain().map_err(|_| Error::InvalidConfig)?;
                gain.gain().set_value(0.0);
                node.connect_with_audio_node(&gain)
                    .map_err(|_| Error::InvalidConfig)?;
                gain.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (Some(s), Some(src), node, Some(gain))
            }
        };

        Ok(Inner {
            ctx,
            _node: node,
            _gain: gain,
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
        info!(
            "✅ WebAudio device opened: {:?}, ctx.rate={}",
            self.direction,
            inner.ctx.sample_rate() as u32
        );
        self.inner = Some(inner);
        Ok(())
    }

    async fn read_frames_async(&self, buffer: &mut [S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let want_frames = buffer.len() / ch;
        if want_frames == 0 {
            return Ok(0);
        }
        // Defensive: if want_frames >= CAPTURE_FRAMES the wait loop would
        // never break (ring capacity is the hard upper bound).
        if want_frames >= CAPTURE_FRAMES {
            return Err(Error::InvalidConfig);
        }

        // Hybrid wait: 4 fast yields, then sleep(1ms).
        let mut spins: u32 = 0;
        loop {
            if capture_pair().1.available_data() >= want_frames {
                break;
            }
            spins += 1;
            if spins <= 4 {
                crate::yield_fast().await;
            } else {
                tokio_with_wasm::time::sleep(std::time::Duration::from_millis(1)).await;
                spins = 0;
            }
        }

        let mut scratch = [StereoFrame::ZERO; MAX_FRAMES];
        let mut total = 0;
        while total < want_frames {
            let chunk = (want_frames - total).min(MAX_FRAMES);
            let got = capture_pair().1.read(&mut scratch[..chunk]);
            if got == 0 {
                break;
            }
            for i in 0..got {
                let l = scratch[i].0[0];
                let r = scratch[i].0[1];
                let out = total + i;
                if ch == 1 {
                    buffer[out] = S::from_f32((l + r) * 0.5);
                } else {
                    buffer[out * ch] = S::from_f32(l);
                    buffer[out * ch + 1] = S::from_f32(r);
                }
            }
            total += got;
        }
        Ok(total)
    }

    async fn write_frames_async(&mut self, buffer: &[S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let want_frames = buffer.len() / ch;
        if want_frames == 0 {
            return Ok(0);
        }
        if want_frames >= PLAYBACK_FRAMES {
            return Err(Error::InvalidConfig);
        }

        let mut spins: u32 = 0;
        loop {
            if playback_pair().0.available_space() >= want_frames {
                break;
            }
            spins += 1;
            if spins <= 4 {
                crate::yield_fast().await;
            } else {
                tokio_with_wasm::time::sleep(std::time::Duration::from_millis(1)).await;
                spins = 0;
            }
        }

        let mut scratch = [StereoFrame::ZERO; MAX_FRAMES];
        let mut total = 0;
        while total < want_frames {
            let chunk = (want_frames - total).min(MAX_FRAMES);
            for i in 0..chunk {
                let s = total + i;
                let (l, r) = if ch == 1 {
                    let v = buffer[s].to_f32();
                    (v, v)
                } else {
                    (buffer[s * ch].to_f32(), buffer[s * ch + 1].to_f32())
                };
                scratch[i] = StereoFrame([l, r]);
            }
            let written = playback_pair().0.write(&scratch[..chunk]);
            if written == 0 {
                break;
            }
            total += written;
        }
        Ok(total)
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
        let avail = capture_pair().1.available_data();
        let to_read = want_frames.min(avail);
        if to_read == 0 {
            return Ok(0);
        }

        let mut scratch = [StereoFrame::ZERO; MAX_FRAMES];
        let mut total = 0;
        while total < to_read {
            let chunk = (to_read - total).min(MAX_FRAMES);
            let got = capture_pair().1.read(&mut scratch[..chunk]);
            if got == 0 {
                break;
            }
            for i in 0..got {
                let l = scratch[i].0[0];
                let r = scratch[i].0[1];
                let out = total + i;
                if ch == 1 {
                    buffer[out] = S::from_f32((l + r) * 0.5);
                } else {
                    buffer[out * ch] = S::from_f32(l);
                    buffer[out * ch + 1] = S::from_f32(r);
                }
            }
            total += got;
        }
        Ok(total)
    }

    fn write_frames(&mut self, buffer: &[S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let want_frames = buffer.len() / ch;
        let avail = playback_pair().0.available_space();
        let to_write = want_frames.min(avail);
        if to_write == 0 {
            return Ok(0);
        }

        let mut scratch = [StereoFrame::ZERO; MAX_FRAMES];
        let mut total = 0;
        while total < to_write {
            let chunk = (to_write - total).min(MAX_FRAMES);
            for i in 0..chunk {
                let s = total + i;
                let (l, r) = if ch == 1 {
                    let v = buffer[s].to_f32();
                    (v, v)
                } else {
                    (buffer[s * ch].to_f32(), buffer[s * ch + 1].to_f32())
                };
                scratch[i] = StereoFrame([l, r]);
            }
            let written = playback_pair().0.write(&scratch[..chunk]);
            if written == 0 {
                break;
            }
            total += written;
        }
        Ok(total)
    }

    /// Drain both rings so a stop/restart does not deliver stale samples.
    ///
    /// Must not be called while a stream is running — the playback drain
    /// conflicts with the worklet's read side on PLAYBACK_RING.
    fn reset(&mut self) -> Result<()> {
        let mut scratch = [StereoFrame::ZERO; MAX_FRAMES];

        // capture ring: read and discard everything
        loop {
            let n = capture_pair().1.read(&mut scratch);
            if n == 0 {
                break;
            }
        }

        // playback ring: same, drain the reader side
        loop {
            let n = playback_pair().1.read(&mut scratch);
            if n == 0 {
                break;
            }
        }

        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        // Shared ctx is not closed — the other backend uses it.
        // Dropping Inner disconnects and drops the AudioWorkletNode,
        // the gain (if any), and the mic source node.
        self.inner = None;
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
