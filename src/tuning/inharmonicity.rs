//! Per-piano inharmonicity engine (issue #23): the physically-grounded
//! replacement for the placeholder `StretchCurve::from_profile`.
//!
//! A real piano string is stiff, so its partials are stretched sharp of exact
//! integer multiples of the fundamental:
//!
//! ```text
//! f_n = n * f0 * sqrt(1 + B * n^2)
//! ```
//!
//! `B` (the inharmonicity coefficient) is what makes stretch tuning necessary
//! and what distinguishes one piano from the population average. This module:
//!
//! 1. Fits `B` per measured note from that note's recorded partials
//!    ([`fit_note_inharmonicity`]) — *robustly*, because the deepest bass
//!    notes can carry a couple of mislocated high-order partials from the
//!    `B = 0` harmonic search (issue #22), and a naive least-squares over the
//!    raw partial list would be corrupted by them.
//! 2. Smooths `log(B)` across the keyboard ([`smooth_log_b`]) — `B` spans
//!    orders of magnitude and the bass-bridge break is a regional feature, so
//!    a local kernel in log space beats one global polynomial.
//! 3. Derives the 88 per-key stretch offsets ([`stretch_offsets_from_b`]) by
//!    widening each octave just enough to still the beats of its coincident
//!    partial pairs (2:1 / 4:2 / 6:3), integrated octave-by-octave outward
//!    from A4.

use anyhow::{anyhow, Result};

use crate::audio::Partial;

use super::notes::NOTE_COUNT;
use super::profile::PianoProfile;

/// Index of A4 (MIDI 69) in the 0..88 keyboard array. The stretch curve is
/// anchored here (offset 0) and integrated outward.
const A4_INDEX: usize = 48;

/// Fewest partials that make the `(f0, B)` line fit well-posed.
const MIN_PARTIALS_FOR_FIT: usize = 2;

/// Never trim the partial set below this many points: a two-point fit is
/// exact (zero residual) and would happily "explain" any leftover pair, so we
/// keep enough spread that `B` stays pinned by real data, not by whichever two
/// peaks survived.
const MIN_PARTIALS_AFTER_TRIM: usize = 3;

/// A partial whose measured frequency lands more than this many cents from the
/// current model is treated as mislocated and dropped before refitting. A
/// clean partial sits within a couple of cents; a bass partial the `B = 0`
/// search snapped onto the wrong peak is off by tens to hundreds of cents, so
/// this threshold separates them cleanly without discarding honest noise.
const OUTLIER_CENTS: f32 = 25.0;

/// Fewest notes with a usable `B` fit needed to define a per-piano curve.
/// Below this (a legacy profile with no partials, or one where nothing fit)
/// [`stretch_from_profile`] errors so the caller can fall back to the
/// population-average Railsback curve.
const MIN_FITTED_NOTES: usize = 3;

/// Gaussian smoothing bandwidth (in semitones) for `log(B)` across the
/// keyboard. Wide enough to reject per-note fit noise, narrow enough that the
/// bass-bridge break stays a break rather than being averaged into the tenor.
const SMOOTH_SIGMA_SEMITONES: f64 = 4.0;

/// Floor applied to `B` before taking its logarithm, so a note that fits an
/// essentially harmonic `B ~= 0` can't produce `ln(0) = -inf`.
const B_FLOOR: f64 = 1e-6;

/// Coincident partial pairs used to set each octave's width, as
/// `(j, weight)` where the pair is partial `2j` of the lower note against
/// partial `j` of the upper (j=1 -> 2:1, j=2 -> 4:2, j=3 -> 6:3). Lower pairs
/// beat more audibly and are measured more reliably, so they carry more
/// weight; the higher pairs pull the octave wider the way a tuner hears it in
/// the bass.
const OCTAVE_PAIR_WEIGHTS: [(f64, f64); 3] = [(1.0, 1.0), (2.0, 0.5), (3.0, 0.25)];

