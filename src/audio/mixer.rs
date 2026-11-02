use std::collections::HashMap;

use uuid::Uuid;

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapRb};

use crate::stream::session::AudioSink;

/// Mixer sample rate (mono).
pub const RATE: usize = 48_000;
/// Each camera can buffer up to 0.5 s.
const CAPACITY: usize = RATE / 2;
/// Keep at most 250 ms queued so audio stays close to live.
const MAX_BACKLOG: usize = RATE / 4;

struct Track {
    cons: HeapCons<f32>,
    /// 0 when muted.
    gain: f32,
}

/// Sums the audio of all cameras, each with its own volume.
pub struct Mixer {
    tracks: HashMap<Uuid, Track>,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new() -> Self {
        Self { tracks: HashMap::new(), scratch: Vec::new() }
    }

    /// Registers a camera and returns the sink its session writes samples into.
    /// New sources start muted.
    pub fn add_source(&mut self, id: Uuid) -> AudioSink {
        let (mut prod, cons) = HeapRb::<f32>::new(CAPACITY).split();
        self.tracks.insert(id, Track { cons, gain: 0.0 });
        Box::new(move |samples: &[f32]| {
            // Overflow means nobody is listening fast enough: drop the excess.
            prod.push_slice(samples);
        })
    }

    pub fn remove_source(&mut self, id: Uuid) {
        self.tracks.remove(&id);
    }

    pub fn set_gain(&mut self, id: Uuid, volume: f32, muted: bool) {
        if let Some(t) = self.tracks.get_mut(&id) {
            t.gain = if muted { 0.0 } else { volume.clamp(0.0, 1.0) };
        }
    }

    pub fn buffered(&self, id: Uuid) -> usize {
        self.tracks.get(&id).map_or(0, |t| t.cons.occupied_len())
    }

    /// Fills `out` with the mix (mono, 48 kHz), clamped to [-1, 1].
    pub fn fill(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        self.scratch.resize(out.len(), 0.0);
        for track in self.tracks.values_mut() {
            let queued = track.cons.occupied_len();
            if queued > MAX_BACKLOG {
                track.cons.skip(queued - MAX_BACKLOG);
            }
            let n = track.cons.pop_slice(&mut self.scratch);
            if track.gain == 0.0 {
                continue;
            }
            for (o, s) in out.iter_mut().zip(&self.scratch[..n]) {
                *o += s * track.gain;
            }
        }
        for o in out.iter_mut() {
            *o = o.clamp(-1.0, 1.0);
        }
    }
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixer_with(n: usize) -> (Mixer, Vec<(Uuid, AudioSink)>) {
        let mut m = Mixer::new();
        let sinks = (0..n)
            .map(|_| {
                let id = Uuid::new_v4();
                let sink = m.add_source(id);
                (id, sink)
            })
            .collect();
        (m, sinks)
    }

    #[test]
    fn muted_source_is_silent() {
        let (mut m, mut s) = mixer_with(1);
        (s[0].1)(&[0.7; 4]);
        let mut out = [9.0; 4];
        m.fill(&mut out);
        assert_eq!(out, [0.0; 4]);
    }

    #[test]
    fn volume_scales() {
        let (mut m, mut s) = mixer_with(1);
        m.set_gain(s[0].0, 0.5, false);
        (s[0].1)(&[1.0]);
        let mut out = [0.0; 1];
        m.fill(&mut out);
        assert_eq!(out, [0.5]);
    }

    #[test]
    fn sums_sources() {
        let (mut m, mut s) = mixer_with(2);
        for (id, _) in &s {
            m.set_gain(*id, 1.0, false);
        }
        (s[0].1)(&[0.3]);
        (s[1].1)(&[0.4]);
        let mut out = [0.0; 1];
        m.fill(&mut out);
        assert!((out[0] - 0.7f32).abs() < 1e-6);
    }

    #[test]
    fn clips_to_unit() {
        let (mut m, mut s) = mixer_with(2);
        for (id, _) in &s {
            m.set_gain(*id, 1.0, false);
        }
        (s[0].1)(&[0.8, -0.8]);
        (s[1].1)(&[0.8, -0.8]);
        let mut out = [0.0; 2];
        m.fill(&mut out);
        assert_eq!(out, [1.0, -1.0]);
    }

    #[test]
    fn missing_source_is_silence() {
        let (mut m, s) = mixer_with(1);
        m.set_gain(s[0].0, 1.0, false);
        let mut out = [5.0; 8];
        m.fill(&mut out);
        assert_eq!(out, [0.0; 8]);
    }

    #[test]
    fn drops_when_too_far_behind() {
        let (mut m, mut s) = mixer_with(1);
        let id = s[0].0;
        (s[0].1)(&vec![0.1; RATE * 4 / 10]); // 0.4 s
        let mut out = vec![0.0; RATE / 100]; // 10 ms
        m.fill(&mut out);
        assert!(m.buffered(id) <= RATE / 4, "buffered = {}", m.buffered(id));
    }

    #[test]
    fn removed_source_stops_contributing() {
        let (mut m, mut s) = mixer_with(1);
        let id = s[0].0;
        m.set_gain(id, 1.0, false);
        (s[0].1)(&[0.5]);
        m.remove_source(id);
        let mut out = [0.0; 1];
        m.fill(&mut out);
        assert_eq!(out, [0.0]);
    }
}
