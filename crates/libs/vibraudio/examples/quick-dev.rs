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
use vibraudio_ringbuffer::create_pair;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 Loopback: Микрофон → Буфер → Колонки");
    println!("====================================================");

    const FRAMES: usize = 512;
    const CHANNELS: usize = 2;
    const SAMPLES: usize = FRAMES * CHANNELS;
    const RING_SIZE: usize = SAMPLES * 4;

    let config = StreamConfig {
        sample_rate: 48000,
        channels: CHANNELS as u16,
    };

    let playback = DefaultBackend::<i16>::open("default", StreamDirection::Playback)?;
    let capture = DefaultBackend::<i16>::open("default", StreamDirection::Capture)?;

    let audio_config = AudioConfig::new(config.sample_rate, config.channels, 10_000);

    playback.configure(&audio_config)?;
    capture.configure(&audio_config)?;

    let (writer, reader) = create_pair::<{ RING_SIZE }, i16>();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // PRODUCER
    let producer_handle = thread::spawn(move || {
        let mut buffer = [0i16; SAMPLES];

        while running_clone.load(Ordering::SeqCst) {
            match capture.read_frames(&mut buffer, config.channels) {
                Ok(frames_read) => {
                    if frames_read > 0 {
                        let samples = frames_read * config.channels as usize;

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

    let running_clone2 = running.clone();
    let consumer_handle = thread::spawn(move || {
        let mut buffer = [0i16; SAMPLES];

        while running_clone2.load(Ordering::SeqCst) {
            let samples = reader.read(&mut buffer);

            if samples > 0 {
                if let Err(e) = playback.write_frames(&buffer[..samples], config.channels) {
                    eprintln!("❌ Playback error: {}", e);
                }
            }

            #[cfg(target_os = "windows")]
            thread::sleep(Duration::from_micros(10));
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
