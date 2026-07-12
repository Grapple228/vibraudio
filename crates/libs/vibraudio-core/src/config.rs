#[derive(Debug, Clone, Copy)]
pub enum SampleFormat {
    S16Le,
    S16Be,
    FloatLe,
}

impl SampleFormat {
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::S16Le | SampleFormat::S16Be => 2,
            SampleFormat::FloatLe => 4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub latency: u32,
}

impl AudioConfig {
    pub const fn new(sample_rate: u32, channels: u16, latency: u32) -> Self {
        AudioConfig {
            sample_rate,
            channels,
            latency,
        }
    }
}
