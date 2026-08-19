use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use vibraudio::{
    core::{AudioConfig, StreamDirection},
    platform::DefaultBackend,
};
use vibraudio_core::{stream::StreamConfig, Backend};
use vibraudio_ringbuffer::create_pair;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 Loopback: Микрофон → Кольцевой буфер → Колонки");
    println!("==============================================================");

    const FRAMES: usize = 256;
    const CHANNELS: usize = 2;
    const SAMPLES: usize = FRAMES * CHANNELS;
    const RING_SIZE: usize = SAMPLES * 64;

    let config = StreamConfig {
        sample_rate: 48000,
        channels: CHANNELS as u16,
    };

    let playback = DefaultBackend::<i16>::open("default", StreamDirection::Playback)?;
    let capture = DefaultBackend::<i16>::open("default", StreamDirection::Capture)?;

    let audio_config = AudioConfig::new(config.sample_rate, config.channels, 5_000);

    playback.configure(&audio_config)?;
    capture.configure(&audio_config)?;

    let (mut writer, reader) = create_pair::<{ RING_SIZE }, i16>();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // PRODUCER
    let producer = thread::spawn(move || {
        let mut buffer = [0i16; SAMPLES];
        let mut total = 0u64;
        let mut last = Instant::now();
        let mut empty_reads = 0u32;

        while running_clone.load(Ordering::SeqCst) {
            match capture.read_frames(&mut buffer, config.channels) {
                Ok(frames_read) => {
                    if frames_read > 0 {
                        let samples = frames_read * config.channels as usize;
                        total += samples as u64;
                        empty_reads = 0;

                        if last.elapsed() >= Duration::from_secs(1) {
                            println!("📊 PRODUCER: {} samples/sec", total);
                            total = 0;
                            last = Instant::now();
                        }

                        // ✅ ЖДЕМ МЕСТО
                        while writer.available_space() < samples
                            && running_clone.load(Ordering::SeqCst)
                        {
                            thread::sleep(Duration::from_micros(10));
                        }

                        if !running_clone.load(Ordering::SeqCst) {
                            break;
                        }

                        // ✅ ПИШЕМ ТОЛЬКО ТО, ЧТО ПРОЧИТАЛИ!
                        if let Some(slice) = writer.reserve(samples) {
                            slice.copy_from_slice(&buffer[..samples]);
                            writer.commit(samples);
                        }
                    } else {
                        empty_reads += 1;
                        if empty_reads < 10 {
                            thread::sleep(Duration::from_micros(100));
                        }
                    }
                }
                Err(_) => {}
            }
        }
    });

    // CONSUMER
    let running_clone2 = running.clone();
    let consumer = thread::spawn(move || {
        let mut buffer = [0i16; SAMPLES];
        let mut total = 0u64;
        let mut last = Instant::now();
        let mut underruns = 0u32;

        while running_clone2.load(Ordering::SeqCst) {
            // ✅ ЧИТАЕМ ИЗ БУФЕРА
            let samples = reader.read(&mut buffer);

            if samples > 0 {
                total += samples as u64;

                if last.elapsed() >= Duration::from_secs(1) {
                    println!(
                        "📊 CONSUMER: {} samples/sec, underruns={}",
                        total, underruns
                    );
                    total = 0;
                    underruns = 0;
                    last = Instant::now();
                }

                // ✅ ПИШЕМ ТОЛЬКО ТО, ЧТО ПРОЧИТАЛИ!
                if let Err(e) = playback.write_frames(&buffer[..samples], config.channels) {
                    eprintln!("Playback error: {}", e);
                }
            } else {
                underruns += 1;
                if underruns % 100 == 0 {
                    // Не спамим
                }
                thread::sleep(Duration::from_micros(10));
            }
        }
    });

    println!("Press Enter to stop...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    running.store(false, Ordering::SeqCst);
    producer.join().unwrap();
    consumer.join().unwrap();

    println!("✅ Done!");
    Ok(())
}
