#[doc = "hidden"]
pub mod core {
    pub use vibraudio_core::*;
}

#[doc = "hidden"]
pub mod mp3 {
    pub use vibraudio_mp3::*;
}

pub mod devices;

#[doc = "hidden"]
pub mod ringbuffer {
    pub use vibraudio_ringbuffer::*;
}

#[doc = "hidden"]
pub mod thread {
    pub use vibraudio_thread::*;
}

pub mod backend {
    #[cfg(target_os = "linux")]
    pub mod alsa {
        pub use vibraudio_alsa::*;
    }

    #[cfg(target_os = "windows")]
    pub mod wasapi {
        pub use vibraudio_wasapi::*;
    }
}

pub mod platform {
    #[cfg(target_os = "linux")]
    pub type DefaultBackend<S> = vibraudio_alsa::AlsaBackend<S>;

    #[cfg(target_os = "linux")]
    pub use vibraudio_alsa::FRAMES;

    #[cfg(target_os = "windows")]
    pub type DefaultBackend<S> = vibraudio_wasapi::WasapiBackend<S>;

    #[cfg(target_os = "windows")]
    pub use vibraudio_wasapi::FRAMES;

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    compile_error!("Unsupported platform");
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("vibraudio currently supports only Linux and Windows");
