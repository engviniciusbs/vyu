use std::time::{Duration, Instant};

use super::StreamVariant;

/// Tile height (physical px) above which we switch to the high-quality stream.
const UP_PX: f32 = 540.0;
/// Below this we go back to the low-quality stream (hysteresis band).
const DOWN_PX: f32 = 400.0;
/// A new choice must hold this long before we act on it.
const STABLE: Duration = Duration::from_secs(1);

/// Chooses `stream1`/`stream2` from the tile size, without flapping.
pub struct AutoQuality {
    current: StreamVariant,
    candidate: Option<(StreamVariant, Instant)>,
}

impl AutoQuality {
    pub fn new() -> Self {
        Self { current: StreamVariant::Low, candidate: None }
    }

    pub fn update(&mut self, tile_h_px: f32, now: Instant) -> StreamVariant {
        let desired = match self.current {
            StreamVariant::Low if tile_h_px > UP_PX => StreamVariant::High,
            StreamVariant::High if tile_h_px < DOWN_PX => StreamVariant::Low,
            cur => cur,
        };
        if desired == self.current {
            self.candidate = None;
        } else {
            match self.candidate {
                Some((v, since)) if v == desired => {
                    if now.duration_since(since) >= STABLE {
                        self.current = desired;
                        self.candidate = None;
                    }
                }
                _ => self.candidate = Some((desired, now)),
            }
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_quality_hysteresis() {
        let t0 = Instant::now();
        let s = |secs: f32| t0 + Duration::from_secs_f32(secs);
        let mut q = AutoQuality::new();
        assert_eq!(q.update(600.0, s(0.0)), StreamVariant::Low);
        assert_eq!(q.update(600.0, s(1.0)), StreamVariant::High);
        assert_eq!(q.update(450.0, s(2.0)), StreamVariant::High);
        assert_eq!(q.update(350.0, s(3.0)), StreamVariant::High);
        assert_eq!(q.update(350.0, s(4.0)), StreamVariant::Low);
        assert_eq!(q.update(600.0, s(5.0)), StreamVariant::Low);
        assert_eq!(q.update(600.0, s(5.5)), StreamVariant::Low);
    }

    #[test]
    fn flicker_resets_stability_timer() {
        let t0 = Instant::now();
        let s = |secs: f32| t0 + Duration::from_secs_f32(secs);
        let mut q = AutoQuality::new();
        q.update(600.0, s(0.0));
        q.update(300.0, s(0.6)); // back below: candidate dropped
        assert_eq!(q.update(600.0, s(1.1)), StreamVariant::Low);
        assert_eq!(q.update(600.0, s(2.2)), StreamVariant::High);
    }
}
