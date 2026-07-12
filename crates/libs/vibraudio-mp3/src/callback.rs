// ./crates/libs/vibraudio-mp3/src/callback.rs
use crate::decoder::Mp3StreamDecoder;
use std::io::Read;
use vibraudio_core::{callback::AudioCallback, Error, Result};

/// MP3 колбэк для использования с Player
pub struct Mp3Callback<R: Read> {
    decoder: Mp3StreamDecoder<R>,
    pcm_buffer: [i16; 2304],
    channels: u16,
    sample_rate: u32,
}

impl<R: Read> Mp3Callback<R> {
    pub fn new(reader: R) -> Self {
        Self {
            decoder: Mp3StreamDecoder::new(reader),
            pcm_buffer: [0i16; 2304],
            channels: 2,
            sample_rate: 44100,
        }
    }
}

impl<R: Read> AudioCallback for Mp3Callback<R> {
    fn on_audio_required(
        &mut self,
        output: &mut [i16],
        channels: u16,
        sample_rate: u32,
    ) -> Result<usize> {
        // Проверяем соответствие формата
        if channels != self.channels || sample_rate != self.sample_rate {
            // Пробуем получить информацию из первого фрейма
            if let Ok(info) = self.decoder.decode_next_frame(&mut self.pcm_buffer) {
                self.channels = info.channels;
                self.sample_rate = info.sample_rate;

                // Если все еще не совпадает - ошибка
                if channels != self.channels || sample_rate != self.sample_rate {
                    return Err(Error::InvalidConfig);
                }

                // Копируем первый фрейм
                let samples = info.samples * info.channels as usize;
                let copy = samples.min(output.len());
                output[..copy].copy_from_slice(&self.pcm_buffer[..copy]);
                return Ok(copy);
            }
            return Err(Error::InvalidConfig);
        }

        let mut total_samples = 0;
        let max_samples = output.len();

        while total_samples < max_samples {
            match self.decoder.decode_next_frame(&mut self.pcm_buffer) {
                Ok(info) => {
                    let frame_samples = info.samples * info.channels as usize;
                    let remaining = max_samples - total_samples;
                    let copy_samples = frame_samples.min(remaining);

                    output[total_samples..total_samples + copy_samples]
                        .copy_from_slice(&self.pcm_buffer[..copy_samples]);
                    total_samples += copy_samples;

                    if copy_samples < frame_samples {
                        // Сохраняем остаток для следующего раза
                        // TODO: добавить leftover буфер
                        break;
                    }
                }
                Err(Error::EndOfInput) => break,
                Err(e) => return Err(e),
            }
        }

        // Если не набрали данных - заполняем тишиной
        if total_samples < max_samples {
            for i in total_samples..max_samples {
                output[i] = 0;
            }
        }

        Ok(total_samples)
    }
}
