//! FFT-based partial (overtone) analysis.
//!
//! Time-domain pitch detection ([`crate::audio::PitchDetector`]) finds a single
//! fundamental but says nothing about a note's overtone structure. Piano tuning
//! needs that structure:
//!
//! - **Treble refinement** (issue #14): an integer YIN lag is a coarse grid
//!   (165 cents/step at C8). Re-reading the fundamental's magnitude peak from
//!   the spectrum with sub-bin interpolation delivers tuner-grade accuracy.
//! - **Bass f0** (issue #15): the A0–B2 fundamental is 20–40 dB below its
//!   partials and time-domain detectors lock onto an octave. Measuring the
//!   partial frequencies lets a downstream least-squares stage recover the true
//!   f0 and the inharmonicity coefficient B.
//! - **Inharmonicity engine** (issues #22 / #23): per-piano stretch is fit from
//!   the measured partial frequencies/amplitudes of each profiled note.
//!
//! Real strings are stiff, so partial `n` sits sharp of `n * f0` by
//! `sqrt(1 + B * n^2)`. This module predicts each partial's location from an f0
//! estimate (optionally with a known B) and searches a narrow band of the
//! magnitude spectrum around it, refining to sub-bin precision with parabolic
//! interpolation. It is the shared FFT foundation those consumers build on.

use std::sync::Mutex;

use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};

/// A single measured partial (overtone) of a played note.
///
/// `n == 1` is the fundamental. Serde-serializable so profiles can persist the
/// partial list alongside each note (issue #22).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Partial {
    /// Partial number (1 = fundamental, 2 = first overtone, ...).
    pub n: u16,
    /// Measured frequency in Hz (sub-bin, parabolically interpolated).
    pub freq_hz: f32,
    /// Linear amplitude, normalized so a full-scale sine reads ~1.0.
    pub amplitude: f32,
}

/// Result of partial-based fundamental estimation for the weak/absent-
/// fundamental bass register (issue #15).
///
/// Carries the fitted inharmonicity coefficient alongside the frequency, not
/// just a bare `f32`, so a future per-note profiling engine (issues #22/#23)
/// can reuse this measurement instead of re-deriving `B` from scratch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BassF0Estimate {
    /// Fitted fundamental frequency in Hz.
    pub frequency: f32,
    /// Fitted inharmonicity coefficient B (`f_n = n*f0*sqrt(1+B*n^2)`),
    /// clamped to `>= 0.0` (stiffness only ever stretches partials sharp).
    pub inharmonicity_b: f32,
    /// Confidence in `[0, 1]` derived from the partial-match residual —
    /// meaningful in this register, unlike time-domain CMND depth against a
    /// weak or absent fundamental.
    pub confidence: f32,
    /// Number of partials the least-squares fit was built from.
    pub partials_matched: u16,
}

/// FFT partial analyzer with a reusable, size-caching FFT planner.
///
/// Cheap to keep around and call repeatedly; the planner caches its plans per
/// transform size, so back-to-back calls on same-length windows reuse work.
pub struct PartialAnalyzer {
    sample_rate: u32,
    max_partials: u16,
    // NOTE: rustfft's planner caches computed plans internally; a Mutex keeps
    // the analyzer Send + Sync (the capture thread may drive it) while still
    // reusing plans across calls.
    planner: Mutex<FftPlanner<f32>>,
}

/// Default highest partial recovered (`f_n` up to the 8th overtone).
const DEFAULT_MAX_PARTIALS: u16 = 8;

/// Peaks below this fraction of the strongest recovered partial are treated as
/// leakage/noise and dropped — this is what keeps an absent bass fundamental
/// from being reported as a fabricated partial.
const PEAK_FLOOR_RATIO: f32 = 0.02;

/// Upper bound on the zero-padded transform size, so a pathologically long
/// window can't allocate an enormous buffer.
const MAX_FFT_SIZE: usize = 1 << 18;

/// A single Hann-windowed, zero-padded FFT's worth of shared state: the
/// magnitude spectrum over the positive-frequency half, the bin width in Hz,
/// and the coherent-gain sum S1 (amplitude normalization). Bundled so the
/// bass grid search's many partial-location calls ([`PartialAnalyzer::locate_partials`])
/// can share one FFT instead of taking it apart into loose arguments.
struct MagnitudeSpectrum {
    mags: Vec<f32>,
    bin_hz: f32,
    s1: f32,
}

