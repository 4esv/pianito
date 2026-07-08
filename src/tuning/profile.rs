//! Piano profiling for deviation-based tuning order.

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::audio::Partial;

use super::notes::{Note, NOTES, NOTE_COUNT};

/// A single profiled note measurement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfiledNote {
    /// MIDI note number (21-108).
    pub midi: u8,
    /// Detected frequency in Hz.
    pub frequency: f32,
    /// Deviation from target in cents.
    pub cents: f32,
    /// When this measurement was taken.
    pub timestamp: DateTime<Utc>,
    /// Measured partials (n=1..~8) from the FFT analyzer (issue #22), the
    /// raw data issue #23's inharmonicity fit builds `B` from. Empty for
    /// profiles saved before this field existed, or when confirm captured
    /// no partials (silence/noise).
    #[serde(default)]
    pub partials: Vec<Partial>,
}

impl ProfiledNote {
    /// Create a new profiled note with no partial data.
    pub fn new(midi: u8, frequency: f32, cents: f32) -> Self {
        Self::with_partials(midi, frequency, cents, Vec::new())
    }

    /// Create a new profiled note, including its measured partial spectrum.
    pub fn with_partials(midi: u8, frequency: f32, cents: f32, partials: Vec<Partial>) -> Self {
        Self {
            midi,
            frequency,
            cents,
            timestamp: Utc::now(),
            partials,
        }
    }
}

/// Current on-disk schema version for [`PianoProfile`].
const SCHEMA_VERSION: u32 = 1;

/// Default for the `version` field when it's absent from the JSON.
///
/// Profiles saved before this field existed predate versioning entirely, so
/// treating them as `1` is correct, not just a filler value.
fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// A complete piano profile with measurements for all 88 keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoProfile {
    /// Schema version. See [`default_schema_version`].
    #[serde(default = "default_schema_version")]
    pub version: u32,
    /// Unique profile ID (ISO 8601 timestamp).
    pub id: String,
    /// Measurements for each note (index 0 = A0, index 87 = C8).
    pub notes: Vec<Option<ProfiledNote>>,
    /// When this profile was created.
    pub created_at: DateTime<Utc>,
}

impl PianoProfile {
    /// Create a new empty profile.
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            version: SCHEMA_VERSION,
            id: now.to_rfc3339(),
            notes: vec![None; NOTE_COUNT],
            created_at: now,
        }
    }

    /// Record a note measurement.
    pub fn record_note(&mut self, midi: u8, frequency: f32, cents: f32) {
        self.record_note_with_partials(midi, frequency, cents, Vec::new());
    }

    /// Record a note measurement along with its measured partial spectrum
    /// (issue #22), captured by the FFT analyzer against the same audio
    /// window the frequency/cents reading came from.
    pub fn record_note_with_partials(
        &mut self,
        midi: u8,
        frequency: f32,
        cents: f32,
        partials: Vec<Partial>,
    ) {
        if let Some(idx) = Self::midi_to_index(midi) {
            if idx < self.notes.len() {
                self.notes[idx] = Some(ProfiledNote::with_partials(
                    midi, frequency, cents, partials,
                ));
            }
        }
    }

    /// Check if all 88 notes have been profiled.
    pub fn is_complete(&self) -> bool {
        self.notes.iter().all(|n| n.is_some())
    }

    /// Get progress as (completed, total).
    pub fn progress(&self) -> (usize, usize) {
        let completed = self.notes.iter().filter(|n| n.is_some()).count();
        (completed, NOTE_COUNT)
    }

    /// Calculate average absolute deviation in cents.
    pub fn average_deviation(&self) -> f32 {
        let (sum, count) = self
            .notes
            .iter()
            .filter_map(|n| n.as_ref())
            .fold((0.0, 0), |(sum, count), note| {
                (sum + note.cents.abs(), count + 1)
            });

        if count == 0 {
            0.0
        } else {
            sum / count as f32
        }
    }

    /// Get the n worst notes by absolute deviation.
    pub fn worst_notes(&self, n: usize) -> Vec<&ProfiledNote> {
        let mut profiled: Vec<_> = self.notes.iter().filter_map(|n| n.as_ref()).collect();
        profiled.sort_by(|a, b| {
            b.cents
                .abs()
                .partial_cmp(&a.cents.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        profiled.into_iter().take(n).collect()
    }

    /// Get notes sorted by absolute deviation (worst first).
    pub fn notes_by_deviation(&self) -> Vec<(usize, &ProfiledNote)> {
        let mut indexed: Vec<_> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.as_ref().map(|note| (i, note)))
            .collect();

        indexed.sort_by(|(_, a), (_, b)| {
            b.cents
                .abs()
                .partial_cmp(&a.cents.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        indexed
    }

    /// Get the profiles directory path.
    pub fn profiles_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "pianito").map(|dirs| dirs.data_dir().join("profiles"))
    }

    /// Get the path for this profile's file.
    fn profile_path(&self) -> Option<PathBuf> {
        Self::profiles_dir().map(|dir| {
            let safe_id = self.id.replace(':', "-");
            dir.join(format!("{}.json", safe_id))
        })
    }

    /// Save profile to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .profile_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine profiles directory"))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;

        Ok(())
    }

    /// Load a profile from a file path.
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut profile: PianoProfile = serde_json::from_str(&content)?;
        // NOTE: hand-edited or truncated files may not have exactly 88 slots;
        // normalize so indexed consumers can rely on NOTE_COUNT entries.
        profile.notes.resize(NOTE_COUNT, None);
        Ok(profile)
    }

    /// List all saved profiles, most recent first.
    pub fn list_all() -> anyhow::Result<Vec<PianoProfile>> {
        let profiles_dir = match Self::profiles_dir() {
            Some(dir) => dir,
            None => return Ok(Vec::new()),
        };

        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles: Vec<PianoProfile> = Vec::new();

        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(profile) = Self::load(&path) {
                    profiles.push(profile);
                }
            }
        }

        profiles.sort_by_key(|p| std::cmp::Reverse(p.created_at));

        Ok(profiles)
    }

    /// Convert MIDI note number to array index.
    fn midi_to_index(midi: u8) -> Option<usize> {
        if (21..=108).contains(&midi) {
            Some((midi - 21) as usize)
        } else {
            None
        }
    }

    /// Get note info for a chromatic index.
    pub fn note_at(index: usize) -> Option<&'static Note> {
        NOTES.get(index)
    }
}

