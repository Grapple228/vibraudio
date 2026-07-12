use vibraudio_core::{
    sample::Sample, AudioConfig, Backend, Error, PcmDevice, Result, StreamDirection,
};

use crate::{
    error::FromAlsa,
    ffi::{self, SndPcmFormatT},
};
use std::{ffi::CString, marker::PhantomData};

unsafe impl<S: Sample> Send for AlsaBackend<S> {}
unsafe impl<S: Sample> Sync for AlsaBackend<S> {}

pub struct AlsaBackend<S: Sample> {
    handle: *mut ffi::SndPcm,
    c_name: CString,
    _phantom: PhantomData<S>,
}

impl<S: Sample> AlsaBackend<S> {
    fn sample_format() -> ffi::SndPcmFormatT {
        if std::mem::size_of::<S>() == 4 {
            ffi::SndPcmFormatT::FloatLe
        } else {
            ffi::SndPcmFormatT::S16Le
        }
    }
}

impl<S: Sample> Backend<S> for AlsaBackend<S> {
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

        Ok(AlsaBackend {
            handle,
            c_name,
            _phantom: PhantomData,
        })
    }

    fn configure(&self, config: &AudioConfig) -> Result<()> {
        let err = unsafe {
            crate::ffi::snd_pcm_set_params(
                self.handle,
                SndPcmFormatT::from_sample_format(S::sample_format()),
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

    fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize> {
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

    fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize> {
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

    fn reset(&self) -> Result<()> {
        let err = unsafe { ffi::snd_pcm_reset(self.handle) };
        if err < 0 {
            return Err(Error::from_alsa(err));
        }
        Ok(())
    }

    fn close(&self) -> Result<()> {
        unsafe {
            ffi::snd_pcm_drain(self.handle);
            ffi::snd_pcm_close(self.handle);
        }
        Ok(())
    }
}

impl<S: Sample> Drop for AlsaBackend<S> {
    fn drop(&mut self) {
        _ = self.close();

        tracing::debug!(
            "Device {} is dropped",
            self.c_name.to_str().unwrap_or("undefined")
        );
    }
}
