use std::ffi::{c_int, CStr};

use crate::ffi;

pub trait FromAlsa {
    fn from_alsa(code: c_int) -> vibraudio_core::Error;
}

impl FromAlsa for vibraudio_core::Error {
    fn from_alsa(code: c_int) -> vibraudio_core::Error {
        let message = unsafe {
            let ptr = ffi::snd_strerror(code);
            if ptr.is_null() {
                "Unknown ALSA error"
            } else {
                CStr::from_ptr(ptr).to_str().unwrap_or("Unknown ALSA error")
            }
        };

        vibraudio_core::Error::Ffi { code, message }
    }
}
