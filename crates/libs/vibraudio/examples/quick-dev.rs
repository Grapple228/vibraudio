use vibraudio::core::AudioConfig;
use vibraudio_thread::{MmcssValue, Priority};

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
    speakers.run(reader, Priority::Critical, MmcssValue::Audio)?;

    let mut mic = Mic::new(config)?;
    mic.run(writer, Priority::Critical, MmcssValue::Audio)?;

    println!("Press Enter to stop...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    speakers.stop();
    mic.stop();

    Ok(())
}
