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

    /// Create a test source with an inharmonic partial stack.
    ///
    /// Real piano strings are stiff: partial `n` sits sharp of `n * f0` by a
    /// factor `sqrt(1 + B*n^2)`, where `B` is the inharmonicity coefficient.
    /// Each entry in `partials` is `(partial number, amplitude)`; the
    /// fundamental is just partial 1 and may be omitted entirely to model the
    /// weak/missing bass fundamental that defeats time-domain detectors.
    ///
    /// Unlike [`sine_with_harmonics`](Self::sine_with_harmonics), the output is
    /// not normalized, so the requested amplitudes survive into the spectrum
    /// for amplitude-recovery tests.
    pub fn inharmonic(
        fundamental: f32,
        b: f32,
        partials: &[(u16, f32)], // (partial number, amplitude)
        duration_secs: f32,
        sample_rate: u32,
    ) -> Self {
        let num_samples = (sample_rate as f32 * duration_secs) as usize;
        let mut samples = vec![0.0; num_samples];

        for &(n, amplitude) in partials {
            let nf = n as f32;
            let freq = nf * fundamental * (1.0 + b * nf * nf).sqrt();
            for (i, sample) in samples.iter_mut().enumerate() {
                let t = i as f32 / sample_rate as f32;
                *sample += amplitude * (2.0 * std::f32::consts::PI * freq * t).sin();
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

/// Average interleaved frames of `channels` samples down to mono, filling
/// `buffer`. Returns the number of mono samples written.
fn downmix_to_mono(
    mut samples: impl Iterator<Item = f32>,
    channels: usize,
    buffer: &mut [f32],
) -> usize {
    if channels == 0 {
        return 0;
    }

    let mut count = 0;
    while count < buffer.len() {
        let mut sum = 0.0;
        let mut got = 0;
        for _ in 0..channels {
            match samples.next() {
                Some(s) => {
                    sum += s;
                    got += 1;
                }
                None => break,
            }
        }
        if got == 0 {
            break;
        }
        buffer[count] = sum / got as f32;
        count += 1;
    }

    count
}

impl<R: Read + Seek + Send> AudioSource for WavAudioSource<R> {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize {
        let spec = self.reader.spec();
        // NOTE: hound yields samples interleaved across channels; average each
        // frame to mono, otherwise a stereo file reads stretched 2x in time
        // and every pitch detects exactly one octave low.
        let channels = spec.channels as usize;

        match spec.sample_format {
            hound::SampleFormat::Float => {
                let samples = self.reader.samples::<f32>().map_while(Result::ok);
                downmix_to_mono(samples, channels, buffer)
            }
            hound::SampleFormat::Int => {
                // WARNING: hound accepts WAVE_FORMAT_EXTENSIBLE headers whose
                // wValidBitsPerSample exceeds 32; shifting by that overflows
                // (a panic in debug builds), so reject such files instead.
                if !(1..=32).contains(&spec.bits_per_sample) {
                    return 0;
                }
                let max_val = (1u64 << (spec.bits_per_sample - 1)) as f32;
                let samples = self
                    .reader
                    .samples::<i32>()
                    .map_while(Result::ok)
                    .map(|s| s as f32 / max_val);
                downmix_to_mono(samples, channels, buffer)
            }
        }
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

    /// Write an in-memory 16-bit WAV with the given channel count, duplicating
    /// the sine value across all channels of each frame.
    fn wav_sine_i16(frequency: f32, duration: f32, sample_rate: u32, channels: u16) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for i in 0..(sample_rate as f32 * duration) as usize {
            let t = i as f32 / sample_rate as f32;
            let s = (2.0 * std::f32::consts::PI * frequency * t).sin();
            let v = (s * i16::MAX as f32) as i16;
            for _ in 0..channels {
                writer.write_sample(v).unwrap();
            }
        }
        writer.finalize().unwrap();

        cursor.into_inner()
    }

    #[test]
    fn test_downmix_to_mono_averages_frames() {
        let interleaved = [0.8f32, 0.2, -0.4, -0.6, 1.0]; // trailing partial frame
        let mut buffer = [0.0f32; 4];

        let count = downmix_to_mono(interleaved.iter().copied(), 2, &mut buffer);

        assert_eq!(count, 3);
        assert!((buffer[0] - 0.5).abs() < 1e-6);
        assert!((buffer[1] + 0.5).abs() < 1e-6);
        assert!((buffer[2] - 1.0).abs() < 1e-6); // partial frame: average of what's there
    }

    #[test]
    fn test_wav_mono_int_reads_normalized() {
        let bytes = wav_sine_i16(440.0, 0.05, 44100, 1);
        let mut source = WavAudioSource::new(std::io::Cursor::new(bytes)).unwrap();

        let mut buffer = vec![0.0f32; 2205];
        let read = source.read_samples(&mut buffer);

        assert_eq!(read, 2205);
        let max = buffer.iter().cloned().fold(0.0_f32, f32::max);
        assert!(max > 0.9, "expected near full scale, got {}", max);
    }

    #[test]
    fn test_wav_stereo_downmixes_one_sample_per_frame() {
        let bytes = wav_sine_i16(440.0, 0.05, 44100, 2);
        let mut source = WavAudioSource::new(std::io::Cursor::new(bytes)).unwrap();

        // 0.05s at 44.1kHz = 2205 frames; a stereo file must yield 2205 mono
        // samples, not 4410 interleaved ones.
        let mut buffer = vec![0.0f32; 4410];
        let read = source.read_samples(&mut buffer);
        assert_eq!(read, 2205);
    }

    #[test]
    fn test_wav_stereo_detects_correct_octave() {
        // Regression: interleaved stereo used to read stretched 2x in time,
        // so a 440 Hz file detected as 220 Hz (one octave low).
        let bytes = wav_sine_i16(440.0, 0.2, 44100, 2);
        let mut source = WavAudioSource::new(std::io::Cursor::new(bytes)).unwrap();

        let mut buffer = vec![0.0f32; 8820];
        let read = source.read_samples(&mut buffer);
        assert_eq!(read, 8820);

        let result = crate::audio::PitchDetector::new(44100)
            .detect(&buffer[..read])
            .expect("should detect pitch in stereo WAV");
        assert!(
            (result.frequency - 440.0).abs() < 2.0,
            "expected ~440Hz, got {}",
            result.frequency
        );
    }

    #[test]
    fn test_wav_stereo_float_averages_channels() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for _ in 0..100 {
            writer.write_sample(0.8f32).unwrap();
            writer.write_sample(0.2f32).unwrap();
        }
        writer.finalize().unwrap();

        let mut source = WavAudioSource::new(std::io::Cursor::new(cursor.into_inner())).unwrap();
        let mut buffer = [0.0f32; 200];
        let read = source.read_samples(&mut buffer);

        assert_eq!(read, 100);
        for &s in &buffer[..100] {
            assert!(
                (s - 0.5).abs() < 1e-6,
                "expected L/R average 0.5, got {}",
                s
            );
        }
    }

    /// Hand-craft a WAVE_FORMAT_EXTENSIBLE file whose wValidBitsPerSample
    /// exceeds the container size; hound 3.5 accepts it and reports the raw
    /// value as `bits_per_sample`.
    fn extensible_wav_with_valid_bits(valid_bits: u16) -> Vec<u8> {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&0u32.to_le_bytes()); // placeholder, patched below
        b.extend_from_slice(b"WAVE");
        // fmt chunk (WAVEFORMATEXTENSIBLE, 40 bytes)
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&0xFFFEu16.to_le_bytes()); // wFormatTag: EXTENSIBLE
        b.extend_from_slice(&1u16.to_le_bytes()); // nChannels
        b.extend_from_slice(&44100u32.to_le_bytes()); // nSamplesPerSec
        b.extend_from_slice(&88200u32.to_le_bytes()); // nAvgBytesPerSec
        b.extend_from_slice(&2u16.to_le_bytes()); // nBlockAlign
        b.extend_from_slice(&16u16.to_le_bytes()); // wBitsPerSample
        b.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        b.extend_from_slice(&valid_bits.to_le_bytes()); // wValidBitsPerSample
        b.extend_from_slice(&0u32.to_le_bytes()); // dwChannelMask
                                                  // KSDATAFORMAT_SUBTYPE_PCM
        b.extend_from_slice(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ]);
        // data chunk: two 16-bit samples
        b.extend_from_slice(b"data");
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&1000i16.to_le_bytes());
        b.extend_from_slice(&(-1000i16).to_le_bytes());
        // Patch RIFF chunk size
        let riff_size = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&riff_size.to_le_bytes());
        b
    }

    #[test]
    fn test_wav_oversized_valid_bits_does_not_panic() {
        // Regression: `1 << (bits_per_sample - 1)` overflowed (debug panic)
        // when a crafted EXTENSIBLE header claimed more than 32 valid bits.
        for valid_bits in [64u16, 999] {
            let bytes = extensible_wav_with_valid_bits(valid_bits);
            let mut source =
                WavAudioSource::new(std::io::Cursor::new(bytes)).expect("hound accepts the header");

            let mut buffer = [0.0f32; 16];
            assert_eq!(source.read_samples(&mut buffer), 0);
        }
    }
}
