use std::ffi::{c_int, CStr, CString};
use std::marker::PhantomData;
use std::ptr;

use crate::backend::Backend;
use crate::sample::Sample;
use crate::{AudioConfig, Error, Result};

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

impl<B: Backend<S>, S: Sample> PcmDevice<B, S> {
    pub fn open(device_name: &str, direction: StreamDirection) -> Result<Self> {
        let backend = B::open(device_name, direction)?;
        Ok(PcmDevice {
            backend,
            _phantom: PhantomData,
        })
    }

    pub fn configure(&self, config: &AudioConfig) -> Result<()> {
        self.backend.configure(config)
    }

    pub fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize> {
        self.backend.write_frames(buffer, channels)
    }

    pub fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize> {
        self.backend.read_frames(buffer, channels)
    }

    fn reset(&self) -> Result<()> {
        self.backend.reset()
    }

    fn close(&self) -> Result<()> {
        self.backend.close()
    }
}
