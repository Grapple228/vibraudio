//! Main Crate Error

use std::ffi::{c_int, CStr};

use derive_more::derive::From;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    DeviceNotFound,
    InvalidConfig,
    EndOfInput,
    DecodeFailed,

    Ffi {
        code: c_int,
        message: &'static str,
    },
    InvalidParameter,

    #[from]
    Io(std::io::Error),
}

// region:    --- Error Boilerplate

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}

// endregion: --- Error Boilerplate