impl PartialAnalyzer {
    /// Create a new analyzer for the given sample rate.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            max_partials: DEFAULT_MAX_PARTIALS,
            planner: Mutex::new(FftPlanner::new()),
        }
    }

    /// Set the highest partial number to recover (default 8).
    pub fn with_max_partials(mut self, max_partials: u16) -> Self {
        self.max_partials = max_partials.max(1);
        self
    }

    /// Analyze assuming harmonic partials (`B = 0`) as the search guide.
    ///
    /// Inharmonic partials are sharp, so the search band still finds them; pass
    /// a known `B` via [`analyze_with_inharmonicity`](Self::analyze_with_inharmonicity)
    /// for a tighter, better-centered search when it's available.
    pub fn analyze(&self, samples: &[f32], f0: f32) -> Vec<Partial> {
        self.analyze_with_inharmonicity(samples, f0, 0.0)
    }

    /// Analyze using an inharmonicity coefficient `B` to place the search band.
    ///
    /// Predicts partial `n` at `n * f0 * sqrt(1 + B * n^2)` and searches a band
    /// narrower than the partial spacing around each prediction, refining the
    /// magnitude peak to sub-bin precision.
    pub fn analyze_with_inharmonicity(&self, samples: &[f32], f0: f32, b: f32) -> Vec<Partial> {
        if f0 <= 0.0 {
            return Vec::new();
        }
        let Some(spectrum) = self.magnitude_spectrum(samples) else {
            return Vec::new();
        };
        self.locate_partials(&spectrum, f0, b, 1, self.max_partials)
    }

    /// Refine a coarse fundamental estimate to its sub-bin spectral peak.
    ///
    /// Thin convenience for treble refinement (issue #14): keep YIN as the
    /// octave/candidate selector, then read the true frequency off the FFT.
    pub fn refine_fundamental(&self, samples: &[f32], coarse_f0: f32) -> Option<f32> {
        self.analyze(samples, coarse_f0)
            .into_iter()
            .find(|p| p.n == 1)
            .map(|p| p.freq_hz)
    }

    /// Estimate the fundamental of a weak/absent-fundamental bass note from
    /// its partials, given an approximate (guided-mode) target frequency
    /// (issue #15).
    ///
    /// Real A0-B2 fundamentals sit 20-40 dB below their overtones, and a
    /// time-domain detector either locks onto a partial outright or, more
    /// insidiously, onto the inharmonic composite's quasi-periodic beat —
    /// which is *not* the true f0, just close enough to look plausible (the
    /// "naive GCD is biased sharp" the issue calls out). This instead:
    ///
    /// 1. Grid-searches candidate `(f0, B)` pairs within +/-100 cents of
    ///    `target_hz` (a guided note may itself be mistuned) against the raw
    ///    magnitude spectrum, scoring how much partial-2..6 energy each pair
    ///    explains.
    /// 2. Re-locates those partials to sub-bin precision at the winning
    ///    candidate, so the refine step's search band is centered by the
    ///    *inharmonic* prediction rather than a naive harmonic one.
    /// 3. Least-squares refits `(f0, B)` from the located peaks
    ///    ([`fit_f0_and_b`]) for precision beyond the coarse grid.
    ///
    /// Returns `None` when fewer than two partials are located (an ill-posed
    /// fit), the fit lands implausibly far from `target_hz` (a wrong note or
    /// pure noise), or `target_hz` is non-positive.
    pub fn estimate_bass_f0(&self, samples: &[f32], target_hz: f32) -> Option<BassF0Estimate> {
        if target_hz <= 0.0 {
            return None;
        }
        let spectrum = self.magnitude_spectrum(samples)?;
        let nyquist = self.sample_rate as f32 / 2.0;

        let (coarse_f0, coarse_b) =
            coarse_bass_grid_search(&spectrum.mags, spectrum.bin_hz, nyquist, target_hz)?;

        let matched = self.locate_partials(
            &spectrum,
            coarse_f0,
            coarse_b,
            BASS_MATCH_MIN_PARTIAL,
            BASS_MATCH_MAX_PARTIAL,
        );
        if matched.len() < 2 {
            return None;
        }

        let (f0_fit, b_fit) = fit_f0_and_b(&matched)?;
        if !f0_fit.is_finite() || f0_fit <= 0.0 {
            return None;
        }
        // Physically B >= 0 (stiffness only ever stretches partials sharp);
        // noise can nudge an otherwise-good fit slightly negative.
        let b_fit = b_fit.max(0.0);

        // Sanity: the fit must stay near the guided target, not lock onto an
        // unrelated partial family (wrong note, octave, noise floor).
        let cents_from_target = 1200.0 * (f0_fit / target_hz).log2();
        if cents_from_target.abs() > BASS_SEARCH_CENTS + BASS_FIT_DRIFT_MARGIN_CENTS {
            return None;
        }

        Some(BassF0Estimate {
            frequency: f0_fit,
            inharmonicity_b: b_fit,
            confidence: partial_match_confidence(&matched, f0_fit, b_fit),
            partials_matched: matched.len() as u16,
        })
    }

    /// Shared Hann-windowed, zero-padded FFT magnitude spectrum used by every
    /// partial search.
    fn magnitude_spectrum(&self, samples: &[f32]) -> Option<MagnitudeSpectrum> {
        if samples.len() < 4 {
            return None;
        }

        let fft_size = fft_size_for(samples.len());
        let bin_hz = self.sample_rate as f32 / fft_size as f32;

        // Hann window into a zero-padded complex buffer. S1 (coherent gain) is
        // the window sum over the real samples; a bin-centered sine of
        // amplitude A peaks at A * S1 / 2, so amplitude = 2 * peak / S1.
        let mut buffer = vec![Complex::new(0.0f32, 0.0); fft_size];
        let len = samples.len();
        let mut s1 = 0.0f32;
        for (i, (slot, &s)) in buffer.iter_mut().zip(samples.iter()).enumerate() {
            let w = hann(i, len);
            s1 += w;
            slot.re = s * w;
        }
        if s1 <= 0.0 {
            return None;
        }

        let fft = self.planner.lock().unwrap().plan_fft_forward(fft_size);
        fft.process(&mut buffer);

        // Magnitude spectrum over the non-redundant (positive-frequency) half.
        let half = fft_size / 2;
        let mags: Vec<f32> = buffer[..half].iter().map(|c| c.norm()).collect();
        Some(MagnitudeSpectrum { mags, bin_hz, s1 })
    }

    /// Locate partials `lo..=hi` at `n * f0 * sqrt(1 + B * n^2)` in an
    /// already-computed magnitude spectrum, refining each to sub-bin
    /// precision. Shared by [`analyze_with_inharmonicity`](Self::analyze_with_inharmonicity)
    /// and [`estimate_bass_f0`](Self::estimate_bass_f0) so the bass search's
    /// repeated grid evaluations don't each pay for a fresh FFT.
    fn locate_partials(
        &self,
        spectrum: &MagnitudeSpectrum,
        f0: f32,
        b: f32,
        lo_n: u16,
        hi_n: u16,
    ) -> Vec<Partial> {
        let MagnitudeSpectrum { mags, bin_hz, s1 } = spectrum;
        let bin_hz = *bin_hz;
        let nyquist = self.sample_rate as f32 / 2.0;
        let half = mags.len();

        // Search band: just under half the partial spacing so a sharpened
        // partial n never collides with partial n+1, floored so low f0 (bass,
        // few bins between partials) still has a usable window.
        let f0_bins = f0 / bin_hz;
        let radius = ((0.42 * f0_bins).round() as usize).max(2);

        let mut found: Vec<Partial> = Vec::new();
        for n in lo_n..=hi_n {
            let nf = n as f32;
            let predicted = nf * f0 * (1.0 + b * nf * nf).sqrt();
            if predicted >= nyquist {
                break;
            }
            let center = (predicted / bin_hz).round() as isize;
            let low = (center - radius as isize).max(1) as usize;
            let high = ((center + radius as isize) as usize).min(half.saturating_sub(2));
            if low >= high {
                continue;
            }

            let peak_bin = (low..=high).max_by(|&a, &b| mags[a].total_cmp(&mags[b]));
            let Some(peak_bin) = peak_bin else { continue };
            if peak_bin == 0 || peak_bin + 1 >= half {
                continue;
            }

            let (delta, peak_mag) =
                parabolic_peak(mags[peak_bin - 1], mags[peak_bin], mags[peak_bin + 1]);
            let freq_hz = (peak_bin as f32 + delta) * bin_hz;
            found.push(Partial {
                n,
                freq_hz,
                amplitude: 2.0 * peak_mag / s1,
            });
        }

        // Drop leakage/noise peaks (e.g. the band around an absent fundamental).
        // No peak energy at all (silence) means there is nothing to report.
        let max_amp = found.iter().map(|p| p.amplitude).fold(0.0f32, f32::max);
        if max_amp <= 0.0 {
            return Vec::new();
        }
        let floor = max_amp * PEAK_FLOOR_RATIO;
        found.retain(|p| p.amplitude >= floor);

        found
    }
}