/// A fitted inharmonicity model for a single note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteFit {
    /// Fitted fundamental frequency in Hz.
    pub f0: f32,
    /// Fitted inharmonicity coefficient `B` (clamped `>= 0`: stiffness only
    /// ever stretches partials sharp).
    pub b: f32,
}

/// Robustly fit `(f0, B)` for one note from its measured partials via the
/// linearization `(f_n / n)^2 = f0^2 + (f0^2 * B) * n^2` — an exact algebraic
/// rearrangement of `f_n = n*f0*sqrt(1+B*n^2)`.
///
/// Two robustness measures guard against the mislocated high-order partials
/// the bass `B = 0` search can leave in a profile (issue #22):
///
/// - **Amplitude weighting.** Louder partials (the strong low overtones of a
///   bass note) dominate the fit over weak, possibly-spurious high peaks.
/// - **Iterative outlier trimming.** After each fit, the partial that lands
///   furthest (in cents) from the model is dropped if it exceeds
///   [`OUTLIER_CENTS`], and the fit is repeated — down to
///   [`MIN_PARTIALS_AFTER_TRIM`] points.
///
/// Returns `None` when fewer than [`MIN_PARTIALS_FOR_FIT`] usable partials are
/// present or the fit is degenerate/unphysical.
pub fn fit_note_inharmonicity(partials: &[Partial]) -> Option<NoteFit> {
    let mut pts: Vec<(f64, f64, f64)> = partials
        .iter()
        .filter(|p| p.n >= 1 && p.freq_hz > 0.0 && p.amplitude > 0.0)
        .map(|p| (f64::from(p.n), f64::from(p.freq_hz), f64::from(p.amplitude)))
        .collect();

    if pts.len() < MIN_PARTIALS_FOR_FIT {
        return None;
    }

    loop {
        let (f0_sq, slope) = weighted_line_fit(&pts)?;
        if !f0_sq.is_finite() || f0_sq <= 0.0 || !slope.is_finite() {
            return None;
        }
        let f0 = f0_sq.sqrt();
        let b = (slope / f0_sq).max(0.0);

        // Find the partial furthest from the current model, measured in cents.
        let mut worst_idx = 0usize;
        let mut worst_cents = 0.0f64;
        for (k, &(n, freq, _)) in pts.iter().enumerate() {
            let predicted = n * f0 * (1.0 + b * n * n).sqrt();
            let cents = (1200.0 * (freq / predicted).log2()).abs();
            if cents > worst_cents {
                worst_cents = cents;
                worst_idx = k;
            }
        }

        if worst_cents as f32 > OUTLIER_CENTS && pts.len() > MIN_PARTIALS_AFTER_TRIM {
            pts.remove(worst_idx);
            continue;
        }

        return Some(NoteFit {
            f0: f0 as f32,
            b: b as f32,
        });
    }
}

