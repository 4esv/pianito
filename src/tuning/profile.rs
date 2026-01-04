//! Piano profiling for deviation-based tuning order.

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
}

impl ProfiledNote {
    /// Create a new profiled note.
    pub fn new(midi: u8, frequency: f32, cents: f32) -> Self {
        Self {
            midi,
            frequency,
            cents,
            timestamp: Utc::now(),
        }
    }
}

/// A complete piano profile with measurements for all 88 keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoProfile {
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
            id: now.to_rfc3339(),
            notes: vec![None; NOTE_COUNT],
            created_at: now,
        }
    }

    /// Record a note measurement.
    pub fn record_note(&mut self, midi: u8, frequency: f32, cents: f32) {
        if let Some(idx) = Self::midi_to_index(midi) {
            if idx < self.notes.len() {
                self.notes[idx] = Some(ProfiledNote::new(midi, frequency, cents));
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
        let profile: PianoProfile = serde_json::from_str(&content)?;
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

        profiles.sort_by(|a, b| b.created_at.cmp(&a.created_at));

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
    fn test_midi_to_index() {
        assert_eq!(PianoProfile::midi_to_index(21), Some(0)); // A0
        assert_eq!(PianoProfile::midi_to_index(69), Some(48)); // A4
        assert_eq!(PianoProfile::midi_to_index(108), Some(87)); // C8
        assert_eq!(PianoProfile::midi_to_index(20), None); // Out of range
        assert_eq!(PianoProfile::midi_to_index(109), None); // Out of range
    }
}
