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
// yet - and again when a loaded profile can't be fit (a legacy profile with
// no partials, so `StretchCurve::from_profile` errors). Issue #23's fitted
// inharmonicity model lives entirely behind that builder; this enum and its
// CLI/config surface are untouched.

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
                // A profile we can fit yields the per-piano curve; one we
                // can't (legacy, no partials) falls back to Railsback rather
                // than failing the resolve.
                Some(profile) => StretchCurve::from_profile(profile)
                    .unwrap_or_else(|_| StretchCurve::railsback_default()),
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

    /// Build a per-piano curve from a loaded profile by fitting each note's
    /// inharmonicity coefficient `B` from its recorded partials and deriving
    /// the stretch that keeps every octave's coincident partials beatless
    /// (issue #23's [`crate::tuning::inharmonicity`] engine). This is the
    /// physically-grounded curve for *this* instrument rather than a
    /// population average.
    ///
    /// Errors when the profile has too few notes with usable partials to fit
    /// a curve (a legacy profile saved before partials existed, or one where
    /// nothing fit); callers fall back to the Railsback default.
    pub fn from_profile(profile: &PianoProfile) -> anyhow::Result<Self> {
        let offsets = crate::tuning::inharmonicity::stretch_from_profile(profile)?;
        Ok(Self::from_offsets(offsets))
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
    ///
    /// The raw [`Self::calculate_stretch`] quadratic has its zero at C4
    /// (MIDI 60), but consumers treat A4 (MIDI 69) as the reference pitch:
    /// `offset_cents(69)` is the cents offset applied on top of the
    /// configured A4. Anchoring the default at C4 would leave A4 at
    /// +0.84 cents, so with `--a4 430` the actual A4 target would be
    /// 430.21 Hz (issue #88). Subtract `calculate_stretch(69)` from every
    /// entry - a pure vertical shift, so the curve's shape (the per-semitone
    /// deltas) is unchanged and only the zero-anchor moves from C4 to A4.
    fn generate_railsback_curve() -> [f32; 88] {
        let anchor = Self::calculate_stretch(69);
        let mut offsets = [0.0_f32; 88];

        for (i, offset) in offsets.iter_mut().enumerate() {
            let midi = (i + 21) as u8;
            *offset = Self::calculate_stretch(midi) - anchor;
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
    /// These are the *un-anchored* values; [`Self::generate_railsback_curve`]
    /// subtracts this function's A4 value from every entry so the built
    /// default reads 0.0 cents at A4.
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
    use crate::audio::Partial;

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

    // NOTE: characterization test for the Railsback default's exact per-note
    // stretch offsets, bit for bit. It was introduced for issue #18 to prove
    // the data/builder refactor changed no observable behavior; issue #88 then
    // re-anchored the curve at A4 (a pure vertical shift of
    // `calculate_stretch(69)`), so the table below pins the *anchored* values.
    // Any further drift must be intentional and this table updated with it.
    #[test]
    fn test_railsback_offsets_characterization() {
        // Index 0 = A0 (MIDI 21) ... index 87 = C8 (MIDI 108). Captured from
        // StretchCurve::railsback_default() via f32::to_bits() for exact
        // reproduction (avoids decimal-literal rounding drift).
        // NOTE: `let`, not `const` - const `f32::from_bits` is only stable
        // since Rust 1.83, and this crate's MSRV floor is 1.82. Non-const
        // `f32::from_bits` has been stable since 1.20, so a runtime binding
        // keeps the exact-bits table while compiling on the declared MSRV.
        #[rustfmt::skip]
        let expected: [f32; 88] = [
            f32::from_bits(3246679438), f32::from_bits(3246133486), f32::from_bits(3245321055), f32::from_bits(3244530290), f32::from_bits(3243761190), f32::from_bits(3243013754), f32::from_bits(3242287984), f32::from_bits(3241583879),
            f32::from_bits(3240901437), f32::from_bits(3240240661), f32::from_bits(3239601551), f32::from_bits(3238984102), f32::from_bits(3238388322), f32::from_bits(3237625719), f32::from_bits(3236520815), f32::from_bits(3235459241),
            f32::from_bits(3234440995), f32::from_bits(3233466080), f32::from_bits(3232534493), f32::from_bits(3231646238), f32::from_bits(3230801311), f32::from_bits(3229999713), f32::from_bits(3228868810), f32::from_bits(3227438936),
            f32::from_bits(3226095718), f32::from_bits(3224839158), f32::from_bits(3223669260), f32::from_bits(3222586021), f32::from_bits(3221589440), f32::from_bits(3220133567), f32::from_bits(3218487042), f32::from_bits(3217013836),
            f32::from_bits(3215713948), f32::from_bits(3214587379), f32::from_bits(3213634128), f32::from_bits(3212854196), f32::from_bits(3211658299), f32::from_bits(3210791707), f32::from_bits(3210271752), f32::from_bits(3210098434),
            f32::from_bits(3209925116), f32::from_bits(3209405161), f32::from_bits(3208538569), f32::from_bits(3207325340), f32::from_bits(3205765475), f32::from_bits(3203269691), f32::from_bits(3198763416), f32::from_bits(3191068076),
            f32::from_bits(0), f32::from_bits(1044970984), f32::from_bits(1054052860), f32::from_bits(1059495056), f32::from_bits(1063828012), f32::from_bits(1066930411), f32::from_bits(1069443529), f32::from_bits(1072129965),
            f32::from_bits(1074365770), f32::from_bits(1075882306), f32::from_bits(1077485502), f32::from_bits(1079175356), f32::from_bits(1080951866), f32::from_bits(1082472736), f32::from_bits(1083447651), f32::from_bits(1084465897),
            f32::from_bits(1085527471), f32::from_bits(1086632375), f32::from_bits(1087780612), f32::from_bits(1088972172), f32::from_bits(1090207070), f32::from_bits(1091002165), f32::from_bits(1091662941), f32::from_bits(1092345383),
            f32::from_bits(1093049488), f32::from_bits(1093775258), f32::from_bits(1094522694), f32::from_bits(1095291794), f32::from_bits(1096082559), f32::from_bits(1096894990), f32::from_bits(1097729083), f32::from_bits(1098584844),
            f32::from_bits(1099184958), f32::from_bits(1099634501), f32::from_bits(1100094880), f32::from_bits(1100566088), f32::from_bits(1101048129), f32::from_bits(1101541003), f32::from_bits(1102044712), f32::from_bits(1102559249),
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

        // Spot-check readable landmarks so a future reader can sanity-check
        // the bit table above at a glance. Note C4 now sits 0.84 cents flat
        // of A4: the curve is anchored at A4, not C4 (issue #88).
        assert!((curve.offset_cents(21) - (-16.549_587)).abs() < 0.001); // A0
        assert!((curve.offset_cents(60) - (-0.836_776_85)).abs() < 0.001); // C4
        assert!(curve.offset_cents(69).abs() < 0.001, "A4 anchored"); // A4
        assert!((curve.offset_cents(108) - 22.964_876).abs() < 0.001); // C8
    }

    #[test]
    fn test_railsback_default_is_anchored_at_a4() {
        // Issue #88: the default curve must read 0.0 cents at A4 (MIDI 69),
        // matching how the per-piano profile curve is anchored - consumers
        // apply this offset on top of the configured A4, so a non-zero A4
        // here biases every target.
        let curve = StretchCurve::railsback_default();
        assert!(curve.offset_cents(69).abs() < 0.001, "A4 anchored");
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

    /// Partial stack of a stiff string with a known inharmonicity `B`.
    fn inharmonic_partials(f0: f32, b: f32) -> Vec<Partial> {
        (1..=6u16)
            .map(|n| {
                let nf = n as f32;
                Partial {
                    n,
                    freq_hz: nf * f0 * (1.0 + b * nf * nf).sqrt(),
                    amplitude: 1.0 / nf,
                }
            })
            .collect()
    }

    /// A full 88-key profile carrying the partials of a piano with mild,
    /// realistic inharmonicity (higher toward the extremes).
    fn profile_with_inharmonicity() -> PianoProfile {
        let mut profile = PianoProfile::new();
        for i in 0..88u8 {
            let midi = i + 21;
            let bass = ((69.0 - midi as f32) / 48.0).max(0.0);
            let treble = ((midi as f32 - 69.0) / 39.0).max(0.0);
            let b = 0.0002 + 0.0006 * bass * bass + 0.0006 * treble * treble;
            let f0 = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
            profile.record_note_with_partials(midi, f0, 0.0, inharmonic_partials(f0, b));
        }
        profile
    }

    #[test]
    fn test_stretch_mode_profile_with_partials_uses_fitted_curve() {
        let profile = profile_with_inharmonicity();
        let curve = StretchMode::Profile
            .resolve(Some(&profile))
            .expect("some curve");

        // The fitted per-piano curve is bass-flat, treble-sharp, anchored at A4.
        assert!(curve.offset_cents(21) < 0.0, "A0 flat");
        assert!(curve.offset_cents(108) > 0.0, "C8 sharp");
        assert!(curve.offset_cents(69).abs() < 0.001, "A4 anchored");
        // ...and it is the fitted curve, not the Railsback fallback.
        assert_ne!(
            curve.offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_stretch_mode_profile_without_partials_falls_back_to_railsback() {
        // A legacy profile (measured cents but no partials) can't be fit, so
        // Profile mode falls back to the Railsback default rather than 0.
        let mut profile = PianoProfile::new();
        profile.record_note(21, 27.0, -12.5); // no partials

        let curve = StretchMode::Profile
            .resolve(Some(&profile))
            .expect("some curve");
        assert_eq!(
            curve.offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_from_profile_errors_without_partials() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 440.0, 3.25); // A4, no partials
        assert!(StretchCurve::from_profile(&profile).is_err());
    }

    #[test]
    fn test_from_profile_with_partials_yields_monotonic_curve() {
        let profile = profile_with_inharmonicity();
        let curve = StretchCurve::from_profile(&profile).expect("fits a curve");

        let mut prev = curve.offset_cents(21);
        for midi in 22..=108 {
            let current = curve.offset_cents(midi);
            assert!(
                current >= prev - 1e-3,
                "curve should rise across the keyboard: MIDI {midi} {current} < {prev}"
            );
            prev = current;
        }
    }
}
