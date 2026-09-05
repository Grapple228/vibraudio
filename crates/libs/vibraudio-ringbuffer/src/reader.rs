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

    pub fn read_add(&self, dst: &mut [S], volume: S) -> usize {
        unsafe { self.inner.read_add_data(dst, volume) }
    }

    /// Возвращает данные как слайсы (без копирования)
    pub fn as_slices(&self) -> (&[S], &[S]) {
        unsafe { self.inner.as_slices() }
    }

    /// Продвигает указатель чтения
    pub fn advance(&self, count: usize) {
        unsafe { self.inner.advance_read(count) }
    }

    pub fn available_data(&self) -> usize {
        self.inner.available_data()
    }
}
