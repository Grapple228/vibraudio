// ./src/backend.rs
use crate::error::FromWindows;
use crate::ffi::*;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;
use tracing::{debug, error, info};
use vibraudio_core::{
    sample::Sample, AudioConfig, AudioFormatInfo, Backend, Error, Result, StreamDirection,
};

const COINIT_MULTITHREADED: u32 = 0x0;

static COM_INITIALIZED: OnceLock<()> = OnceLock::new();

fn ensure_com_initialized() {
    COM_INITIALIZED.get_or_init(|| unsafe {
        let hr = CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED);
        if hr < 0 {
            error!("CoInitializeEx failed with code: {}", hr);
        } else {
            debug!("COM initialized successfully");
        }
    });
}

pub struct WasapiBackend<S: Sample> {
    client: *mut IAudioClient,
    render_client: AtomicPtr<IAudioRenderClient>,
    capture_client: AtomicPtr<IAudioCaptureClient>,
    device: *mut IMMDevice,
    direction: StreamDirection,
    buffer_size_frames: u32,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    _phantom: PhantomData<S>,
}

impl<S: Sample> WasapiBackend<S> {
    pub fn new() -> Result<Self> {
        ensure_com_initialized();
        Ok(WasapiBackend {
            client: ptr::null_mut(),
            render_client: AtomicPtr::new(ptr::null_mut()),
            capture_client: AtomicPtr::new(ptr::null_mut()),
            device: ptr::null_mut(),
            direction: StreamDirection::Playback,
            buffer_size_frames: 0,
            channels: 0,
            sample_rate: 0,
            bits_per_sample: 0,
            _phantom: PhantomData,
        })
    }

    unsafe fn get_default_device(direction: StreamDirection) -> Result<*mut IMMDevice> {
        let mut enumerator: *mut IMMDeviceEnumerator = ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_MMDeviceEnumerator,
            ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IMMDeviceEnumerator,
            &mut enumerator as *mut *mut _ as *mut *mut c_void,
        );

        if hr < 0 || enumerator.is_null() {
            error!("Failed to create MMDeviceEnumerator: {}", hr);
            return Err(Error::from_windows(hr));
        }

        let mut device: *mut IMMDevice = ptr::null_mut();
        let flow = match direction {
            StreamDirection::Playback => EDataFlow::eRender,
            StreamDirection::Capture => EDataFlow::eCapture,
        };

        let hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(
            enumerator,
            flow,
            ERole::eConsole,
            &mut device,
        );

        com_release(enumerator as *mut c_void);

        if hr < 0 || device.is_null() {
            error!("Failed to get default audio endpoint: {}", hr);
            return Err(Error::from_windows(hr));
        }

