//! Microphone input capture using cpal.

use super::traits::AudioSource;

/// Microphone capture using the system's default input device.
pub struct MicCapture {
    // TODO: Implement cpal-based capture
}

impl MicCapture {
    /// Create a new microphone capture instance.
    pub fn new() -> anyhow::Result<Self> {
        todo!("Implement MicCapture")
    }
}

impl AudioSource for MicCapture {
    fn read_samples(&mut self, _buffer: &mut [f32]) -> usize {
        todo!("Implement read_samples")
    }

    fn sample_rate(&self) -> u32 {
        44100
    }
}
