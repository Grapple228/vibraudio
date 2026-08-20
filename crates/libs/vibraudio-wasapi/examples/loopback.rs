fn main() -> Result<(), Box<dyn std::error::Error>> {
    use vibraudio_core::{AudioConfig, Backend, StreamDirection};
    use vibraudio_wasapi::WasapiBackend;

    println!("🎤 WASAPI Loopback: Microphone -> Speakers");
    println!("==========================================");

    let capture = WasapiBackend::<i16>::open("default", StreamDirection::Capture)?;
    let playback = WasapiBackend::<i16>::open("default", StreamDirection::Playback)?;

    let config = AudioConfig::new(48000, 2, 15_000);
    playback.configure(&config).expect("playback config");
    capture.configure(&config).expect("capture config");

    let mut buffer = [0i16; 2048];

    println!("Recording... Press Enter to stop.");

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();

    // Handle Ctrl+C
    ctrlc::set_handler(move || {
        running_clone.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        match capture.read_frames(&mut buffer, config.channels) {
            Ok(frames_read) => {
                if frames_read > 0 {
                    let samples = frames_read * config.channels as usize;
                    playback.write_frames(&buffer[..samples], config.channels)?;
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    println!("Stopped.");
    Ok(())
}
