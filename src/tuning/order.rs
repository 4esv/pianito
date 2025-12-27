//! Tuning order logic for stable piano tuning.

use super::notes::Note;

/// Tuning order generator following traditional piano tuning order.
pub struct TuningOrder {
    // TODO: Implement tuning order
}

impl TuningOrder {
    /// Create a new tuning order.
    pub fn new() -> Self {
        Self {}
    }

    /// Get the ordered list of notes to tune.
    ///
    /// Order:
    /// 1. Temperament octave (F3-F4): 12 notes
    /// 2. Octaves upward (F4→C8)
    /// 3. Octaves downward (F3→A0)
    pub fn notes(&self) -> Vec<&'static Note> {
        todo!("Implement tuning order")
    }
}

impl Default for TuningOrder {
    fn default() -> Self {
        Self::new()
    }
}
