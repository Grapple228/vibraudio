use crate::inner::RingBufferInner;
use std::sync::Arc;
use vibraudio_core::sample::Sample;

pub struct BufferReader<const N: usize, S: Sample> {
    pub(crate) inner: Arc<RingBufferInner<N, S>>,
}

impl<const N: usize, S: Sample> BufferReader<N, S> {
    pub fn read(&self, dst: &mut [S]) -> usize {
        unsafe { self.inner.read_data(dst) }
    }

    pub fn available_data(&self) -> usize {
        self.inner.available_data()
    }
}
