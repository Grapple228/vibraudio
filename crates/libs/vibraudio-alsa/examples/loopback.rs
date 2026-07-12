use vibraudio_alsa::AlsaBackend;
use vibraudio_core::{AudioConfig, PcmDevice, SampleFormat, StreamDirection};

fn main() {
    // Open the default ALSA device for capture (microphone)
    let capture = PcmDevice::<AlsaBackend>::open("default", StreamDirection::Capture)
        .expect("Failed to open capture device");

    // Open the default ALSA device for playback (speakers)
    let playback = PcmDevice::<AlsaBackend>::open("default", StreamDirection::Playback)
        .expect("Failed to open playback device");

    // Both devices share the same config: 48kHz, mono, signed 16-bit LE
    let config = AudioConfig::new(48000, 1, SampleFormat::S16Le, 15_000);
    capture
        .configure(&config)
        .expect("Failed to configure capture");
    playback
        .configure(&config)
        .expect("Failed to configure playback");

    // Stack-allocated buffer holds 1024 mono frames per iteration
    let mut buffer = [0i16; 1024];

    println!("Loopback active. Press Ctrl+C to stop.");

    loop {
        match capture.read_frames(&mut buffer, config.channels) {
            Ok(frames_read) => {
                if frames_read > 0 {
                    let samples = frames_read * config.channels as usize;
                    playback
                        .write_frames(&buffer[..samples], config.channels)
                        .expect("Write failed");
                }
            }
            Err(e) => {
                eprintln!("Capture error: {}", e);
                break;
            }
        }
    }
}
