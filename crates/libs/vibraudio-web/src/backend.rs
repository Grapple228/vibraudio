use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use tracing::{debug, info, warn};
use vibraudio_core::{
    AudioConfig, AudioFormatInfo, Backend, Channels, Error, Result, StreamDirection, sample::Sample,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioContextState, AudioWorkletNode, MediaStream, MediaStreamAudioSourceNode,
    MediaStreamConstraints, MessageEvent,
};

pub const FRAMES: usize = 512;

thread_local! {
    static CAPTURE_BUF: RefCell<[f32; FRAMES]> = RefCell::new([0.0; FRAMES]);
    static CAPTURE_LEN: Cell<usize> = const { Cell::new(0) };
}

pub struct WebBackend<S: Sample> {
    ctx: AudioContext,
    node: AudioWorkletNode,
    direction: StreamDirection,
    _capture_cb: Closure<dyn FnMut(MessageEvent)>,
    // Для Capture держим MediaStream и его source живыми, иначе GC их уберёт
    // и микрофон перестанет поступать в worklet.
    _mic_source: Option<MediaStreamAudioSourceNode>,
    _mic_stream: Option<MediaStream>,
    _phantom: PhantomData<S>,
}

impl<S: Sample> WebBackend<S> {
    pub async fn open_async(_name: &str, direction: StreamDirection) -> Result<Self> {
        let ctx = AudioContext::new().map_err(|_| Error::DeviceNotFound)?;

        if ctx.state() == AudioContextState::Suspended {
            if let Ok(promise) = ctx.resume() {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            }
            if ctx.state() == AudioContextState::Suspended {
                warn!("AudioContext suspended. Sound requires a user gesture (click).");
            }
        }

        let worklet = ctx.audio_worklet().map_err(|_| Error::DeviceNotFound)?;
        let promise = worklet
            .add_module("worklet.js")
            .map_err(|_| Error::DeviceNotFound)?;
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(|_| Error::DeviceNotFound)?;

        let (node, mic_source, mic_stream) = match direction {
            StreamDirection::Playback => {
                let node = AudioWorkletNode::new(&ctx, "vibraudio-playback")
                    .map_err(|_| Error::InvalidConfig)?;
                node.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;
                (node, None, None)
            }
            StreamDirection::Capture => {
                let window = web_sys::window().ok_or(Error::DeviceNotFound)?;
                let navigator = window.navigator();
                let media_devices = navigator
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

                let node = AudioWorkletNode::new(&ctx, "vibraudio-capture")
                    .map_err(|_| Error::InvalidConfig)?;

                // Подключаем микрофон ко ВХОДУ worklet'а
                source
                    .connect_with_audio_node(&node)
                    .map_err(|_| Error::InvalidConfig)?;

                // Подключаем worklet к destination через GainNode с громкостью 0,
                // чтобы он участвовал в pull-графе и process() вызывался.
                // Выход всё равно нулевой (мы не пишем в outputs), так что эха не будет.
                let gain = ctx.create_gain().map_err(|_| Error::InvalidConfig)?;
                gain.gain().set_value(0.0);
                node.connect_with_audio_node(&gain)
                    .map_err(|_| Error::InvalidConfig)?;
                gain.connect_with_audio_node(&ctx.destination())
                    .map_err(|_| Error::InvalidConfig)?;

                (node, Some(source), Some(stream))
            }
        };

        // Capture: worklet -> CAPTURE_BUF
        let port = node.port().map_err(|_| Error::InvalidConfig)?;
        let cb = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(array) = event.data().dyn_ref::<js_sys::Float32Array>() {
                let n = (array.length() as usize).min(FRAMES);
                let mut tmp = [0.0f32; FRAMES];
                array.copy_to(&mut tmp[..n]);
                CAPTURE_BUF.with(|buf| {
                    let mut buf = buf.borrow_mut();
                    let len = CAPTURE_LEN.get();
                    let copy = n.min(FRAMES - len);
                    buf[len..len + copy].copy_from_slice(&tmp[..copy]);
                    CAPTURE_LEN.set(len + copy);
                });
            }
        });
        port.set_onmessage(Some(cb.as_ref().unchecked_ref()));

        info!("✅ WebAudio device opened for {:?}", direction);

        Ok(Self {
            ctx,
            node,
            direction,
            _capture_cb: cb,
            _mic_source: mic_source,
            _mic_stream: mic_stream,
            _phantom: PhantomData,
        })
    }
}

impl<S: Sample> Backend<S> for WebBackend<S> {
    fn open(_name: &str, _direction: StreamDirection) -> Result<Self> {
        Err(Error::InvalidConfig)
    }

    fn configure(&self, _config: &AudioConfig) -> Result<()> {
        Ok(())
    }

    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        Ok(vec![AudioFormatInfo::new(Channels::Stereo, 48_000, 32, 3)])
    }

    // Исправленный write_frames: отправляет ВСЕ данные, разбивая на чанки
    fn write_frames(&self, buffer: &[S], channels: Channels) -> Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let ch = channels.as_usize();
        let total_frames = buffer.len() / ch;

        let mut offset = 0;
        while offset < buffer.len() {
            let end = (offset + FRAMES).min(buffer.len());
            let chunk = &buffer[offset..end];

            let mut tmp = [0.0f32; FRAMES];
            let n = chunk.len();
            for (dst, src) in tmp[..n].iter_mut().zip(chunk.iter()) {
                *dst = src.to_f32();
            }

            let array = js_sys::Float32Array::from(&tmp[..n]);
            self.node
                .port()
                .map_err(|_| Error::InvalidConfig)?
                .post_message(&array)
                .map_err(|_| Error::InvalidConfig)?;

            offset = end;
        }

        Ok(total_frames)
    }

    fn read_frames(&self, buffer: &mut [S], channels: Channels) -> Result<usize> {
        let max_frames = buffer.len() / channels.as_usize();
        CAPTURE_BUF.with(|buf| {
            let cap = buf.borrow();
            let len = CAPTURE_LEN.get();
            let n = (max_frames * channels.as_usize()).min(len);
            if n == 0 {
                return Ok(0);
            }
            for (dst, src) in buffer[..n].iter_mut().zip(cap[..n].iter()) {
                *dst = S::from_f32(*src);
            }
            drop(cap);
            let mut cap = buf.borrow_mut();
            cap.copy_within(n..len, 0);
            CAPTURE_LEN.set(len - n);
            Ok(n / channels.as_usize())
        })
    }

    fn reset(&self) -> Result<()> {
        CAPTURE_LEN.set(0);
        Ok(())
    }

    fn close(&self) -> Result<()> {
        let _ = self.ctx.close();
        Ok(())
    }
}

impl<S: Sample> Drop for WebBackend<S> {
    fn drop(&mut self) {
        debug!("WebBackend is dropped");
        let _ = self.close();
    }
}

unsafe impl<S: Sample> Send for WebBackend<S> {}
unsafe impl<S: Sample> Sync for WebBackend<S> {}
