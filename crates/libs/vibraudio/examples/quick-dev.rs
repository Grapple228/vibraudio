// ./crates/libs/vibraudio/examples/microphone_loopback_ring.rs
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
    platform::DefaultBackend,
};
use vibraudio_core::{stream::StreamConfig, Backend};
use vibraudio_ringbuffer::BufferWriter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 Loopback: Микрофон → Кольцевой буфер → Колонки");
    println!("==============================================================");

    const BUFFER_SIZE: usize = 128;

    let config = StreamConfig {
        sample_rate: 48000,
        channels: 1,
    };

    let capture = DefaultBackend::<i16>::open("default", StreamDirection::Capture)?;
    let playback = DefaultBackend::<i16>::open("default", StreamDirection::Playback)?;

    let audio_config = AudioConfig::new(config.sample_rate, config.channels, 15_000);
    capture.configure(&audio_config)?;
    playback.configure(&audio_config)?;

    let mut writer = BufferWriter::<{ BUFFER_SIZE * 2 }, i16>::new();
    let reader = writer.reader();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // PRODUCER: читаем с микрофона
    let producer_handle = thread::spawn(move || {
        while running_clone.load(Ordering::SeqCst) {
            while writer.available_space() < BUFFER_SIZE && running_clone.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_micros(100));
            }

            if !running_clone.load(Ordering::SeqCst) {
                break;
            }

            if let Some(slice) = writer.reserve(BUFFER_SIZE) {
                match capture.read_frames(slice, config.channels) {
                    Ok(frames_read) => {
                        if frames_read > 0 {
                            let samples = frames_read * config.channels as usize;
                            writer.commit(samples);
                        }
                    }
                    Err(e) => {
                        eprintln!("Capture error: {}", e);
                        break;
                    }
                }
            }
        }
    });

    // CONSUMER: читаем из буфера и пишем в колонки
    let running_clone2 = running.clone();
    let consumer_handle = thread::spawn(move || {
        let mut buffer = [0i16; BUFFER_SIZE];

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
