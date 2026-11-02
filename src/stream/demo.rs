//! Synthetic cameras for trying the app without hardware (`VYU_DEMO=1`).

use std::time::{Duration, Instant};

use super::StreamVariant;
use super::frame::Frame;
use super::session::{Source, SourceError, SourceEvent, SourceFactory};

const FPS: u32 = 15;
const AUDIO_RATE: u32 = 48_000;

/// `seed` varies the color so each demo camera looks different.
#[derive(Clone)]
pub struct DemoFactory {
    pub seed: u32,
}

impl SourceFactory for DemoFactory {
    fn open(&self, v: StreamVariant) -> Result<Box<dyn Source>, SourceError> {
        let (w, h) = match v {
            StreamVariant::High => (1280, 720),
            StreamVariant::Low => (640, 360),
        };
        Ok(Box::new(DemoSource { w, h, seed: self.seed, tick: 0, next: Instant::now(), audio_turn: false, phase: 0.0 }))
    }
}

struct DemoSource {
    w: u32,
    h: u32,
    seed: u32,
    tick: u32,
    next: Instant,
    audio_turn: bool,
    phase: f32,
}

impl DemoSource {
    fn frame(&self) -> Frame {
        let (w, h) = (self.w as usize, self.h as usize);
        let hue = (self.seed.wrapping_mul(67) % 255) as u8;
        let bar = (self.tick as usize * 8) % w;
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let on_bar = x.abs_diff(bar) < w / 40;
                let v = (y * 160 / h) as u8;
                rgba[i] = if on_bar { 255 } else { v / 2 + hue / 3 };
                rgba[i + 1] = if on_bar { 255 } else { v / 3 };
                rgba[i + 2] = if on_bar { 255 } else { v.saturating_add(hue / 4) };
                rgba[i + 3] = 255;
            }
        }
        Frame { width: self.w, height: self.h, rgba, seq: 0 }
    }

    fn tone(&mut self) -> Vec<f32> {
        let n = (AUDIO_RATE / FPS) as usize;
        let freq = 220.0 + (self.seed % 5) as f32 * 110.0;
        (0..n)
            .map(|_| {
                self.phase = (self.phase + freq / AUDIO_RATE as f32) % 1.0;
                (self.phase * std::f32::consts::TAU).sin() * 0.2
            })
            .collect()
    }
}

impl Source for DemoSource {
    fn next_event(&mut self) -> Result<SourceEvent, SourceError> {
        self.audio_turn = !self.audio_turn;
        if self.audio_turn {
            let now = Instant::now();
            if self.next > now {
                std::thread::sleep(self.next - now);
            }
            self.next += Duration::from_secs(1) / FPS;
            self.tick += 1;
            Ok(SourceEvent::Video(self.frame()))
        } else {
            Ok(SourceEvent::Audio(self.tone()))
        }
    }

    fn has_audio(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_frame(v: StreamVariant) -> Frame {
        let mut src = DemoFactory { seed: 3 }.open(v).unwrap();
        loop {
            if let SourceEvent::Video(f) = src.next_event().unwrap() {
                return f;
            }
        }
    }

    #[test]
    fn low_variant_is_360p() {
        let f = first_frame(StreamVariant::Low);
        assert_eq!((f.width, f.height), (640, 360));
        assert_eq!(f.rgba.len(), 640 * 360 * 4);
    }

    #[test]
    fn high_variant_is_720p() {
        let f = first_frame(StreamVariant::High);
        assert_eq!((f.width, f.height), (1280, 720));
    }

    #[test]
    fn demo_has_audio() {
        let mut src = DemoFactory { seed: 1 }.open(StreamVariant::Low).unwrap();
        assert!(src.has_audio());
        let got_audio = (0..4).any(|_| matches!(src.next_event(), Ok(SourceEvent::Audio(s)) if !s.is_empty()));
        assert!(got_audio);
    }
}
