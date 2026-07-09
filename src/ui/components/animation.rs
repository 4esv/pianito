//! Frame-driven animation state for the hero tuning screen (issue #32).
//!
//! Two independent pieces of testable logic, both advanced once per render
//! tick (issue #28's ~60fps loop) rather than once per raw pitch update
//! (~10Hz): [`NeedleTrail`] smooths the displayed cents value and keeps a
//! fading trail of recent positions, and [`LockAnimation`] drives the
//! flash-then-hold cue when a reading settles into the in-tune zone.

use std::collections::VecDeque;
use std::time::Duration;

/// Exponential smoothing rate: fraction of the remaining distance to the
/// target closed per second of wall-clock time. Framerate-independent (see
/// [`smooth_toward`]) so the animation reads the same at any tick cadence.
pub const SMOOTHING_RATE_PER_SEC: f32 = 10.0;

/// Number of past smoothed positions kept for the fading needle trail.
pub const TRAIL_LEN: usize = 6;

/// Move `current` a fraction of the remaining distance toward `target`,
/// scaled by `dt` so the same rate constant produces the same apparent
/// speed regardless of how often it's called. Standard framerate-
/// independent exponential smoothing: `1 - e^(-rate*dt)` is the fraction of
/// the gap closed this tick.
pub fn smooth_toward(current: f32, target: f32, dt: Duration, rate_per_sec: f32) -> f32 {
    let dt_secs = dt.as_secs_f32();
    if dt_secs <= 0.0 {
        return current;
    }
    let alpha = 1.0 - (-rate_per_sec * dt_secs).exp();
    current + (target - current) * alpha
}

/// Needle animation state carried across render ticks: an exponentially
/// smoothed cents position (so the displayed needle interpolates instead of
/// snapping to each raw ~10Hz reading), plus a bounded trail of recent
/// smoothed positions for the fading-trail effect.
pub struct NeedleTrail {
    smoothed: f32,
    positions: VecDeque<f32>,
}

impl NeedleTrail {
    /// A trail with no history, smoothed position starting at 0 cents.
    pub fn new() -> Self {
        Self {
            smoothed: 0.0,
            positions: VecDeque::with_capacity(TRAIL_LEN),
        }
    }

    /// Advance smoothing toward `target` by `dt`, then record the new
    /// position in the trail buffer, dropping the oldest once full.
    pub fn update(&mut self, target: f32, dt: Duration) {
        self.smoothed = smooth_toward(self.smoothed, target, dt, SMOOTHING_RATE_PER_SEC);
        if self.positions.len() >= TRAIL_LEN {
            self.positions.pop_front();
        }
        self.positions.push_back(self.smoothed);
    }

    /// The current smoothed cents position.
    pub fn smoothed(&self) -> f32 {
        self.smoothed
    }

    /// Trail positions oldest-first, each paired with a fade weight in
    /// `(0, 1]` where `1.0` is the newest (current) position and older
    /// entries fade toward 0.
    pub fn trail(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        let len = self.positions.len();
        self.positions
            .iter()
            .enumerate()
            .map(move |(i, &pos)| (pos, (i + 1) as f32 / len as f32))
    }
}

impl Default for NeedleTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// Phase of the lock-flash state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockPhase {
    /// Not in the in-tune zone.
    Idle,
    /// Just entered the in-tune zone; flashing for this many more frames
    /// (including the current one).
    Flashing(u8),
    /// Past the flash; steady as long as the reading stays in tune.
    Holding,
}

/// Lock-flash state machine (issue #32): enters on the first in-tune frame,
/// plays [`LockAnimation::FLASH_FRAMES`] frames of a flash/pulse, then holds
/// steady until the reading drifts back out of tolerance, at which point it
/// releases to idle (so re-entering the zone flashes again rather than
/// looking like a continuation).
pub struct LockAnimation {
    phase: LockPhase,
}

impl LockAnimation {
    /// Frames the in-tune zone flashes before settling into a steady hold.
    /// At the ~60fps render tick this is roughly 300ms - long enough to
    /// read as a deliberate pop, short enough to not overstay it.
    pub const FLASH_FRAMES: u8 = 18;

