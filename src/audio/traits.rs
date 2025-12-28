//! Audio I/O traits for abstraction and mocking.

use std::io::{Read, Seek};

/// Audio input source trait.
pub trait AudioSource {
    /// Read samples into the buffer, returning the number of samples read.
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize;

    /// Get the sample rate in Hz.
    fn sample_rate(&self) -> u32;
}

/// Audio output sink trait.
pub trait AudioSink {
    /// Write samples to the output.
    fn write_samples(&mut self, samples: &[f32]);

    /// Get the sample rate in Hz.
    fn sample_rate(&self) -> u32;
}

/// Test audio source backed by a buffer.
pub struct TestAudioSource {
    samples: Vec<f32>,
    position: usize,
    sample_rate: u32,
}

impl TestAudioSource {
    /// Create a new test source from samples.
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            position: 0,
            sample_rate,
        }
    }

    /// Create a test source with a sine wave.
    pub fn sine(frequency: f32, duration_secs: f32, sample_rate: u32) -> Self {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let sample = (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push(sample);
        }

        Self::new(samples, sample_rate)
    }

    /// Create a test source with a sine wave plus harmonics.
    pub fn sine_with_harmonics(
        fundamental: f32,
        harmonics: &[(f32, f32)], // (harmonic number, amplitude)
        duration_secs: f32,
        sample_rate: u32,
    ) -> Self {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = vec![0.0; num_samples];

        // Add fundamental
        for (i, sample) in samples.iter_mut().enumerate() {
            let t = i as f32 / sample_rate as f32;
            *sample += (2.0 * std::f32::consts::PI * fundamental * t).sin();
        }

        // Add harmonics
        for &(harmonic_num, amplitude) in harmonics {
            let freq = fundamental * harmonic_num;
            for (i, sample) in samples.iter_mut().enumerate() {
                let t = i as f32 / sample_rate as f32;
                *sample += amplitude * (2.0 * std::f32::consts::PI * freq * t).sin();
            }
        }

        // Normalize
        let max = samples.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        if max > 0.0 {
            for sample in &mut samples {
                *sample /= max;
            }
        }

        Self::new(samples, sample_rate)
    }

    /// Reset position to start.
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get a reference to the underlying samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

impl AudioSource for TestAudioSource {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize {
        let remaining = self.samples.len() - self.position;
        let to_read = buffer.len().min(remaining);

        buffer[..to_read].copy_from_slice(&self.samples[self.position..self.position + to_read]);
        self.position += to_read;

        to_read
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// WAV file audio source.
pub struct WavAudioSource<R: Read + Seek> {
    reader: hound::WavReader<R>,
    sample_rate: u32,
}

impl<R: Read + Seek + Send> WavAudioSource<R> {
    /// Create a new WAV source from a reader.
    pub fn new(reader: R) -> Result<Self, hound::Error> {
        let wav_reader = hound::WavReader::new(reader)?;
        let sample_rate = wav_reader.spec().sample_rate;

        Ok(Self {
            reader: wav_reader,
            sample_rate,
        })
    }
}

impl WavAudioSource<std::io::BufReader<std::fs::File>> {
    /// Open a WAV file from path.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, hound::Error> {
        let wav_reader = hound::WavReader::open(path)?;
        let sample_rate = wav_reader.spec().sample_rate;

        Ok(Self {
            reader: wav_reader,
            sample_rate,
        })
    }
}

impl<R: Read + Seek + Send> AudioSource for WavAudioSource<R> {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize {
        let spec = self.reader.spec();
        let mut count = 0;

        match spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(buffer.len()).flatten() {
                    buffer[count] = s;
                    count += 1;
                }
            }
            hound::SampleFormat::Int => {
                let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
                for s in self.reader.samples::<i32>().take(buffer.len()).flatten() {
                    buffer[count] = s as f32 / max_val;
                    count += 1;
                }
            }
        }

        count
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Test audio sink that collects samples.
pub struct TestAudioSink {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl TestAudioSink {
    /// Create a new test sink.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate,
        }
    }

    /// Get collected samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Clear collected samples.
    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

impl AudioSink for TestAudioSink {
    fn write_samples(&mut self, samples: &[f32]) {
        self.samples.extend_from_slice(samples);
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_source_reads_samples() {
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let mut source = TestAudioSource::new(samples.clone(), 44100);

        let mut buffer = [0.0; 3];
        assert_eq!(source.read_samples(&mut buffer), 3);
        assert_eq!(buffer, [0.1, 0.2, 0.3]);

        assert_eq!(source.read_samples(&mut buffer), 2);
        assert_eq!(&buffer[..2], &[0.4, 0.5]);
    }

    #[test]
    fn test_sine_generation() {
        let source = TestAudioSource::sine(440.0, 0.1, 44100);
        assert_eq!(source.samples.len(), 4410);
        assert_eq!(source.sample_rate(), 44100);

        // Check that samples oscillate around zero
        let max = source.samples.iter().cloned().fold(0.0_f32, f32::max);
        let min = source.samples.iter().cloned().fold(0.0_f32, f32::min);
        assert!(max > 0.9, "max should be close to 1.0, got {}", max);
        assert!(min < -0.9, "min should be close to -1.0, got {}", min);
    }

    #[test]
    fn test_audio_sink_collects() {
        let mut sink = TestAudioSink::new(44100);
        sink.write_samples(&[0.1, 0.2]);
        sink.write_samples(&[0.3, 0.4]);
        assert_eq!(sink.samples(), &[0.1, 0.2, 0.3, 0.4]);
    }
}
