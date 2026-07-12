mod error;

mod backend;
pub mod callback;
pub mod config;
pub mod device;
pub mod sample;

pub use backend::Backend;
pub use config::AudioConfig;
pub use config::SampleFormat;
pub use device::PcmDevice;
pub use device::StreamDirection;
pub use error::{Error, Result};

pub fn init() -> Result<()> {
    // LOGGING INITIALIZATION
    tracing_subscriber::fmt()
        .without_time() // For early development
        .with_target(false)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    Ok(())
}
