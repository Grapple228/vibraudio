use crate::{AudioConfig, Error, Result, StreamDirection};
use std::ffi::CString;

pub trait Backend: Sized {
    fn open(name: &str, direction: StreamDirection) -> Result<Self>;
    fn configure(&self, config: &AudioConfig) -> Result<()>;
    fn write_frames(&self, buffer: &[i16], channels: u16) -> Result<usize>;
    fn read_frames(&self, buffer: &mut [i16], channels: u16) -> Result<usize>;
}