    /// Starts idle (not locked).
    pub fn new() -> Self {
        Self {
            phase: LockPhase::Idle,
        }
    }

    /// Advance the state machine by one render tick, given whether the
    /// current reading is within the in-tune zone this tick. Any
    /// out-of-tolerance reading releases immediately back to `Idle`,
    /// regardless of which phase it was in.
    pub fn update(&mut self, in_tune: bool) {
        if !in_tune {
            self.phase = LockPhase::Idle;
            return;
        }
        // Entering the zone counts as the flash's first frame, so
        // `FLASH_FRAMES` calls of `update(true)` starting from `Idle` land
        // exactly on `Holding` - not `FLASH_FRAMES + 1`.
        self.phase = match self.phase {
            LockPhase::Idle => LockPhase::Flashing(Self::FLASH_FRAMES.saturating_sub(1)),
            LockPhase::Flashing(n) if n <= 1 => LockPhase::Holding,
            LockPhase::Flashing(n) => LockPhase::Flashing(n - 1),
            LockPhase::Holding => LockPhase::Holding,
        };
    }

    /// True once the flash has started (flashing or holding) - the reading
    /// is currently considered "locked in".
    pub fn is_locked(&self) -> bool {
        !matches!(self.phase, LockPhase::Idle)
    }

    /// True only during the initial flash/pulse frames, not the steady hold
    /// that follows it.
    pub fn is_flashing(&self) -> bool {
        matches!(self.phase, LockPhase::Flashing(_))
    }
}

