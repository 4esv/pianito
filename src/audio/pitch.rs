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
        let tau = self.find_threshold_crossing(&cmnd, tau_min, tau_max)?;

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

        // If no threshold crossing, find absolute minimum (fallback)
        let mut min_tau = tau_min;
        let mut min_val = cmnd[tau_min];

        #[allow(clippy::needless_range_loop)]
        for tau in tau_min + 1..tau_max {
            if cmnd[tau] < min_val {
                min_val = cmnd[tau];
                min_tau = tau;
            }
        }

        // Only return if it's a reasonable minimum
        if min_val < 0.5 {
            Some(min_tau)
        } else {
            None
        }
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
