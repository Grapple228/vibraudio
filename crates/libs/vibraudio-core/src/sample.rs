use crate::SampleFormat;

/// Трейт для аудио семплов
pub trait Sample: Copy + Send + PartialEq + 'static {
    const ZERO: Self;

    fn from_f32(value: f32) -> Self;
    fn to_f32(&self) -> f32;
    fn silence() -> Self;

    /// Проверяет, является ли семпл тишиной
    fn is_silence(&self) -> bool {
        Self::ZERO == *self
    }

    fn sample_format() -> SampleFormat {
        SampleFormat::S16Le
    }

    fn add(&self, other: Self) -> Self;
    fn mul(&self, other: Self) -> Self;
}

impl Sample for i16 {
    const ZERO: Self = 0_i16;

    #[inline(always)]
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * 32767.0) as i16
    }

    #[inline(always)]
    fn to_f32(&self) -> f32 {
        *self as f32 / 32767.0
    }

    #[inline(always)]
    fn silence() -> Self {
        0
    }

    #[inline(always)]
    fn add(&self, other: Self) -> Self {
        *self + other
    }

    #[inline(always)]
    fn mul(&self, other: Self) -> Self {
        *self * other
    }
}

impl Sample for f32 {
    const ZERO: Self = 0.0_f32;

    #[inline(always)]
    fn from_f32(value: f32) -> Self {
        value.clamp(-1.0, 1.0)
    }

    #[inline(always)]
    fn to_f32(&self) -> f32 {
        *self
    }

    #[inline(always)]
    fn silence() -> Self {
        0.0
    }

    #[inline(always)]
    fn add(&self, other: Self) -> Self {
        *self + other
    }

    #[inline(always)]
    fn mul(&self, other: Self) -> Self {
        *self * other
    }
}
