//! Audio I/O traits for abstraction and mocking.

/// Audio input source trait.
pub trait AudioSource: Send {
    /// Read samples into the buffer, returning the number of samples read.
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize;

    /// Get the sample rate in Hz.
    fn sample_rate(&self) -> u32;
}

/// Audio output sink trait.
pub trait AudioSink: Send {
    /// Write samples to the output.
    fn write_samples(&mut self, samples: &[f32]);

    /// Get the sample rate in Hz.
    fn sample_rate(&self) -> u32;
}