/// Zero-pad length: next power of two ≥ `len`, giving a finer interpolation
/// grid and the fastest transforms, capped at [`MAX_FFT_SIZE`].
fn fft_size_for(len: usize) -> usize {
    len.next_power_of_two().clamp(4, MAX_FFT_SIZE)
}

/// Hann window coefficient for sample `i` of a length-`len` window.
fn hann(i: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = 2.0 * std::f32::consts::PI * i as f32 / (len - 1) as f32;
    0.5 * (1.0 - phase.cos())
}

/// Parabolic interpolation of a magnitude peak from three samples straddling a
/// local maximum. Returns `(sub-bin offset in [-0.5, 0.5], interpolated peak)`.
///
/// Interpolates in the log domain: a Hann main lobe is near-Gaussian there, so
/// a parabola fits it far better than in linear magnitude — the accuracy that
/// makes sub-cent treble refinement possible.
fn parabolic_peak(m0: f32, m1: f32, m2: f32) -> (f32, f32) {
    // WARNING: log(0) is -inf; nudge a silent bin up to a tiny floor so the
    // interpolation stays finite for near-empty search bands.
    let floor = 1e-12;
    let y0 = m0.max(floor).ln();
    let y1 = m1.max(floor).ln();
    let y2 = m2.max(floor).ln();

    let denom = y0 - 2.0 * y1 + y2;
    if denom.abs() < 1e-12 {
        return (0.0, m1);
    }
    let delta = 0.5 * (y0 - y2) / denom;
    let peak_log = y1 - 0.25 * (y0 - y2) * delta;
    (delta.clamp(-0.5, 0.5), peak_log.exp())
}

