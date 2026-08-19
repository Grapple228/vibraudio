use vibraudio_core::sample::Sample;

use crate::{inner::RingBufferInner, BufferReader};
use std::sync::{atomic::Ordering, Arc};

pub struct BufferWriter<const N: usize, S: Sample> {
    pub(crate) inner: Arc<RingBufferInner<N, S>>,
}

impl<const N: usize, S: Sample> BufferWriter<N, S> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RingBufferInner::new()),
        }
    }

    pub fn reader(&self) -> BufferReader<N, S> {
        BufferReader {
            inner: self.inner.clone(),
        }
    }

    pub fn write(&self, src: &[S]) -> usize {
        unsafe { self.inner.write_data(src) }
    }

    pub fn available_space(&self) -> usize {
        self.inner.available_space()
    }

    pub fn reserve(&mut self, size: usize) -> Option<&mut [S]> {
        if self.available_space() < size {
            return None;
        }
        let write = self.inner.write_pos.load(Ordering::Acquire);
        unsafe {
            let data_ptr = self.inner.data.get() as *mut S;
            let data_slice = std::slice::from_raw_parts_mut(data_ptr, N);
            Some(&mut data_slice[write..write + size])
        }
    }

    pub fn commit(&mut self, size: usize) {
        let write = self.inner.write_pos.load(Ordering::Acquire);
        let new_write = (write + size) % N;
        self.inner.write_pos.store(new_write, Ordering::Release);
    }
}
