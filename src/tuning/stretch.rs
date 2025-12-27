//! Stretch tuning (Railsback curve) for piano inharmonicity compensation.

/// Stretch tuning curve based on the Railsback curve.
pub struct StretchCurve {
    // TODO: Implement lookup table
}

impl StretchCurve {
    /// Create a new stretch curve.
    pub fn new() -> Self {
        Self {}
    }

    /// Get the stretch offset in cents for a given MIDI note.
    pub fn offset_cents(&self, _midi_note: u8) -> f32 {
        todo!("Implement stretch curve")
    }
}

impl Default for StretchCurve {
    fn default() -> Self {
        Self::new()
    }
}
