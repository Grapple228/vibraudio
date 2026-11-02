use crate::platform::DefaultBackend;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use vibraudio_core::AudioConfig;
use vibraudio_core::Backend;
use vibraudio_core::StreamDirection;
use vibraudio_core::sample::Sample;
use vibraudio_core::{Error, Result};
use vibraudio_ringbuffer::{BufferReader, BufferWriter};
use vibraudio_thread::Priority;

#[cfg(target_arch = "wasm32")]
use std::sync::atomic::AtomicUsize;

#[cfg(target_arch = "wasm32")]
static MIC_ERRORS: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "wasm32")]
static PLAY_ERRORS: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "wasm32")]
static MIC_OVERFLOWS: AtomicUsize = AtomicUsize::new(0);

/// Number of `read_frames_async` failures since start. Exposed to JS.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn mic_errors() -> usize {
    MIC_ERRORS.load(Ordering::Relaxed)
}

/// Number of `write_frames_async` failures since start. Exposed to JS.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn play_errors() -> usize {
    PLAY_ERRORS.load(Ordering::Relaxed)
}

/// Number of times the mic ring buffer was full (MicSource not draining).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn mic_overflows() -> usize {
    MIC_OVERFLOWS.load(Ordering::Relaxed)
}

#[cfg(not(target_arch = "wasm32"))]
use vibraudio_thread::thread::{self, JoinHandle};

#[cfg(target_arch = "wasm32")]
type TaskHandle = tokio_with_wasm::task::JoinHandle<()>;

pub const RING_SIZE: usize = crate::platform::FRAMES * 2;

pub struct Speakers<S: Sample> {
    config: AudioConfig,
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<JoinHandle<()>>,
    #[cfg(target_arch = "wasm32")]
    handle: Option<TaskHandle>,
    running: Arc<AtomicBool>,
    phantom: PhantomData<S>,
}

impl<S: Sample> Speakers<S> {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
            phantom: PhantomData,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run<const SIZE: usize>(
        &mut self,
        reader: BufferReader<SIZE, S>,
        priority: Priority,
        #[cfg(target_os = "windows")] mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let mut backend = DefaultBackend::<S>::open("default", StreamDirection::Playback)?;
        backend.configure(&self.config)?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let _handle = vibraudio_thread::configure_audio_thread(
                priority,
                #[cfg(target_os = "windows")]
                mcss_value,
            );

            let mut buffer = [S::ZERO; crate::platform::FRAMES * 2];
            let max_samples = crate::platform::FRAMES * channels as usize;

            while running.load(Ordering::SeqCst) {
                let read = reader.read(&mut buffer[..max_samples]);

                if read > 0 {
                    if let Err(e) = backend.write_frames(&buffer[..read], channels) {
                        tracing::error!("❌ Playback error: {e}");
                    }
                }

                #[cfg(target_os = "windows")]
                thread::sleep(Duration::from_micros(10));
            }
        });

        self.handle = Some(handle);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 1:1 with `run`, async methods only.
    #[cfg(target_arch = "wasm32")]
    pub async fn run_async<const SIZE: usize>(
        &mut self,
        reader: BufferReader<SIZE, S>,
        _priority: Priority,
        #[cfg(target_os = "windows")] _mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        use vibraudio_core::BackendAsync;

        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let mut backend =
            DefaultBackend::<S>::open_async("default", StreamDirection::Playback).await?;
        backend.configure_async(&self.config).await?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = tokio_with_wasm::spawn(async move {
            let mut buffer = [S::ZERO; crate::platform::FRAMES * 2];
            let max_samples = crate::platform::FRAMES * channels.as_usize();

            while running.load(Ordering::SeqCst) {
                let read = reader.read(&mut buffer[..max_samples]);

                if read > 0 {
                    // write_frames_async blocks internally (hybrid wait)
                    // until PLAYBACK_BUF accepts all frames — like
                    // snd_pcm_writei on native. No outer yield needed.
                    if backend
                        .write_frames_async(&buffer[..read], channels)
                        .await
                        .is_err()
                    {
                        // Counter instead of tracing: no allocation, so
                        // no memory.grow, so the worklet view in worklet.js
                        // never gets detached by a log line.
                        PLAY_ERRORS.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    // ring(speakers) is empty — SpeakersSink hasn't written
                    // yet. Sleep instead of yield so we don't busy-yield
                    // ~200 times over the 20 ms tick.
                    tokio_with_wasm::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            }
        });

        self.handle = Some(handle);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            #[cfg(not(target_arch = "wasm32"))]
            let _ = handle.join();
            #[cfg(target_arch = "wasm32")]
            handle.abort();
        }
    }
}

