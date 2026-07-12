use crate::Result;

/// Трейт для обработки аудио данных
pub trait AudioCallback {
    /// Вызывается когда нужно заполнить буфер аудиоданными
    ///
    /// # Arguments
    /// * `output` - буфер для заполнения PCM данными (i16)
    /// * `channels` - количество каналов
    /// * `sample_rate` - частота дискретизации
    ///
    /// # Returns
    /// * `Ok(usize)` - количество записанных семплов
    /// * `Err` - ошибка
    fn on_audio_required(
        &mut self,
        output: &mut [i16],
        channels: u16,
        sample_rate: u32,
    ) -> Result<usize>;
}

pub struct SilenceCallback;

impl AudioCallback for SilenceCallback {
    fn on_audio_required(
        &mut self,
        output: &mut [i16],
        _channels: u16,
        _sample_rate: u32,
    ) -> Result<usize> {
        for sample in output.iter_mut() {
            *sample = 0;
        }
        Ok(output.len())
    }
}
