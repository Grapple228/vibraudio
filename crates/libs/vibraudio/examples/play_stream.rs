// ./crates/libs/vibraudio/examples/play_stream.rs
use std::{fs::File, io::BufReader, time::Instant};
use vibraudio::{
    backend::DefaultBackend,
    core::{AudioConfig, PcmDevice, SampleFormat, StreamDirection},
    mp3::decoder::Mp3StreamDecoder,
};
use vibraudio_core::Error;
use vibraudio_mp3::ffi::MINIMP3_MAX_SAMPLES_PER_FRAME;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: play_stream <path-to-mp3>");
        std::process::exit(1);
    }

    let file = File::open(&args[1])?;
    let reader = BufReader::new(file);
    let mut decoder = Mp3StreamDecoder::new(reader);

    let device = PcmDevice::<DefaultBackend>::open("default", StreamDirection::Playback)?;

    let mut pcm_buffer = [0i16; MINIMP3_MAX_SAMPLES_PER_FRAME];
    let mut configured = false;
    let mut total_frames = 0;
    let start = Instant::now();

    println!("Playing: {} (streaming, zero alloc)", args[1]);

    loop {
        match decoder.decode_next_frame(&mut pcm_buffer) {
            Ok(info) => {
                if !configured {
                    let config = AudioConfig::new(
                        info.sample_rate,
                        info.channels,
                        SampleFormat::S16Le,
                        20_000,
                    );
                    device.configure(&config)?;
                    configured = true;
                    println!(
                        "Format: {} Hz, {} channels",
                        info.sample_rate, info.channels
                    );
                }

                let samples = info.samples * info.channels as usize;
                device.write_frames(&pcm_buffer[..samples], info.channels)?;
                total_frames += 1;
            }
            Err(Error::EndOfInput) => break,
            Err(Error::DecodeFailed) => continue,
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    let elapsed = start.elapsed();
    println!("Playback complete.");
    println!("Total frames: {}", total_frames);
    println!("Duration: {:.2}s", elapsed.as_secs_f64());

    Ok(())
}
