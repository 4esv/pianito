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
        if f0 <= 0.0 || samples.len() < 4 {
            return Vec::new();
        }

        let fft_size = fft_size_for(samples.len());
        let nyquist = self.sample_rate as f32 / 2.0;
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
            return Vec::new();
        }

        let fft = self.planner.lock().unwrap().plan_fft_forward(fft_size);
        fft.process(&mut buffer);

        // Magnitude spectrum over the non-redundant (positive-frequency) half.
        let half = fft_size / 2;
        let mags: Vec<f32> = buffer[..half].iter().map(|c| c.norm()).collect();

        // Search band: just under half the partial spacing so a sharpened
        // partial n never collides with partial n+1, floored so low f0 (bass,
        // few bins between partials) still has a usable window.
        let f0_bins = f0 / bin_hz;
        let radius = ((0.42 * f0_bins).round() as usize).max(2);

        let mut found: Vec<Partial> = Vec::new();
        for n in 1..=self.max_partials {
            let nf = n as f32;
            let predicted = nf * f0 * (1.0 + b * nf * nf).sqrt();
            if predicted >= nyquist {
                break;
            }
            let center = (predicted / bin_hz).round() as isize;
            let lo = (center - radius as isize).max(1) as usize;
            let hi = ((center + radius as isize) as usize).min(half - 2);
            if lo >= hi {
                continue;
            }

            let peak_bin = (lo..=hi).max_by(|&a, &b| mags[a].total_cmp(&mags[b]));
            let Some(peak_bin) = peak_bin else { continue };
            if peak_bin == 0 || peak_bin + 1 >= half {
                continue;
            }

            let (delta, peak_mag) =
                parabolic_peak(mags[peak_bin - 1], mags[peak_bin], mags[peak_bin + 1]);
            let freq_hz = (peak_bin as f32 + delta) * bin_hz;
            let amplitude = 2.0 * peak_mag / s1;
            found.push(Partial {
                n,
                freq_hz,
                amplitude,
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

    /// Ordinary least squares recovering `(f0, B)` from a partial set via the
    /// linearization `(f_n / n)^2 = f0^2 + f0^2 B * n^2`. This mirrors what the
    /// downstream bass/inharmonicity stages (#15/#23) will do, and proves the
    /// analyzer's partials are precise enough to feed them.
    fn fit_f0_b(partials: &[Partial]) -> (f32, f32) {
        let (mut sx, mut sy, mut sxx, mut sxy, mut nn) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
        for p in partials {
            let x = (p.n as f64).powi(2);
            let y = (p.freq_hz as f64 / p.n as f64).powi(2);
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
            nn += 1.0;
        }
        let denom = nn * sxx - sx * sx;
        let slope = (nn * sxy - sx * sy) / denom;
        let intercept = (sy - slope * sx) / nn;
        let f0 = intercept.sqrt();
        let b = slope / intercept;
        (f0 as f32, b as f32)
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
}