        Ok(device)
    }

    unsafe fn activate_audio_client(device: *mut IMMDevice) -> Result<*mut IAudioClient> {
        if device.is_null() {
            return Err(Error::InvalidConfig);
        }

        let mut client: *mut IAudioClient = ptr::null_mut();
        let hr = IMMDevice_Activate(
            device,
            &IID_IAudioClient,
            CLSCTX_INPROC_SERVER,
            ptr::null_mut(),
            &mut client as *mut *mut _ as *mut *mut c_void,
        );

        if hr < 0 || client.is_null() {
            error!("Failed to activate audio client: {}", hr);
            return Err(Error::from_windows(hr));
        }

        Ok(client)
    }

    unsafe fn find_best_format(&self, requested: &WAVEFORMATEX) -> Result<WAVEFORMATEX> {
        if self.client.is_null() {
            return Err(Error::InvalidConfig);
        }

        // Try exact format first
        let mut closest: *mut WAVEFORMATEX = ptr::null_mut();
        let hr = IAudioClient_IsFormatSupported(
            self.client,
            AUDCLNT_SHAREMODE_SHARED,
            requested,
            &mut closest,
        );

        if hr == 0 {
            // Exact match
            return Ok(requested.clone());
        } else if hr == 1 && !closest.is_null() {
            // Closest match available
            let format = (*closest).clone();
            CoTaskMemFree(closest as *mut c_void);
            return Ok(format);
        }

        // Try common formats
        let common_formats = [
            (2, 48000, 16),
            (2, 44100, 16),
            (2, 48000, 24),
            (2, 44100, 24),
            (1, 48000, 16),
            (1, 44100, 16),
            (2, 48000, 32),
            (2, 44100, 32),
        ];

        for (ch, rate, bits) in common_formats {
            let bytes = (bits / 8) as u32;
            let test_format = WAVEFORMATEX {
                wFormatTag: if bits == 32 { 3 } else { 1 },
                nChannels: ch,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * ch as u32 * bytes,
                nBlockAlign: ch * bytes as u16,
                wBitsPerSample: bits,
                cbSize: 0,
            };

            let mut closest_match: *mut WAVEFORMATEX = ptr::null_mut();
            let hr = IAudioClient_IsFormatSupported(
                self.client,
                AUDCLNT_SHAREMODE_SHARED,
                &test_format,
                &mut closest_match,
            );

            if hr == 0 {
                return Ok(test_format);
            } else if hr == 1 && !closest_match.is_null() {
                let format = (*closest_match).clone();
                CoTaskMemFree(closest_match as *mut c_void);
                return Ok(format);
            }
        }

        // Fallback to mix format
        let mut mix_format: *mut WAVEFORMATEX = ptr::null_mut();
        let hr = IAudioClient_GetMixFormat(self.client, &mut mix_format);
        if hr >= 0 && !mix_format.is_null() {
            let format = (*mix_format).clone();
            CoTaskMemFree(mix_format as *mut c_void);
            return Ok(format);
        }

        Err(Error::InvalidConfig)
    }

    unsafe fn enumerate_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        if self.client.is_null() {
            return Err(Error::InvalidConfig);
        }

        let mut formats = Vec::new();

        // Get mix format
        let mut mix_format: *mut WAVEFORMATEX = ptr::null_mut();
        let hr = IAudioClient_GetMixFormat(self.client, &mut mix_format);
        if hr >= 0 && !mix_format.is_null() {
            let f = &*mix_format;
            formats.push(AudioFormatInfo::new(
                f.nChannels,
                f.nSamplesPerSec,
                f.wBitsPerSample,
                f.wFormatTag,
            ));
            CoTaskMemFree(mix_format as *mut c_void);
        }

        // Test common formats
        let test_formats = [
            (1, 44100, 16),
            (1, 44100, 24),
            (1, 48000, 16),
            (1, 48000, 24),
            (2, 44100, 16),
            (2, 44100, 24),
            (2, 48000, 16),
            (2, 48000, 24),
            (2, 44100, 32),
            (2, 48000, 32),
            (2, 96000, 24),
        ];

        for (ch, rate, bits) in test_formats {
            let bytes = (bits / 8) as u32;
            let fmt = WAVEFORMATEX {
                wFormatTag: if bits == 32 { 3 } else { 1 },
                nChannels: ch,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * ch as u32 * bytes,
                nBlockAlign: ch * bytes as u16,
                wBitsPerSample: bits,
                cbSize: 0,
            };

            let mut closest: *mut WAVEFORMATEX = ptr::null_mut();
            let hr = IAudioClient_IsFormatSupported(
                self.client,
                AUDCLNT_SHAREMODE_SHARED,
                &fmt,
                &mut closest,
            );

            if hr == 0 {
                let info = AudioFormatInfo::new(ch, rate, bits, fmt.wFormatTag);
                if !formats.contains(&info) {
                    formats.push(info);
                }
            } else if hr == 1 && !closest.is_null() {
                let f = &*closest;
                let info = AudioFormatInfo::new(
                    f.nChannels,
                    f.nSamplesPerSec,
                    f.wBitsPerSample,
                    f.wFormatTag,
                );
                if !formats.contains(&info) {
                    formats.push(info);
                }
                CoTaskMemFree(closest as *mut c_void);
            }
        }

        formats.sort_by(|a, b| {
            a.channels
                .cmp(&b.channels)
                .then(a.sample_rate.cmp(&b.sample_rate))
                .then(a.bits_per_sample.cmp(&b.bits_per_sample))
        });
        formats.dedup();

        Ok(formats)
    }
}