pub struct Mic<S: Sample> {
    config: AudioConfig,
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<JoinHandle<()>>,
    #[cfg(target_arch = "wasm32")]
    handle: Option<TaskHandle>,
    running: Arc<AtomicBool>,
    phantom: PhantomData<S>,
}

impl<S: Sample> Mic<S> {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            handle: None,
            #[cfg(target_arch = "wasm32")]
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
            phantom: PhantomData,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run<const SIZE: usize>(
        &mut self,
        writer: BufferWriter<SIZE, S>,
        priority: Priority,
        #[cfg(target_os = "windows")] mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let mut backend = DefaultBackend::<S>::open("default", StreamDirection::Capture)?;
        backend.configure(&self.config)?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let _handle = vibraudio_thread::configure_audio_thread(
                priority,
                #[cfg(target_os = "windows")]
                mcss_value,
            );

            let mut buffer = [S::ZERO; crate::platform::FRAMES * 2];

            while running.load(Ordering::SeqCst) {
                match backend.read_frames(&mut buffer, channels) {
                    Ok(frames_read) => {
                        if frames_read > 0 {
                            let samples = frames_read * channels as usize;
                            let written = writer.write(&buffer[..samples]);
                            if written < samples {
                                thread::sleep(Duration::from_micros(10));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ Capture error: {e}");
                    }
                }

                #[cfg(target_os = "windows")]
                thread::sleep(Duration::from_micros(10));
            }
        });

        self.handle = Some(handle);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 1:1 with `run`, async methods only.
    #[cfg(target_arch = "wasm32")]
    pub async fn run_async<const SIZE: usize>(
        &mut self,
        writer: BufferWriter<SIZE, S>,
        _priority: Priority,
        #[cfg(target_os = "windows")] _mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        use vibraudio_core::BackendAsync;

        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let mut backend =
            DefaultBackend::<S>::open_async("default", StreamDirection::Capture).await?;
        backend.configure_async(&self.config).await?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = tokio_with_wasm::spawn(async move {
            let mut buffer = [S::ZERO; crate::platform::FRAMES * 2];

            while running.load(Ordering::SeqCst) {
                // read_frames_async blocks internally (hybrid wait) until
                // CAPTURE_BUF has exactly buffer.len()/ch frames — like
                // snd_pcm_readi on native.
                match backend.read_frames_async(&mut buffer, channels).await {
                    Ok(frames_read) => {
                        if frames_read > 0 {
                            let samples = frames_read * channels as usize;
                            let written = writer.write(&buffer[..samples]);
                            if written < samples {
                                // ring(mic) is full — MicSource not draining.
                                // Count instead of tracing; yield once so
                                // other tasks get a tick, then continue.
                                MIC_OVERFLOWS.fetch_add(1, Ordering::Relaxed);
                                vibraudio_web::yield_fast().await;
                            }
                        }
                        // No trailing yield_fast here: the next loop
                        // iteration calls read_frames_async which yields
                        // internally if CAPTURE_BUF is empty.
                    }
                    Err(_e) => {
                        // Counter instead of tracing: no allocation.
                        MIC_ERRORS.fetch_add(1, Ordering::Relaxed);
                        // Small sleep so a persistent error doesn't spin.
                        tokio_with_wasm::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                }
            }
        });

        self.handle = Some(handle);
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            #[cfg(not(target_arch = "wasm32"))]
            let _ = handle.join();
            #[cfg(target_arch = "wasm32")]
            handle.abort();
        }
    }
}
