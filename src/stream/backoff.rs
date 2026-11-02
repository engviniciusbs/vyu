use std::time::Duration;

const STEPS_SECS: [u64; 6] = [1, 2, 4, 8, 16, 30];

/// Reconnect delays: 1, 2, 4, 8, 16, then 30 s forever.
pub struct Backoff {
    attempt: usize,
}

impl Backoff {
    pub fn new() -> Self {
        Self { attempt: 0 }
    }

    pub fn next(&mut self) -> Duration {
        let secs = STEPS_SECS[self.attempt.min(STEPS_SECS.len() - 1)];
        self.attempt += 1;
        Duration::from_secs(secs)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_sequence() {
        let mut b = Backoff::new();
        let got: Vec<u64> = (0..7).map(|_| b.next().as_secs()).collect();
        assert_eq!(got, vec![1, 2, 4, 8, 16, 30, 30]);
        b.reset();
        assert_eq!(b.next(), Duration::from_secs(1));
    }
}
