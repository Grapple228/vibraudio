use crate::ffi::{self, Mp3Dec, Mp3FrameInfo, MINIMP3_MAX_SAMPLES_PER_FRAME};
use core::mem::MaybeUninit;
use std::io::Read;
use vibraudio_core::{Error, Result};

pub const MP3_CHUNK_SIZE: usize = 8192;

#[derive(Debug, Clone, Copy)]
pub struct DecodeResult {
    pub samples: usize,
    pub channels: u16,
    pub sample_rate: u32,
    pub frame_bytes: usize,
}

pub struct Mp3Decoder {
    dec: Mp3Dec,
}

impl Mp3Decoder {
    pub fn new() -> Self {
        let mut dec = unsafe { MaybeUninit::<Mp3Dec>::zeroed().assume_init() };
        unsafe { ffi::mp3dec_init(&mut dec) };
        Mp3Decoder { dec }
    }

    pub fn decode_frame(&mut self, input: &[u8], pcm_buffer: &mut [i16]) -> Result<DecodeResult> {
        let mut info = unsafe { MaybeUninit::<Mp3FrameInfo>::zeroed().assume_init() };

        let samples = unsafe {
            ffi::mp3dec_decode_frame(
                &mut self.dec,
                input.as_ptr(),
                input.len() as i32,
                pcm_buffer.as_mut_ptr(),
                &mut info,
            )
        };

        if samples == 0 && info.frame_bytes == 0 {
            return Err(Error::EndOfInput);
        }

        if samples == 0 && info.frame_bytes > 0 {
            return Err(Error::DecodeFailed);
        }

        Ok(DecodeResult {
            samples: samples as usize,
            channels: info.channels as u16,
            sample_rate: info.hz as u32,
            frame_bytes: info.frame_bytes as usize,
        })
    }
}

pub struct Mp3StreamDecoder<R: Read> {
    reader: R,
    decoder: Mp3Decoder,
    buffer: [u8; MP3_CHUNK_SIZE * 8],
    buffer_len: usize,
    pos: usize,
}

impl<R: Read> Mp3StreamDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: Mp3Decoder::new(),
            buffer: [0u8; MP3_CHUNK_SIZE * 8],
            buffer_len: 0,
            pos: 0,
        }
    }

    pub fn decode_next_frame(&mut self, output: &mut [i16]) -> Result<DecodeResult> {
        loop {
            let available = self.buffer_len - self.pos;

            // if not enough data - read more
            if available < 4096 {
                // move if pos > 0 or have leftover
                if self.pos > 0 && self.buffer_len > self.pos {
                    let remaining = self.buffer_len - self.pos;
                    if remaining > 0 {
                        self.buffer.copy_within(self.pos..self.buffer_len, 0);
                    }
                    self.buffer_len = remaining;
                    self.pos = 0;
                } else if self.pos > 0 {
                    self.buffer_len = 0;
                    self.pos = 0;
                }

                // read until buffer is filled
                while self.buffer_len < self.buffer.len() {
                    let start = self.buffer_len;
                    let chunk_size = (self.buffer.len() - self.buffer_len).min(MP3_CHUNK_SIZE);
                    let chunk = &mut self.buffer[start..start + chunk_size];

                    match self.reader.read(chunk) {
                        Ok(0) => {
                            if self.buffer_len == 0 {
                                return Err(Error::EndOfInput);
                            }
                            break;
                        }
                        Ok(n) => {
                            self.buffer_len += n;
                        }
                        Err(e) => return Err(Error::Io(e)),
                    }
                }
            }

            // trying to decode
            let input = &self.buffer[self.pos..self.buffer_len];
            match self.decoder.decode_frame(input, output) {
                Ok(info) => {
                    self.pos += info.frame_bytes;
                    return Ok(info);
                }
                Err(Error::DecodeFailed) => {
                    self.pos += 1;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl<R: Read> From<R> for Mp3StreamDecoder<R> {
    fn from(reader: R) -> Self {
        Self::new(reader)
    }
}
