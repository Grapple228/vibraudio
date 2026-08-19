// ./src/backend.rs
use crate::error::FromWindows;
use crate::ffi::*;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;
use tracing::{debug, error, info, warn};
use vibraudio_core::{
    sample::Sample, AudioConfig, AudioFormatInfo, Backend, Error, Result, StreamDirection,
};
use windows::Win32::System::Com::COINIT_MULTITHREADED;

static COM_INITIALIZED: OnceLock<()> = OnceLock::new();
static ENUMERATOR: AtomicPtr<IMMDeviceEnumerator> = AtomicPtr::new(ptr::null_mut());

fn ensure_com_initialized() {
    COM_INITIALIZED.get_or_init(|| unsafe {
        CoInitializeEx(ptr::null_mut(), COINIT_MULTITHREADED.0 as u32);
    });
}

fn get_enumerator() -> *mut IMMDeviceEnumerator {
    let mut ptr = ENUMERATOR.load(Ordering::SeqCst);
    if ptr.is_null() {
        unsafe {
            let mut enumerator: *mut IMMDeviceEnumerator = ptr::null_mut();
            CoCreateInstance(
                &CLSID_MMDeviceEnumerator,
                ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_IMMDeviceEnumerator,
                &mut enumerator as *mut *mut IMMDeviceEnumerator as *mut *mut c_void,
            );
            match ENUMERATOR.compare_exchange(
                ptr::null_mut(),
                enumerator,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => ptr = enumerator,
                Err(existing) => {
                    if !enumerator.is_null() {
                        com_release(enumerator as *mut c_void);
                    }
                    ptr = existing;
                }
            }
        }
    }
    ptr
}

pub struct WasapiBackend<S: Sample> {
    device: *mut IMMDevice,
    client: *mut IAudioClient,
    render_client: *mut IAudioRenderClient,
    capture_client: *mut IAudioCaptureClient,
    direction: StreamDirection,
    buffer_size: u32,
    channels: u16,
    _phantom: PhantomData<S>,
}

impl<S: Sample> WasapiBackend<S> {
    pub fn new() -> Result<Self> {
        ensure_com_initialized();
        Ok(WasapiBackend {
            device: ptr::null_mut(),
            client: ptr::null_mut(),
            render_client: ptr::null_mut(),
            capture_client: ptr::null_mut(),
            direction: StreamDirection::Playback,
            buffer_size: 0,
            channels: 0,
            _phantom: PhantomData,
        })
    }

    unsafe fn enumerate_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        let mut supported = Vec::new();
        let mut mix_format_ptr: *mut WAVEFORMATEX = ptr::null_mut();
        if IAudioClient_GetMixFormat(self.client, &mut mix_format_ptr) >= 0
            && !mix_format_ptr.is_null()
        {
            let f = &*mix_format_ptr;
            supported.push(AudioFormatInfo::new(
                f.nChannels,
                f.nSamplesPerSec,
                f.wBitsPerSample,
                f.wFormatTag,
            ));
            CoTaskMemFree(mix_format_ptr as *mut c_void);
        }
        for &ch in &[1u16, 2u16] {
            for &rate in &[44100u32, 48000u32] {
                for &bits in &[16u16, 24u16, 32u16] {
                    let fmt = WAVEFORMATEX {
                        wFormatTag: 1,
                        nChannels: ch,
                        nSamplesPerSec: rate,
                        nAvgBytesPerSec: rate * ch as u32 * (bits / 8) as u32,
                        nBlockAlign: ch * (bits / 8) as u16,
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
                        let info = AudioFormatInfo::new(ch, rate, bits, 1);
                        if !supported.contains(&info) {
                            supported.push(info);
                        }
                    } else if hr == 1 && !closest.is_null() {
                        let f = &*closest;
                        let info = AudioFormatInfo::new(
                            f.nChannels,
                            f.nSamplesPerSec,
                            f.wBitsPerSample,
                            f.wFormatTag,
                        );
                        if !supported.contains(&info) {
                            supported.push(info);
                        }
                        CoTaskMemFree(closest as *mut c_void);
                    }
                }
            }
        }
        supported.sort_by(|a, b| a.sample_rate.cmp(&b.sample_rate));
        Ok(supported)
    }
}

impl<S: Sample> Backend<S> for WasapiBackend<S> {
    fn supported_formats(&self) -> Result<Vec<AudioFormatInfo>> {
        unsafe { self.enumerate_formats() }
    }

    fn configure_with_format(&self, format: &AudioFormatInfo, latency_us: u32) -> Result<()> {
        let info = AudioConfig::new(format.sample_rate, format.channels, latency_us);
        self.configure(&info)
    }

    fn open(name: &str, direction: StreamDirection) -> Result<Self> {
        let mut backend = Self::new()?;
        backend.direction = direction;
        backend.device = get_default_device(direction)?;
        backend.client = activate_audio_client(backend.device)?;
        info!("✅ WASAPI device opened for {:?}", direction);
        Ok(backend)
    }

