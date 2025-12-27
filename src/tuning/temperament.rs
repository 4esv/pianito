//! Equal temperament calculations.

use super::notes::Note;

/// Equal temperament calculator.
#[derive(Debug, Clone, Copy)]
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
    /// Uses the formula: f = A4 * 2^((n - 69) / 12)
    pub fn frequency(&self, midi_note: u8) -> f32 {
        // A4 is MIDI note 69
        self.a4_freq * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0)
    }

    /// Calculate the frequency for a Note.
    pub fn frequency_for_note(&self, note: &Note) -> f32 {
        self.frequency(note.midi)
    }

    /// Convert a frequency to cents deviation from a target frequency.
    /// Positive = sharp, negative = flat.
    pub fn cents_from_target(&self, frequency: f32, target: f32) -> f32 {
        1200.0 * (frequency / target).log2()
    }

    /// Convert frequency deviation to cents.
    pub fn frequency_to_cents(&self, frequency: f32, midi_note: u8) -> f32 {
        let target = self.frequency(midi_note);
        self.cents_from_target(frequency, target)
    }

    /// Convert cents deviation to frequency.
    /// Given a target frequency and cents offset, return the actual frequency.
    pub fn cents_to_frequency(&self, target: f32, cents: f32) -> f32 {
        target * Self::cents_to_ratio(cents)
    }

    /// Convert cents deviation to frequency ratio.
    pub fn cents_to_ratio(cents: f32) -> f32 {
        2.0_f32.powf(cents / 1200.0)
    }

    /// Find the nearest MIDI note for a given frequency.
    /// Returns (midi_note, cents_deviation).
    pub fn nearest_note(&self, frequency: f32) -> (u8, f32) {
        // Calculate fractional MIDI note
        let midi_float = 69.0 + 12.0 * (frequency / self.a4_freq).log2();
        let midi_note = midi_float.round() as u8;

        // Calculate cents deviation
        let target_freq = self.frequency(midi_note);
        let cents = self.cents_from_target(frequency, target_freq);

        (midi_note, cents)
    }
}

impl Default for Temperament {
    fn default() -> Self {
        Self::new()
    }
}

