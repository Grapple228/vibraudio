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
    use vibraudio::devices::{Mic, Speakers, RING_SIZE};

    println!("🎤 Loopback: Микрофон → Буфер → Колонки");
    println!("====================================================");

    let config = AudioConfig {
        sample_rate: 48000,
        channels: 2,
        latency: 10_000,
    };

    let (writer, reader) = vibraudio_ringbuffer::create_pair::<{ RING_SIZE }, i16>();

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
