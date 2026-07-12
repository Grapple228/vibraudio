use core::ffi::{c_char, c_int, c_uint, c_void};

use vibraudio_core::{SampleFormat, StreamDirection};

pub enum SndPcm {}

pub type SndPcmSframesT = i64;
pub type SndPcmUframesT = u64;

// Stream direction: playback sends audio out, capture records audio in
#[repr(C)]
pub enum SndPcmStreamT {
    Playback = 0,
    Capture = 1,
}

// Sample format: the encoding of each audio sample
#[repr(C)]
pub enum SndPcmFormatT {
    S16Le = 2,
    S16Be = 3,
    FloatLe = 14,
}

// Access mode: how samples are laid out in the buffer
#[repr(C)]
pub enum SndPcmAccessT {
    RwInterleaved = 3,
}

unsafe extern "C" {
    pub fn snd_pcm_open(
        pcm: *mut *mut SndPcm,
        name: *const c_char,
        stream: SndPcmStreamT,
        mode: c_int,
    ) -> c_int;

    pub fn snd_pcm_set_params(
        pcm: *mut SndPcm,
        format: SndPcmFormatT,
        access: SndPcmAccessT,
        channels: c_uint,
        rate: c_uint,
        soft_resample: c_int,
        latency: c_uint,
    ) -> c_int;

    pub fn snd_pcm_writei(
        pcm: *mut SndPcm,
        buffer: *const c_void,
        size: SndPcmUframesT,
    ) -> SndPcmSframesT;

    pub fn snd_pcm_readi(
        pcm: *mut SndPcm,
        buffer: *mut c_void,
        size: SndPcmUframesT,
    ) -> SndPcmSframesT;

    pub fn snd_pcm_drain(pcm: *mut SndPcm) -> c_int;

    pub fn snd_pcm_close(pcm: *mut SndPcm) -> c_int;

    pub fn snd_pcm_recover(pcm: *mut SndPcm, err: c_int, silent: c_int) -> c_int;

    pub fn snd_strerror(errnum: c_int) -> *const c_char;
}

impl SndPcmFormatT {
    pub(crate) const fn from_sample_format(format: SampleFormat) -> Self {
        match format {
            SampleFormat::S16Le => Self::S16Le,
            SampleFormat::S16Be => Self::S16Be,
            SampleFormat::FloatLe => Self::FloatLe,
        }
    }
}

impl SndPcmStreamT {
    pub const fn from_direction(direction: StreamDirection) -> Self {
        match direction {
            StreamDirection::Playback => Self::Playback,
            StreamDirection::Capture => Self::Capture,
        }
    }
}