// ---- Bass f0 estimation (issue #15) ---------------------------------------
//
// A0-B2's fundamental is weak or absent, so a time-domain estimator either
// locks onto a partial or, worse, onto the inharmonic composite's
// quasi-periodic beat (which is *not* 1/f0 — inharmonicity stretches the
// higher partials, so a naive integer-period GCD comes out sharp). Guided
// mode already knows the note, so instead of trusting a period at all: grid
// search candidate (f0, B) pairs near the target against the raw spectrum,
// then least-squares refit from the located partials.

/// Cents to either side of the guided target searched for the true (possibly
/// mistuned) string pitch (issue #15: "+/-100 cents of target").
const BASS_SEARCH_CENTS: f32 = 100.0;

/// Candidate f0 grid resolution across the +/-100-cent search band (~5-cent
/// steps). The coarse grid only needs to land close enough that
/// [`PartialAnalyzer::locate_partials`]'s own search radius (a fraction of the
/// partial spacing) recovers the true peaks in the refine pass — it does not
/// need to be the final answer.
const BASS_F0_GRID_STEPS: usize = 41;

/// Candidate inharmonicity values tried during the coarse grid search,
/// spanning the physically observed bass range (issue #17's fixture
/// generator: B ~ 0.010 at A0 down to ~0.0004 by the tenor break). A wrong B
/// guess shifts high partials enough to fall outside the refine pass's search
/// band (partial drift scales with `n^2`), so B must be grid-searched
/// alongside f0, not assumed to be 0 the way [`PartialAnalyzer::analyze`]
/// does for its better-conditioned callers.
const BASS_B_GRID: [f32; 8] = [0.0, 0.0002, 0.0005, 0.001, 0.002, 0.004, 0.007, 0.012];

