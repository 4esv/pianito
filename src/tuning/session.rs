//! Session state and persistence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tuning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TuningMode {
    /// Quick tune relative to current pitch center.
    Quick,
    /// Concert pitch tuning (A4 = 440Hz or custom).
    Concert,
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

/// A tuning session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
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

    /// Check if the session is complete.
    pub fn is_complete(&self) -> bool {
        self.current_note_index >= 88
    }

    /// Save session to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        todo!("Implement session save")
    }

    /// Load the most recent incomplete session.
    pub fn load_recent() -> anyhow::Result<Option<Self>> {
        todo!("Implement session load")
    }
}
