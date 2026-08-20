use vibraudio_core::AudioConfig;
use vibraudio_core::{Error, Result};

mod speakers;

pub trait SinkTrait {
    fn configure(config: &AudioConfig) -> Result<()>;

    fn start() -> Result<()>;

    fn stop() -> Result<()>;

    fn write() -> Result<()>;
}
