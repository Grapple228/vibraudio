use vibraudio_core::{AudioConfig, Backend, Error, Result, StreamDirection};

use crate::{
    error::FromAlsa,
    ffi::{self, SndPcmFormatT},
};
use std::ffi::CString;

unsafe impl Send for AlsaBackend {}

pub struct AlsaBackend {
    handle: *mut ffi::SndPcm,
    c_name: CString,
}

impl Backend for AlsaBackend {
    fn open(name: &str, direction: StreamDirection) -> Result<Self> {
        let c_name = CString::new(name).map_err(|_| Error::DeviceNotFound)?;
        let mut handle: *mut ffi::SndPcm = std::ptr::null_mut();

        let err = unsafe {
            ffi::snd_pcm_open(
                &mut handle,
                c_name.as_ptr(),
                ffi::SndPcmStreamT::from_direction(direction),
                0,
            )
        };

        if err < 0 {
            return Err(Error::from_alsa(err));
        }

        Ok(AlsaBackend { handle, c_name })
    }

    fn configure(&self, config: &AudioConfig) -> Result<()> {
        let err = unsafe {
            crate::ffi::snd_pcm_set_params(
                self.handle,
                SndPcmFormatT::from_sample_format(config.format),
                crate::ffi::SndPcmAccessT::RwInterleaved,
                config.channels as u32,
                config.sample_rate,
                1,
                config.latency,
            )
        };

        if err < 0 {
            return Err(Error::from_alsa(err));
        }

        Ok(())
    }

    fn write_frames(&self, buffer: &[i16], channels: u16) -> Result<usize> {
        let frames = buffer.len() / channels as usize;
        let mut written = 0;

        while written < frames {
            let remaining = frames - written;
            let offset = written * channels as usize;

            let result: crate::ffi::SndPcmSframesT = unsafe {
                crate::ffi::snd_pcm_writei(
                    self.handle,
                    buffer[offset..].as_ptr() as *const _,
                    remaining as crate::ffi::SndPcmUframesT,
                )
            };

            if result < 0 {
                let recovered =
                    unsafe { crate::ffi::snd_pcm_recover(self.handle, result as i32, 1) };

                if recovered < 0 {
                    return Err(Error::from_alsa(recovered));
                }

                continue;
            }

            written += result as usize;
        }

        Ok(written)
    }

    fn read_frames(&self, buffer: &mut [i16], channels: u16) -> Result<usize> {
        let frames = buffer.len() / channels as usize;
        let result: crate::ffi::SndPcmSframesT = unsafe {
            crate::ffi::snd_pcm_readi(
                self.handle,
                buffer.as_mut_ptr() as *mut _,
                frames as crate::ffi::SndPcmUframesT,
            )
        };

        if result < 0 {
            let recovered = unsafe { crate::ffi::snd_pcm_recover(self.handle, result as i32, 1) };
            if recovered < 0 {
                return Err(Error::from_alsa(recovered));
            }
            return Ok(0);
        }

        Ok(result as usize)
    }
}

impl Drop for AlsaBackend {
    fn drop(&mut self) {
        unsafe {
            crate::ffi::snd_pcm_drain(self.handle);
            crate::ffi::snd_pcm_close(self.handle);
        }

        tracing::debug!(
            "Device {} is dropped",
            self.c_name.to_str().unwrap_or("undefined")
        );
    }
}