    fn configure(&self, config: &AudioConfig) -> Result<()> {
        unsafe {
            let bytes = S::sample_format().bytes_per_sample() as u32;
            let format = WAVEFORMATEX {
                wFormatTag: 1,
                nChannels: config.channels,
                nSamplesPerSec: config.sample_rate,
                nAvgBytesPerSec: config.sample_rate * config.channels as u32 * bytes,
                nBlockAlign: config.channels * bytes as u16,
                wBitsPerSample: (bytes * 8) as u16,
                cbSize: 0,
            };

            let hr = IAudioClient_Initialize(
                self.client,
                AUDCLNT_SHAREMODE_SHARED,
                0,
                (config.latency as u64) * 10000,
                0,
                &format,
                ptr::null(),
            );
            if hr < 0 {
                return Err(Error::from_windows(hr as u32));
            }

            let mut buffer_size: u32 = 0;
            IAudioClient_GetBufferSize(self.client, &mut buffer_size);

            let self_mut = self as *const Self as *mut Self;
            (*self_mut).buffer_size = buffer_size;
            (*self_mut).channels = config.channels;

            match self.direction {
                StreamDirection::Playback => {
                    let mut rc: *mut IAudioRenderClient = ptr::null_mut();
                    IAudioClient_GetService(
                        self.client,
                        &IID_IAudioRenderClient,
                        &mut rc as *mut *mut _ as *mut *mut c_void,
                    );
                    (*self_mut).render_client = rc;
                }
                StreamDirection::Capture => {
                    let mut cc: *mut IAudioCaptureClient = ptr::null_mut();
                    IAudioClient_GetService(
                        self.client,
                        &IID_IAudioCaptureClient,
                        &mut cc as *mut *mut _ as *mut *mut c_void,
                    );
                    (*self_mut).capture_client = cc;
                }
            }

            IAudioClient_Start(self.client);
            Ok(())
        }
    }

    fn write_frames(&self, buffer: &[S], channels: u16) -> Result<usize> {
        unsafe {
            if self.render_client.is_null() {
                return Err(Error::InvalidConfig);
            }
            let n = (buffer.len() / channels as usize) as u32;
            if n == 0 {
                return Ok(0);
            }
            let mut data: *mut u8 = ptr::null_mut();
            IAudioRenderClient_GetBuffer(self.render_client, n, &mut data);
            if data.is_null() {
                return Ok(0);
            }
            let size = n as usize * channels as usize * mem::size_of::<S>();
            ptr::copy_nonoverlapping(buffer.as_ptr() as *const u8, data, size);
            IAudioRenderClient_ReleaseBuffer(self.render_client, n, 0);
            Ok(n as usize)
        }
    }

    fn read_frames(&self, buffer: &mut [S], channels: u16) -> Result<usize> {
        unsafe {
            if self.capture_client.is_null() {
                return Err(Error::InvalidConfig);
            }
            let mut data: *mut u8 = ptr::null_mut();
            let mut n: u32 = 0;
            let mut flags: u32 = 0;
            let hr = IAudioCaptureClient_GetBuffer(
                self.capture_client,
                &mut data,
                &mut n,
                &mut flags,
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if hr < 0 || n == 0 || data.is_null() {
                return Ok(0);
            }
            let max = (buffer.len() / channels as usize) as u32;
            let frames = n.min(max);
            let size = frames as usize * channels as usize * mem::size_of::<S>();
            ptr::copy_nonoverlapping(data, buffer.as_mut_ptr() as *mut u8, size);
            IAudioCaptureClient_ReleaseBuffer(self.capture_client, n);
            Ok(frames as usize)
        }
    }

    fn reset(&self) -> Result<()> {
        unsafe {
            if !self.client.is_null() {
                IAudioClient_Reset(self.client);
            }
        }
        Ok(())
    }

    fn close(&self) -> Result<()> {
        unsafe {
            if !self.client.is_null() {
                IAudioClient_Stop(self.client);
            }
        }
        Ok(())
    }
}

impl<S: Sample> Drop for WasapiBackend<S> {
    fn drop(&mut self) {
        unsafe {
            if !self.render_client.is_null() {
                com_release(self.render_client as *mut c_void);
            }
            if !self.capture_client.is_null() {
                com_release(self.capture_client as *mut c_void);
            }
            if !self.client.is_null() {
                com_release(self.client as *mut c_void);
            }
            if !self.device.is_null() {
                com_release(self.device as *mut c_void);
            }
        }
    }
}

fn get_default_device(direction: StreamDirection) -> Result<*mut IMMDevice> {
    unsafe {
        let enumerator = get_enumerator();
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
        if hr < 0 || device.is_null() {
            return Err(Error::from_windows(hr as u32));
        }
        Ok(device)
    }
}

fn activate_audio_client(device: *mut IMMDevice) -> Result<*mut IAudioClient> {
    unsafe {
        let mut client: *mut IAudioClient = ptr::null_mut();
        let hr = IMMDevice_Activate(
            device,
            &IID_IAudioClient,
            CLSCTX_INPROC_SERVER,
            ptr::null_mut(),
            &mut client as *mut *mut _ as *mut *mut c_void,
        );
        if hr < 0 || client.is_null() {
            return Err(Error::from_windows(hr as u32));
        }
        Ok(client)
    }
}

unsafe impl<S: Sample> Send for WasapiBackend<S> {}
unsafe impl<S: Sample> Sync for WasapiBackend<S> {}
