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

    /// Get note by MIDI number.
    pub fn from_midi(midi: u8) -> Option<&'static Note> {
        if !(21..=108).contains(&midi) {
            return None;
        }
        NOTES.get((midi - 21) as usize)
    }

    /// Get note by name (e.g., "A4", "C#5").
    pub fn from_name(name: &str) -> Option<&'static Note> {
        NOTES.iter().find(|n| n.display_name() == name)
    }
}

/// Note names in chromatic order.
const NOTE_NAMES: [&str; 12] = [
    "A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#",
];

/// Generate all 88 piano notes.
/// Piano range: A0 (MIDI 21) to C8 (MIDI 108)
///
/// String counts:
/// - A0 to Bb1 (MIDI 21-34): 1 string (monochord)
/// - B1 to G#3 (MIDI 35-56): 2 strings (bichord)
/// - A3 to C8 (MIDI 57-108): 3 strings (trichord)
const fn generate_notes() -> [Note; 88] {
    let mut notes = [Note::new(0, "", 0, 0); 88];
    let mut i = 0;

    while i < 88 {
        let midi = (i + 21) as u8;

        // Calculate octave and note index
        // MIDI 21 = A0, MIDI 24 = C1, etc.
        // A is at position 0 in our NOTE_NAMES array
        let semitones_from_a0 = i;
        let octave = if semitones_from_a0 < 3 {
            0 // A0, A#0, B0
        } else {
            ((semitones_from_a0 - 3) / 12 + 1) as i8
        };

        let note_idx = semitones_from_a0 % 12;

        // Determine string count
        // Bb1 (A#1) = MIDI 34, B1 = MIDI 35, A3 = MIDI 57
        let strings = if midi <= 34 {
            1 // A0-Bb1: monochord
        } else if midi <= 56 {
            2 // B1-G#3: bichord
        } else {
            3 // A3-C8: trichord
        };

        notes[i] = Note::new(midi, NOTE_NAMES[note_idx], octave, strings);
        i += 1;
    }

    notes
}

/// All 88 piano notes from A0 to C8.
pub static NOTES: [Note; 88] = generate_notes();

/// Get a note by index (0 = A0, 87 = C8).
pub fn note_at(index: usize) -> Option<&'static Note> {
    NOTES.get(index)
}

/// Total number of notes on a standard piano.
pub const NOTE_COUNT: usize = 88;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_count() {
        assert_eq!(NOTES.len(), 88);
    }

    #[test]
    fn test_first_note_a0() {
        let a0 = &NOTES[0];
        assert_eq!(a0.midi, 21);
        assert_eq!(a0.name, "A");
        assert_eq!(a0.octave, 0);
        assert_eq!(a0.strings, 1);
        assert_eq!(a0.display_name(), "A0");
    }

    #[test]
    fn test_last_note_c8() {
        let c8 = &NOTES[87];
        assert_eq!(c8.midi, 108);
        assert_eq!(c8.name, "C");
        assert_eq!(c8.octave, 8);
        assert_eq!(c8.strings, 3);
        assert_eq!(c8.display_name(), "C8");
    }

    #[test]
    fn test_middle_c() {
        // Middle C is C4, MIDI 60
        let c4 = Note::from_midi(60).expect("C4 should exist");
        assert_eq!(c4.name, "C");
        assert_eq!(c4.octave, 4);
        assert_eq!(c4.display_name(), "C4");
    }

    #[test]
    fn test_a4_concert_pitch() {
        // A4 is MIDI 69
        let a4 = Note::from_midi(69).expect("A4 should exist");
        assert_eq!(a4.name, "A");
        assert_eq!(a4.octave, 4);
        assert_eq!(a4.display_name(), "A4");
    }

    #[test]
    fn test_string_counts() {
        // Monochord: A0-Bb1 (MIDI 21-34)
        assert_eq!(Note::from_midi(21).unwrap().strings, 1); // A0
        assert_eq!(Note::from_midi(34).unwrap().strings, 1); // Bb1 (A#1)

        // Bichord: B1-G#3 (MIDI 35-56)
        assert_eq!(Note::from_midi(35).unwrap().strings, 2); // B1
        assert_eq!(Note::from_midi(56).unwrap().strings, 2); // G#3

        // Trichord: A3-C8 (MIDI 57-108)
        assert_eq!(Note::from_midi(57).unwrap().strings, 3); // A3
        assert_eq!(Note::from_midi(108).unwrap().strings, 3); // C8
    }

    #[test]
    fn test_from_name() {
        let a4 = Note::from_name("A4").expect("A4 should exist");
        assert_eq!(a4.midi, 69);

        let csharp5 = Note::from_name("C#5").expect("C#5 should exist");
        assert_eq!(csharp5.midi, 73);
    }

    #[test]
    fn test_trichord_detection() {
        assert!(!Note::from_midi(21).unwrap().is_trichord()); // A0 (monochord)
        assert!(!Note::from_midi(35).unwrap().is_trichord()); // B1 (bichord)
        assert!(!Note::from_midi(56).unwrap().is_trichord()); // G#3 (bichord)
        assert!(Note::from_midi(57).unwrap().is_trichord()); // A3 (trichord)
        assert!(Note::from_midi(69).unwrap().is_trichord()); // A4 (trichord)
    }

    #[test]
    fn test_midi_sequence() {
        // Verify MIDI numbers are sequential
        for (i, note) in NOTES.iter().enumerate() {
            assert_eq!(note.midi as usize, i + 21);
        }
    }

    #[test]
    fn test_chromatic_sequence() {
        // Check that notes follow chromatic order
        let expected_sequence = [
            ("A", 0),
            ("A#", 0),
            ("B", 0),
            ("C", 1),
            ("C#", 1),
            ("D", 1),
            ("D#", 1),
            ("E", 1),
            ("F", 1),
            ("F#", 1),
            ("G", 1),
            ("G#", 1),
            ("A", 1),
            ("A#", 1),
            ("B", 1),
            ("C", 2),
        ];

        for (i, &(name, octave)) in expected_sequence.iter().enumerate() {
            let note = &NOTES[i];
            assert_eq!(note.name, name, "Note {} should be {}", i, name);
            assert_eq!(
                note.octave, octave,
                "Note {} octave should be {}",
                i, octave
            );
        }
    }
}
