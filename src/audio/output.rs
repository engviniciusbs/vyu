//! Plays the mixer through the default output device (cpal / WASAPI).

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, Stream, StreamConfig};

use super::Mixer;
use super::mixer::RATE;

/// Keeps the output stream alive; dropping it stops the sound.
pub struct AudioOutput {
    _stream: Stream,
}

impl AudioOutput {
    /// Starts playback on the default device. `Err` means the app runs without sound.
    pub fn start(mixer: Arc<Mutex<Mixer>>) -> Result<AudioOutput, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("Nenhum dispositivo de áudio encontrado")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config = supported.config();
        tracing::info!("áudio: {} Hz, {} canais, {:?}", config.sample_rate, config.channels, format);
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, mixer),
            SampleFormat::I16 => build::<i16>(&device, config, mixer),
            SampleFormat::U16 => build::<u16>(&device, config, mixer),
            other => return Err(format!("formato de áudio não suportado: {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(AudioOutput { _stream: stream })
    }
}

fn build<T>(device: &cpal::Device, config: StreamConfig, mixer: Arc<Mutex<Mixer>>) -> Result<Stream, String>
where
    T: SizedSample + cpal::FromSample<f32>,
{
    let channels = config.channels as usize;
    let rate = config.sample_rate;
    let mut mixed: Vec<f32> = Vec::new();
    let mut stretched: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let frames = data.len() / channels;
                mixed.resize(mixer_frames_for(frames, rate), 0.0);
                if let Ok(mut m) = mixer.lock() {
                    m.fill(&mut mixed);
                } else {
                    mixed.fill(0.0);
                }
                stretched.resize(frames, 0.0);
                stretch(&mixed, &mut stretched);
                for (frame, s) in data.chunks_mut(channels).zip(&stretched) {
                    frame.fill(T::from_sample(*s));
                }
            },
            |e| tracing::warn!("erro no áudio: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// How many 48 kHz mixer samples cover `frames` at the device rate.
fn mixer_frames_for(frames: usize, device_rate: u32) -> usize {
    (frames * RATE).div_ceil(device_rate.max(1) as usize)
}

/// Linear resampling of `src` onto `out` (lengths define the ratio).
fn stretch(src: &[f32], out: &mut [f32]) {
    if src.len() == out.len() {
        out.copy_from_slice(src);
        return;
    }
    if src.is_empty() {
        out.fill(0.0);
        return;
    }
    let step = src.len() as f32 / out.len() as f32;
    for (i, o) in out.iter_mut().enumerate() {
        let pos = i as f32 * step;
        let i0 = (pos.floor() as usize).min(src.len() - 1);
        let i1 = (i0 + 1).min(src.len() - 1);
        let t = pos - i0 as f32;
        *o = src[i0] * (1.0 - t) + src[i1] * t;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_identity() {
        let src = [0.1, 0.2, 0.3, 0.4];
        let mut out = [0.0; 4];
        stretch(&src, &mut out);
        assert_eq!(out, src);
    }

    #[test]
    fn stretch_interpolates_linearly() {
        let src = [0.0, 1.0];
        let mut out = [0.0; 4];
        stretch(&src, &mut out);
        assert_eq!(out, [0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn input_len_for_device_rate() {
        assert_eq!(mixer_frames_for(480, 48_000), 480);
        assert_eq!(mixer_frames_for(441, 44_100), 480);
    }
}