/// Amplitude-weighted least-squares line fit `y = intercept + slope * x` over
/// the linearized partials (`x = n^2`, `y = (f_n / n)^2`, `w = amplitude`).
/// Returns `(intercept, slope)` = `(f0^2, f0^2 * B)`, or `None` when the
/// normal equations are singular (all partials share one `n`).
fn weighted_line_fit(pts: &[(f64, f64, f64)]) -> Option<(f64, f64)> {
    let (mut sw, mut swx, mut swy, mut swxx, mut swxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &(n, freq, amp) in pts {
        let x = n * n;
        let y = (freq / n).powi(2);
        sw += amp;
        swx += amp * x;
        swy += amp * y;
        swxx += amp * x * x;
        swxy += amp * x * y;
    }
    let denom = sw * swxx - swx * swx;
    if denom.abs() < 1e-12 {
        return None;
    }
    let slope = (sw * swxy - swx * swy) / denom;
    let intercept = (swy - slope * swx) / sw;
    Some((intercept, slope))
}

/// Build the 88 per-key stretch offsets (in cents) for a profile, fitting `B`
/// per note, smoothing `log(B)` across the keyboard, and integrating the
/// octave-coincidence widening outward from A4.
///
/// Errors when the profile carries too few notes with usable partials
/// ([`MIN_FITTED_NOTES`]) to define a curve — the caller falls back to the
/// Railsback default (legacy profiles with no partials land here).
pub fn stretch_from_profile(profile: &PianoProfile) -> Result<[f32; NOTE_COUNT]> {
    let mut measured: Vec<(usize, f32)> = Vec::new();
    for (idx, slot) in profile.notes.iter().enumerate().take(NOTE_COUNT) {
        let Some(note) = slot.as_ref() else { continue };
        if note.partials.is_empty() {
            continue;
        }
        if let Some(fit) = fit_note_inharmonicity(&note.partials) {
            if fit.b.is_finite() && fit.b > 0.0 {
                measured.push((idx, fit.b));
            }
        }
    }

    if measured.len() < MIN_FITTED_NOTES {
        return Err(anyhow!(
            "profile has too few notes with usable partials ({} < {}) to fit a per-piano stretch curve",
            measured.len(),
            MIN_FITTED_NOTES
        ));
    }

    let b = smooth_log_b(&measured)
        .ok_or_else(|| anyhow!("could not smooth inharmonicity across the keyboard"))?;
    Ok(stretch_offsets_from_b(&b))
}

/// Smooth the measured per-note `B` values into a `B` for every one of the 88
/// keys, working in `log(B)` space with a Gaussian kernel of
/// [`SMOOTH_SIGMA_SEMITONES`]. `measured` is `(key index, B)`; keys with no
/// measurement nearby fall back to the nearest measured value. Returns `None`
/// only for an empty input.
fn smooth_log_b(measured: &[(usize, f32)]) -> Option<[f32; NOTE_COUNT]> {
    if measured.is_empty() {
        return None;
    }

    let mut out = [0.0f32; NOTE_COUNT];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut wsum = 0.0f64;
        let mut lnsum = 0.0f64;
        for &(idx, b) in measured {
            let d = i as f64 - idx as f64;
            let w = (-0.5 * (d / SMOOTH_SIGMA_SEMITONES).powi(2)).exp();
            wsum += w;
            lnsum += w * f64::from(b).max(B_FLOOR).ln();
        }
        let ln_b = if wsum > 1e-12 {
            lnsum / wsum
        } else {
            let nearest = measured
                .iter()
                .min_by_key(|(idx, _)| (*idx as i64 - i as i64).abs())
                .expect("measured is non-empty");
            f64::from(nearest.1).max(B_FLOOR).ln()
        };
        *slot = ln_b.exp() as f32;
    }
    Some(out)
}

/// `B` at a keyboard index, extrapolated above the top key by continuing the
/// top-octave `log(B)` slope. Needed because the upward octave step at the
/// highest keys reaches partners past C8, and treble `B` keeps climbing there.
fn b_at(b: &[f32; NOTE_COUNT], idx: i32) -> f32 {
    if idx < 0 {
        return b[0];
    }
    let i = idx as usize;
    if i < NOTE_COUNT {
        return b[i];
    }
    let top = NOTE_COUNT - 1; // 87
    let reference = top - 12; // 75
    let ln_top = f64::from(b[top]).max(B_FLOOR).ln();
    let ln_ref = f64::from(b[reference]).max(B_FLOOR).ln();
    let slope = (ln_top - ln_ref) / 12.0;
    (ln_top + slope * (i - top) as f64).exp() as f32
}