/// Partials used to match/fit the bass fundamental (issue #15: "partials
/// 2-6"). The fundamental itself is excluded even where present (e.g. near
/// B2, where it has recovered most of its strength) so the estimator behaves
/// uniformly across the whole weak-fundamental register.
const BASS_MATCH_MIN_PARTIAL: u16 = 2;
const BASS_MATCH_MAX_PARTIAL: u16 = 6;

/// Slack added to [`BASS_SEARCH_CENTS`] when sanity-checking the final fit
/// against the guided target. The fit itself can overshoot the raw search
/// band slightly (the coarse grid's candidate f0 is not the fit's final
/// value), so the rejection gate needs a little more room than the search
/// band itself, or a good fit near the band's edge would be discarded.
const BASS_FIT_DRIFT_MARGIN_CENTS: f32 = 20.0;

/// Confidence floor: an RMS partial-match residual at or above this many
/// cents scores 0. A clean fit resolves to a couple of cents; a mismatch
/// (wrong note, pure noise, no real periodicity) blows well past it.
const BASS_MAX_RESIDUAL_CENTS: f32 = 15.0;

/// Coarse-to-fine search for the `(f0, B)` pair that best explains partials
/// 2-6 of the raw magnitude spectrum, within [`BASS_SEARCH_CENTS`] of
/// `target_hz`. Scores each candidate by the magnitude at its nearest bin per
/// partial (no sub-bin interpolation — this is a coarse localization pass,
/// not the final measurement) and returns the best-scoring pair, or `None` if
/// no candidate explains any energy at all.
fn coarse_bass_grid_search(
    mags: &[f32],
    bin_hz: f32,
    nyquist: f32,
    target_hz: f32,
) -> Option<(f32, f32)> {
    let edge = 2.0f32.powf(BASS_SEARCH_CENTS / 1200.0);
    let f_lo = target_hz / edge;
    let f_hi = target_hz * edge;

    let mut best_score = 0.0f32;
    let mut best: Option<(f32, f32)> = None;

    for i in 0..BASS_F0_GRID_STEPS {
        let t = i as f32 / (BASS_F0_GRID_STEPS - 1) as f32;
        let f0 = f_lo * (f_hi / f_lo).powf(t); // log-spaced across the cents band
        for &b in &BASS_B_GRID {
            let mut score = 0.0f32;
            for n in BASS_MATCH_MIN_PARTIAL..=BASS_MATCH_MAX_PARTIAL {
                let nf = n as f32;
                let predicted = nf * f0 * (1.0 + b * nf * nf).sqrt();
                if predicted >= nyquist {
                    continue;
                }
                let bin = (predicted / bin_hz).round() as usize;
                if bin == 0 || bin >= mags.len() {
                    continue;
                }
                score += mags[bin];
            }
            if score > best_score {
                best_score = score;
                best = Some((f0, b));
            }
        }
    }

    best
}

/// RMS cents deviation of measured partials from the `(f0, B)` model, mapped
/// to a `[0, 1]` confidence via [`BASS_MAX_RESIDUAL_CENTS`].
fn partial_match_confidence(partials: &[Partial], f0: f32, b: f32) -> f32 {
    if partials.is_empty() {
        return 0.0;
    }
    let mut sum_sq = 0.0f64;
    for p in partials {
        let nf = p.n as f32;
        let predicted = nf * f0 * (1.0 + b * nf * nf).sqrt();
        let cents = 1200.0 * (p.freq_hz / predicted).log2();
        sum_sq += f64::from(cents) * f64::from(cents);
    }
    let rms = (sum_sq / partials.len() as f64).sqrt() as f32;
    (1.0 - rms / BASS_MAX_RESIDUAL_CENTS).clamp(0.0, 1.0)
}

