use std::ffi::{c_int, CStr, CString};
use std::ptr;

use crate::backend::Backend;
use crate::{AudioConfig, Error, Result};

#[derive(Debug, Clone, Copy)]
pub enum StreamDirection {
    Playback,
    Capture,
}

pub struct PcmDevice<B: Backend> {
    backend: B,
}

impl<B: Backend> PcmDevice<B> {
    pub fn open(device_name: &str, direction: StreamDirection) -> Result<Self> {
        let backend = B::open(device_name, direction)?;
        Ok(PcmDevice { backend })
    }

    pub fn configure(&self, config: &AudioConfig) -> Result<()> {
        self.backend.configure(config)
    }

    pub fn write_frames(&self, buffer: &[i16], channels: u16) -> Result<usize> {
        self.backend.write_frames(buffer, channels)
    }

    pub fn read_frames(&self, buffer: &mut [i16], channels: u16) -> Result<usize> {
        self.backend.read_frames(buffer, channels)
    }
}
