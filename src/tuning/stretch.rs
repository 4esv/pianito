//! Stretch tuning (Railsback curve) for piano inharmonicity compensation.
//!
//! Piano strings exhibit inharmonicity - their overtones are slightly sharper
//! than perfect integer multiples of the fundamental. Professional piano tuning
//! compensates with "stretch tuning" where bass notes are tuned slightly flat
//! and treble notes slightly sharp.
//!
//! `StretchCurve` is plain data (a per-key cents table) plus builders that
//! populate it. `railsback_default()` builds the population-average curve
//! below; `from_offsets()` is the general constructor for any other source -
//! a fixed table, or (issue #23) 88 offsets computed from a piano's measured
//! inharmonicity. Consumers (`App::target_for_midi`) only ever call
//! `offset_cents()` / `apply()`, so adding builders is the entire surface
//! those issues need.
//!
//! `StretchMode` (issue #19) is the user-selectable surface over those
//! builders: `off` / `railsback` / `profile`, wired through CLI + config into
//! [`StretchMode::resolve`].

use serde::{Deserialize, Serialize};

use super::profile::PianoProfile;

/// User-selectable stretch application mode (CLI `--stretch` / config file
/// `stretch` key). Doc comments double as `clap` `--help` text for each
/// value, so keep them short and user-facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum StretchMode {
    /// No stretch: pure equal-temperament targets.
    Off,
    /// The built-in Railsback-inspired curve (same for every piano).
    #[default]
    Railsback,
    /// The per-piano curve measured from a loaded profile (falls back to
    /// Railsback when none is loaded).
    Profile,
}

// NOTE: `Profile` falls back to `Railsback` in `resolve` below rather than
// silently reverting to pure equal temperament when no profile is loaded
// yet. `StretchCurve::from_profile` is the literal measured-deviation table
// today; issue #23's fitted inharmonicity model replaces just that builder,
// not this enum or its CLI/config surface.

impl StretchMode {
    /// Resolve this mode into a concrete curve (or `None` for no stretch).
    /// `profile` is the currently loaded piano profile, if any; `Profile`
    /// mode without one falls back to the Railsback default rather than
    /// silently reverting to pure equal temperament.
    pub fn resolve(self, profile: Option<&PianoProfile>) -> Option<StretchCurve> {
        match self {
            StretchMode::Off => None,
            StretchMode::Railsback => Some(StretchCurve::railsback_default()),
            StretchMode::Profile => Some(match profile {
                Some(profile) => StretchCurve::from_profile(profile),
                None => StretchCurve::railsback_default(),
            }),
        }
    }
}

/// Stretch tuning curve: per-key cents offsets from equal temperament.
///
/// Backed by a plain `[f32; 88]` table so it can come from any source - the
/// built-in Railsback-inspired default, a fixed table, or (issue #23) offsets
/// fit from a piano's measured inharmonicity - through the same runtime
/// representation and the same `offset_cents()` / `apply()` call sites.
#[derive(Debug, Clone)]
pub struct StretchCurve {
    /// Stretch values in cents for each of the 88 keys.
    /// Index 0 = A0 (MIDI 21), Index 87 = C8 (MIDI 108)
    offsets: [f32; 88],
}

impl StretchCurve {
    /// Build a curve directly from a precomputed per-key cents table.
    /// Index 0 = A0 (MIDI 21), index 87 = C8 (MIDI 108).
    pub fn from_offsets(offsets: [f32; 88]) -> Self {
        Self { offsets }
    }

    /// The built-in Railsback-inspired default: a simplified model based on
    /// typical Railsback curves, identical for every piano (no measurement
    /// involved). Bass notes go progressively flat, the middle stays near
    /// the "temperament zone", and treble notes go progressively sharp.
    pub fn railsback_default() -> Self {
        Self::from_offsets(Self::generate_railsback_curve())
    }

