pub type SampleType = i16;

#[cfg(target_arch = "wasm32")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    panic!("Shoud be native platform")
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use vibraudio::core::AudioConfig;
    use vibraudio::devices::{Mic, Speakers};
    use vibraudio_core::Channels;
    use vibraudio_thread::Priority;

    println!("🎤 Loopback: Микрофон → Буфер → Колонки");
    println!("====================================================");

    let config = AudioConfig {
        sample_rate: 48000,
        channels: Channels::Stereo,
        latency: 15_000,
    };

    let (writer, reader) =
        vibraudio_ringbuffer::create_pair::<{ vibraudio::devices::RING_SIZE }, SampleType>();

    let mut speakers = Speakers::new(config)?;
    speakers.run(
        reader,
        Priority::Critical,
        #[cfg(target_os = "windows")]
        vibraudio_thread::MmcssValue::Audio,
    )?;

    let mut mic = Mic::new(config)?;
    mic.run(
        writer,
        Priority::Critical,
        #[cfg(target_os = "windows")]
        vibraudio_thread::MmcssValue::Audio,
    )?;

    println!("Press Enter to stop...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    speakers.stop();
    mic.stop();

    Ok(())
}