/// Cents by which the octave from a note with `b_lower` to the note an octave
/// above (`b_upper`) must be widened past a pure 2:1 to still the beats of its
/// coincident partial pairs.
///
/// For pair `j` the beatless ratio `f_upper / f_lower` that makes partial `2j`
/// of the lower note coincide with partial `j` of the upper is
/// `2j*sqrt(1+b_lower*(2j)^2) / (j*sqrt(1+b_upper*j^2))`. The pairs disagree
/// (all pull wider, by different amounts), so the tuned ratio is their
/// weighted mean — the compromise that minimizes the weighted partial-beat
/// residual. The widening is `1200 * log2(ratio / 2)`.
fn octave_widening_cents(b_lower: f32, b_upper: f32) -> f32 {
    let bl = f64::from(b_lower);
    let bu = f64::from(b_upper);
    let (mut num, mut den) = (0.0f64, 0.0f64);
    for (j, w) in OCTAVE_PAIR_WEIGHTS {
        let lower_partial = 2.0 * j * (1.0 + bl * (2.0 * j) * (2.0 * j)).sqrt();
        let upper_partial = j * (1.0 + bu * j * j).sqrt();
        num += w * (lower_partial / upper_partial);
        den += w;
    }
    let ratio = num / den;
    (1200.0 * (ratio / 2.0).log2()) as f32
}