    /// Build a curve from a loaded profile's measured per-key deviations
    /// (`ProfiledNote::cents`), i.e. the piano's own measured stretch rather
    /// than a generic population curve. Keys the profile never measured
    /// (skipped notes, or an incomplete profile) keep 0.0 (no stretch) at
    /// that index.
    pub fn from_profile(profile: &PianoProfile) -> Self {
        let mut offsets = [0.0_f32; 88];
        for (offset, note) in offsets.iter_mut().zip(profile.notes.iter()) {
            if let Some(note) = note {
                *offset = note.cents;
            }
        }
        Self::from_offsets(offsets)
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

    /// Generate the Railsback-inspired default table.
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
    /// Uses a sign-preserving quadratic curve (20 * x^2 * sign(x)):
    /// - A0 (21): approximately -15.7 cents
    /// - C4 (60): approximately 0 cents
    /// - C8 (108): approximately +23.8 cents
    ///
    /// NOTE: `center`/`range` are NOT symmetric around the 88-key span
    /// (MIDI 21-108, midpoint 64.5, half-span 43.5). `center = 60` (middle
    /// C) puts the curve's zero-crossing 4.5 semitones below the keyboard's
    /// true midpoint, and `range = 44` is a half-span measured from that
    /// off-center point rather than from the midpoint - despite the doc
    /// above once calling it "half the piano range". The net effect: A0
    /// reaches only x ~= -0.885 while C8 reaches x ~= 1.091, so the treble
    /// end is stretched more aggressively per semitone than the bass end.
    /// This is unchanged by the #18 refactor (data/builders only, no curve-
    /// shape change - see the characterization test below); a truly
    /// symmetric or measurement-driven curve is #23's job.
    fn calculate_stretch(midi: u8) -> f32 {
        // Center of the piano (around middle C)
        let center: f32 = 60.0;
        let range: f32 = 44.0; // see asymmetry NOTE above - not a true half-span

        // Normalized position: -1 at low end, 0 at center, +1 at high end
        let x = (midi as f32 - center) / range;

        // Sign-preserving quadratic: flat at center, steepens toward
        // extremes. This gives approximately:
        // - x = -0.89 (A0): stretch ≈ -15.7
        // - x = 0 (C4): stretch ≈ 0
        // - x = 1.09 (C8): stretch ≈ +23.8
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
        Self::railsback_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bass_is_flat() {
        let curve = StretchCurve::railsback_default();

        // A0 should be significantly flat
        let a0 = curve.offset_cents(21);
        assert!(a0 < -10.0, "A0 should be flat, got {} cents", a0);

        // C2 should be moderately flat
        let c2 = curve.offset_cents(36);
        assert!(c2 < 0.0, "C2 should be flat, got {} cents", c2);
    }

    #[test]
    fn test_treble_is_sharp() {
        let curve = StretchCurve::railsback_default();

        // C8 should be significantly sharp
        let c8 = curve.offset_cents(108);
        assert!(c8 > 10.0, "C8 should be sharp, got {} cents", c8);

        // C7 should be moderately sharp
        let c7 = curve.offset_cents(96);
        assert!(c7 > 0.0, "C7 should be sharp, got {} cents", c7);
    }

    #[test]
    fn test_middle_is_near_zero() {
        let curve = StretchCurve::railsback_default();

        // A4 should be close to 0
        let a4 = curve.offset_cents(69);
        assert!(a4.abs() < 3.0, "A4 should be near 0 cents, got {}", a4);

        // C4 should be close to 0
        let c4 = curve.offset_cents(60);
        assert!(c4.abs() < 3.0, "C4 should be near 0 cents, got {}", c4);
    }

    #[test]
    fn test_curve_is_monotonic() {
        let curve = StretchCurve::railsback_default();

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
        let curve = StretchCurve::railsback_default();

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
        let curve = StretchCurve::railsback_default();

        // Out of range should return 0
        assert_eq!(curve.offset_cents(20), 0.0);
        assert_eq!(curve.offset_cents(109), 0.0);
    }

    // NOTE: characterization test for issue #18 - pins the exact per-note
    // stretch offsets produced by the pre-refactor implementation, bit for
    // bit. This is the proof that the data/builder refactor changed no
    // observable behavior: it must stay green, unmodified in its expected
    // values, across the refactor commit (only the constructor call below is
    // renamed alongside the production call sites).
    #[test]
    fn test_railsback_offsets_characterization() {
        // Index 0 = A0 (MIDI 21) ... index 87 = C8 (MIDI 108). Captured from
        // StretchCurve::new() (pre-refactor name; now railsback_default()) via
        // f32::to_bits() for exact reproduction (avoids decimal-literal
        // rounding drift).
        // NOTE: `let`, not `const` - const `f32::from_bits` is only stable
        // since Rust 1.83, and this crate's MSRV floor is 1.82. Non-const
        // `f32::from_bits` has been stable since 1.20, so a runtime binding
        // keeps the exact-bits table while compiling on the declared MSRV.
        #[rustfmt::skip]
        let expected: [f32; 88] = [
            f32::from_bits(3246090155), f32::from_bits(3245256062), f32::from_bits(3244443631), f32::from_bits(3243652866), f32::from_bits(3242883766), f32::from_bits(3242136330), f32::from_bits(3241410560), f32::from_bits(3240706455),
            f32::from_bits(3240024013), f32::from_bits(3239363237), f32::from_bits(3238724127), f32::from_bits(3238106678), f32::from_bits(3237019108), f32::from_bits(3235870871), f32::from_bits(3234765967), f32::from_bits(3233704393),
            f32::from_bits(3232686147), f32::from_bits(3231711232), f32::from_bits(3230779645), f32::from_bits(3229891390), f32::from_bits(3228478846), f32::from_bits(3226875650), f32::from_bits(3225359114), f32::from_bits(3223929239),
            f32::from_bits(3222586021), f32::from_bits(3221329462), f32::from_bits(3219093655), f32::from_bits(3216927177), f32::from_bits(3214934016), f32::from_bits(3213114174), f32::from_bits(3210098434), f32::from_bits(3207152023),
            f32::from_bits(3204552246), f32::from_bits(3200149961), f32::from_bits(3196336958), f32::from_bits(3190374807), f32::from_bits(3183372745), f32::from_bits(3173597591), f32::from_bits(3156820375), f32::from_bits(0),
            f32::from_bits(1009336727), f32::from_bits(1026113943), f32::from_bits(1035889097), f32::from_bits(1042891159), f32::from_bits(1048853310), f32::from_bits(1052666313), f32::from_bits(1057068598), f32::from_bits(1059668375),
            f32::from_bits(1062614786), f32::from_bits(1065630526), f32::from_bits(1067450368), f32::from_bits(1069443529), f32::from_bits(1071610007), f32::from_bits(1073845814), f32::from_bits(1075102373), f32::from_bits(1076445591),
            f32::from_bits(1077875466), f32::from_bits(1079392002), f32::from_bits(1080995198), f32::from_bits(1082407742), f32::from_bits(1083295997), f32::from_bits(1084227584), f32::from_bits(1085202499), f32::from_bits(1086220745),
            f32::from_bits(1087282319), f32::from_bits(1088387223), f32::from_bits(1089535460), f32::from_bits(1090623030), f32::from_bits(1091240479), f32::from_bits(1091879589), f32::from_bits(1092540365), f32::from_bits(1093222807),
            f32::from_bits(1093926912), f32::from_bits(1094652682), f32::from_bits(1095400118), f32::from_bits(1096169218), f32::from_bits(1096959983), f32::from_bits(1097772414), f32::from_bits(1098606507), f32::from_bits(1099184958),
            f32::from_bits(1099623670), f32::from_bits(1100073213), f32::from_bits(1100533592), f32::from_bits(1101004800), f32::from_bits(1101486841), f32::from_bits(1101979715), f32::from_bits(1102483424), f32::from_bits(1102997961),
        ];

        let curve = StretchCurve::railsback_default();
        for (i, &expected) in expected.iter().enumerate() {
            let midi = (i + 21) as u8;
            let actual = curve.offset_cents(midi);
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "MIDI {} offset drifted: expected {} ({:#010x}), got {} ({:#010x})",
                midi,
                expected,
                expected.to_bits(),
                actual,
                actual.to_bits()
            );
        }

        // Spot-check readable landmarks against the issue's own numbers so a
        // future reader can sanity-check the bit table above at a glance.
        assert!((curve.offset_cents(21) - (-15.712_81)).abs() < 0.001); // A0
        assert!(curve.offset_cents(60).abs() < 0.001); // C4
        assert!((curve.offset_cents(69) - 0.836_776_85).abs() < 0.001); // A4
        assert!((curve.offset_cents(108) - 23.801_653).abs() < 0.001); // C8
    }

    #[test]
    fn test_stretch_magnitudes() {
        let curve = StretchCurve::railsback_default();

        // Verify approximate magnitudes match Railsback expectations
        let a0 = curve.offset_cents(21);
        assert!(
            (-25.0..=-10.0).contains(&a0),
            "A0 stretch {} out of expected range",
            a0
        );

        let c8 = curve.offset_cents(108);
        assert!(
            (10.0..=25.0).contains(&c8),
            "C8 stretch {} out of expected range",
            c8
        );
    }

    #[test]
    fn test_stretch_mode_defaults_to_railsback() {
        assert_eq!(StretchMode::default(), StretchMode::Railsback);
    }

    #[test]
    fn test_stretch_mode_off_resolves_to_none() {
        assert!(StretchMode::Off.resolve(None).is_none());
        // A loaded profile must not override an explicit `off`.
        assert!(StretchMode::Off
            .resolve(Some(&PianoProfile::new()))
            .is_none());
    }

    #[test]
    fn test_stretch_mode_railsback_resolves_to_railsback_curve() {
        let curve = StretchMode::Railsback.resolve(None).expect("some curve");
        assert_eq!(
            curve.offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_stretch_mode_profile_without_profile_falls_back_to_railsback() {
        let curve = StretchMode::Profile
            .resolve(None)
            .expect("falls back to some curve");
        assert_eq!(
            curve.offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_stretch_mode_profile_with_profile_uses_measured_curve() {
        let mut profile = PianoProfile::new();
        profile.record_note(21, 27.0, -12.5); // A0, measured 12.5 cents flat

        let curve = StretchMode::Profile
            .resolve(Some(&profile))
            .expect("some curve");
        assert_eq!(curve.offset_cents(21), -12.5);
        // Unmeasured keys stay at 0 (no stretch), not the Railsback value.
        assert_eq!(curve.offset_cents(108), 0.0);
    }

    #[test]
    fn test_from_profile_uses_measured_cents_and_zero_for_unmeasured_keys() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 440.0, 3.25); // A4

        let curve = StretchCurve::from_profile(&profile);
        assert_eq!(curve.offset_cents(69), 3.25);
        assert_eq!(curve.offset_cents(21), 0.0);
        assert_eq!(curve.offset_cents(108), 0.0);
    }
}
