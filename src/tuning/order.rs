//! Tuning order logic for stable piano tuning.
//!
//! Traditional piano tuning follows a specific order for stability:
//! 1. Temperament octave (F3-F4): 12 notes that form the foundation
//! 2. Octaves upward (F4→C8): Each note tuned as octave from below
//! 3. Octaves downward (F3→A0): Each note tuned as octave from above

use super::notes::{Note, NOTES};

/// MIDI note numbers for key reference points.
const F3_MIDI: u8 = 53;
const F4_MIDI: u8 = 65;
const A0_MIDI: u8 = 21;
const C8_MIDI: u8 = 108;

/// Index in NOTES array for key reference points.
const F3_INDEX: usize = (F3_MIDI - A0_MIDI) as usize; // 32
const F4_INDEX: usize = (F4_MIDI - A0_MIDI) as usize; // 44
const C8_INDEX: usize = (C8_MIDI - A0_MIDI) as usize; // 87

/// Tuning order generator following traditional piano tuning order.
pub struct TuningOrder {
    /// Ordered indices into the NOTES array.
    order: Vec<usize>,
}

impl TuningOrder {
    /// Create a new tuning order.
    pub fn new() -> Self {
        Self {
            order: Self::generate_order(),
        }
    }

    /// Generate the traditional tuning order.
    ///
    /// Order:
    /// 1. Temperament octave (F3-F4): 12 notes, indices 32-44
    /// 2. Octaves upward (F#4→C8): indices 45-87
    /// 3. Octaves downward (E3→A0): indices 31-0
    fn generate_order() -> Vec<usize> {
        let mut order = Vec::with_capacity(88);

        // 1. Temperament octave: F3 to F4 (inclusive)
        // This is 13 notes (F3, F#3, G3, G#3, A3, A#3, B3, C4, C#4, D4, D#4, E4, F4)
        for i in F3_INDEX..=F4_INDEX {
            order.push(i);
        }

        // 2. Octaves upward: F#4 to C8
        for i in (F4_INDEX + 1)..=C8_INDEX {
            order.push(i);
        }

        // 3. Octaves downward: E3 to A0
        for i in (0..F3_INDEX).rev() {
            order.push(i);
        }

        order
    }

    /// Get the ordered list of note indices.
    pub fn indices(&self) -> &[usize] {
        &self.order
    }

    /// Get the ordered list of notes to tune.
    pub fn notes(&self) -> Vec<&'static Note> {
        self.order.iter().map(|&i| &NOTES[i]).collect()
    }

    /// Get the note at a specific position in the tuning order.
    pub fn note_at(&self, position: usize) -> Option<&'static Note> {
        self.order.get(position).map(|&i| &NOTES[i])
    }

    /// Get the total number of notes.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Check if the order is empty.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Find the position of a note in the tuning order.
    pub fn position_of(&self, midi: u8) -> Option<usize> {
        if midi < A0_MIDI || midi > C8_MIDI {
            return None;
        }
        let note_index = (midi - A0_MIDI) as usize;
        self.order.iter().position(|&i| i == note_index)
    }

    /// Check if we're in the temperament octave phase.
    pub fn is_temperament_phase(&self, position: usize) -> bool {
        position < 13 // F3 to F4 is 13 notes
    }

    /// Check if we're in the upward phase.
    pub fn is_upward_phase(&self, position: usize) -> bool {
        position >= 13 && position < 13 + 43 // F#4 to C8 is 43 notes
    }

    /// Check if we're in the downward phase.
    pub fn is_downward_phase(&self, position: usize) -> bool {
        position >= 13 + 43 // E3 to A0 is 32 notes
    }

    /// Get the phase name for a position.
    pub fn phase_name(&self, position: usize) -> &'static str {
        if self.is_temperament_phase(position) {
            "Temperament Octave"
        } else if self.is_upward_phase(position) {
            "Octaves Up"
        } else {
            "Octaves Down"
        }
    }
}

