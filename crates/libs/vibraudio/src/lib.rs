#[doc = "hidden"]
pub mod core {
    pub use vibraudio_core::*;
}

#[doc = "hidden"]
pub mod mp3 {
    pub use vibraudio_mp3::*;
}

pub mod backend {
    #[cfg(not(any(target_os = "linux")))]
    compile_error!("Unsupported platform");

    #[doc = "hidden"]
    pub mod alsa {
        pub use vibraudio_alsa::*;
    }
}
