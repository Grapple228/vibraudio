use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use vibraudio_core::{sample::Sample, Backend};

use crate::BufferWriter;

pub struct Producer<const N: usize> {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl<const N: usize> Producer<N> {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn start<const SIZE: usize, F, S: Sample>(
        &mut self,
        mut writer: BufferWriter<N, S>,
        mut callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&mut [S]) -> vibraudio_core::Result<usize> + Send + 'static,
    {
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let mut pcm_buffer = [S::default(); SIZE];

            while running.load(Ordering::SeqCst) {
                match callback(&mut pcm_buffer) {
                    Ok(samples) => {
                        if samples > 0 {
                            while writer.available_space() < samples
                                && running.load(Ordering::SeqCst)
                            {
                                thread::sleep(Duration::from_micros(100));
                            }

                            if !running.load(Ordering::SeqCst) {
                                break;
                            }

                            writer.write(&pcm_buffer[..samples]);
                        } else {
                            thread::sleep(Duration::from_micros(100));
                        }
                    }
                    Err(e) => {
                        eprintln!("Producer error: {}", e);
                        break;
                    }
                }
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