/// Known frequencies for all 88 piano notes at A4=440Hz.
/// Used for validation in tests.
pub const REFERENCE_FREQUENCIES: [(u8, f32); 88] = [
    (21, 27.5),       // A0
    (22, 29.135),     // A#0
    (23, 30.868),     // B0
    (24, 32.703),     // C1
    (25, 34.648),     // C#1
    (26, 36.708),     // D1
    (27, 38.891),     // D#1
    (28, 41.203),     // E1
    (29, 43.654),     // F1
    (30, 46.249),     // F#1
    (31, 48.999),     // G1
    (32, 51.913),     // G#1
    (33, 55.0),       // A1
    (34, 58.270),     // A#1
    (35, 61.735),     // B1
    (36, 65.406),     // C2
    (37, 69.296),     // C#2
    (38, 73.416),     // D2
    (39, 77.782),     // D#2
    (40, 82.407),     // E2
    (41, 87.307),     // F2
    (42, 92.499),     // F#2
    (43, 97.999),     // G2
    (44, 103.826),    // G#2
    (45, 110.0),      // A2
    (46, 116.541),    // A#2
    (47, 123.471),    // B2
    (48, 130.813),    // C3
    (49, 138.591),    // C#3
    (50, 146.832),    // D3
    (51, 155.563),    // D#3
    (52, 164.814),    // E3
    (53, 174.614),    // F3
    (54, 184.997),    // F#3
    (55, 195.998),    // G3
    (56, 207.652),    // G#3
    (57, 220.0),      // A3
    (58, 233.082),    // A#3
    (59, 246.942),    // B3
    (60, 261.626),    // C4 (Middle C)
    (61, 277.183),    // C#4
    (62, 293.665),    // D4
    (63, 311.127),    // D#4
    (64, 329.628),    // E4
    (65, 349.228),    // F4
    (66, 369.994),    // F#4
    (67, 391.995),    // G4
    (68, 415.305),    // G#4
    (69, 440.0),      // A4 (Concert Pitch)
    (70, 466.164),    // A#4
    (71, 493.883),    // B4
    (72, 523.251),    // C5
    (73, 554.365),    // C#5
    (74, 587.330),    // D5
    (75, 622.254),    // D#5
    (76, 659.255),    // E5
    (77, 698.456),    // F5
    (78, 739.989),    // F#5
    (79, 783.991),    // G5
    (80, 830.609),    // G#5
    (81, 880.0),      // A5
    (82, 932.328),    // A#5
    (83, 987.767),    // B5
    (84, 1046.502),   // C6
    (85, 1108.731),   // C#6
    (86, 1174.659),   // D6
    (87, 1244.508),   // D#6
    (88, 1318.510),   // E6
    (89, 1396.913),   // F6
    (90, 1479.978),   // F#6
    (91, 1567.982),   // G6
    (92, 1661.219),   // G#6
    (93, 1760.0),     // A6
    (94, 1864.655),   // A#6
    (95, 1975.533),   // B6
    (96, 2093.005),   // C7
    (97, 2217.461),   // C#7
    (98, 2349.318),   // D7
    (99, 2489.016),   // D#7
    (100, 2637.020),  // E7
    (101, 2793.826),  // F7
    (102, 2959.955),  // F#7
    (103, 3135.963),  // G7
    (104, 3322.438),  // G#7
    (105, 3520.0),    // A7
    (106, 3729.310),  // A#7
    (107, 3951.066),  // B7
    (108, 4186.009),  // C8
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a4_default() {
        let temp = Temperament::new();
        assert_eq!(temp.a4(), 440.0);
    }

    #[test]
    fn test_a4_custom() {
        let temp = Temperament::with_a4(442.0);
        assert_eq!(temp.a4(), 442.0);
    }

    #[test]
    fn test_a4_frequency() {
        let temp = Temperament::new();
        let freq = temp.frequency(69);
        assert!((freq - 440.0).abs() < 0.001, "A4 should be 440Hz, got {}", freq);
    }

    #[test]
    fn test_all_88_notes_at_440() {
        let temp = Temperament::new();

        for &(midi, expected) in &REFERENCE_FREQUENCIES {
            let calculated = temp.frequency(midi);
            let error = (calculated - expected).abs();
            let relative_error = error / expected;

            assert!(
                relative_error < 0.001,
                "MIDI {} should be {:.3}Hz, got {:.3}Hz (error: {:.4}%)",
                midi, expected, calculated, relative_error * 100.0
            );
        }
    }

    #[test]
    fn test_octave_relationships() {
        let temp = Temperament::new();

        // Each octave should be exactly 2x the frequency
        for midi in 21..=96 {
            let lower = temp.frequency(midi);
            let higher = temp.frequency(midi + 12);
            let ratio = higher / lower;

            assert!(
                (ratio - 2.0).abs() < 0.0001,
                "Octave ratio for MIDI {} should be 2.0, got {}",
                midi, ratio
            );
        }
    }

    #[test]
    fn test_custom_a4_442() {
        let temp = Temperament::with_a4(442.0);

        // A4 should be exactly 442
        assert_eq!(temp.frequency(69), 442.0);

        // A5 should be 884
        let a5 = temp.frequency(81);
        assert!((a5 - 884.0).abs() < 0.001);

        // Ratio to 440Hz tuning
        let ratio = 442.0 / 440.0;
        let temp_440 = Temperament::new();

        for midi in 21..=108 {
            let freq_442 = temp.frequency(midi);
            let freq_440 = temp_440.frequency(midi);
            let actual_ratio = freq_442 / freq_440;

            assert!(
                (actual_ratio - ratio).abs() < 0.0001,
                "All notes should scale by same ratio"
            );
        }
    }

    #[test]
    fn test_cents_conversion_zero() {
        let temp = Temperament::new();

        // Exact pitch should be 0 cents
        let cents = temp.cents_from_target(440.0, 440.0);
        assert!((cents).abs() < 0.001, "Same frequency should be 0 cents, got {}", cents);
    }

    #[test]
    fn test_cents_conversion_semitone() {
        let temp = Temperament::new();

        // One semitone = 100 cents
        let a4 = 440.0;
        let asharp4 = temp.frequency(70); // A#4

        let cents = temp.cents_from_target(asharp4, a4);
        assert!(
            (cents - 100.0).abs() < 0.1,
            "Semitone should be 100 cents, got {}",
            cents
        );
    }

    #[test]
    fn test_cents_conversion_octave() {
        let temp = Temperament::new();

        // One octave = 1200 cents
        let cents = temp.cents_from_target(880.0, 440.0);
        assert!(
            (cents - 1200.0).abs() < 0.1,
            "Octave should be 1200 cents, got {}",
            cents
        );
    }

    #[test]
    fn test_cents_roundtrip() {
        let temp = Temperament::new();
        let target = 440.0;

        for cents in [-50.0, -25.0, -10.0, 0.0, 10.0, 25.0, 50.0] {
            let freq = temp.cents_to_frequency(target, cents);
            let recovered_cents = temp.cents_from_target(freq, target);

            assert!(
                (recovered_cents - cents).abs() < 0.01,
                "Cents roundtrip failed for {}: got {}",
                cents, recovered_cents
            );
        }
    }

    #[test]
    fn test_nearest_note() {
        let temp = Temperament::new();

        // Exact A4
        let (midi, cents) = temp.nearest_note(440.0);
        assert_eq!(midi, 69);
        assert!(cents.abs() < 0.1);

        // A4 + 25 cents
        let freq = temp.cents_to_frequency(440.0, 25.0);
        let (midi, cents) = temp.nearest_note(freq);
        assert_eq!(midi, 69);
        assert!((cents - 25.0).abs() < 0.1);

        // Between A4 and A#4 (should round to nearest)
        let freq = temp.cents_to_frequency(440.0, 49.0);
        let (midi, _) = temp.nearest_note(freq);
        assert_eq!(midi, 69); // Still A4

        let freq = temp.cents_to_frequency(440.0, 51.0);
        let (midi, _) = temp.nearest_note(freq);
        assert_eq!(midi, 70); // A#4
    }

    #[test]
    fn test_frequency_to_cents() {
        let temp = Temperament::new();

        // A4 at exactly 440Hz
        let cents = temp.frequency_to_cents(440.0, 69);
        assert!(cents.abs() < 0.01);

        // A4 at 442Hz (slightly sharp)
        let cents = temp.frequency_to_cents(442.0, 69);
        assert!(cents > 0.0);
        assert!((cents - 7.85).abs() < 0.1); // ~7.85 cents sharp
    }
}
