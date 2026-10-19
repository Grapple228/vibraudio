use std::marker::PhantomData;
use std::sync::Mutex;
use tracing::{debug, info, warn};
use vibraudio_core::{
    AudioConfig, AudioFormatInfo, Backend, Channels, Error, Result, StreamDirection, sample::Sample,
};
use vibraudio_ringbuffer::{BufferReader, BufferWriter};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioContextState, AudioWorkletNode, AudioWorkletNodeOptions, MediaStream,
    MediaStreamAudioSourceNode, MediaStreamConstraints,
};

pub const FRAMES: usize = 512;
const RING_SIZE: usize = 8192;
const MAX_FRAMES: usize = 128;
const MAX_CHANNELS: usize = 2;
const CHUNK: usize = 256;

// ProcessorState — свой у каждого WebBackend. Принадлежит audio-потоку.
// Для Playback: reader + output (worklet читает из reader, пишет в output).
// Для Capture:  writer + input  (worklet пишет в writer из input).
enum ProcessorState {
    Playback {
        reader: BufferReader<RING_SIZE, f32>,
        output: [f32; MAX_FRAMES * MAX_CHANNELS],
    },
    Capture {
        writer: BufferWriter<RING_SIZE, f32>,
        input: [f32; MAX_FRAMES * MAX_CHANNELS],
    },
}

pub struct WebBackend<S: Sample> {
    direction: StreamDirection,
    inner: Mutex<Option<Inner>>,
    _mic_source: Mutex<Option<MediaStreamAudioSourceNode>>,
    _mic_stream: Mutex<Option<MediaStream>>,
    _phantom: PhantomData<S>,
}

struct Inner {
    ctx: AudioContext,
    processor_handle: u32,
    // Для Playback: main thread пишет сюда через write_frames.
    playback_writer: Option<BufferWriter<RING_SIZE, f32>>,
    // Для Capture: main thread читает отсюда через read_frames.
    capture_reader: Option<BufferReader<RING_SIZE, f32>>,
}

impl<S: Sample> WebBackend<S> {
    async fn build(
        direction: StreamDirection,
        config: &AudioConfig,
    ) -> Result<(
        Inner,
        Option<MediaStreamAudioSourceNode>,
        Option<MediaStream>,
    )> {
        let ctx = AudioContext::new().map_err(|_| Error::DeviceNotFound)?;

        info!(
            "AudioContext native sampleRate = {}, config.sample_rate = {}",
            ctx.sample_rate(),
            config.sample_rate
        );

        if ctx.state() == AudioContextState::Suspended {
            if let Ok(promise) = ctx.resume() {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            }
        }

        let worklet = ctx.audio_worklet().map_err(|_| Error::DeviceNotFound)?;
        let promise = worklet
            .add_module("worklet.js")
            .map_err(|_| Error::DeviceNotFound)?;
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|_| Error::DeviceNotFound)?;

        let wasm_module = wasm_bindgen::module();
        let wasm_memory = wasm_bindgen::memory();

        // Свой ProcessorState и своя пара ringbuffer для каждого бэкенда.
        let (state, playback_writer, capture_reader) = match direction {
            StreamDirection::Playback => {
                let (writer, reader) = vibraudio_ringbuffer::create_pair::<RING_SIZE, f32>();
                let state = ProcessorState::Playback {
                    reader,
                    output: [0.0; MAX_FRAMES * MAX_CHANNELS],
                };
                (state, Some(writer), None)
            }
            StreamDirection::Capture => {
                let (writer, reader) = vibraudio_ringbuffer::create_pair::<RING_SIZE, f32>();
                let state = ProcessorState::Capture {
                    writer,
                    input: [0.0; MAX_FRAMES * MAX_CHANNELS],
                };
                (state, None, Some(reader))
            }
        };

        let processor_handle = Box::into_raw(Box::new(state)) as u32;

