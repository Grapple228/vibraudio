use std::time::Instant;
use vibraudio_alsa::AlsaBackend;
use vibraudio_core::{AudioConfig, PcmDevice, SampleFormat, StreamDirection};

fn main() {
    // Open the ALSA "null" device which discards all written audio
    let device = PcmDevice::<AlsaBackend>::open("null", StreamDirection::Playback)
        .expect("Failed to open null device");

    // Configure for standard CD-quality stereo playback
    let config = AudioConfig::new(44100, 2, SampleFormat::S16Le, 20_000);
    device
        .configure(&config)
        .expect("Failed to configure device");

    // A stereo buffer of 1024 frames (2048 i16 samples)
    let buffer = [0i16; 2048];
    let iterations = 1000;

    // Warm up: let the kernel path stabilize
    for _ in 0..10 {
        let _ = device.write_frames(&buffer, 2);
    }

    // Timed run: measure raw write_frames throughput
    let start = Instant::now();
    for _ in 0..iterations {
        device.write_frames(&buffer, 2).expect("Write failed");
    }
    let elapsed = start.elapsed();

    let per_call = elapsed / iterations;
    println!(
        "write_frames x {}: {:?} total, {:?} per call",
        iterations, elapsed, per_call
    );
}