/// Integrate the octave-coincidence widening into 88 per-key stretch offsets
/// (cents from equal temperament), anchored at A4 (offset 0) and accumulated
/// outward. The local per-semitone slope is the octave widening at that key
/// spread over its 12 semitones, so the curve stays smooth and every pitch
/// class — not just A — gets an offset.
fn stretch_offsets_from_b(b: &[f32; NOTE_COUNT]) -> [f32; NOTE_COUNT] {
    let rate = |j: i32| octave_widening_cents(b_at(b, j), b_at(b, j + 12)) / 12.0;

    let mut offsets = [0.0f32; NOTE_COUNT];
    for i in (A4_INDEX + 1)..NOTE_COUNT {
        offsets[i] = offsets[i - 1] + rate(i as i32 - 1);
    }
    for i in (0..A4_INDEX).rev() {
        offsets[i] = offsets[i + 1] - rate(i as i32);
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthesize the partial stack of a stiff string with known `(f0, B)`.
    fn inharmonic_partials(f0: f32, b: f32, spec: &[(u16, f32)]) -> Vec<Partial> {
        spec.iter()
            .map(|&(n, amplitude)| {
                let nf = n as f32;
                Partial {
                    n,
                    freq_hz: nf * f0 * (1.0 + b * nf * nf).sqrt(),
                    amplitude,
                }
            })
            .collect()
    }

    fn cents(measured: f32, reference: f32) -> f32 {
        1200.0 * (measured / reference).log2()
    }

    #[test]
    fn fit_recovers_known_b() {
        let f0 = 110.0; // A2
        let b = 0.0008;
        let partials = inharmonic_partials(
            f0,
            b,
            &[(1, 1.0), (2, 0.7), (3, 0.5), (4, 0.4), (5, 0.3), (6, 0.2)],
        );

        let fit = fit_note_inharmonicity(&partials).expect("well-posed fit");
        assert!(cents(fit.f0, f0).abs() < 1.0, "f0 fit {} vs {}", fit.f0, f0);
        assert!((fit.b - b).abs() < 5e-5, "B fit {} vs true {}", fit.b, b);
    }

    #[test]
    fn fit_is_robust_to_mislocated_high_partials() {
        // A0-like: strong low overtones from a stiff string, but the two
        // highest-order "partials" are mislocated onto the harmonic (B = 0)
        // grid the bass search used — flat of where a B = 0.008 string
        // actually puts them. A naive least-squares over all seven would be
        // dragged toward a much smaller B; the robust fit must trim them.
        let f0 = 27.5;
        let b = 0.008;
        let mut partials =
            inharmonic_partials(f0, b, &[(1, 0.1), (2, 1.0), (3, 0.9), (4, 0.7), (5, 0.5)]);
        // Bad high-order partials placed at the *harmonic* positions n*f0.
        partials.push(Partial {
            n: 6,
            freq_hz: 6.0 * f0,
            amplitude: 0.4,
        });
        partials.push(Partial {
            n: 7,
            freq_hz: 7.0 * f0,
            amplitude: 0.3,
        });

        let fit = fit_note_inharmonicity(&partials).expect("robust fit");
        assert!(
            (fit.b - b).abs() < 1.5e-3,
            "robust B fit {} should stay near true {} despite bad partials",
            fit.b,
            b
        );
        assert!(cents(fit.f0, f0).abs() < 5.0, "f0 fit {} vs {}", fit.f0, f0);

        // Guard the guard: the naive fit over the raw list really is corrupted,
        // so the robustness above is doing work.
        let pts: Vec<(f64, f64, f64)> = partials
            .iter()
            .map(|p| (f64::from(p.n), f64::from(p.freq_hz), f64::from(p.amplitude)))
            .collect();
        let (f0_sq, slope) = weighted_line_fit(&pts).expect("naive fit");
        let naive_b = (slope / f0_sq) as f32;
        assert!(
            (naive_b - b).abs() > 1.5e-3,
            "naive fit B {} should be visibly corrupted (else the test proves nothing)",
            naive_b
        );
    }

    #[test]
    fn fit_rejects_too_few_partials() {
        assert!(fit_note_inharmonicity(&[]).is_none());
        assert!(fit_note_inharmonicity(&[Partial {
            n: 2,
            freq_hz: 110.0,
            amplitude: 1.0
        }])
        .is_none());
    }

    #[test]
    fn octave_widening_is_zero_without_inharmonicity() {
        assert!(
            octave_widening_cents(0.0, 0.0).abs() < 1e-4,
            "a perfectly harmonic octave needs no widening"
        );
    }

    #[test]
    fn octave_widening_grows_with_inharmonicity() {
        let mild = octave_widening_cents(0.0003, 0.0002);
        let strong = octave_widening_cents(0.0012, 0.0006);
        assert!(mild > 0.0, "any inharmonicity widens the octave: {mild}");
        assert!(
            strong > mild,
            "more inharmonicity widens it further: {strong} vs {mild}"
        );
    }

    #[test]
    fn octave_widening_matches_independent_beatless_formula() {
        // Re-derive the weighted-mean beatless ratio straight from the
        // stiff-string partial formula (weights hardcoded as the spec) and
        // check the implementation reproduces it. If the production weights
        // ever change, this fails loudly rather than drifting silently.
        let expected = |b_l: f64, b_u: f64| -> f64 {
            let weights = [(1.0f64, 1.0f64), (2.0, 0.5), (3.0, 0.25)];
            let (mut num, mut den) = (0.0, 0.0);
            for (j, w) in weights {
                let lower = 2.0 * j * (1.0 + b_l * (2.0 * j) * (2.0 * j)).sqrt();
                let upper = j * (1.0 + b_u * j * j).sqrt();
                num += w * (lower / upper);
                den += w;
            }
            1200.0 * (num / den / 2.0).log2()
        };

        for &(b_l, b_u) in &[(0.0005, 0.0003), (0.002, 0.001), (0.0001, 0.00008)] {
            let got = octave_widening_cents(b_l as f32, b_u as f32) as f64;
            assert!(
                (got - expected(b_l, b_u)).abs() < 0.01,
                "widening({b_l},{b_u}) = {got}, expected {}",
                expected(b_l, b_u)
            );
        }
    }

    #[test]
    fn zero_inharmonicity_yields_flat_stretch() {
        let offsets = stretch_offsets_from_b(&[0.0; NOTE_COUNT]);
        for (i, &o) in offsets.iter().enumerate() {
            assert!(
                o.abs() < 1e-2,
                "key {i} offset {o} should be ~0 for a harmonic (B=0) piano"
            );
        }
    }

    /// A realistic keyboard `B`: lowest in the tenor, rising toward both ends
    /// (the classic inharmonicity valley), in grand-piano-plausible ranges.
    fn realistic_b() -> [f32; NOTE_COUNT] {
        let mut b = [0.0f32; NOTE_COUNT];
        for (i, slot) in b.iter_mut().enumerate() {
            let midi = (i + 21) as f32;
            let bass = ((69.0 - midi) / 48.0).max(0.0);
            let treble = ((midi - 69.0) / 39.0).max(0.0);
            *slot = 0.0002 + 0.0006 * bass * bass + 0.0006 * treble * treble;
        }
        b
    }

    #[test]
    fn stretch_from_realistic_b_is_monotonic_and_railsback_shaped() {
        let offsets = stretch_offsets_from_b(&realistic_b());

        // A4 is the anchor.
        assert_eq!(offsets[A4_INDEX], 0.0);

        // Monotonic non-decreasing across the whole keyboard (widening is
        // positive at every step for a physical B curve).
        for i in 1..NOTE_COUNT {
            assert!(
                offsets[i] >= offsets[i - 1] - 1e-4,
                "curve dips at key {i}: {} < {}",
                offsets[i],
                offsets[i - 1]
            );
        }

        // Bass flat, treble sharp, temperament zone near zero.
        assert!(
            offsets[0] < -2.0,
            "A0 should be clearly flat: {}",
            offsets[0]
        );
        assert!(
            offsets[NOTE_COUNT - 1] > 2.0,
            "C8 should be clearly sharp: {}",
            offsets[NOTE_COUNT - 1]
        );
        assert!(
            offsets[39].abs() < 6.0,
            "C4 should stay near the temperament zone: {}",
            offsets[39]
        );

        // Published Railsback magnitudes live in the low tens of cents at the
        // extremes; a physical fit must not blow past that.
        assert!(
            (-45.0..=-2.0).contains(&offsets[0]),
            "A0 stretch {} outside a Railsback-plausible range",
            offsets[0]
        );
        assert!(
            (2.0..=45.0).contains(&offsets[NOTE_COUNT - 1]),
            "C8 stretch {} outside a Railsback-plausible range",
            offsets[NOTE_COUNT - 1]
        );
    }

    /// A full 88-key profile whose notes carry the partial stacks of a piano
    /// with the [`realistic_b`] inharmonicity curve.
    fn profile_with_inharmonicity() -> PianoProfile {
        let mut profile = PianoProfile::new();
        let b = realistic_b();
        for (i, &b_i) in b.iter().enumerate() {
            let midi = (i + 21) as u8;
            let f0 = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
            let partials = inharmonic_partials(
                f0,
                b_i,
                &[
                    (1, 1.0),
                    (2, 0.7),
                    (3, 0.5),
                    (4, 0.35),
                    (5, 0.25),
                    (6, 0.18),
                ],
            );
            profile.record_note_with_partials(midi, f0, 0.0, partials);
        }
        profile
    }

    #[test]
    fn stretch_from_profile_recovers_a_sane_curve() {
        let profile = profile_with_inharmonicity();
        let offsets = stretch_from_profile(&profile).expect("full profile fits a curve");

        assert_eq!(offsets[A4_INDEX], 0.0);
        assert!(offsets[0] < -2.0, "A0 flat: {}", offsets[0]);
        assert!(
            offsets[NOTE_COUNT - 1] > 2.0,
            "C8 sharp: {}",
            offsets[NOTE_COUNT - 1]
        );
        for i in 1..NOTE_COUNT {
            assert!(offsets[i] >= offsets[i - 1] - 1e-3, "curve dips at key {i}");
        }
    }

    #[test]
    fn stretch_from_profile_errors_without_partials() {
        // A legacy profile (measurements but no partials) can't be fit; the
        // caller must fall back to Railsback.
        let mut profile = PianoProfile::new();
        for i in 0..NOTE_COUNT {
            profile.record_note((i + 21) as u8, 440.0, 0.0);
        }
        assert!(
            stretch_from_profile(&profile).is_err(),
            "no partials -> no fit -> error (Railsback fallback)"
        );
    }

    #[test]
    fn stretch_from_profile_errors_with_too_few_fitted_notes() {
        let mut profile = PianoProfile::new();
        // Only two notes carry partials — below MIN_FITTED_NOTES.
        for midi in [33u8, 69] {
            let f0 = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
            let partials = inharmonic_partials(f0, 0.0004, &[(1, 1.0), (2, 0.7), (3, 0.5)]);
            profile.record_note_with_partials(midi, f0, 0.0, partials);
        }
        assert!(stretch_from_profile(&profile).is_err());
    }
}