        let opts = js_sys::Object::new();
        js_sys::Reflect::set(&opts, &JsValue::from_str("module"), &wasm_module).unwrap();
        js_sys::Reflect::set(&opts, &JsValue::from_str("memory"), &wasm_memory).unwrap();
        js_sys::Reflect::set(
            &opts,
            &JsValue::from_str("processorHandle"),
            &JsValue::from(processor_handle),
        )
        .unwrap();

        let mut node_opts = AudioWorkletNodeOptions::new();
        node_opts.set_processor_options(Some(&opts));

        let (mic_source, mic_stream) = match direction {
            StreamDirection::Playback => {
                let node =
                    AudioWorkletNode::new_with_options(&ctx, "vibraudio-playback", &node_opts)
                        .map_err(|_| Error::InvalidConfig)?;
                node.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (None, None)
            }
            StreamDirection::Capture => {
                let window = web_sys::window().ok_or(Error::DeviceNotFound)?;
                let media_devices = window
                    .navigator()
                    .media_devices()
                    .map_err(|_| Error::DeviceNotFound)?;
                let constraints = MediaStreamConstraints::new();
                constraints.set_audio(&JsValue::from(true));
                let promise = media_devices
                    .get_user_media_with_constraints(&constraints)
                    .map_err(|_| Error::DeviceNotFound)?;
                let stream = wasm_bindgen_futures::JsFuture::from(promise)
                    .await
                    .map_err(|_| Error::DeviceNotFound)?
                    .dyn_into::<MediaStream>()
                    .map_err(|_| Error::DeviceNotFound)?;
                let source = ctx
                    .create_media_stream_source(&stream)
                    .map_err(|_| Error::InvalidConfig)?;
                let node =
                    AudioWorkletNode::new_with_options(&ctx, "vibraudio-capture", &node_opts)
                        .map_err(|_| Error::InvalidConfig)?;
                source
                    .connect_with_audio_node(&node)
                    .map_err(|_| Error::InvalidConfig)?;
                let gain = ctx.create_gain().map_err(|_| Error::InvalidConfig)?;
                gain.gain().set_value(0.0);
                node.connect_with_audio_node(&gain)
                    .map_err(|_| Error::InvalidConfig)?;
                gain.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (Some(source), Some(stream))
            }
        };

        Ok((
            Inner {
                ctx,
                processor_handle,
                playback_writer,
                capture_reader,
            },
            mic_source,
            mic_stream,
        ))
    }
}

// ============================================================
// Экспортируемые функции. Каждая вызывается из worklet'а
// со СВОИМ handle, поэтому каждый backend получает свой ProcessorState.
// ============================================================

#[wasm_bindgen]
pub fn init_processor(_handle: u32, sample_rate: f32, max_frames: u32) {
    info!(
        "Rust worklet processor init: sample_rate={}, max_frames={}",
        sample_rate, max_frames
    );
}

#[wasm_bindgen]
pub fn process_playback(handle: u32, frames: u32) {
    let n = (frames as usize) * MAX_CHANNELS;
    unsafe {
        let state = &mut *(handle as *mut ProcessorState);
        if let ProcessorState::Playback { reader, output } = state {
            let read = reader.read(&mut output[..n]);
            for i in read..n {
                output[i] = 0.0;
            }
        }
    }
}

#[wasm_bindgen]
pub fn playback_output_ptr(handle: u32) -> u32 {
    unsafe {
        let state = &*(handle as *mut ProcessorState);
        match state {
            ProcessorState::Playback { output, .. } => output.as_ptr() as u32,
            _ => 0,
        }
    }
}

#[wasm_bindgen]
pub fn capture_input_ptr(handle: u32) -> u32 {
    unsafe {
        let state = &mut *(handle as *mut ProcessorState);
        match state {
            ProcessorState::Capture { input, .. } => input.as_mut_ptr() as u32,
            _ => 0,
        }
    }
}

