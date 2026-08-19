use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use vibraudio_core::sample::Sample;

pub struct RingBufferInner<const N: usize, S: Sample> {
    pub(crate) data: UnsafeCell<[S; N]>,
    pub(crate) write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

unsafe impl<const N: usize, S: Sample> Send for RingBufferInner<N, S> {}
unsafe impl<const N: usize, S: Sample> Sync for RingBufferInner<N, S> {}

impl<const N: usize, S: Sample> RingBufferInner<N, S> {
    pub fn new() -> Self {
        Self {
            data: UnsafeCell::new([S::ZERO; N]),
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    pub fn available_data(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        if write >= read {
            write - read
        } else {
            N - (read - write)
        }
    }

    pub fn available_space(&self) -> usize {
        N - self.available_data() - 1
    }

    pub unsafe fn read_data(&self, dst: &mut [S]) -> usize {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        let used = if write >= read {
            write - read
        } else {
            N - (read - write)
        };
        let to_read = dst.len().min(used);

        if to_read == 0 {
            return 0;
        }

        let data_ptr = self.data.get() as *mut S;
        let data_slice = std::slice::from_raw_parts_mut(data_ptr, N);

        let end = read + to_read;
        if end <= N {
            dst[..to_read].copy_from_slice(&data_slice[read..end]);
        } else {
            let first_part = N - read;
            dst[..first_part].copy_from_slice(&data_slice[read..N]);
            dst[first_part..to_read].copy_from_slice(&data_slice[..end - N]);
        }

        let new_read = if end == N { 0 } else { end % N };
        self.read_pos.store(new_read, Ordering::Release);

        to_read
    }

    pub unsafe fn write_data(&self, src: &[S]) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);

        let used = if write >= read {
            write - read
        } else {
            N - (read - write)
        };
        let free = N - used - 1;
        let to_write = src.len().min(free);

        if to_write == 0 {
            return 0;
        }

        let data_ptr = self.data.get() as *mut S;
        let data_slice = std::slice::from_raw_parts_mut(data_ptr, N);

        let end = write + to_write;
        if end <= N {
            data_slice[write..end].copy_from_slice(&src[..to_write]);
        } else {
            let first_part = N - write;
            data_slice[write..N].copy_from_slice(&src[..first_part]);
            data_slice[..end - N].copy_from_slice(&src[first_part..to_write]);
        }

        let new_write = if end == N { 0 } else { end % N };
        self.write_pos.store(new_write, Ordering::Release);

        to_write
    }
}
