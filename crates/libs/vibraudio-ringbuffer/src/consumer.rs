use crate::BufferReader;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use vibraudio_core::{
    sample::Sample, stream::StreamConfig, AudioConfig, Backend, PcmDevice, StreamDirection,
};

pub struct Consumer<const N: usize> {
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl<const N: usize> Consumer<N> {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn start<const SIZE: usize, B: Backend<S>, S: Sample>(
        &mut self,
        device_name: &str,
        config: StreamConfig,
        mut reader: BufferReader<N, S>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let device = B::open(device_name, StreamDirection::Playback)?;
        let audio_config = AudioConfig::new(config.sample_rate, config.channels, 15_000);
        device.configure(&audio_config)?;

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();

        let handle = thread::spawn(move || {
            let mut device = device;
            let mut output_buffer = [S::default(); SIZE];

            while running.load(Ordering::SeqCst) {
                while reader.available_data() < output_buffer.len()
                    && running.load(Ordering::SeqCst)
                {
                    thread::sleep(Duration::from_micros(100));
                }

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                let samples = reader.read(&mut output_buffer);

                if samples > 0 {
                    if let Err(e) = device.write_frames(&output_buffer[..samples], config.channels)
                    {
                        eprintln!("Consumer write error: {}", e);
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
