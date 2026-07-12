// ./crates/libs/vibraudio/examples/cpal_like_mp3.rs
//! Producer-Consumer с Lock-Free кольцевым буфером

use std::{
    cell::UnsafeCell,
    fs::File,
    io::BufReader,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use vibraudio::{
    core::{AudioConfig, PcmDevice, StreamDirection},
    mp3::decoder::Mp3StreamDecoder,
};
use vibraudio_alsa::AlsaBackend;
use vibraudio_core::{sample::Sample, stream::StreamConfig, Error};
use vibraudio_mp3::ffi::MINIMP3_MAX_SAMPLES_PER_FRAME;
use vibraudio_ringbuffer::{consumer::Consumer, producer::Producer, BufferWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: mp3_ring <path-to-mp3>");
        std::process::exit(1);
    }

    println!("🎵 Producer-Consumer MP3 Player");
    println!("===============================");

    let file = File::open(&args[1])?;
    let reader = BufReader::new(file);
    let mut decoder = Mp3StreamDecoder::new(reader);
    let mut configured = false;

    let config = StreamConfig {
        sample_rate: 44100,
        channels: 2,
    };

    let mut writer = BufferWriter::<{ 4096 }, i16>::new();
    let reader = writer.reader();

    let mut producer = Producer::new();
    producer.start::<2304, _, i16>(writer, move |data: &mut [i16]| {
        match decoder.decode_next_frame(data) {
            Ok(info) => {
                if !configured {
                    println!(
                        "✅ Format: {} Hz, {} channels",
                        info.sample_rate, info.channels
                    );
                    configured = true;
                }
                let samples = info.samples * info.channels as usize;
                Ok(samples.min(data.len()))
            }
            Err(Error::EndOfInput) => Ok(0),
            Err(Error::DecodeFailed) => Ok(0),
            Err(e) => Err(e),
        }
    })?;

    let mut consumer = Consumer::new();
    consumer.start::<2304, AlsaBackend<i16>, i16>("default", config, reader)?;

    println!("▶️ Playing: {}", args[1]);
    println!("Press Enter to stop...");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    producer.stop();
    consumer.stop();

    println!("✅ Done!");

    Ok(())
}
