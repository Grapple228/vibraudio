use crate::SampleFormat;

/// Трейт для аудио семплов
pub trait Sample: Copy + Send + Sync + 'static {
    const ZERO: Self;

    fn from_f32(value: f32) -> Self;
    fn to_f32(&self) -> f32;
    fn silence() -> Self;

    /// Проверяет, является ли семпл тишиной
    fn is_silence(&self) -> bool {
        self.to_f32() == 0.0
    }

    fn sample_format() -> SampleFormat {
        SampleFormat::S16Le
    }
}

impl Sample for i16 {
    const ZERO: Self = 0_i16;

    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * 32767.0) as i16
    }
    fn to_f32(&self) -> f32 {
        *self as f32 / 32767.0
    }
    fn silence() -> Self {
        0
    }
}

impl Sample for f32 {
    const ZERO: Self = 0.0_f32;

    fn from_f32(value: f32) -> Self {
        value.clamp(-1.0, 1.0)
    }
    fn to_f32(&self) -> f32 {
        *self
    }
    fn silence() -> Self {
        0.0
    }
}
