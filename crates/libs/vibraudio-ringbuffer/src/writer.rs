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

    /// Возвращает свободное место как слайсы (без копирования)
    pub fn as_slices_mut(&self) -> (&mut [S], &mut [S]) {
        unsafe { self.inner.as_slices_mut() }
    }

    /// Продвигает указатель записи
    pub fn advance(&self, count: usize) {
        unsafe { self.inner.advance_write(count) }
    }

    pub fn available_space(&self) -> usize {
        self.inner.available_space()
    }

    /// Резервирует место для записи (с обработкой wrap-around)
    pub fn reserve(&self, size: usize) -> Option<(&mut [S], &mut [S])> {
        if self.available_space() < size {
            return None;
        }

        unsafe {
            let (first, second) = self.inner.as_slices_mut();

            if first.len() >= size {
                // Всё помещается в первый слайс
                Some((&mut first[..size], &mut []))
            } else {
                // Нужно использовать оба слайса
                let remaining = size - first.len();
                Some((first, &mut second[..remaining]))
            }
        }
    }

    pub fn commit(&mut self, size: usize) {
        let write = self.inner.write_pos.load(Ordering::Acquire);
        let new_write = (write + size) % N;
        self.inner.write_pos.store(new_write, Ordering::Release);
    }
}
