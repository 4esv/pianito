//! Reference tone generation.

use super::traits::AudioSink;

/// Reference tone generator for pure sine waves.
pub struct ReferenceTone {
    sample_rate: u32,
}

impl ReferenceTone {
    /// Create a new reference tone generator.
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Generate a sine wave at the given frequency.
    pub fn generate(&self, frequency: f32, duration_secs: f32) -> Vec<f32> {
        let num_samples = (self.sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / self.sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push(sample);
        }

        samples
    }

    /// Play a reference tone through the given sink.
    pub fn play<S: AudioSink>(&self, sink: &mut S, frequency: f32, duration_secs: f32) {
        let samples = self.generate(frequency, duration_secs);
        sink.write_samples(&samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::traits::TestAudioSink;

    #[test]
    fn test_generate_correct_sample_count() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 1.0);
        assert_eq!(samples.len(), 44100);
    }

    #[test]
    fn test_generate_half_second() {
        let gen = ReferenceTone::new(48000);
        let samples = gen.generate(440.0, 0.5);
        assert_eq!(samples.len(), 24000);
    }

    #[test]
    fn test_generate_short_duration() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 0.1);
        assert_eq!(samples.len(), 4410);
    }

    #[test]
    fn test_sine_wave_range() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 0.1);

        let max = samples.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min = samples.iter().cloned().fold(f32::INFINITY, f32::min);

        // Sine wave should be in range [-1, 1]
        assert!(max > 0.99 && max <= 1.0, "max should be ~1.0, got {}", max);
        assert!(
            min < -0.99 && min >= -1.0,
            "min should be ~-1.0, got {}",
            min
        );
    }

    #[test]
    fn test_zero_crossings_match_frequency() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 1.0);

        // Count positive zero crossings
        let mut crossings = 0;
        for i in 1..samples.len() {
            if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
                crossings += 1;
            }
        }

        // 440 Hz should have ~440 positive zero crossings in 1 second
        assert!(
            (crossings as f32 - 440.0).abs() < 2.0,
            "Expected ~440 crossings, got {}",
            crossings
        );
    }

    #[test]
    fn test_zero_crossings_middle_c() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(261.63, 1.0); // Middle C

        let mut crossings = 0;
        for i in 1..samples.len() {
            if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
                crossings += 1;
            }
        }

        assert!(
            (crossings as f32 - 261.63).abs() < 2.0,
            "Expected ~262 crossings, got {}",
            crossings
        );
    }

    #[test]
    fn test_different_sample_rates() {
        let gen1 = ReferenceTone::new(44100);
        let gen2 = ReferenceTone::new(48000);

        let samples1 = gen1.generate(440.0, 1.0);
        let samples2 = gen2.generate(440.0, 1.0);

        assert_eq!(samples1.len(), 44100);
        assert_eq!(samples2.len(), 48000);
    }

    #[test]
    fn test_play_sends_to_sink() {
        let gen = ReferenceTone::new(44100);
        let mut sink = TestAudioSink::new(44100);

        gen.play(&mut sink, 440.0, 0.1);

        assert_eq!(sink.samples().len(), 4410);
    }

    #[test]
    fn test_play_multiple_times() {
        let gen = ReferenceTone::new(44100);
        let mut sink = TestAudioSink::new(44100);

        gen.play(&mut sink, 440.0, 0.1);
        gen.play(&mut sink, 880.0, 0.1);

        // Should have accumulated both
        assert_eq!(sink.samples().len(), 8820);
    }

    #[test]
    fn test_zero_duration() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 0.0);
        assert_eq!(samples.len(), 0);
    }

    #[test]
    fn test_sine_wave_starts_at_zero() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 0.1);
        assert!(samples[0].abs() < 0.01, "First sample should be ~0");
    }
}
