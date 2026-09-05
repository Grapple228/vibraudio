//! Loopback с микрофона на колонки через кольцевой буфер

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use vibraudio::{
    core::{AudioConfig, StreamDirection},
    devices::RING_SIZE,
    platform::DefaultBackend,
};
use vibraudio_core::{sample::Sample, stream::StreamConfig, Backend};
use vibraudio_ringbuffer::BufferWriter;
use vibraudio_thread::Priority;

pub type SampleType = f32;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 Loopback: Микрофон → Кольцевой буфер → Колонки");
    println!("=================================================");

    let config = StreamConfig {
        sample_rate: 48000,
        channels: 2,
    };

    let capture = DefaultBackend::<SampleType>::open("default", StreamDirection::Capture)?;
    let playback = DefaultBackend::<SampleType>::open("default", StreamDirection::Playback)?;

    let audio_config = AudioConfig::new(config.sample_rate, config.channels, 15_000);
    capture.configure(&audio_config)?;
    playback.configure(&audio_config)?;

    let writer = BufferWriter::<RING_SIZE, SampleType>::new();
    let reader = writer.reader();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // PRODUCER: читаем с микрофона
    let producer_handle = thread::spawn(move || {
        let _handle = vibraudio_thread::configure_audio_thread(
            Priority::Critical,
            #[cfg(target_os = "windows")]
            vibraudio_thread::MmcssValue::Audio,
        );

        let mut buffer = [SampleType::ZERO; vibraudio::platform::FRAMES * 2];

        while running_clone.load(Ordering::SeqCst) {
            match capture.read_frames(&mut buffer, config.channels) {
                Ok(frames_read) => {
                    if frames_read > 0 {
                        let samples = frames_read * config.channels as usize;
                        writer.write(&buffer[..samples]);
                    }
                }
                Err(e) => {
                    eprintln!("Capture error: {}", e);
                    break;
                }
            }
        }
    });

    // CONSUMER: читаем из буфера и пишем в колонки
    let running_clone2 = running.clone();
    let consumer_handle = thread::spawn(move || {
        let _handle = vibraudio_thread::configure_audio_thread(
            Priority::Critical,
            #[cfg(target_os = "windows")]
            vibraudio_thread::MmcssValue::Audio,
        );

        let mut buffer = [SampleType::ZERO; vibraudio::platform::FRAMES * 2];

        while running_clone2.load(Ordering::SeqCst) {
            let samples = reader.read(&mut buffer);

            if samples > 0 {
                if let Err(e) = playback.write_frames(&buffer[..samples], config.channels) {
                    eprintln!("Playback error: {}", e);
                    break;
                }
            } else {
                thread::sleep(Duration::from_micros(100));
            }
        }
    });

    println!("Press Enter to stop...");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    running.store(false, Ordering::SeqCst);
    producer_handle.join().unwrap();
    consumer_handle.join().unwrap();

    println!("✅ Done!");

    Ok(())
}
