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