impl Default for LockAnimation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- smoothing: fractional movement toward the target --

    #[test]
    fn test_smooth_toward_moves_partway_not_all_the_way() {
        let next = smooth_toward(0.0, 10.0, Duration::from_millis(16), SMOOTHING_RATE_PER_SEC);
        assert!(next > 0.0, "must move toward the target: {next}");
        assert!(next < 10.0, "must not jump straight to the target: {next}");
    }

    #[test]
    fn test_smooth_toward_zero_dt_does_not_move() {
        let next = smooth_toward(0.0, 10.0, Duration::ZERO, SMOOTHING_RATE_PER_SEC);
        assert_eq!(next, 0.0);
    }

    #[test]
    fn test_smooth_toward_moves_correct_direction_when_target_is_below() {
        let next = smooth_toward(10.0, 0.0, Duration::from_millis(16), SMOOTHING_RATE_PER_SEC);
        assert!(next < 10.0);
        assert!(next > 0.0);
    }

    #[test]
    fn test_smooth_toward_converges_after_many_ticks() {
        let mut current = 0.0;
        for _ in 0..300 {
            current = smooth_toward(
                current,
                10.0,
                Duration::from_millis(16),
                SMOOTHING_RATE_PER_SEC,
            );
        }
        assert!(
            (current - 10.0).abs() < 0.01,
            "should have converged close to target: {current}"
        );
    }

    #[test]
    fn test_smooth_toward_larger_dt_moves_further() {
        let short = smooth_toward(0.0, 10.0, Duration::from_millis(16), SMOOTHING_RATE_PER_SEC);
        let long = smooth_toward(
            0.0,
            10.0,
            Duration::from_millis(160),
            SMOOTHING_RATE_PER_SEC,
        );
        assert!(long > short, "a longer dt should close more of the gap");
    }

    // -- needle trail: bounded ring buffer with fading weights --

    #[test]
    fn test_needle_trail_starts_at_zero_with_no_history() {
        let trail = NeedleTrail::new();
        assert_eq!(trail.smoothed(), 0.0);
        assert_eq!(trail.trail().count(), 0);
    }

    #[test]
    fn test_needle_trail_records_a_position_per_update() {
        let mut trail = NeedleTrail::new();
        trail.update(10.0, Duration::from_millis(16));
        trail.update(10.0, Duration::from_millis(16));
        assert_eq!(trail.trail().count(), 2);
    }

    #[test]
    fn test_needle_trail_is_bounded_to_trail_len() {
        let mut trail = NeedleTrail::new();
        for _ in 0..(TRAIL_LEN + 10) {
            trail.update(5.0, Duration::from_millis(16));
        }
        assert_eq!(trail.trail().count(), TRAIL_LEN);
    }

    #[test]
    fn test_needle_trail_drops_oldest_once_full() {
        let mut trail = NeedleTrail::new();
        // Push a falling sequence of smoothed positions using huge dt so
        // smoothing snaps straight to each target - makes the recorded
        // trail values distinguishable and easy to assert order on.
        for target in [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            trail.update(target, Duration::from_secs(10));
        }
        let positions: Vec<f32> = trail.trail().map(|(pos, _)| pos).collect();
        assert_eq!(positions.len(), TRAIL_LEN);
        // The oldest push (target 0.0) must have been dropped.
        assert!(!positions.contains(&0.0), "oldest entry must be evicted");
        assert_eq!(
            positions.first().copied(),
            Some(1.0),
            "oldest surviving is next"
        );
        assert_eq!(positions.last().copied(), Some(6.0), "newest is last");
    }

    #[test]
    fn test_needle_trail_newest_has_full_weight() {
        let mut trail = NeedleTrail::new();
        trail.update(1.0, Duration::from_secs(10));
        trail.update(2.0, Duration::from_secs(10));
        trail.update(3.0, Duration::from_secs(10));

        let weights: Vec<f32> = trail.trail().map(|(_, w)| w).collect();
        assert_eq!(
            *weights.last().unwrap(),
            1.0,
            "newest entry is fully opaque"
        );
    }

    #[test]
    fn test_needle_trail_weights_increase_with_recency() {
        let mut trail = NeedleTrail::new();
        for target in [1.0, 2.0, 3.0, 4.0] {
            trail.update(target, Duration::from_secs(10));
        }
        let weights: Vec<f32> = trail.trail().map(|(_, w)| w).collect();
        for pair in weights.windows(2) {
            assert!(
                pair[1] > pair[0],
                "weights must strictly increase: {weights:?}"
            );
        }
    }

    // -- lock animation: enters on in-tune, flashes N frames, holds, releases on drift --

    #[test]
    fn test_lock_animation_starts_idle() {
        let lock = LockAnimation::new();
        assert!(!lock.is_locked());
        assert!(!lock.is_flashing());
    }

    #[test]
    fn test_lock_animation_stays_idle_while_out_of_tune() {
        let mut lock = LockAnimation::new();
        for _ in 0..5 {
            lock.update(false);
        }
        assert!(!lock.is_locked());
    }

    #[test]
    fn test_lock_animation_enters_flashing_on_first_in_tune_frame() {
        let mut lock = LockAnimation::new();
        lock.update(true);
        assert!(lock.is_locked());
        assert!(lock.is_flashing());
    }

    #[test]
    fn test_lock_animation_flashes_then_holds() {
        let mut lock = LockAnimation::new();
        for _ in 0..LockAnimation::FLASH_FRAMES {
            lock.update(true);
        }
        assert!(lock.is_locked(), "must still be locked after the flash");
        assert!(!lock.is_flashing(), "flash must have ended");

        // Continues to hold on further in-tune frames.
        lock.update(true);
        assert!(lock.is_locked());
        assert!(!lock.is_flashing());
    }

    #[test]
    fn test_lock_animation_releases_on_drift_after_holding() {
        let mut lock = LockAnimation::new();
        for _ in 0..(LockAnimation::FLASH_FRAMES + 3) {
            lock.update(true);
        }
        assert!(lock.is_locked());

        lock.update(false);
        assert!(!lock.is_locked(), "drift must release the lock");
    }

    #[test]
    fn test_lock_animation_released_mid_flash_returns_to_idle() {
        let mut lock = LockAnimation::new();
        lock.update(true);
        assert!(lock.is_flashing());

        lock.update(false);
        assert!(!lock.is_locked());
        assert!(!lock.is_flashing());
    }

    #[test]
    fn test_lock_animation_re_entering_flashes_again() {
        let mut lock = LockAnimation::new();
        for _ in 0..(LockAnimation::FLASH_FRAMES + 3) {
            lock.update(true);
        }
        lock.update(false); // release
        lock.update(true); // re-enter

        assert!(
            lock.is_flashing(),
            "re-entering must flash again, not resume holding"
        );
    }
}
