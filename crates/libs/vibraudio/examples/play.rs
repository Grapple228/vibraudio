// PCM Audio Primer:
// - Sample: A single amplitude value at a point in time (e.g., an i16 from -32768 to 32767)
// - Frame: One sample per channel. Stereo frame = [Left, Right] = 2 samples.
// - Sample Rate: Frames per second. 44100 Hz means 44100 frames/sec.
// - Interleaved: Samples alternate channels: [L0, R0, L1, R1, L2, R2, ...]
// - Period: A chunk of frames transferred to hardware at once (the ring buffer unit).
// - Underrun (XRUN): When the application fails to provide data fast enough.

use std::time::Instant;

use vibraudio::backend::DefaultBackend;
use vibraudio_core::{AudioConfig, Error, PcmDevice, SampleFormat, StreamDirection};
use vibraudio_mp3::{decoder::Mp3Decoder, ffi::MINIMP3_MAX_SAMPLES_PER_FRAME};

fn main() {
    vibraudio_core::init();

    // Require the user to pass an MP3 file path as an argument
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: play <path-to-mp3>");
        std::process::exit(1);
    }

    // The one heap allocation: reading the entire MP3 file into memory
    let mp3_data = std::fs::read(&args[1]).expect("Failed to read MP3 file");

    // Open the default ALSA playback device
    let device = PcmDevice::<DefaultBackend>::open("default", StreamDirection::Playback)
        .expect("Failed to open audio device");

    let mut decoder = Mp3Decoder::new();

    // Stack-allocated PCM buffer - no heap involved in the decode loop
    let mut pcm_buffer = [0i16; MINIMP3_MAX_SAMPLES_PER_FRAME];
    let mut offset: usize = 0;
    let mut configured = false;

    while offset < mp3_data.len() {
        let input = &mp3_data[offset..];

        match decoder.decode_frame(input, &mut pcm_buffer) {
            Ok(result) => {
                // Configure the device on the first successful decode
                if !configured {
                    let config = AudioConfig::new(
                        result.sample_rate,
                        result.channels,
                        SampleFormat::S16Le,
                        20_000,
                    );

                    device
                        .configure(&config)
                        .expect("Failed to configure device");
                    configured = true;
                }

                // Write only the decoded samples to the device
                let total_samples = result.samples * result.channels as usize;

                device
                    .write_frames(&pcm_buffer[..total_samples], result.channels)
                    .expect("Failed to write audio");

                offset += result.frame_bytes;
            }
            Err(Error::EndOfInput) => break,
            Err(Error::DecodeFailed) => {
                offset += 1;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    println!("Playback complete.");
}
