use crate::Channels;

#[derive(Clone, Copy)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub channels: Channels,
}
