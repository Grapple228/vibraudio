use crate::platform::DefaultBackend;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::{thread, time::Duration};
use vibraudio_core::sample::Sample;
use vibraudio_core::AudioConfig;
use vibraudio_core::Backend;
use vibraudio_core::StreamDirection;
use vibraudio_core::{Error, Result};
use vibraudio_ringbuffer::{BufferReader, BufferWriter};
use vibraudio_thread::Priority;

pub struct Speakers<S: Sample> {
    config: AudioConfig,
    handle: Option<JoinHandle<()>>,

    running: Arc<AtomicBool>,
    phantom: PhantomData<S>,
}

pub const RING_SIZE: usize = crate::platform::FRAMES * 2;

impl<S: Sample> Speakers<S> {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
            phantom: PhantomData,
        })
    }

    pub fn run<const SIZE: usize>(
        &mut self,
        reader: BufferReader<SIZE, S>,
        priority: Priority,
        #[cfg(target_os = "windows")] mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let backend = DefaultBackend::<S>::open("default", StreamDirection::Playback)?;
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
                        eprintln!("❌ Playback error: {}", e);
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

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub struct Mic<S: Sample> {
    config: AudioConfig,
    handle: Option<JoinHandle<()>>,

    running: Arc<AtomicBool>,
    phantom: PhantomData<S>,
}

impl<S: Sample> Mic<S> {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
            phantom: PhantomData,
        })
    }

    pub fn run<const SIZE: usize>(
        &mut self,
        writer: BufferWriter<SIZE, S>,
        priority: Priority,
        #[cfg(target_os = "windows")] mcss_value: vibraudio_thread::MmcssValue,
    ) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let backend = DefaultBackend::<S>::open("default", StreamDirection::Capture)?;
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
                        eprintln!("❌ Capture error: {}", e);
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

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
