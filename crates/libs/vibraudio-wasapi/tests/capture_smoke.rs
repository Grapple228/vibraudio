// crates/libs/vibraudio-wasapi/tests/capture_smoke.rs

use vibraudio_core::{AudioConfig, PcmDevice, StreamDirection};
use vibraudio_wasapi::WasapiBackend;

#[test]
fn capture_default_device() {
    // Пытаемся открыть устройство захвата по умолчанию
    let device =
        match PcmDevice::<WasapiBackend<i16>, i16>::open("default", StreamDirection::Capture) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: no capture device available");
                return;
            }
        };

    let config = AudioConfig::new(44100, 1, 30_000);

    // Конфигурация может не поддерживаться на некоторых устройствах
    if device.configure(&config).is_err() {
        eprintln!("Skipping: unsupported configuration");
        return;
    }

    // Read frames into a stack buffer
    let mut buffer = [0i16; 1024];
    match device.read_frames(&mut buffer, config.channels) {
        Ok(frames) => {
            // Должны получить какое-то количество фреймов
            assert!(
                frames <= 1024,
                "Received more frames than buffer capacity: {} > 1024",
                frames
            );
        }
        Err(_) => {
            // Может не быть данных сразу после открытия
            eprintln!("Skipping: no data available yet");
        }
    }
}

#[test]
fn playback_default_device() {
    let device =
        match PcmDevice::<WasapiBackend<i16>, i16>::open("default", StreamDirection::Playback) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: no playback device available");
                return;
            }
        };

    let config = AudioConfig::new(44100, 2, 30_000);

    if device.configure(&config).is_err() {
        eprintln!("Skipping: unsupported configuration");
        return;
    }

    // Write silence
    let silence = vec![0i16; 1024 * 2]; // 1024 stereo frames
    match device.write_frames(&silence, config.channels) {
        Ok(frames) => {
            assert!(frames > 0, "Should write at least some frames");
        }
        Err(_) => {
            eprintln!("Skipping: write failed");
        }
    }
}

#[test]
fn loopback_basic() {
    // Тест базового лупбека (mic -> speakers)
    let capture =
        match PcmDevice::<WasapiBackend<i16>, i16>::open("default", StreamDirection::Capture) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: capture device not available");
                return;
            }
        };

    let playback =
        match PcmDevice::<WasapiBackend<i16>, i16>::open("default", StreamDirection::Playback) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: playback device not available");
                return;
            }
        };

    let config = AudioConfig::new(44100, 2, 30_000);

    if capture.configure(&config).is_err() || playback.configure(&config).is_err() {
        eprintln!("Skipping: unsupported configuration");
        return;
    }

    let mut buffer = [0i16; 2048]; // 1024 stereo frames

    // Пробуем прочитать и записать несколько раз
    for _ in 0..3 {
        match capture.read_frames(&mut buffer, config.channels) {
            Ok(frames_read) => {
                if frames_read > 0 {
                    let samples = frames_read * config.channels as usize;
                    match playback.write_frames(&buffer[..samples], config.channels) {
                        Ok(_) => {} // Успешно
                        Err(_) => break,
                    }
                }
            }
            Err(_) => break,
        }
    }
}

#[test]
fn multiple_configurations() {
    // Тестируем разные конфигурации
    let device =
        match PcmDevice::<WasapiBackend<i16>, i16>::open("default", StreamDirection::Playback) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: playback device not available");
                return;
            }
        };

    let configs = [
        AudioConfig::new(44100, 1, 30_000), // Mono CD quality
        AudioConfig::new(44100, 2, 30_000), // Stereo CD quality
        AudioConfig::new(48000, 1, 20_000), // Mono DVD quality
        AudioConfig::new(48000, 2, 20_000), // Stereo DVD quality
    ];

    for config in &configs {
        if device.configure(config).is_ok() {
            // Если сконфигурировалось - пробуем записать тишину
            let silence = [0i16; 1024];
            let _ = device.write_frames(&silence, config.channels);

            // Сбрасываем для следующей конфигурации
            let _ = device.reset();
        }
    }
}

#[test]
fn device_lifecycle() {
    // Тест жизненного цикла устройства
    {
        let device = match PcmDevice::<WasapiBackend<i16>, i16>::open(
            "default",
            StreamDirection::Playback,
        ) {
            Ok(d) => d,
            Err(_) => {
                eprintln!("Skipping: device not available");
                return;
            }
        };

        let config = AudioConfig::new(44100, 2, 30_000);
        if device.configure(&config).is_err() {
            return;
        }

        // Несколько циклов записи-сброса
        for _ in 0..3 {
            let silence = [0i16; 512];
            let _ = device.write_frames(&silence, config.channels);
        }

        let _ = device.reset();

        // Еще запись после сброса
        let silence = [0i16; 512];
        let _ = device.write_frames(&silence, config.channels);

        // Устройство закроется при dropped
    }
}
