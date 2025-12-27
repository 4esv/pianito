//! Session state and persistence.

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Tuning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TuningMode {
    /// Quick tune relative to current pitch center.
    Quick,
    /// Concert pitch tuning (A4 = 440Hz or custom).
    Concert,
}

impl Default for TuningMode {
    fn default() -> Self {
        Self::Concert
    }
}

/// A completed note in a tuning session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedNote {
    /// Note name (e.g., "F3").
    pub note: String,
    /// Final cents deviation from target.
    pub final_cents: f32,
    /// Timestamp when completed.
    pub timestamp: DateTime<Utc>,
}

impl CompletedNote {
    /// Create a new completed note.
    pub fn new(note: impl Into<String>, final_cents: f32) -> Self {
        Self {
            note: note.into(),
            final_cents,
            timestamp: Utc::now(),
        }
    }
}

/// A tuning session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID (ISO 8601 timestamp).
    pub id: String,
    /// Tuning mode.
    pub mode: TuningMode,
    /// A4 reference frequency.
    pub a4_reference: f32,
    /// Piano's offset from concert pitch in cents (for quick tune).
    pub piano_offset_cents: f32,
    /// Current note index in tuning order.
    pub current_note_index: usize,
    /// Completed notes.
    pub completed_notes: Vec<CompletedNote>,
    /// Session creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session.
    pub fn new(mode: TuningMode, a4_reference: f32) -> Self {
        let now = Utc::now();
        Self {
            id: now.to_rfc3339(),
            mode,
            a4_reference,
            piano_offset_cents: 0.0,
            current_note_index: 0,
            completed_notes: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a quick tune session.
    pub fn quick_tune(piano_offset_cents: f32) -> Self {
        let mut session = Self::new(TuningMode::Quick, 440.0);
        session.piano_offset_cents = piano_offset_cents;
        session
    }

    /// Create a concert pitch session.
    pub fn concert_pitch(a4_reference: f32) -> Self {
        Self::new(TuningMode::Concert, a4_reference)
    }

    /// Check if the session is complete.
    pub fn is_complete(&self) -> bool {
        self.current_note_index >= 88
    }

    /// Mark a note as completed.
    pub fn complete_note(&mut self, note_name: impl Into<String>, final_cents: f32) {
        self.completed_notes
            .push(CompletedNote::new(note_name, final_cents));
        self.current_note_index += 1;
        self.updated_at = Utc::now();
    }

    /// Skip to the next note without recording completion.
    pub fn skip_note(&mut self) {
        self.current_note_index += 1;
        self.updated_at = Utc::now();
    }

    /// Get the sessions directory path.
    fn sessions_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "onkey").map(|dirs| dirs.data_dir().join("sessions"))
    }

    /// Get the path for this session's file.
    fn session_path(&self) -> Option<PathBuf> {
        Self::sessions_dir().map(|dir| {
            // Sanitize the ID for use as filename
            let safe_id = self.id.replace(':', "-");
            dir.join(format!("{}.json", safe_id))
        })
    }

    /// Save session to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .session_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine sessions directory"))?;

        // Create directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;

        Ok(())
    }

    /// Load a session from a file path.
    pub fn load(path: impl AsRef<std::path::Path>) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let session: Session = serde_json::from_str(&content)?;
        Ok(session)
    }

    /// Load the most recent incomplete session.
    pub fn load_recent() -> anyhow::Result<Option<Self>> {
        let sessions_dir = match Self::sessions_dir() {
            Some(dir) => dir,
            None => return Ok(None),
        };

        if !sessions_dir.exists() {
            return Ok(None);
        }

        let mut sessions: Vec<(PathBuf, Session)> = Vec::new();

        for entry in fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(session) = Self::load(&path) {
                    if !session.is_complete() {
                        sessions.push((path, session));
                    }
                }
            }
        }

        // Sort by updated_at descending
        sessions.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));

        Ok(sessions.into_iter().next().map(|(_, s)| s))
    }

    /// List all sessions, most recent first.
    pub fn list_all() -> anyhow::Result<Vec<Session>> {
        let sessions_dir = match Self::sessions_dir() {
            Some(dir) => dir,
            None => return Ok(Vec::new()),
        };

        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions: Vec<Session> = Vec::new();

        for entry in fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(session) = Self::load(&path) {
                    sessions.push(session);
                }
            }
        }

        // Sort by created_at descending
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(sessions)
    }

    /// Delete this session file.
    pub fn delete(&self) -> anyhow::Result<()> {
        if let Some(path) = self.session_path() {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    /// Delete all sessions.
    pub fn reset_all() -> anyhow::Result<()> {
        let sessions_dir = match Self::sessions_dir() {
            Some(dir) => dir,
            None => return Ok(()),
        };

        if sessions_dir.exists() {
            fs::remove_dir_all(&sessions_dir)?;
        }

        Ok(())
    }

    /// Get average deviation in cents for completed notes.
    pub fn average_deviation(&self) -> f32 {
        if self.completed_notes.is_empty() {
            return 0.0;
        }

        let sum: f32 = self
            .completed_notes
            .iter()
            .map(|n| n.final_cents.abs())
            .sum();

        sum / self.completed_notes.len() as f32
    }

    /// Get progress as a percentage.
    pub fn progress_percent(&self) -> f32 {
        (self.current_note_index as f32 / 88.0) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_session() -> Session {
        Session::new(TuningMode::Concert, 440.0)
    }

    #[test]
    fn test_new_session() {
        let session = create_test_session();
        assert_eq!(session.mode, TuningMode::Concert);
        assert_eq!(session.a4_reference, 440.0);
        assert_eq!(session.current_note_index, 0);
        assert!(session.completed_notes.is_empty());
        assert!(!session.is_complete());
    }

    #[test]
    fn test_quick_tune_session() {
        let session = Session::quick_tune(-15.0);
        assert_eq!(session.mode, TuningMode::Quick);
        assert_eq!(session.piano_offset_cents, -15.0);
    }

    #[test]
    fn test_concert_pitch_session() {
        let session = Session::concert_pitch(442.0);
        assert_eq!(session.mode, TuningMode::Concert);
        assert_eq!(session.a4_reference, 442.0);
    }

    #[test]
    fn test_complete_note() {
        let mut session = create_test_session();
        session.complete_note("F3", 1.5);

        assert_eq!(session.current_note_index, 1);
        assert_eq!(session.completed_notes.len(), 1);
        assert_eq!(session.completed_notes[0].note, "F3");
        assert_eq!(session.completed_notes[0].final_cents, 1.5);
    }

    #[test]
    fn test_skip_note() {
        let mut session = create_test_session();
        session.skip_note();

        assert_eq!(session.current_note_index, 1);
        assert!(session.completed_notes.is_empty());
    }

    #[test]
    fn test_is_complete() {
        let mut session = create_test_session();
        assert!(!session.is_complete());

        session.current_note_index = 87;
        assert!(!session.is_complete());

        session.current_note_index = 88;
        assert!(session.is_complete());
    }

    #[test]
    fn test_average_deviation() {
        let mut session = create_test_session();
        assert_eq!(session.average_deviation(), 0.0);

        session.complete_note("F3", 2.0);
        session.complete_note("F#3", -4.0);
        session.complete_note("G3", 3.0);

        // Average of |2.0|, |-4.0|, |3.0| = (2 + 4 + 3) / 3 = 3.0
        assert!((session.average_deviation() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_progress_percent() {
        let mut session = create_test_session();
        assert_eq!(session.progress_percent(), 0.0);

        session.current_note_index = 44;
        assert!((session.progress_percent() - 50.0).abs() < 0.1);

        session.current_note_index = 88;
        assert_eq!(session.progress_percent(), 100.0);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut session = create_test_session();
        session.complete_note("F3", 1.5);
        session.complete_note("F#3", -0.5);

        let json = serde_json::to_string(&session).expect("Should serialize");
        let restored: Session = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(restored.id, session.id);
        assert_eq!(restored.mode, session.mode);
        assert_eq!(restored.a4_reference, session.a4_reference);
        assert_eq!(restored.current_note_index, session.current_note_index);
        assert_eq!(restored.completed_notes.len(), 2);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().expect("Should create temp dir");
        let session_path = temp_dir.path().join("test_session.json");

        let mut session = create_test_session();
        session.complete_note("F3", 1.5);

        // Save manually to temp location
        let json = serde_json::to_string_pretty(&session).expect("Should serialize");
        fs::write(&session_path, json).expect("Should write file");

        // Load
        let loaded = Session::load(&session_path).expect("Should load");
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.completed_notes.len(), 1);
    }

    #[test]
    fn test_tuning_mode_serialization() {
        // Test that modes serialize to expected strings
        let quick_json = serde_json::to_string(&TuningMode::Quick).expect("serialize");
        let concert_json = serde_json::to_string(&TuningMode::Concert).expect("serialize");

        assert_eq!(quick_json, "\"quick\"");
        assert_eq!(concert_json, "\"concert\"");

        // Test deserialization
        let quick: TuningMode = serde_json::from_str("\"quick\"").expect("deserialize");
        let concert: TuningMode = serde_json::from_str("\"concert\"").expect("deserialize");

        assert_eq!(quick, TuningMode::Quick);
        assert_eq!(concert, TuningMode::Concert);
    }

    #[test]
    fn test_session_updates_timestamp() {
        let mut session = create_test_session();
        let original_updated = session.updated_at;

        // Small delay to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        session.complete_note("F3", 0.0);
        assert!(session.updated_at > original_updated);
    }

    #[test]
    fn test_completed_note_creation() {
        let note = CompletedNote::new("A4", -2.5);
        assert_eq!(note.note, "A4");
        assert_eq!(note.final_cents, -2.5);
        // Timestamp should be recent
        assert!(note.timestamp <= Utc::now());
    }
}
