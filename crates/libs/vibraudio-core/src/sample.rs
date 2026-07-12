pub trait Sample: Copy + Send + Sync + 'static {
    fn from_f32(value: f32) -> Self;
    fn to_f32(&self) -> f32;
    fn silence() -> Self;
}

impl Sample for i16 {
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
