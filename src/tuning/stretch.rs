//! Stretch tuning (Railsback curve) for piano inharmonicity compensation.
//!
//! Piano strings exhibit inharmonicity - their overtones are slightly sharper
//! than perfect integer multiples of the fundamental. Professional piano tuning
//! compensates with "stretch tuning" where bass notes are tuned slightly flat
//! and treble notes slightly sharp.

/// Stretch tuning curve based on the Railsback curve.
///
/// The Railsback curve is an empirical curve showing how piano tuners
/// deviate from equal temperament to achieve the most pleasing sound.
#[derive(Debug, Clone)]
pub struct StretchCurve {
    /// Stretch values in cents for each of the 88 keys.
    /// Index 0 = A0 (MIDI 21), Index 87 = C8 (MIDI 108)
    offsets: [f32; 88],
}

impl StretchCurve {
    /// Create a new stretch curve with default Railsback-inspired values.
    pub fn new() -> Self {
        Self {
            offsets: Self::generate_railsback_curve(),
        }
    }

    /// Get the stretch offset in cents for a given MIDI note.
    /// Positive values = tune sharp, negative = tune flat.
    pub fn offset_cents(&self, midi_note: u8) -> f32 {
        if !(21..=108).contains(&midi_note) {
            return 0.0;
        }
        self.offsets[(midi_note - 21) as usize]
    }

    /// Get the stretch offset for a note by index (0-87).
    pub fn offset_cents_by_index(&self, index: usize) -> f32 {
        self.offsets.get(index).copied().unwrap_or(0.0)
    }

    /// Generate a simplified Railsback-style stretch curve.
    ///
    /// This is a simplified model based on typical Railsback curves:
    /// - Bass notes (A0-C3): progressively flat, up to -20 cents at A0
    /// - Middle octaves (C3-F5): close to 0, the "temperament zone"
    /// - Treble notes (F5-C8): progressively sharp, up to +20 cents at C8
    fn generate_railsback_curve() -> [f32; 88] {
        let mut offsets = [0.0_f32; 88];

        for (i, offset) in offsets.iter_mut().enumerate() {
            let midi = (i + 21) as u8;
            *offset = Self::calculate_stretch(midi);
        }

        offsets
    }

    /// Calculate stretch for a single note.
    ///
    /// Uses a smooth cubic curve across the entire range:
    /// - A0 (21): approximately -20 cents
    /// - C4 (60): approximately 0 cents
    /// - C8 (108): approximately +20 cents
    fn calculate_stretch(midi: u8) -> f32 {
        // Center of the piano (around middle C)
        let center: f32 = 60.0;
        let range: f32 = 44.0; // Half the piano range

        // Normalized position: -1 at low end, 0 at center, +1 at high end
        let x = (midi as f32 - center) / range;

        // Cubic function: starts flat at center, steepens toward extremes
        // This gives approximately:
        // - x = -0.89 (A0): stretch ≈ -20
        // - x = 0 (C4): stretch ≈ 0
        // - x = 1.09 (C8): stretch ≈ +20
        20.0 * x * x * x.signum()
    }

    /// Apply stretch to a base frequency.
    pub fn apply(&self, base_frequency: f32, midi_note: u8) -> f32 {
        let cents_offset = self.offset_cents(midi_note);
        base_frequency * 2.0_f32.powf(cents_offset / 1200.0)
    }
}

impl Default for StretchCurve {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bass_is_flat() {
        let curve = StretchCurve::new();

        // A0 should be significantly flat
        let a0 = curve.offset_cents(21);
        assert!(a0 < -10.0, "A0 should be flat, got {} cents", a0);

        // C2 should be moderately flat
        let c2 = curve.offset_cents(36);
        assert!(c2 < 0.0, "C2 should be flat, got {} cents", c2);
    }

    #[test]
    fn test_treble_is_sharp() {
        let curve = StretchCurve::new();

        // C8 should be significantly sharp
        let c8 = curve.offset_cents(108);
        assert!(c8 > 10.0, "C8 should be sharp, got {} cents", c8);

        // C7 should be moderately sharp
        let c7 = curve.offset_cents(96);
        assert!(c7 > 0.0, "C7 should be sharp, got {} cents", c7);
    }

    #[test]
    fn test_middle_is_near_zero() {
        let curve = StretchCurve::new();

        // A4 should be close to 0
        let a4 = curve.offset_cents(69);
        assert!(a4.abs() < 3.0, "A4 should be near 0 cents, got {}", a4);

        // C4 should be close to 0
        let c4 = curve.offset_cents(60);
        assert!(c4.abs() < 3.0, "C4 should be near 0 cents, got {}", c4);
    }

    #[test]
    fn test_curve_is_monotonic() {
        let curve = StretchCurve::new();

        // The entire curve should be monotonically increasing
        let mut prev = curve.offset_cents(21);
        for midi in 22..=108 {
            let current = curve.offset_cents(midi);
            assert!(
                current >= prev,
                "Curve should be monotonic: MIDI {} ({:.2}) < MIDI {} ({:.2})",
                midi,
                current,
                midi - 1,
                prev
            );
            prev = current;
        }
    }

    #[test]
    fn test_apply_stretch() {
        let curve = StretchCurve::new();

        // A4 at 440Hz with minimal stretch should stay near 440
        let stretched = curve.apply(440.0, 69);
        let deviation = (stretched - 440.0).abs();
        assert!(
            deviation < 1.0,
            "A4 stretch should be minimal, got {} Hz deviation",
            deviation
        );

        // A0 at 27.5Hz with negative stretch should be slightly lower
        let base = 27.5;
        let stretched = curve.apply(base, 21);
        assert!(
            stretched < base,
            "A0 should be stretched flat: {} < {}",
            stretched,
            base
        );

        // C8 at 4186Hz with positive stretch should be slightly higher
        let base = 4186.0;
        let stretched = curve.apply(base, 108);
        assert!(
            stretched > base,
            "C8 should be stretched sharp: {} > {}",
            stretched,
            base
        );
    }

    #[test]
    fn test_bounds_checking() {
        let curve = StretchCurve::new();

        // Out of range should return 0
        assert_eq!(curve.offset_cents(20), 0.0);
        assert_eq!(curve.offset_cents(109), 0.0);
    }

    #[test]
    fn test_stretch_magnitudes() {
        let curve = StretchCurve::new();

        // Verify approximate magnitudes match Railsback expectations
        let a0 = curve.offset_cents(21);
        assert!(
            a0 >= -25.0 && a0 <= -10.0,
            "A0 stretch {} out of expected range",
            a0
        );

        let c8 = curve.offset_cents(108);
        assert!(
            c8 >= 10.0 && c8 <= 25.0,
            "C8 stretch {} out of expected range",
            c8
        );
    }
}