#[wasm_bindgen]
pub fn process_capture(handle: u32, frames: u32) {
    let n = (frames as usize) * MAX_CHANNELS;
    unsafe {
        let state = &mut *(handle as *mut ProcessorState);
        if let ProcessorState::Capture { writer, input } = state {
            let _ = writer.write(&input[..n]);
        }
    }
}

// ============================================================
// Backend impl — как AlsaBackend, только через ringbuffer.
// ============================================================

impl<S: Sample> Backend<S> for WebBackend<S> {
    async fn open_async(_name: &str, direction: StreamDirection) -> Result<Self> {
        Ok(Self {
            direction,
            inner: Mutex::new(None),
            _mic_source: Mutex::new(None),
            _mic_stream: Mutex::new(None),
            _phantom: PhantomData,
        })
    }

    async fn configure_async(&self, config: &AudioConfig) -> Result<()> {
        let (inner, mic_source, mic_stream) = Self::build(self.direction, config).await?;
        *self.inner.lock().unwrap() = Some(inner);
        *self._mic_source.lock().unwrap() = mic_source;
        *self._mic_stream.lock().unwrap() = mic_stream;
        info!(
            "✅ WebAudio device opened for {:?} (native sample_rate)",
            self.direction
        );
        Ok(())
    }

    fn open(_name: &str, _direction: StreamDirection) -> Result<Self> {
        Err(Error::AsyncRequired)
    }

    fn configure(&self, config: &AudioConfig) -> Result<()> {
        if config.channels.as_usize() == 0 || config.sample_rate == 0 {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }

    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        Ok(vec![AudioFormatInfo::new(Channels::Stereo, 48_000, 32, 3)])
    }

    fn write_frames(&self, buffer: &[S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let guard = self.inner.lock().unwrap();
        let Some(inner) = guard.as_ref() else {
            return Err(Error::InvalidConfig);
        };
        let Some(writer) = inner.playback_writer.as_ref() else {
            return Err(Error::InvalidConfig);
        };

        let mut tmp = [0.0f32; CHUNK];
        let mut offset = 0usize;
        let mut total = 0usize;

        while offset < buffer.len() {
            let end = (offset + CHUNK).min(buffer.len());
            let n = end - offset;
            for i in 0..n {
                tmp[i] = buffer[offset + i].to_f32();
            }
            let written = writer.write(&tmp[..n]);
            total += written;
            if written < n {
                break; // ringbuffer полон
            }
            offset = end;
        }

        Ok(total / ch)
    }

    fn read_frames(&self, buffer: &mut [S], channels: Channels) -> Result<usize> {
        let ch = channels.as_usize();
        let guard = self.inner.lock().unwrap();
        let Some(inner) = guard.as_ref() else {
            return Err(Error::InvalidConfig);
        };
        let Some(reader) = inner.capture_reader.as_ref() else {
            return Err(Error::InvalidConfig);
        };

        let mut tmp = [0.0f32; CHUNK];
        let mut offset = 0usize;
        let mut total = 0usize;

        while offset < buffer.len() {
            let end = (offset + CHUNK).min(buffer.len());
            let n = end - offset;
            let read = reader.read(&mut tmp[..n]);
            for i in 0..read {
                buffer[offset + i] = S::from_f32(tmp[i]);
            }
            total += read;
            if read < n {
                break; // ringbuffer пуст
            }
            offset = end;
        }

        Ok(total / ch)
    }

    fn reset(&self) -> Result<()> {
        Ok(())
    }

    fn close(&self) -> Result<()> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(inner) = guard.take() {
            let _ = inner.ctx.close();
            if inner.processor_handle != 0 {
                unsafe {
                    let _ = Box::from_raw(inner.processor_handle as *mut ProcessorState);
                }
            }
        }
        Ok(())
    }
}

// SAFETY: WebBackend используется только в main thread.
// ProcessorState — только audio-поток. Ringbuffer — SPSC.
unsafe impl<S: Sample> Send for WebBackend<S> {}
unsafe impl<S: Sample> Sync for WebBackend<S> {}
