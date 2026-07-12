use core::ffi::c_int;

// Maximum number of PCM samples a single MP3 frame can decode to
pub const MINIMP3_MAX_SAMPLES_PER_FRAME: usize = 2304;

// Decoder state struct: must match the C mp3dec_t layout exactly
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Mp3Dec {
    pub mdct_overlap: [[f32; 288]; 2],
    pub qmf_state: [f32; 960],
    pub reserv: c_int,
    pub free_format_bytes: c_int,
    pub header: [u8; 4],
    pub reserv_buf: [u8; 511],
}

// Info returned after decoding a frame
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Mp3FrameInfo {
    pub frame_bytes: c_int,
    pub frame_offset: c_int,
    pub channels: c_int,
    pub hz: c_int,
    pub layer: c_int,
    pub bitrate_kbps: c_int,
}

unsafe extern "C" {
    // Initialize the decoder state to zero
    pub fn mp3dec_init(dec: *mut Mp3Dec);

    // Decode one MP3 frame from input into the caller's PCM buffer
    pub fn mp3dec_decode_frame(
        dec: *mut Mp3Dec,
        mp3: *const u8,
        mp3_bytes: c_int,
        pcm: *mut i16,
        info: *mut Mp3FrameInfo,
    ) -> c_int;
}
