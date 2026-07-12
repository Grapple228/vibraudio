// ./crates/libs/vibraudio/examples/play_mp3_callback.rs
use std::{
    fs::File,
    io::BufReader,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use vibraudio::{
    backend::DefaultBackend,
    core::Error,
    core::Result,
    core::{AudioConfig, SampleFormat},
};
use vibraudio_core::{callback::AudioCallback, PcmDevice, StreamDirection};
use vibraudio_mp3::callback::Mp3Callback;

#[derive(Debug, Clone, Copy)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: u32,
}

impl StreamConfig {
    pub fn new(sample_rate: u32, channels: u16, buffer_frames: u32) -> Self {
        Self {
            sample_rate,
            channels,
            buffer_frames,
        }
    }
}

/// Информация о текущем блоке аудио
#[derive(Debug, Clone, Copy)]
pub struct OutputCallbackInfo {
    pub timestamp: Instant,
    pub frames_written: u64,
    pub underrun_count: u64,
}

pub struct AudioStream<T: Sample> {
    device: PcmDevice<DefaultBackend>,
    config: StreamConfig,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
    // Статистика
    frames_written: Arc<std::sync::atomic::AtomicU64>,
    underrun_count: Arc<std::sync::atomic::AtomicU64>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Sample> AudioStream<T> {
    /// Создает новый выходной поток (как build_output_stream в CPAL)
    pub fn build_output_stream<F, E>(
        device_name: &str,
        config: StreamConfig,
        mut data_callback: F,
        mut error_callback: E,
    ) -> Result<Self>
    where
        F: FnMut(&mut [T], &OutputCallbackInfo) -> Result<()> + Send + 'static,
        E: FnMut(Error) + Send + 'static,
    {
        // Открываем устройство
        let device = PcmDevice::<DefaultBackend>::open(device_name, StreamDirection::Playback)?;

        // Настраиваем устройство
        let audio_config = AudioConfig::new(
            config.sample_rate,
            config.channels,
            if std::mem::size_of::<T>() == 4 {
                SampleFormat::FloatLe
            } else {
                SampleFormat::S16Le
            },
            config.buffer_frames * 1000 / config.sample_rate * 1000,
        );
        device.configure(&audio_config)?;

        let is_playing = Arc::new(AtomicBool::new(false));
        let is_paused = Arc::new(AtomicBool::new(false));
        let frames_written = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let underrun_count = Arc::new(std::sync::atomic::AtomicU64::new(0));

        // Клонируем для потока
        let is_playing_clone = is_playing.clone();
        let is_paused_clone = is_paused.clone();
        let frames_written_clone = frames_written.clone();
        let underrun_count_clone = underrun_count.clone();

        // Создаем поток (но пока не запускаем)
        let handle = thread::Builder::new()
            .name("audio-stream".to_string())
            .spawn(move || {
                let mut buffer = [T::silence(); 4096];
                let mut device = device;
                let mut info = OutputCallbackInfo {
                    timestamp: Instant::now(),
                    frames_written: 0,
                    underrun_count: 0,
                };

                while is_playing_clone.load(Ordering::SeqCst) {
                    // Проверяем паузу
                    if is_paused_clone.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }

                    // Вызываем callback
                    if let Err(e) = data_callback(&mut buffer, &info) {
                        error_callback(e);
                        break;
                    }

                    // Записываем в устройство
                    let frames = buffer.len() / config.channels as usize;
                    match device.write_frames(&buffer, config.channels) {
                        Ok(written) => {
                            if written < frames {
                                let underrun = underrun_count_clone.fetch_add(1, Ordering::SeqCst);
                                info.underrun_count = underrun + 1;
                            }
                            let total =
                                frames_written_clone.fetch_add(written as u64, Ordering::SeqCst);
                            info.frames_written = total + written as u64;
                            info.timestamp = Instant::now();
                        }
                        Err(e) => {
                            error_callback(e);
                            break;
                        }
                    }
                }
            })
            .unwrap();

        Ok(Self {
            device: PcmDevice::<DefaultBackend>::open(device_name, StreamDirection::Playback)?,
            config,
            is_playing,
            is_paused,
            thread_handle: Some(handle),
            frames_written,
            underrun_count,
            _phantom: PhantomData::default(),
        })
    }

    /// Запускает воспроизведение
    pub fn play(&mut self) -> Result<()> {
        self.is_playing.store(true, Ordering::SeqCst);
        self.is_paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Приостанавливает воспроизведение
    pub fn pause(&mut self) {
        self.is_paused.store(true, Ordering::SeqCst);
    }

    /// Останавливает воспроизведение
    pub fn stop(&mut self) {
        self.is_playing.store(false, Ordering::SeqCst);
        self.is_paused.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Возвращает количество записанных фреймов
    pub fn frames_written(&self) -> u64 {
        self.frames_written.load(Ordering::SeqCst)
    }

    /// Возвращает количество underrun'ов
    pub fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::SeqCst)
    }
}

impl<T: Sample> Drop for AudioStream<T> {
    fn drop(&mut self) {
        self.stop();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: play_mp3_callback <path-to-mp3>");
        std::process::exit(1);
    }

    // 1. Создаем MP3 колбэк
    let file = File::open(&args[1])?;
    let reader = BufReader::new(file);
    let mut mp3 = Mp3Callback::new(reader);

    // 2. Конфигурация аудио
    let config = AudioConfig::new(
        44100, // sample rate
        2,     // channels (стерео)
        SampleFormat::S16Le,
        20_000, // latency в микросекундах
    );

    // 3. Создаем плеер с замыканием
    let mut player = Player::new(
        move |output, channels, sample_rate| {
            // MP3 колбэк заполняет буфер
            mp3.on_audio_required(output, channels, sample_rate)
        },
        config,
    );

    // 4. Открываем устройство
    player.open("default")?;

    println!("🎵 Playing: {}", args[1]);
    println!("Controls:");
    println!("  Enter - stop");
    println!("  p - pause");
    println!("  r - resume");

    // 5. Запускаем воспроизведение
    player.play_with_callback()?;

    // 6. Управление в реальном времени
    loop {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        match input.trim() {
            "p" => {
                // TODO: добавить метод pause в Player
                // player.pause();
                println!("⏸️ Pause - not implemented yet");
            }
            "r" => {
                // TODO: добавить метод resume в Player
                // player.resume();
                println!("▶️ Resume - not implemented yet");
            }
            "" | "s" | "q" => {
                player.stop();
                println!("⏹️ Stopped");
                break;
            }
            _ => {
                println!("Unknown command. Press Enter to stop.");
            }
        }
    }

    Ok(())
}
