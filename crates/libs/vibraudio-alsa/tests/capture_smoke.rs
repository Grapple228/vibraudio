use vibraudio_alsa::AlsaBackend;
use vibraudio_core::{AudioConfig, PcmDevice, StreamDirection};

#[test]
fn capture_null_device() {
    // Open the null device for capture - discards/generates silence
    let device = match PcmDevice::<AlsaBackend<i16>, i16>::open("null", StreamDirection::Capture) {
        Ok(d) => d,
        Err(_) => {
            // null capture may not be available on all ALSA configs
            eprintln!("Skipping: null capture device not available");
            return;
        }
    };

    let config = AudioConfig::new(48000, 1, 20_000);
    device
        .configure(&config)
        .expect("Failed to configure null capture");

    // Read frames into a stack buffer
    let mut buffer = [0i16; 1024];
    let frames = device
        .read_frames(&mut buffer, config.channels)
        .expect("read_frames failed on null device");

    // The null device should return frames (possibly zeros)
    assert!(frames <= 1024, "Received more frames than buffer capacity");
}
