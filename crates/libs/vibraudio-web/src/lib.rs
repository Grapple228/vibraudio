mod backend;

use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;

pub use backend::FRAMES;
pub use backend::WebBackend;

const BUF_SIZE: usize = 4096;

thread_local! {
    static CAPTURE_BUF: RefCell<[f32; BUF_SIZE]> = RefCell::new([0.0; BUF_SIZE]);
    static CAPTURE_LEN: Cell<usize> = const { Cell::new(0) };

    static PLAYBACK_BUF: RefCell<[f32; BUF_SIZE]> = RefCell::new([0.0; BUF_SIZE]);
    static PLAYBACK_LEN: Cell<usize> = const { Cell::new(0) };
}

#[wasm_bindgen]
pub fn process_block(input: &[f32], output: &mut [f32]) {
    // 1. capture: input -> CAPTURE_BUF (кольцевой, но для простоты — до заполнения)
    CAPTURE_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        let len = CAPTURE_LEN.get();
        let space = BUF_SIZE - len;
        let n = input.len().min(space);
        buf[len..len + n].copy_from_slice(&input[..n]);
        CAPTURE_LEN.set(len + n);
    });

    // 2. playback: PLAYBACK_BUF -> output
    PLAYBACK_BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        let len = PLAYBACK_LEN.get();
        let n = output.len().min(len);
        output[..n].copy_from_slice(&buf[..n]);
        // сдвигаем остаток влево
        buf.copy_within(n..len, 0);
        PLAYBACK_LEN.set(len - n);
        // если output больше, чем было данных — остаток нулями
        for sample in &mut output[n..] {
            *sample = 0.0;
        }
    });
}