impl Default for PianoProfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_profile() {
        let profile = PianoProfile::new();
        assert!(!profile.is_complete());
        assert_eq!(profile.progress(), (0, 88));
        assert_eq!(profile.average_deviation(), 0.0);
    }

    #[test]
    fn test_record_note() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 7.85); // A4, slightly sharp

        assert_eq!(profile.progress(), (1, 88));
        assert!(!profile.is_complete());

        let note = profile.notes[48].as_ref().expect("A4 should be recorded");
        assert_eq!(note.midi, 69);
        assert!((note.cents - 7.85).abs() < 0.01);
    }

    #[test]
    fn test_average_deviation() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 10.0);
        profile.record_note(70, 467.0, -20.0);
        profile.record_note(71, 494.0, 30.0);

        // Average of |10|, |-20|, |30| = 60/3 = 20
        assert!((profile.average_deviation() - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_worst_notes() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 440.0, 5.0);
        profile.record_note(70, 466.0, -25.0);
        profile.record_note(71, 494.0, 15.0);

        let worst = profile.worst_notes(2);
        assert_eq!(worst.len(), 2);
        assert_eq!(worst[0].midi, 70); // -25 cents (highest abs)
        assert_eq!(worst[1].midi, 71); // 15 cents
    }

    #[test]
    fn test_notes_by_deviation() {
        let mut profile = PianoProfile::new();
        profile.record_note(21, 27.5, 2.0); // A0
        profile.record_note(69, 440.0, -50.0); // A4
        profile.record_note(108, 4186.0, 10.0); // C8

        let sorted = profile.notes_by_deviation();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].1.midi, 69); // -50 cents
        assert_eq!(sorted[1].1.midi, 108); // 10 cents
        assert_eq!(sorted[2].1.midi, 21); // 2 cents
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        // Characterizes the current on-disk shape before schema versioning
        // is added: id, notes, and created_at must survive a round trip
        // unchanged.
        let mut profile = PianoProfile::new();
        profile.record_note(21, 27.5, 1.0); // A0
        profile.record_note(69, 442.0, 7.85); // A4

        let json = serde_json::to_string(&profile).expect("Should serialize");
        let restored: PianoProfile = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(restored.id, profile.id);
        assert_eq!(restored.created_at, profile.created_at);
        assert_eq!(restored.notes.len(), profile.notes.len());

        let a0 = restored.notes[0].as_ref().expect("A0 recorded");
        assert_eq!(a0.midi, 21);
        assert!((a0.cents - 1.0).abs() < 0.01);

        let a4 = restored.notes[48].as_ref().expect("A4 recorded");
        assert_eq!(a4.midi, 69);
        assert!((a4.cents - 7.85).abs() < 0.01);
    }

    #[test]
    fn test_load_normalizes_note_count() {
        let temp_dir = tempfile::TempDir::new().expect("Should create temp dir");
        let path = temp_dir.path().join("truncated.json");

        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 7.85); // A4, index 48
        profile.notes.truncate(50);
        let json = serde_json::to_string(&profile).expect("Should serialize");
        fs::write(&path, json).expect("Should write file");

        let loaded = PianoProfile::load(&path).expect("Should load");
        assert_eq!(loaded.notes.len(), NOTE_COUNT);
        assert!(loaded.notes[48].is_some(), "Measured note survives");
        assert!(loaded.notes[87].is_none(), "Missing slots pad with None");
    }

    #[test]
    fn test_new_profile_has_current_schema_version() {
        assert_eq!(PianoProfile::new().version, SCHEMA_VERSION);
    }

    #[test]
    fn test_load_defaults_version_for_pre_version_profiles() {
        let temp_dir = tempfile::TempDir::new().expect("Should create temp dir");
        let path = temp_dir.path().join("pre_version.json");

        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 7.85);

        // Strip `version` to simulate a profile saved before it existed.
        let mut value = serde_json::to_value(&profile).expect("Should serialize");
        value
            .as_object_mut()
            .expect("profile object")
            .remove("version");
        fs::write(&path, value.to_string()).expect("Should write file");

        let loaded = PianoProfile::load(&path).expect("Should load");
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn test_load_pre_version_fixture_upgrades_to_v1() {
        // Hand-written fixture matching the real on-disk shape saved by
        // pianito before `version` existed (no derived-then-stripped value).
        let temp_dir = tempfile::TempDir::new().expect("Should create temp dir");
        let path = temp_dir.path().join("fixture.json");

        let fixture = r#"{
            "id": "2024-01-01T00:00:00+00:00",
            "notes": [],
            "created_at": "2024-01-01T00:00:00+00:00"
        }"#;
        fs::write(&path, fixture).expect("Should write fixture");

        let loaded = PianoProfile::load(&path).expect("pre-version fixture must still load");
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.notes.len(), NOTE_COUNT);
    }

    #[test]
    fn test_midi_to_index() {
        assert_eq!(PianoProfile::midi_to_index(21), Some(0)); // A0
        assert_eq!(PianoProfile::midi_to_index(69), Some(48)); // A4
        assert_eq!(PianoProfile::midi_to_index(108), Some(87)); // C8
        assert_eq!(PianoProfile::midi_to_index(20), None); // Out of range
        assert_eq!(PianoProfile::midi_to_index(109), None); // Out of range
    }

    fn sample_partials() -> Vec<Partial> {
        vec![
            Partial {
                n: 1,
                freq_hz: 55.02,
                amplitude: 0.9,
            },
            Partial {
                n: 2,
                freq_hz: 110.08,
                amplitude: 0.6,
            },
            Partial {
                n: 3,
                freq_hz: 165.31,
                amplitude: 0.3,
            },
        ]
    }

    #[test]
    fn test_record_note_has_no_partials() {
        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 7.85);

        let note = profile.notes[48].as_ref().expect("A4 recorded");
        assert!(note.partials.is_empty());
    }

    #[test]
    fn test_record_note_with_partials_stores_them() {
        let mut profile = PianoProfile::new();
        profile.record_note_with_partials(33, 55.0, 0.5, sample_partials());

        let note = profile.notes[12].as_ref().expect("A1 recorded"); // midi 33 -> index 12
        assert_eq!(note.partials.len(), 3);
        assert_eq!(note.partials[1].n, 2);
        assert!((note.partials[1].freq_hz - 110.08).abs() < 0.001);
    }

    #[test]
    fn test_partials_round_trip_through_serde() {
        let mut profile = PianoProfile::new();
        profile.record_note_with_partials(33, 55.0, 0.5, sample_partials());

        let json = serde_json::to_string(&profile).expect("Should serialize");
        let restored: PianoProfile = serde_json::from_str(&json).expect("Should deserialize");

        let note = restored.notes[12].as_ref().expect("A1 recorded");
        assert_eq!(note.partials, sample_partials());
    }

    #[test]
    fn test_load_legacy_profile_without_partials_defaults_to_empty() {
        // Simulates a profile saved before `partials` existed on
        // `ProfiledNote` (issue #22): strip the field from an otherwise-real
        // serialized profile and confirm it still loads, defaulting to empty
        // rather than failing.
        let temp_dir = tempfile::TempDir::new().expect("Should create temp dir");
        let path = temp_dir.path().join("legacy.json");

        let mut profile = PianoProfile::new();
        profile.record_note(69, 442.0, 7.85); // A4, index 48

        let mut value = serde_json::to_value(&profile).expect("Should serialize");
        value["notes"][48]
            .as_object_mut()
            .expect("A4 note object")
            .remove("partials");
        fs::write(&path, value.to_string()).expect("Should write fixture");

        let loaded = PianoProfile::load(&path).expect("legacy profile without partials must load");
        let note = loaded.notes[48].as_ref().expect("A4 recorded");
        assert_eq!(note.midi, 69);
        assert!(
            note.partials.is_empty(),
            "missing field must default to empty, not fail"
        );
    }
}
