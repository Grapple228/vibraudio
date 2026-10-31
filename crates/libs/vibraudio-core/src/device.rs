use crate::backend::Backend;
use crate::sample::Sample;
use crate::{AudioConfig, Channels, Result};
use std::marker::PhantomData;

#[derive(Debug, Clone, Copy)]
pub enum StreamDirection {
    Playback,
    Capture,
}

pub struct PcmDevice<B: Backend<S>, S>
where
    S: Sample,
{
    backend: B,
    _phantom: PhantomData<S>,
}

#[allow(unused)]
impl<B: Backend<S>, S: Sample> PcmDevice<B, S> {
    pub fn open(device_name: &str, direction: StreamDirection) -> Result<Self> {
        let backend = B::open(device_name, direction)?;
        Ok(PcmDevice {
            backend,
            _phantom: PhantomData,
        })
    }

    #[inline(always)]
    pub fn configure(&mut self, config: &AudioConfig) -> Result<()> {
        self.backend.configure(config)
    }

    #[inline(always)]
    pub fn write_frames(&mut self, buffer: &[S], channels: Channels) -> Result<usize> {
        self.backend.write_frames(buffer, channels)
    }

    #[inline(always)]
    pub fn read_frames(&self, buffer: &mut [S], channels: Channels) -> Result<usize> {
        self.backend.read_frames(buffer, channels)
    }

    #[inline(always)]
    pub fn reset(&mut self) -> Result<()> {
        self.backend.reset()
    }

    #[inline(always)]
    pub fn close(&mut self) -> Result<()> {
        self.backend.close()
    }
}
