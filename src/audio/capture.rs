//! Microphone input capture using cpal.

use super::traits::AudioSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// Error type for audio capture.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("No input device available")]
    NoInputDevice,
    #[error("Failed to get device config: {0}")]
    ConfigError(#[from] cpal::DefaultStreamConfigError),
    #[error("Failed to build stream: {0}")]
    BuildStreamError(#[from] cpal::BuildStreamError),
    #[error("Failed to play stream: {0}")]
    PlayStreamError(#[from] cpal::PlayStreamError),
}

/// Shared buffer for audio samples.
struct SharedBuffer {
    samples: Vec<f32>,
    read_pos: usize,
}

/// Microphone capture using the system's default input device.
pub struct MicCapture {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<SharedBuffer>>,
    sample_rate: u32,
}

impl MicCapture {
    /// Create a new microphone capture instance.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice)?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;

        let buffer = Arc::new(Mutex::new(SharedBuffer {
            samples: Vec::with_capacity(sample_rate as usize), // 1 second buffer
            read_pos: 0,
        }));

        let buffer_clone = Arc::clone(&buffer);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => Self::build_stream_f32(&device, &config.into(), buffer_clone)?,
            cpal::SampleFormat::I16 => Self::build_stream_i16(&device, &config.into(), buffer_clone)?,
            _ => {
                return Err(CaptureError::BuildStreamError(
                    cpal::BuildStreamError::StreamConfigNotSupported,
                ));
            }
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            sample_rate,
        })
    }

    fn build_stream_f32(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<SharedBuffer>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let channels = config.channels as usize;

        device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();

                // Convert to mono and append to buffer
                for frame in data.chunks(channels) {
                    let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
                    buf.samples.push(mono);
                }

                // Keep buffer at reasonable size (2 seconds max)
                let max_samples = 88200; // 2 seconds at 44100
                if buf.samples.len() > max_samples {
                    let excess = buf.samples.len() - max_samples;
                    buf.samples.drain(0..excess);
                    buf.read_pos = buf.read_pos.saturating_sub(excess);
                }
            },
            |err| {
                eprintln!("Audio capture error: {}", err);
            },
            None,
        )
    }

    fn build_stream_i16(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<SharedBuffer>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let channels = config.channels as usize;

        device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();

                // Convert to mono f32 and append to buffer
                for frame in data.chunks(channels) {
                    let mono: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / channels as f32;
                    buf.samples.push(mono);
                }

                // Keep buffer at reasonable size (2 seconds max)
                let max_samples = 88200; // 2 seconds at 44100
                if buf.samples.len() > max_samples {
                    let excess = buf.samples.len() - max_samples;
                    buf.samples.drain(0..excess);
                    buf.read_pos = buf.read_pos.saturating_sub(excess);
                }
            },
            |err| {
                eprintln!("Audio capture error: {}", err);
            },
            None,
        )
    }
}

impl AudioSource for MicCapture {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize {
        let mut buf = self.buffer.lock().unwrap();

        let available = buf.samples.len() - buf.read_pos;
        let to_read = buffer.len().min(available);

        if to_read > 0 {
            buffer[..to_read].copy_from_slice(&buf.samples[buf.read_pos..buf.read_pos + to_read]);
            buf.read_pos += to_read;
        }

        to_read
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Audio output sink using cpal.
pub struct AudioOutput {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl AudioOutput {
    /// Create a new audio output instance.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or(CaptureError::NoInputDevice)?;

        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0;

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = Arc::clone(&buffer);

        let channels = config.channels() as usize;

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut buf = buffer_clone.lock().unwrap();

                for frame in data.chunks_mut(channels) {
                    let sample = if !buf.is_empty() {
                        buf.remove(0)
                    } else {
                        0.0
                    };

                    for s in frame.iter_mut() {
                        *s = sample;
                    }
                }
            },
            |err| {
                eprintln!("Audio output error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            sample_rate,
        })
    }

    /// Queue samples for playback.
    pub fn queue(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock().unwrap();
        buf.extend_from_slice(samples);
    }

    /// Get the sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