/// Least-squares fit of the fundamental and inharmonicity coefficient from a
/// set of measured partials, via the linearization
/// `(f_n/n)^2 = f0^2 + f0^2*B*n^2` — an exact algebraic rearrangement of
/// `f_n = n*f0*sqrt(1+B*n^2)`, not an approximation, so the fit is only as
/// noisy as the input peak measurements.
///
/// Returns `None` for fewer than two partials (an ill-posed line fit), a
/// degenerate input (all the same `n`), or a non-positive fitted intercept
/// (unphysical: `f0^2` cannot be negative).
pub fn fit_f0_and_b(partials: &[Partial]) -> Option<(f32, f32)> {
    if partials.len() < 2 {
        return None;
    }

    let (mut sx, mut sy, mut sxx, mut sxy, mut nn) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for p in partials {
        let x = f64::from(p.n).powi(2);
        let y = (f64::from(p.freq_hz) / f64::from(p.n)).powi(2);
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
        nn += 1.0;
    }

    let denom = nn * sxx - sx * sx;
    if denom.abs() < 1e-9 {
        return None;
    }
    let slope = (nn * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / nn;
    if intercept <= 0.0 {
        return None;
    }

    let f0 = intercept.sqrt();
    let b = slope / intercept;
    Some((f0 as f32, b as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::traits::TestAudioSource;
    use approx::assert_relative_eq;

    const SR: u32 = 44100;

    // The capture thread may drive the analyzer, so the Send + Sync guarantee
    // the type doc-comment promises is load-bearing; assert it at compile time.
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PartialAnalyzer>();
    };

    fn cents(measured: f32, reference: f32) -> f32 {
        1200.0 * (measured / reference).log2()
    }

    fn inharmonic_freq(n: u16, f0: f32, b: f32) -> f32 {
        let nf = n as f32;
        nf * f0 * (1.0 + b * nf * nf).sqrt()
    }

    /// Thin wrapper so existing call sites in this test module read the same
    /// as before; the real least-squares logic is [`fit_f0_and_b`] (issue
    /// #15), which this proves the analyzer's partials are precise enough to
    /// feed.
    fn fit_f0_b(partials: &[Partial]) -> (f32, f32) {
        fit_f0_and_b(partials).expect("well-posed fit in these tests")
    }

    #[test]
    fn recovers_harmonic_partial_frequencies() {
        let f0 = 220.0;
        let src = TestAudioSource::inharmonic(
            f0,
            0.0,
            &[(1, 1.0), (2, 0.6), (3, 0.4), (4, 0.3), (5, 0.2), (6, 0.15)],
            0.2,
            SR,
        );
        let partials = PartialAnalyzer::new(SR).analyze(src.samples(), f0);

        assert!(
            partials.len() >= 6,
            "expected at least 6 partials, got {}",
            partials.len()
        );
        for p in &partials {
            let expected = p.n as f32 * f0;
            let err = cents(p.freq_hz, expected).abs();
            assert!(
                err < 3.0,
                "partial {} at {} Hz, expected {} Hz ({:.2} cents off)",
                p.n,
                p.freq_hz,
                expected,
                err
            );
        }
    }

    #[test]
    fn recovers_inharmonic_partials_and_b() {
        let f0 = 55.0; // A1
        let b_true = 0.0004;
        let src = TestAudioSource::inharmonic(
            f0,
            b_true,
            &[(1, 0.3), (2, 1.0), (3, 0.8), (4, 0.6), (5, 0.5), (6, 0.4)],
            0.4,
            SR,
        );
        let partials = PartialAnalyzer::new(SR).analyze(src.samples(), f0);

        for p in &partials {
            let expected = inharmonic_freq(p.n, f0, b_true);
            assert!(
                (p.freq_hz - expected).abs() < 0.7,
                "partial {}: {} Hz vs expected {} Hz",
                p.n,
                p.freq_hz,
                expected
            );
        }

        let (f0_fit, b_fit) = fit_f0_b(&partials);
        assert!((f0_fit - f0).abs() < 0.3, "f0 fit {} vs {}", f0_fit, f0);
        assert!(
            (b_fit - b_true).abs() < 5e-5,
            "B fit {} vs true {}",
            b_fit,
            b_true
        );
    }

    #[test]
    fn recovers_partials_with_missing_fundamental() {
        let f0 = 41.2; // E1
        let b_true = 0.0005;
        // No partial 1 — the through-a-mic bass reality that traps time-domain
        // detectors. Partials 2..6 carry the note.
        let src = TestAudioSource::inharmonic(
            f0,
            b_true,
            &[(2, 1.0), (3, 0.8), (4, 0.7), (5, 0.5), (6, 0.4)],
            0.4,
            SR,
        );
        let partials = PartialAnalyzer::new(SR).analyze(src.samples(), f0);

        assert!(
            partials.iter().all(|p| p.n != 1),
            "absent fundamental must not be fabricated: {partials:?}"
        );
        for n in 2..=6u16 {
            let p = partials
                .iter()
                .find(|p| p.n == n)
                .unwrap_or_else(|| panic!("missing partial {n}: {partials:?}"));
            let expected = inharmonic_freq(n, f0, b_true);
            assert!(
                (p.freq_hz - expected).abs() < 0.7,
                "partial {}: {} Hz vs expected {} Hz",
                n,
                p.freq_hz,
                expected
            );
        }

        // The partials still pin down f0 and B for the downstream fit.
        let (f0_fit, b_fit) = fit_f0_b(&partials);
        assert!((f0_fit - f0).abs() < 0.4, "f0 fit {} vs {}", f0_fit, f0);
        assert!(
            (b_fit - b_true).abs() < 8e-5,
            "B fit {} vs {}",
            b_fit,
            b_true
        );
    }

    #[test]
    fn recovers_relative_amplitudes() {
        let f0 = 261.63; // C4
        let src = TestAudioSource::inharmonic(f0, 0.0, &[(1, 1.0), (2, 0.5), (3, 0.25)], 0.2, SR);
        let partials = PartialAnalyzer::new(SR).analyze(src.samples(), f0);

        let amp = |n: u16| partials.iter().find(|p| p.n == n).unwrap().amplitude;
        let (a1, a2, a3) = (amp(1), amp(2), amp(3));

        assert!(a1 > a2 && a2 > a3, "amplitudes not ordered: {a1} {a2} {a3}");
        assert_relative_eq!(a2 / a1, 0.5, epsilon = 0.08);
        assert_relative_eq!(a3 / a1, 0.25, epsilon = 0.08);
        // Absolute scale: full-scale fundamental should read ~1.0.
        assert!((a1 - 1.0).abs() < 0.15, "fundamental amplitude {a1}");
    }

    #[test]
    fn refines_treble_fundamental_below_one_cent() {
        // Single partials in a 100 ms window (the production treble window),
        // with a coarse estimate offset like a real YIN candidate would be.
        for &f in &[1046.5_f32, 2093.0, 4186.0] {
            let src = TestAudioSource::inharmonic(f, 0.0, &[(1, 1.0)], 0.1, SR);
            let coarse = f * 1.004; // ~7 cents sharp
            let refined = PartialAnalyzer::new(SR)
                .refine_fundamental(src.samples(), coarse)
                .expect("should find fundamental peak");
            let err = cents(refined, f).abs();
            assert!(
                err < 1.0,
                "f={f}: refined {refined} Hz is {err:.3} cents off"
            );
        }
    }

    #[test]
    fn caps_partial_count() {
        let f0 = 100.0;
        let spec: Vec<(u16, f32)> = (1..=12u16).map(|n| (n, 1.0 / n as f32)).collect();
        let src = TestAudioSource::inharmonic(f0, 0.0, &spec, 0.2, SR);
        let partials = PartialAnalyzer::new(SR)
            .with_max_partials(8)
            .analyze(src.samples(), f0);

        assert!(
            partials.iter().all(|p| p.n <= 8),
            "exceeded cap: {partials:?}"
        );
        assert!(partials.len() <= 8);
    }

    #[test]
    fn rejects_degenerate_input() {
        let a = PartialAnalyzer::new(SR);
        assert!(a.analyze(&[], 440.0).is_empty());
        assert!(a.analyze(&[0.0; 1024], 0.0).is_empty());
        assert!(a.analyze(&[0.0; 1024], -10.0).is_empty());
        // Pure silence has no peaks worth reporting.
        assert!(a.analyze(&[0.0; 4096], 440.0).is_empty());
    }

    #[test]
    fn partial_serde_roundtrip() {
        let p = Partial {
            n: 3,
            freq_hz: 660.5,
            amplitude: 0.42,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: Partial = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    // ---- Bass f0 estimation (issue #15) ------------------------------------

    #[test]
    fn fit_f0_and_b_matches_known_partials() {
        let f0 = 55.0;
        let b = 0.006;
        let partials: Vec<Partial> = (2..=6u16)
            .map(|n| Partial {
                n,
                freq_hz: inharmonic_freq(n, f0, b),
                amplitude: 1.0,
            })
            .collect();
        let (f0_fit, b_fit) = fit_f0_and_b(&partials).expect("well-posed fit");
        assert!((f0_fit - f0).abs() < 0.01, "f0 fit {f0_fit} vs {f0}");
        assert!((b_fit - b).abs() < 1e-6, "b fit {b_fit} vs {b}");
    }

    #[test]
    fn fit_f0_and_b_rejects_degenerate_input() {
        assert!(fit_f0_and_b(&[]).is_none());
        assert!(fit_f0_and_b(&[Partial {
            n: 2,
            freq_hz: 110.0,
            amplitude: 1.0
        }])
        .is_none());
    }

    #[test]
    fn estimate_bass_f0_recovers_weak_fundamental_deep_bass() {
        // A0-like: fundamental far weaker than its overtones, sizable
        // inharmonicity (issue #17's fixture generator uses B ~ 0.01 at A0).
        let f0 = 27.5;
        let b = 0.009;
        let src = TestAudioSource::inharmonic(
            f0,
            b,
            &[(1, 0.05), (2, 1.0), (3, 0.9), (4, 0.7), (5, 0.5), (6, 0.35)],
            0.4,
            SR,
        );
        let estimate = PartialAnalyzer::new(SR)
            .estimate_bass_f0(src.samples(), f0)
            .expect("should recover a bass f0 estimate");
        let err = cents(estimate.frequency, f0).abs();
        assert!(
            err < 3.0,
            "expected within 3 cents of {f0} Hz, got {} ({err:.2}c)",
            estimate.frequency
        );
        assert!(
            (estimate.inharmonicity_b - b).abs() < 0.002,
            "B fit {} vs {}",
            estimate.inharmonicity_b,
            b
        );
        assert!(
            estimate.confidence > 0.7,
            "expected high confidence, got {}",
            estimate.confidence
        );
    }

    #[test]
    fn estimate_bass_f0_tolerates_mistuned_target_within_100_cents() {
        // The guided target is the equal-tempered pitch; a real string can sit
        // up to +/-100 cents off it and the search band must still find it.
        let true_f0 = 41.2; // E1
        let b = 0.006;
        let src = TestAudioSource::inharmonic(
            true_f0,
            b,
            &[(2, 1.0), (3, 0.8), (4, 0.6), (5, 0.4), (6, 0.3)],
            0.4,
            SR,
        );
        let mistuned_target = true_f0 * 2f32.powf(80.0 / 1200.0); // 80c sharp of the true pitch
        let estimate = PartialAnalyzer::new(SR)
            .estimate_bass_f0(src.samples(), mistuned_target)
            .expect("should still resolve within the +/-100 cent search band");
        let err = cents(estimate.frequency, true_f0).abs();
        assert!(
            err < 5.0,
            "expected within 5 cents of the true {true_f0} Hz, got {} ({err:.2}c)",
            estimate.frequency
        );
    }

    #[test]
    fn estimate_bass_f0_rejects_noise() {
        let mut noise = vec![0.0f32; (SR as f32 * 0.4) as usize];
        let mut x = 987_654_321_u64;
        for s in &mut noise {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *s = ((x as f64 / u64::MAX as f64) as f32) * 2.0 - 1.0;
        }
        match PartialAnalyzer::new(SR).estimate_bass_f0(&noise, 30.0) {
            None => {}
            Some(e) => assert!(
                e.confidence < 0.5,
                "pure noise should not produce a confident bass estimate, got {e:?}"
            ),
        }
    }

    #[test]
    fn estimate_bass_f0_rejects_degenerate_input() {
        let a = PartialAnalyzer::new(SR);
        assert!(a.estimate_bass_f0(&[], 30.0).is_none());
        assert!(a.estimate_bass_f0(&[0.0; 4096], 0.0).is_none());
        assert!(a.estimate_bass_f0(&[0.0; 4096], -10.0).is_none());
    }
}
