//! Equal temperament calculations.

/// Equal temperament calculator.
pub struct Temperament {
    /// Reference frequency for A4.
    a4_freq: f32,
}

impl Temperament {
    /// Create a new temperament with A4 = 440 Hz.
    pub fn new() -> Self {
        Self { a4_freq: 440.0 }
    }

    /// Create a temperament with a custom A4 reference.
    pub fn with_a4(a4_freq: f32) -> Self {
        Self { a4_freq }
    }

    /// Get the A4 reference frequency.
    pub fn a4(&self) -> f32 {
        self.a4_freq
    }

    /// Calculate the frequency for a given MIDI note number.
    pub fn frequency(&self, midi_note: u8) -> f32 {
        // A4 is MIDI note 69
        self.a4_freq * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
    }

    /// Convert a frequency to cents deviation from a target.
    pub fn cents_from_target(&self, frequency: f32, target: f32) -> f32 {
        1200.0 * (frequency / target).log2()
    }

    /// Convert cents deviation to frequency ratio.
    pub fn cents_to_ratio(cents: f32) -> f32 {
        2.0_f32.powf(cents / 1200.0)
    }
}

impl Default for Temperament {
    fn default() -> Self {
        Self::new()
    }
}
