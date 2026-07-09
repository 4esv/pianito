//! Silence watchdog: detects a dead microphone input (issue #34).
//!
//! On macOS, a TCC mic-permission denial typically delivers a stream of
//! silent all-zero samples rather than a `cpal` error — `MicCapture` reports
//! success and keeps "reading", so nothing in the existing error-reporting
//! path (`take_error`) ever fires. This tracks wall-clock time since the
//! last read that actually carried signal, independent of whether a pitch
//! was *detected* (a quiet room between strikes is not a dead mic).

use std::time::{Duration, Instant};

/// Below this amplitude a sample is treated as exact digital silence rather
/// than quiet signal, guarding against denormal/rounding noise while still
/// catching the literal all-zero frames a TCC denial produces.
const SIGNAL_EPSILON: f32 = 1e-6;

/// Whether `samples` carries any real signal (as opposed to a frame of
/// exact/near-zero silence).
pub fn has_signal(samples: &[f32]) -> bool {
    samples.iter().any(|s| s.abs() > SIGNAL_EPSILON)
}

/// Tracks time since the last mic read that carried real signal, and
/// reports whether the configured timeout has elapsed. Pure state machine —
/// takes `Instant` explicitly rather than reading the clock itself, so
/// timeout behavior is deterministic under test.
pub struct SilenceWatchdog {
    timeout: Duration,
    last_signal_at: Instant,
}

impl SilenceWatchdog {
    /// Idle period before the dead-mic hint is surfaced (issue #34: "~10s").
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

    /// Start the watchdog as of `now` (treated as the most recent signal,
    /// so a session doesn't immediately warn before the mic has had a
    /// chance to produce its first read).
    pub fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            timeout,
            last_signal_at: now,
        }
    }

    /// Record a mic read. `has_signal` resets the idle timer; a read of
    /// silence (or no read at all) leaves it running.
    pub fn observe(&mut self, has_signal: bool, now: Instant) {
        if has_signal {
            self.last_signal_at = now;
        }
    }

    /// Whether the timeout has elapsed since the last signal-bearing read.
    pub fn is_idle(&self, now: Instant) -> bool {
        now.duration_since(self.last_signal_at) >= self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_signal_false_for_all_zero_samples() {
        assert!(!has_signal(&[0.0, 0.0, 0.0]));
    }

    #[test]
    fn test_has_signal_true_when_any_sample_nonzero() {
        assert!(has_signal(&[0.0, 0.0, 0.0002, 0.0]));
    }

    #[test]
    fn test_has_signal_false_for_empty_slice() {
        assert!(!has_signal(&[]));
    }

    #[test]
    fn test_not_idle_before_timeout_elapses() {
        let t0 = Instant::now();
        let watchdog = SilenceWatchdog::new(Duration::from_secs(10), t0);

        assert!(!watchdog.is_idle(t0 + Duration::from_secs(9)));
    }

    #[test]
    fn test_idle_once_timeout_elapses() {
        let t0 = Instant::now();
        let watchdog = SilenceWatchdog::new(Duration::from_secs(10), t0);

        assert!(watchdog.is_idle(t0 + Duration::from_secs(10)));
        assert!(watchdog.is_idle(t0 + Duration::from_secs(30)));
    }

    #[test]
    fn test_signal_resets_the_idle_timer() {
        let t0 = Instant::now();
        let mut watchdog = SilenceWatchdog::new(Duration::from_secs(10), t0);

        let t1 = t0 + Duration::from_secs(9);
        watchdog.observe(true, t1);

        // Still within 10s of t1, not t0.
        assert!(!watchdog.is_idle(t1 + Duration::from_secs(9)));
        assert!(watchdog.is_idle(t1 + Duration::from_secs(10)));
    }

    #[test]
    fn test_silent_reads_do_not_reset_the_timer() {
        let t0 = Instant::now();
        let mut watchdog = SilenceWatchdog::new(Duration::from_secs(10), t0);

        watchdog.observe(false, t0 + Duration::from_secs(5));
        assert!(watchdog.is_idle(t0 + Duration::from_secs(10)));
    }
}