impl<S: Sample> Backend<S> for WasapiBackend<S> {
    fn open(name: &str, direction: StreamDirection) -> Result<Self> {
        ensure_com_initialized();

        let mut backend = Self::new()?;
        backend.direction = direction;

        unsafe {
            backend.device = Self::get_default_device(direction)?;
            backend.client = Self::activate_audio_client(backend.device)?;
        }

        info!("✅ WASAPI device opened for {:?}", direction);
        Ok(backend)
    }

    fn configure(&self, config: &AudioConfig) -> Result<()> {
        unsafe {
            if self.client.is_null() {
                return Err(Error::InvalidConfig);
            }

            // Create requested format
            let bytes_per_sample = S::sample_format().bytes_per_sample() as u32;
            let requested = WAVEFORMATEX {
                wFormatTag: if bytes_per_sample == 4 { 3 } else { 1 },
                nChannels: config.channels,
                nSamplesPerSec: config.sample_rate,
                nAvgBytesPerSec: config.sample_rate * config.channels as u32 * bytes_per_sample,
                nBlockAlign: config.channels * bytes_per_sample as u16,
                wBitsPerSample: (bytes_per_sample * 8) as u16,
                cbSize: 0,
            };

            // Find supported format
            let actual_format = self.find_best_format(&requested)?;

            // Convert latency: microseconds -> 100-nanosecond units
            let buffer_duration = (config.latency as u64) * 10;

            // Initialize client
            let hr = IAudioClient_Initialize(
                self.client,
                AUDCLNT_SHAREMODE_SHARED,
                0,
                buffer_duration,
                0,
                &actual_format,
                ptr::null(),
            );

            if hr < 0 {
                error!("IAudioClient_Initialize failed: {}", hr);
                return Err(Error::from_windows(hr));
            }

            // Get buffer size
            let mut buffer_size: u32 = 0;
            let hr = IAudioClient_GetBufferSize(self.client, &mut buffer_size);
            if hr < 0 {
                error!("IAudioClient_GetBufferSize failed: {}", hr);
                return Err(Error::from_windows(hr));
            }

            // Store configuration
            let self_mut = self as *const Self as *mut Self;
            (*self_mut).buffer_size_frames = buffer_size;
            (*self_mut).channels = actual_format.nChannels;
            (*self_mut).sample_rate = actual_format.nSamplesPerSec;
            (*self_mut).bits_per_sample = actual_format.wBitsPerSample;

            // Get service interface
            match self.direction {
                StreamDirection::Playback => {
                    let mut render_client: *mut IAudioRenderClient = ptr::null_mut();
                    let hr = IAudioClient_GetService(
                        self.client,
                        &IID_IAudioRenderClient,
                        &mut render_client as *mut *mut _ as *mut *mut c_void,
                    );
                    if hr < 0 || render_client.is_null() {
                        error!("Failed to get IAudioRenderClient: {}", hr);
                        return Err(Error::from_windows(hr));
                    }
                    self.render_client.store(render_client, Ordering::SeqCst);
                }
                StreamDirection::Capture => {
                    let mut capture_client: *mut IAudioCaptureClient = ptr::null_mut();
                    let hr = IAudioClient_GetService(
                        self.client,
                        &IID_IAudioCaptureClient,
                        &mut capture_client as *mut *mut _ as *mut *mut c_void,
                    );
                    if hr < 0 || capture_client.is_null() {
                        error!("Failed to get IAudioCaptureClient: {}", hr);
                        return Err(Error::from_windows(hr));
                    }
                    self.capture_client.store(capture_client, Ordering::SeqCst);
                }
            }

            // Start the stream
            let hr = IAudioClient_Start(self.client);
            if hr < 0 {
                error!("IAudioClient_Start failed: {}", hr);
                return Err(Error::from_windows(hr));
            }

            debug!(
                "Configured: {}Hz, {}ch, {}bit, buffer={} frames, latency={}us",
                actual_format.nSamplesPerSec,
                actual_format.nChannels,
                actual_format.wBitsPerSample,
                buffer_size,
                config.latency
            );

            Ok(())
        }
    }

    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        unsafe { self.enumerate_formats() }
    }

    fn configure_with_format(&self, format: &AudioFormatInfo, latency_us: u32) -> Result<()> {
        let config = AudioConfig::new(format.sample_rate, format.channels, latency_us);
        self.configure(&config)
    }

    #[inline(never)]
    fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize> {
        unsafe {
            let render_client = self.render_client.load(Ordering::SeqCst);
            if render_client.is_null() {
                return Err(Error::InvalidConfig);
            }

            let frames_needed = buffer.len() / channels as usize;
            if frames_needed == 0 {
                return Ok(0);
            }

            let mut padding: u32 = 0;
            let hr = IAudioClient_GetCurrentPadding(self.client, &mut padding);
            if hr < 0 {
                return Err(Error::from_windows(hr));
            }

            let available_frames = self.buffer_size_frames - padding;
            if available_frames == 0 {
                return Ok(0);
            }

            let frames_to_write = (frames_needed as u32).min(available_frames);
            if frames_to_write == 0 {
                return Ok(0);
            }

            let mut data: *mut u8 = ptr::null_mut();
            let hr = IAudioRenderClient_GetBuffer(render_client, frames_to_write, &mut data);

            if hr < 0 || data.is_null() {
                return Err(Error::from_windows(hr));
            }

            let bytes_to_copy =
                frames_to_write as usize * channels as usize * std::mem::size_of::<S>();
            ptr::copy_nonoverlapping(buffer.as_ptr() as *const u8, data, bytes_to_copy);

            let hr = IAudioRenderClient_ReleaseBuffer(render_client, frames_to_write, 0);
            if hr < 0 {
                return Err(Error::from_windows(hr));
            }

            Ok(frames_to_write as usize)
        }
    }

    #[inline(never)]
    fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize> {
        unsafe {
            let capture_client = self.capture_client.load(Ordering::SeqCst);
            if capture_client.is_null() {
                return Err(Error::InvalidConfig);
            }

            let mut data: *mut u8 = ptr::null_mut();
            let mut frames_available: u32 = 0;
            let mut flags: u32 = 0;

            let hr = IAudioCaptureClient_GetBuffer(
                capture_client,
                &mut data,
                &mut frames_available,
                &mut flags,
                ptr::null_mut(),
                ptr::null_mut(),
            );

            if hr < 0 || frames_available == 0 || data.is_null() {
                return Ok(0);
            }

            let max_frames = buffer.len() / channels as usize;
            let frames_to_read = (frames_available as usize).min(max_frames);

            if frames_to_read > 0 {
                let bytes_to_copy = frames_to_read * channels as usize * std::mem::size_of::<S>();
                ptr::copy_nonoverlapping(data, buffer.as_mut_ptr() as *mut u8, bytes_to_copy);
            }

            let hr = IAudioCaptureClient_ReleaseBuffer(capture_client, frames_available);
            if hr < 0 {
                return Err(Error::from_windows(hr));
            }

            Ok(frames_to_read)
        }
    }

    fn reset(&self) -> Result<()> {
        unsafe {
            if !self.client.is_null() {
                let hr = IAudioClient_Reset(self.client);
                if hr < 0 {
                    return Err(Error::from_windows(hr));
                }
            }
        }
        Ok(())
    }

    fn close(&self) -> Result<()> {
        unsafe {
            if !self.client.is_null() {
                let hr = IAudioClient_Stop(self.client);
                if hr < 0 {
                    return Err(Error::from_windows(hr));
                }
            }
        }
        Ok(())
    }
}

impl<S: Sample> Drop for WasapiBackend<S> {
    fn drop(&mut self) {
        unsafe {
            if !self.client.is_null() {
                IAudioClient_Stop(self.client);
            }

            let render_client = self.render_client.load(Ordering::SeqCst);
            if !render_client.is_null() {
                com_release(render_client as *mut c_void);
                self.render_client.store(ptr::null_mut(), Ordering::SeqCst);
            }

            let capture_client = self.capture_client.load(Ordering::SeqCst);
            if !capture_client.is_null() {
                com_release(capture_client as *mut c_void);
                self.capture_client.store(ptr::null_mut(), Ordering::SeqCst);
            }

            if !self.client.is_null() {
                com_release(self.client as *mut c_void);
                self.client = ptr::null_mut();
            }

            if !self.device.is_null() {
                com_release(self.device as *mut c_void);
                self.device = ptr::null_mut();
            }
        }
    }
}

unsafe impl<S: Sample> Send for WasapiBackend<S> {}
unsafe impl<S: Sample> Sync for WasapiBackend<S> {}
