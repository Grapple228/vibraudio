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

    /// Возвращает доступные данные как два слайса (для wrap-around)
    ///
    /// # Safety
    /// Нельзя вызывать одновременно с write_data или as_slices_mut
    pub unsafe fn as_slices(&self) -> (&[S], &[S]) {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        if read == write {
            return (&[], &[]);
        }

        let data_ptr = self.data.get() as *const S;
        let data_slice = std::slice::from_raw_parts(data_ptr, N);

        if write > read {
            (&data_slice[read..write], &[])
        } else {
            (&data_slice[read..N], &data_slice[..write])
        }
    }

    /// Возвращает свободное место как два слайса (для wrap-around)
    ///
    /// # Safety
    /// Нельзя вызывать одновременно с read_data или as_slices
    pub unsafe fn as_slices_mut(&self) -> (&mut [S], &mut [S]) {
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        let used = if write >= read {
            write - read
        } else {
            N - (read - write)
        };
        let free = N - used - 1;

        if free == 0 {
            return (&mut [], &mut []);
        }

        let data_ptr = self.data.get() as *mut S;
        let data_slice = std::slice::from_raw_parts_mut(data_ptr, N);

        if write >= read {
            // Свободное место: [write..N) + [0..read)
            let (before_write, after_write) = data_slice.split_at_mut(write);
            let to_end = after_write.len().min(free);
            let remaining = free - to_end;

            (&mut after_write[..to_end], &mut before_write[..remaining])
        } else {
            // write < read: свободное место [write..read)
            let (_, after_write) = data_slice.split_at_mut(write);
            let len = (read - write).min(free);
            (&mut after_write[..len], &mut [])
        }
    }

    /// Продвигает указатель чтения
    ///
    /// # Safety
    /// Нельзя продвигать больше чем available_data
    pub unsafe fn advance_read(&self, count: usize) {
        let read = self.read_pos.load(Ordering::Acquire);
        let new_read = (read + count) % N;
        self.read_pos.store(new_read, Ordering::Release);
    }

    /// Продвигает указатель записи
    ///
    /// # Safety
    /// Нельзя продвигать больше чем available_space
    pub unsafe fn advance_write(&self, count: usize) {
        let write = self.write_pos.load(Ordering::Acquire);
        let new_write = (write + count) % N;
        self.write_pos.store(new_write, Ordering::Release);
    }

    /// Читает данные в dst
    pub unsafe fn read_data(&self, dst: &mut [S]) -> usize {
        let (first, second) = self.as_slices();
        let to_read = dst.len().min(first.len() + second.len());

        if to_read == 0 {
            return 0;
        }

        let first_len = first.len().min(to_read);
        dst[..first_len].copy_from_slice(&first[..first_len]);

        let remaining = to_read - first_len;
        if remaining > 0 {
            dst[first_len..to_read].copy_from_slice(&second[..remaining]);
        }

        self.advance_read(to_read);
        to_read
    }

    /// Читает данные и добавляет их к dst
    pub unsafe fn read_add_data(&self, dst: &mut [S], volume: S) -> usize {
        let (first, second) = self.as_slices();
        let to_read = dst.len().min(first.len() + second.len());

        if to_read == 0 {
            return 0;
        }

        let first_len = first.len().min(to_read);
        for i in 0..first_len {
            dst[i] = dst[i].add(first[i].mul(volume));
        }

        let remaining = to_read - first_len;
        if remaining > 0 {
            for i in 0..remaining {
                dst[first_len + i] = dst[first_len + i].add(second[i].mul(volume));
            }
        }

        self.advance_read(to_read);
        to_read
    }

    /// Записывает данные из src
    pub unsafe fn write_data(&self, src: &[S]) -> usize {
        let (first, second) = self.as_slices_mut();

        let mut written = 0;

        // Пишем в первый слайс
        let first_len = first.len().min(src.len());
        first[..first_len].copy_from_slice(&src[..first_len]);
        written += first_len;

        // Пишем во второй слайс (если остались данные)
        if written < src.len() && !second.is_empty() {
            let second_len = second.len().min(src.len() - written);
            second[..second_len].copy_from_slice(&src[written..written + second_len]);
            written += second_len;
        }

        self.advance_write(written);
        written
    }
}
