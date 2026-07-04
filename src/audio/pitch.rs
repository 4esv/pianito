//! YIN pitch detection algorithm.
//!
//! Implementation based on:
//! de Cheveigné, A., & Kawahara, H. (2002). "YIN, a fundamental frequency estimator for speech and music."

/// Pitch detection result.
#[derive(Debug, Clone, Copy)]
pub struct PitchResult {
    /// Detected frequency in Hz.
    pub frequency: f32,
    /// Confidence score (0.0 to 1.0, higher is better).
    pub confidence: f32,
}

/// YIN-based pitch detector.
pub struct PitchDetector {
    sample_rate: u32,
    threshold: f32,
    min_frequency: f32,
    max_frequency: f32,
}

impl PitchDetector {
    /// Minimum amount by which the `2*tau` dip must beat the current dip for
    /// the octave post-check to move down an octave. Compared against
    /// interpolated dip depths, it sits between the two regimes: every clean
    /// fundamental's dips interpolate to ~0 (gap <= ~0.0005), while a
    /// partial-pick leaves a genuinely shallow dip with a much deeper
    /// true-fundamental dip an octave down (gap >= ~0.012).
    const OCTAVE_MARGIN: f32 = 0.01;

    /// Create a new pitch detector.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            threshold: 0.1,
            min_frequency: 27.5,   // A0
            max_frequency: 4186.0, // C8
        }
    }

    /// Set the confidence threshold for detection.
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    /// Set the frequency range.
    pub fn with_frequency_range(mut self, min: f32, max: f32) -> Self {
        self.min_frequency = min;
        self.max_frequency = max;
        self
    }

    /// Detect pitch from audio samples using the YIN algorithm.
    pub fn detect(&self, samples: &[f32]) -> Option<PitchResult> {
        if samples.len() < 2 {
            return None;
        }

        // Calculate tau range from frequency range
        let tau_min = (self.sample_rate as f32 / self.max_frequency) as usize;
        let tau_max =
            (self.sample_rate as f32 / self.min_frequency).min((samples.len() / 2) as f32) as usize;

        if tau_max <= tau_min || tau_max >= samples.len() / 2 {
            return None;
        }

        // Step 1 & 2: Calculate the difference function
        let diff = self.difference_function(samples, tau_max);

        // Step 3: Cumulative mean normalized difference function
        let cmnd = self.cumulative_mean_normalized_difference(&diff);

        // Step 4: Absolute threshold
        let mut tau = self.find_threshold_crossing(&cmnd, tau_min, tau_max)?;

        // Step 4b: Octave-error post-check. YIN's first-dip rule can lock onto
        // a strong partial (a weak-fundamental bass note, a decaying treble
        // note whose 2nd partial dominates), reporting a pitch an octave high.
        // Walk down to the true fundamental while a deeper dip sits at ~2*tau.
        while let Some(lower) = self.octave_below(&cmnd, tau, tau_max) {
            tau = lower;
        }

        // Step 5: Parabolic interpolation for sub-sample accuracy
        let refined_tau = self.parabolic_interpolation(&cmnd, tau);

        // Calculate frequency
        let frequency = self.sample_rate as f32 / refined_tau;

        // Calculate confidence (1 - cmnd value at the dip)
        let confidence = 1.0 - cmnd[tau].min(1.0);

        Some(PitchResult {
            frequency,
            confidence,
        })
    }

    /// Step 1 & 2: Calculate the difference function.
    fn difference_function(&self, samples: &[f32], max_tau: usize) -> Vec<f32> {
        let mut diff = vec![0.0; max_tau + 1];

        // d(tau) = sum_{j=0}^{W-1} (x_j - x_{j+tau})^2
        for tau in 1..=max_tau {
            let mut sum = 0.0;
            for j in 0..(samples.len() - max_tau) {
                let delta = samples[j] - samples[j + tau];
                sum += delta * delta;
            }
            diff[tau] = sum;
        }

        diff
    }

    /// Step 3: Cumulative mean normalized difference function.
    fn cumulative_mean_normalized_difference(&self, diff: &[f32]) -> Vec<f32> {
        let mut cmnd = vec![0.0; diff.len()];

        if diff.is_empty() {
            return cmnd;
        }

        cmnd[0] = 1.0; // By definition

        let mut running_sum = 0.0;
        for tau in 1..diff.len() {
            running_sum += diff[tau];
            if running_sum > 0.0 {
                cmnd[tau] = diff[tau] * tau as f32 / running_sum;
            } else {
                cmnd[tau] = 1.0;
            }
        }

        cmnd
    }

    /// Step 4: Find the first tau where cmnd drops below threshold.
    fn find_threshold_crossing(
        &self,
        cmnd: &[f32],
        tau_min: usize,
        tau_max: usize,
    ) -> Option<usize> {
        // Find the first dip below threshold
        for tau in tau_min..tau_max {
            if cmnd[tau] < self.threshold {
                // Find the minimum in this dip
                let mut min_tau = tau;
                let mut min_val = cmnd[tau];

                #[allow(clippy::needless_range_loop)]
                for t in tau + 1..tau_max {
                    if cmnd[t] < min_val {
                        min_val = cmnd[t];
                        min_tau = t;
                    } else if cmnd[t] > min_val + 0.01 {
                        // Rising out of dip
                        break;
                    }
                }

                return Some(min_tau);
            }
        }

        // NOTE: no bare global-minimum fallback. When no lag drives cmnd below
        // the threshold, the only thing left is a shallow dip — precisely the
        // regime (inharmonic bass, decaying treble) where a partial's period
        // wins and the "best" minimum is an octave/sub-harmonic off the true
        // fundamental. Without a target-note prior to sanity-check it here,
        // reporting no pitch is safer than reporting a confidently wrong one;
        // the caller shows no reading for the frame.
        None
    }

    /// Step 4b: Octave-error post-check.
    ///
    /// Given the chosen lag `tau`, look for the dip near `2*tau` (the next
    /// octave down). If that lower-frequency dip is *deeper* than the current
    /// one by more than [`Self::OCTAVE_MARGIN`], then `tau` was a partial and
    /// the fundamental sits at the longer lag — return it so the caller adopts
    /// it.
    ///
    /// NOTE: the guard compares interpolated dip *depths*, not raw cmnd bins,
    /// and it is a deeper-by-a-margin test rather than the naive "cmnd[2*tau]
    /// within a small margin of cmnd[tau]" rule. Two reasons:
    /// - On a clean tone the dips at every period multiple all bottom out at ~0,
    ///   so a "within margin" test would octave-*down* every clean note; only a
    ///   genuinely deeper low dip should win.
    /// - At small tau (treble), the true minimum falls between integer samples,
    ///   so the raw `cmnd[tau]` bin is inflated (~0.03 for a clean 3.5 kHz tone)
    ///   even though the note is unambiguous. Parabolic interpolation recovers
    ///   the real ~0 depth, keeping clean treble from being pulled down.
    fn octave_below(&self, cmnd: &[f32], tau: usize, tau_max: usize) -> Option<usize> {
        let target = tau * 2;
        if target >= tau_max || target >= cmnd.len() {
            return None;
        }

        // Inharmonic partials sit slightly off an exact octave, so search a
        // small window around 2*tau for the local minimum rather than trusting
        // the exact bin.
        let window = (tau / 20).max(1);
        let lo = target.saturating_sub(window).max(1);
        let hi = (target + window).min(cmnd.len() - 1).min(tau_max - 1);
        let mut best = lo;
        for t in (lo + 1)..=hi {
            if cmnd[t] < cmnd[best] {
                best = t;
            }
        }

        let gap = self.dip_depth(cmnd, tau) - self.dip_depth(cmnd, best);
        (gap > Self::OCTAVE_MARGIN).then_some(best)
    }

    /// Interpolated depth of the dip at `tau`: the value of the parabola fit
    /// through `cmnd[tau-1..=tau+1]` at its vertex, clamped to be non-negative.
    /// This recovers the true minimum when it lies between integer samples,
    /// which matters at short lags (high frequencies) where the raw bin value
    /// overstates how shallow the dip really is.
    fn dip_depth(&self, cmnd: &[f32], tau: usize) -> f32 {
        if tau == 0 || tau + 1 >= cmnd.len() {
            return cmnd[tau];
        }
        let s0 = cmnd[tau - 1];
        let s1 = cmnd[tau];
        let s2 = cmnd[tau + 1];
        let denominator = s0 - 2.0 * s1 + s2;
        if denominator.abs() < 1e-10 {
            return s1;
        }
        let diff = s2 - s0;
        (s1 - diff * diff / (8.0 * denominator)).max(0.0)
    }

    /// Step 5: Parabolic interpolation for sub-sample accuracy.
    fn parabolic_interpolation(&self, cmnd: &[f32], tau: usize) -> f32 {
        if tau == 0 || tau >= cmnd.len() - 1 {
            return tau as f32;
        }

        let s0 = cmnd[tau - 1];
        let s1 = cmnd[tau];
        let s2 = cmnd[tau + 1];

        // Vertex of parabola through three points
        let denominator = 2.0 * (s0 - 2.0 * s1 + s2);

        if denominator.abs() < 1e-10 {
            return tau as f32;
        }

        let delta = (s0 - s2) / denominator;

        tau as f32 + delta.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::traits::TestAudioSource;

    const SAMPLE_RATE: u32 = 44100;

    fn detect_frequency(frequency: f32) -> Option<PitchResult> {
        let source = TestAudioSource::sine(frequency, 0.2, SAMPLE_RATE);
        let detector = PitchDetector::new(SAMPLE_RATE);
        detector.detect(source.samples())
    }

    /// Synthesize an inharmonic partial stack with an explicit per-partial
    /// amplitude, so the fundamental can be made weak or absent, and
    /// stiff-string inharmonicity f_n = n*f0*sqrt(1 + B*n^2). Real piano
    /// strings look like this (weak bass fundamentals, partials stretched
    /// sharp); the pure-sine fixtures do not, which is exactly why they never
    /// exercise the octave-error path.
    fn inharmonic_stack(f0: f32, partials: &[(f32, f32)], b: f32, dur: f32) -> Vec<f32> {
        let n = (SAMPLE_RATE as f32 * dur) as usize;
        let mut s = vec![0.0f32; n];
        for &(harmonic, amp) in partials {
            let fk = harmonic * f0 * (1.0 + b * harmonic * harmonic).sqrt();
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f32 / SAMPLE_RATE as f32;
                *v += amp * (2.0 * std::f32::consts::PI * fk * t).sin();
            }
        }
        let max = s.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        if max > 0.0 {
            for v in &mut s {
                *v /= max;
            }
        }
        s
    }

    #[test]
    fn test_weak_fundamental_no_octave_up() {
        // A2 (110 Hz) with a weak fundamental, dominant even partials, and
        // mild inharmonicity. YIN's first-dip rule crosses threshold at the
        // 2nd partial's period first and reports ~220 Hz — an octave up —
        // unless the 2*tau post-check pulls it back to the true fundamental.
        let signal = inharmonic_stack(
            110.0,
            &[
                (1.0, 0.1),
                (2.0, 1.0),
                (3.0, 0.1),
                (4.0, 0.7),
                (5.0, 0.08),
                (6.0, 0.5),
            ],
            0.0002,
            0.1,
        );
        let result = PitchDetector::new(SAMPLE_RATE)
            .detect(&signal)
            .expect("should detect a pitch");
        let ratio = result.frequency / 110.0;
        assert!(
            (0.97..1.03).contains(&ratio),
            "expected ~110 Hz fundamental, got {} (ratio {:.2} — octave error)",
            result.frequency,
            ratio
        );
    }

    #[test]
    fn test_shallow_dip_rejected_not_guessed() {
        // Heavily inharmonic bass: no lag ever drives cmnd below the detection
        // threshold, so only a shallow global minimum (~0.28) remains. The old
        // bare `<0.5` fallback returned that minimum unvalidated, landing on a
        // sub-harmonic period (~37 Hz, a fifth below the 55 Hz string). With no
        // target-note prior at this layer, reporting nothing is safer than
        // reporting a wrong pitch — the UI just shows no reading for the frame.
        let signal = inharmonic_stack(
            55.0,
            &[(1.0, 0.1), (2.0, 1.0), (3.0, 0.8), (4.0, 0.6), (5.0, 0.4)],
            0.02,
            0.1,
        );
        let result = PitchDetector::new(SAMPLE_RATE).detect(&signal);
        assert!(
            result.is_none(),
            "shallow-dip signal must be rejected, got {:?}",
            result.map(|r| r.frequency)
        );
    }

    #[test]
    fn test_octave_check_keeps_strong_fundamental() {
        // Regression guard: with a strong fundamental the octave post-check
        // must NOT pull the reading down an octave (cmnd at 2*tau is a hair
        // shallower than at tau for a real fundamental, so it stays put).
        let signal = inharmonic_stack(
            110.0,
            &[(1.0, 1.0), (2.0, 0.6), (3.0, 0.3), (4.0, 0.2)],
            0.0002,
            0.1,
        );
        let result = PitchDetector::new(SAMPLE_RATE)
            .detect(&signal)
            .expect("should detect a pitch");
        let ratio = result.frequency / 110.0;
        assert!(
            (0.97..1.03).contains(&ratio),
            "expected ~110 Hz, got {} (ratio {:.2} — over-corrected down)",
            result.frequency,
            ratio
        );
    }

    #[test]
    fn test_detect_a4_440hz() {
        let result = detect_frequency(440.0).expect("Should detect pitch");
        let error = (result.frequency - 440.0).abs();
        assert!(
            error < 0.5,
            "Expected ~440Hz, got {} (error: {})",
            result.frequency,
            error
        );
        assert!(
            result.confidence > 0.9,
            "Expected high confidence, got {}",
            result.confidence
        );
    }

    #[test]
    fn test_detect_a0_27_5hz() {
        let result = detect_frequency(27.5).expect("Should detect pitch");
        let error = (result.frequency - 27.5).abs();
        assert!(
            error < 0.5,
            "Expected ~27.5Hz, got {} (error: {})",
            result.frequency,
            error
        );
    }

    #[test]
    fn test_detect_c8_4186hz() {
        let result = detect_frequency(4186.0).expect("Should detect pitch");
        let error = (result.frequency - 4186.0).abs();
        // Higher frequencies have more absolute error due to sample rate limitations
        assert!(
            error < 10.0,
            "Expected ~4186Hz, got {} (error: {})",
            result.frequency,
            error
        );
    }

    #[test]
    fn test_detect_middle_c_261hz() {
        let result = detect_frequency(261.63).expect("Should detect pitch");
        let error = (result.frequency - 261.63).abs();
        assert!(
            error < 0.5,
            "Expected ~261.63Hz, got {} (error: {})",
            result.frequency,
            error
        );
    }

    #[test]
    fn test_detect_with_harmonics() {
        // Piano-like signal with harmonics
        let source = TestAudioSource::sine_with_harmonics(
            440.0,
            &[(2.0, 0.5), (3.0, 0.3), (4.0, 0.2)],
            0.2,
            SAMPLE_RATE,
        );
        let detector = PitchDetector::new(SAMPLE_RATE);
        let result = detector
            .detect(source.samples())
            .expect("Should detect pitch");

        let error = (result.frequency - 440.0).abs();
        assert!(
            error < 1.0,
            "Expected ~440Hz fundamental, got {} (error: {})",
            result.frequency,
            error
        );
    }

    #[test]
    fn test_silence_returns_none() {
        let silence = vec![0.0; 4096];
        let detector = PitchDetector::new(SAMPLE_RATE);
        let result = detector.detect(&silence);
        assert!(result.is_none(), "Silence should return None");
    }

    #[test]
    fn test_noise_low_confidence() {
        // Generate noise that's less periodic by using a better PRNG and mixing
        let mut noise = Vec::with_capacity(8192);
        let mut x = 12345_u64;
        for i in 0..8192 {
            // xorshift64
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            // Add some non-linear mixing with index
            let sample = ((x as f64 / u64::MAX as f64) * 2.0 - 1.0) as f32;
            // Mix in high-frequency component to break periodicity
            let high_freq = ((i as f32 * 0.7654321) * std::f32::consts::PI * 73.0).sin() * 0.3;
            noise.push((sample + high_freq).clamp(-1.0, 1.0));
        }

        let detector = PitchDetector::new(SAMPLE_RATE).with_threshold(0.1);
        let result = detector.detect(&noise);

        // Noise should either return None or the detector should reject it
        // due to the threshold (which results in None)
        assert!(
            result.is_none(),
            "Noise should not produce a confident pitch detection"
        );
    }

    #[test]
    fn test_high_threshold_stricter() {
        let source = TestAudioSource::sine(440.0, 0.1, SAMPLE_RATE);

        let loose_detector = PitchDetector::new(SAMPLE_RATE).with_threshold(0.5);
        let strict_detector = PitchDetector::new(SAMPLE_RATE).with_threshold(0.01);

        // Both should detect the clear sine wave
        assert!(loose_detector.detect(source.samples()).is_some());
        assert!(strict_detector.detect(source.samples()).is_some());
    }

    #[test]
    fn test_detect_at_production_window_size() {
        // NOTE: the interactive tuner feeds the detector 100 ms windows
        // (sample_rate / 10 in main.rs). At A0 = 27.5 Hz that is only ~2.75
        // periods and tau_max sits close to the samples.len()/2 guard in
        // detect() — the tightest operating point in the app. Guard it so
        // window/tau changes can't silently break bass detection.
        for &freq in &[27.5_f32, 55.0, 440.0] {
            let source = TestAudioSource::sine(freq, 0.1, SAMPLE_RATE);
            let result = PitchDetector::new(SAMPLE_RATE)
                .detect(source.samples())
                .unwrap_or_else(|| panic!("Should detect {}Hz in a 100ms window", freq));

            let relative_error = (result.frequency - freq).abs() / freq;
            assert!(
                relative_error < 0.01,
                "Expected {}Hz at 100ms window, got {} (relative error: {:.2}%)",
                freq,
                result.frequency,
                relative_error * 100.0
            );
        }
    }

    #[test]
    fn test_various_frequencies() {
        // Test across the piano range
        let test_freqs = [55.0, 110.0, 220.0, 440.0, 880.0, 1760.0, 3520.0];

        for &freq in &test_freqs {
            let result =
                detect_frequency(freq).unwrap_or_else(|| panic!("Should detect {}Hz", freq));

            let error = (result.frequency - freq).abs();
            let relative_error = error / freq;

            assert!(
                relative_error < 0.01,
                "Expected {}Hz, got {} (relative error: {:.2}%)",
                freq,
                result.frequency,
                relative_error * 100.0
            );
        }
    }
}
