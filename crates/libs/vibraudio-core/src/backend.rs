use crate::{sample::Sample, AudioConfig, Error, Result, StreamDirection};
use std::ffi::CString;

pub trait Backend<S: Sample>: Sized + Send + Sync + 'static {
    fn open(name: &str, direction: StreamDirection) -> Result<Self>;
    fn configure(&self, config: &AudioConfig) -> Result<()>;
    fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize>;
    fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize>;
    fn reset(&self) -> Result<()>;
    fn close(&self) -> Result<()>;
}