impl Default for TuningOrder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_length() {
        let order = TuningOrder::new();
        assert_eq!(order.len(), 88, "Should have 88 notes");
    }

    #[test]
    fn test_all_notes_included() {
        let order = TuningOrder::new();
        let mut seen = [false; 88];

        for &idx in order.indices() {
            assert!(!seen[idx], "Note {} appears twice", idx);
            seen[idx] = true;
        }

        for (i, &s) in seen.iter().enumerate() {
            assert!(s, "Note {} is missing", i);
        }
    }

    #[test]
    fn test_starts_with_f3() {
        let order = TuningOrder::new();
        let first = order.note_at(0).expect("Should have first note");
        assert_eq!(first.name, "F");
        assert_eq!(first.octave, 3);
        assert_eq!(first.display_name(), "F3");
    }

    #[test]
    fn test_temperament_octave_first() {
        let order = TuningOrder::new();
        let notes = order.notes();

        // First 13 notes should be F3 through F4
        let expected_names = [
            "F3", "F#3", "G3", "G#3", "A3", "A#3", "B3", "C4", "C#4", "D4", "D#4", "E4", "F4",
        ];

        for (i, expected) in expected_names.iter().enumerate() {
            let note = notes[i];
            assert_eq!(
                note.display_name(),
                *expected,
                "Position {} should be {}, got {}",
                i,
                expected,
                note.display_name()
            );
        }
    }

    #[test]
    fn test_upward_after_temperament() {
        let order = TuningOrder::new();
        let notes = order.notes();

        // After F4 (position 12), should go to F#4 (position 13)
        let f4 = notes[12];
        let fsharp4 = notes[13];

        assert_eq!(f4.display_name(), "F4");
        assert_eq!(fsharp4.display_name(), "F#4");

        // Should continue upward
        for i in 13..55 {
            let current = notes[i].midi;
            let next = notes[i + 1].midi;
            assert_eq!(
                next,
                current + 1,
                "Upward phase should be sequential: {} to {}",
                notes[i].display_name(),
                notes[i + 1].display_name()
            );
        }
    }

    #[test]
    fn test_ends_with_c8_then_downward() {
        let order = TuningOrder::new();
        let notes = order.notes();

        // Position 55 should be C8 (last of upward phase)
        let c8_pos = 13 + 43 - 1; // temperament + upward - 1
        let c8 = notes[c8_pos];
        assert_eq!(c8.display_name(), "C8");

        // After C8 should come E3 (start of downward)
        let e3 = notes[c8_pos + 1];
        assert_eq!(e3.display_name(), "E3");
    }

    #[test]
    fn test_downward_is_descending() {
        let order = TuningOrder::new();
        let notes = order.notes();

        // Downward phase starts at position 56
        let downward_start = 13 + 43;

        for i in downward_start..(87) {
            let current = notes[i].midi;
            let next = notes[i + 1].midi;
            assert_eq!(
                next,
                current - 1,
                "Downward phase should descend: {} to {}",
                notes[i].display_name(),
                notes[i + 1].display_name()
            );
        }
    }

    #[test]
    fn test_ends_with_a0() {
        let order = TuningOrder::new();
        let last = order.note_at(87).expect("Should have last note");
        assert_eq!(last.display_name(), "A0");
    }

    #[test]
    fn test_position_of() {
        let order = TuningOrder::new();

        // F3 should be at position 0
        assert_eq!(order.position_of(F3_MIDI), Some(0));

        // A3 (MIDI 57) should be in the temperament octave
        let a3_pos = order.position_of(57).expect("A3 should be in order");
        assert!(a3_pos < 13, "A3 should be in temperament octave");
        assert_eq!(a3_pos, 4); // F3, F#3, G3, G#3, A3

        // C8 should be at the end of upward phase
        let c8_pos = order.position_of(C8_MIDI).expect("C8 should be in order");
        assert_eq!(c8_pos, 55);

        // A0 should be last
        let a0_pos = order.position_of(A0_MIDI).expect("A0 should be in order");
        assert_eq!(a0_pos, 87);
    }

    #[test]
    fn test_phase_detection() {
        let order = TuningOrder::new();

        assert!(order.is_temperament_phase(0));
        assert!(order.is_temperament_phase(12));
        assert!(!order.is_temperament_phase(13));

        assert!(!order.is_upward_phase(12));
        assert!(order.is_upward_phase(13));
        assert!(order.is_upward_phase(55));
        assert!(!order.is_upward_phase(56));

        assert!(!order.is_downward_phase(55));
        assert!(order.is_downward_phase(56));
        assert!(order.is_downward_phase(87));
    }

    #[test]
    fn test_phase_names() {
        let order = TuningOrder::new();

        assert_eq!(order.phase_name(0), "Temperament Octave");
        assert_eq!(order.phase_name(12), "Temperament Octave");
        assert_eq!(order.phase_name(13), "Octaves Up");
        assert_eq!(order.phase_name(55), "Octaves Up");
        assert_eq!(order.phase_name(56), "Octaves Down");
        assert_eq!(order.phase_name(87), "Octaves Down");
    }
}
