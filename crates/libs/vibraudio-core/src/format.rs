// crates/libs/vibraudio-core/src/format.rs

use crate::AudioConfig;

/// Информация о поддерживаемом аудиоформате
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFormatInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub format_tag: u16,
}

impl AudioFormatInfo {
    pub fn new(channels: u16, sample_rate: u32, bits_per_sample: u16, format_tag: u16) -> Self {
        Self {
            channels,
            sample_rate,
            bits_per_sample,
            format_tag,
        }
    }

    /// Создать из AudioConfig
    pub fn from_config(config: &AudioConfig, bits_per_sample: u16) -> Self {
        Self {
            channels: config.channels,
            sample_rate: config.sample_rate,
            bits_per_sample,
            format_tag: 1, // PCM по умолчанию
        }
    }

    /// Преобразовать в AudioConfig
    pub fn to_config(&self, latency_us: u32) -> AudioConfig {
        AudioConfig::new(self.sample_rate, self.channels, latency_us)
    }

    /// Получить размер одного семпла в байтах
    pub fn bytes_per_sample(&self) -> usize {
        (self.bits_per_sample / 8) as usize
    }

    /// Получить размер одного фрейма в байтах
    pub fn bytes_per_frame(&self) -> usize {
        self.channels as usize * self.bytes_per_sample()
    }
}

impl std::fmt::Display for AudioFormatInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format_name = match self.format_tag {
            1 => "PCM",
            0xFFFE => "WAVE_FORMAT_EXTENSIBLE",
            3 => "IEEE_FLOAT",
            _ => "Unknown",
        };
        write!(
            f,
            "{}ch, {}Hz, {}bit, {}",
            self.channels, self.sample_rate, self.bits_per_sample, format_name
        )
    }
}

/// Параметры для выбора формата
#[derive(Debug, Clone)]
pub struct FormatSelection {
    /// Предпочитаемый формат (если None - используется системный по умолчанию)
    pub preferred: Option<AudioFormatInfo>,
    /// Задержка в микросекундах
    pub latency_us: u32,
    /// Автоматически выбрать ближайший поддерживаемый формат
    pub auto_select: bool,
}

impl Default for FormatSelection {
    fn default() -> Self {
        Self {
            preferred: None,
            latency_us: 30_000,
            auto_select: true,
        }
    }
}

impl FormatSelection {
    pub fn new(format: AudioFormatInfo) -> Self {
        Self {
            preferred: Some(format),
            latency_us: 30_000,
            auto_select: false,
        }
    }

    pub fn with_latency(mut self, latency_us: u32) -> Self {
        self.latency_us = latency_us;
        self
    }

    pub fn with_auto_select(mut self) -> Self {
        self.auto_select = true;
        self
    }
}
