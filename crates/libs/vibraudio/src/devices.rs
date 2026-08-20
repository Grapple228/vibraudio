use crate::platform::DefaultBackend;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::{thread, time::Duration};
use vibraudio_core::AudioConfig;
use vibraudio_core::Backend;
use vibraudio_core::StreamDirection;
use vibraudio_core::{Error, Result};
use vibraudio_ringbuffer::{BufferReader, BufferWriter};

pub struct Speakers {
    config: AudioConfig,
    handle: Option<JoinHandle<()>>,

    running: Arc<AtomicBool>,
}

pub const RING_SIZE: usize = DefaultBackend::<i16>::FRAMES * 2 * 4;

impl Speakers {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&mut self, reader: BufferReader<RING_SIZE, i16>) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let backend = DefaultBackend::<i16>::open("default", StreamDirection::Playback)?;
        backend.configure(&self.config)?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let mut buffer = [0i16; DefaultBackend::<i16>::FRAMES * 2];

            while running.load(Ordering::SeqCst) {
                let samples =
                    reader.read(&mut buffer[..DefaultBackend::<i16>::FRAMES * channels as usize]);

                if samples > 0 {
                    if let Err(e) = backend.write_frames(&buffer[..samples], channels) {
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

pub struct Mic {
    config: AudioConfig,
    handle: Option<JoinHandle<()>>,

    running: Arc<AtomicBool>,
}

impl Mic {
    pub fn new(config: AudioConfig) -> Result<Self> {
        Ok(Self {
            handle: None,
            config,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&mut self, writer: BufferWriter<RING_SIZE, i16>) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let backend = DefaultBackend::<i16>::open("default", StreamDirection::Capture)?;
        backend.configure(&self.config)?;

        let channels = self.config.channels;
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let mut buffer = [0i16; DefaultBackend::<i16>::FRAMES * 2];

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

pub fn tst() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = AudioConfig {
        sample_rate: 48000,
        channels: 2,
        latency: 10_000,
    };

    let (writer, reader) = vibraudio_ringbuffer::create_pair::<RING_SIZE, i16>();

    let mut speakers = Speakers::new(config)?;
    speakers.run(reader)?;

    let mut mic = Mic::new(config)?;
    mic.run(writer)?;

    println!("Press Enter to stop...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    speakers.stop();
    mic.stop();

    Ok(())
}
