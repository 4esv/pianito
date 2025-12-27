//! 88-key piano note definitions.

/// A piano note with its properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    /// MIDI note number (21 = A0, 108 = C8).
    pub midi: u8,
    /// Note name (e.g., "A", "C#").
    pub name: &'static str,
    /// Octave number.
    pub octave: i8,
    /// Number of strings for this note (1, 2, or 3).
    pub strings: u8,
}

impl Note {
    /// Create a new note.
    pub const fn new(midi: u8, name: &'static str, octave: i8, strings: u8) -> Self {
        Self {
            midi,
            name,
            octave,
            strings,
        }
    }

    /// Get display name (e.g., "A4", "C#5").
    pub fn display_name(&self) -> String {
        format!("{}{}", self.name, self.octave)
    }

    /// Check if this is a trichord (3 strings).
    pub fn is_trichord(&self) -> bool {
        self.strings == 3
    }
}

// TODO: Define all 88 notes
/// All 88 piano notes from A0 to C8.
pub static NOTES: &[Note] = &[];
