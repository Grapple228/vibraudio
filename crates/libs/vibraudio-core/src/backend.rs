use crate::{
    format::{AudioFormatInfo, FormatSelection},
    sample::Sample,
    AudioConfig, Result, StreamDirection,
};

pub trait Backend<S: Sample>: Sized + Send + Sync + 'static {
    const FRAMES: usize;

    fn open(name: &str, direction: StreamDirection) -> Result<Self>;
    fn configure(&self, config: &AudioConfig) -> Result<()>;
    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        // По умолчанию возвращаем пустой список
        Ok(Vec::new())
    }

    fn configure_with_format(&self, format: &AudioFormatInfo, latency_us: u32) -> Result<()> {
        // По умолчанию используем старый метод
        let config = format.to_config(latency_us);
        self.configure(&config)
    }

    fn configure_smart(&self, selection: &FormatSelection) -> Result<AudioFormatInfo> {
        if let Some(preferred) = &selection.preferred {
            if !selection.auto_select {
                // Пробуем exact формат
                match self.configure_with_format(preferred, selection.latency_us) {
                    Ok(_) => return Ok(preferred.clone()),
                    Err(_) => {
                        if !selection.auto_select {
                            return Err(crate::Error::InvalidConfig);
                        }
                    }
                }
            }
        }

        // Авто-выбор: получаем список форматов и выбираем лучший
        let formats = self.supported_formats()?;

        if formats.is_empty() {
            // Если список форматов не поддерживается, пробуем стандартные
            let config = AudioConfig::new(44100, 2, selection.latency_us);
            self.configure(&config)?;
            return Ok(AudioFormatInfo::from_config(&config, 16));
        }

        // Ищем предпочитаемый формат или ближайший к нему
        let selected = if let Some(preferred) = &selection.preferred {
            find_best_match(&formats, preferred)
        } else {
            // Берем первый доступный (обычно системный формат)
            formats[0].clone()
        };

        self.configure_with_format(&selected, selection.latency_us)?;
        Ok(selected)
    }

    fn enumerate_formats(name: &str, direction: StreamDirection) -> Result<Vec<AudioFormatInfo>> {
        let device = Self::open(name, direction)?;
        device.supported_formats()
    }

    fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize>;
    fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize>;
    fn reset(&self) -> Result<()>;
    fn close(&self) -> Result<()>;
}

fn find_best_match(available: &[AudioFormatInfo], preferred: &AudioFormatInfo) -> AudioFormatInfo {
    // Сначала ищем точное совпадение
    for format in available {
        if format.channels == preferred.channels
            && format.sample_rate == preferred.sample_rate
            && format.bits_per_sample == preferred.bits_per_sample
        {
            return format.clone();
        }
    }

    // Ищем с той же частотой и каналами
    let mut best: Option<&AudioFormatInfo> = None;
    let mut best_score = i32::MAX;

    for format in available {
        let rate_diff = (format.sample_rate as i32 - preferred.sample_rate as i32).abs();
        let ch_diff = (format.channels as i32 - preferred.channels as i32).abs();
        let bits_diff = (format.bits_per_sample as i32 - preferred.bits_per_sample as i32).abs();

        // Приоритет: точное совпадение каналов и частоты
        let score = if format.channels == preferred.channels
            && format.sample_rate == preferred.sample_rate
        {
            bits_diff
        } else {
            rate_diff * 100 + ch_diff * 1000 + bits_diff
        };

        if score < best_score {
            best_score = score;
            best = Some(format);
        }
    }

    best.cloned().unwrap_or_else(|| available[0].clone())
}
