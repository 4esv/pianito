//! Multi-message status line (issue #34).
//!
//! Replaces the old single-`Option<String>`-per-source priority chain
//! (`audio_warning.or(save_error).or(resume_warning)`), where a persistent
//! audio warning permanently masked a save error. Each source keeps its own
//! slot (by [`StatusId`]) so producers can update or clear their own message
//! without touching another source's; the status line rotates through
//! whichever slots are currently occupied instead of showing only the
//! highest-priority one forever.

use std::time::{Duration, Instant};

/// Identifies *why* a status message exists. Lets a producer replace or
/// retract its own message (e.g. the silence watchdog clearing its hint once
/// signal returns) without disturbing any other source's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusId {
    /// Mic/output stream error from the audio backend.
    AudioWarning,
    /// Session or profile save failure.
    SaveError,
    /// Non-fatal note surfaced while resuming a session.
    ResumeWarning,
    /// Dead-input hint from [`crate::audio::SilenceWatchdog`].
    SilenceWatchdog,
}

/// Display severity, driving status-line styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Soft, informational note.
    Info,
    /// A real problem the user should notice.
    Warning,
}

struct Message {
    id: StatusId,
    text: String,
    severity: Severity,
    /// `None` means the message persists until explicitly cleared/replaced
    /// (e.g. a stream error, which should stay visible until the next one
    /// supersedes it — see `App::reset`'s note on `audio_warning`).
    expires_at: Option<Instant>,
}

/// A small ring of status messages, one per [`StatusId`], rotating through
/// whichever are currently occupied.
#[derive(Default)]
pub struct StatusQueue {
    messages: Vec<Message>,
    cursor: usize,
    last_rotated: Option<Instant>,
}

impl StatusQueue {
    /// How long each message is shown before rotating to the next.
    pub const ROTATE_INTERVAL: Duration = Duration::from_secs(3);

    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a message under `id`, or replace it in place (same slot, same
    /// rotation position) if one already exists.
    pub fn upsert(
        &mut self,
        id: StatusId,
        text: impl Into<String>,
        severity: Severity,
        ttl: Option<Duration>,
        now: Instant,
    ) {
        let expires_at = ttl.map(|d| now + d);
        match self.messages.iter_mut().find(|m| m.id == id) {
            Some(existing) => {
                existing.text = text.into();
                existing.severity = severity;
                existing.expires_at = expires_at;
            }
            None => self.messages.push(Message {
                id,
                text: text.into(),
                severity,
                expires_at,
            }),
        }
    }

    /// Remove `id`'s message, if any (e.g. a save succeeding, or the
    /// watchdog's signal returning).
    pub fn clear(&mut self, id: StatusId) {
        self.messages.retain(|m| m.id != id);
        if self.cursor >= self.messages.len() {
            self.cursor = 0;
        }
    }

    /// Current text for `id`, if a message is enqueued under it — regardless
    /// of which message the rotation is currently displaying. Used to report
    /// a save failure on stderr after the terminal is restored.
    pub fn text_for(&self, id: StatusId) -> Option<&str> {
        self.messages
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.text.as_str())
    }

    /// Expire timed-out messages and advance the rotation. Call once per
    /// render tick; `now` is threaded through explicitly so the timing is
    /// deterministic under test.
    pub fn tick(&mut self, now: Instant) {
        self.messages.retain(|m| match m.expires_at {
            Some(exp) => exp > now,
            None => true,
        });
        if self.cursor >= self.messages.len() {
            self.cursor = 0;
        }

        match self.last_rotated {
            None => self.last_rotated = Some(now),
            Some(last)
                if self.messages.len() > 1 && now.duration_since(last) >= Self::ROTATE_INTERVAL =>
            {
                self.cursor = (self.cursor + 1) % self.messages.len();
                self.last_rotated = Some(now);
            }
            _ => {}
        }
    }

    /// The message the status line should currently show, if any.
    pub fn current(&self) -> Option<(&str, Severity)> {
        self.messages
            .get(self.cursor)
            .map(|m| (m.text.as_str(), m.severity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_queue_shows_nothing() {
        let queue = StatusQueue::new();
        assert!(queue.current().is_none());
    }

    #[test]
    fn test_enqueued_message_becomes_current() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();

        queue.upsert(
            StatusId::AudioWarning,
            "mic error",
            Severity::Warning,
            None,
            t0,
        );

        assert_eq!(queue.current(), Some(("mic error", Severity::Warning)));
    }

    #[test]
    fn test_upsert_same_id_replaces_in_place_without_duplicating() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();

        queue.upsert(StatusId::SaveError, "first", Severity::Warning, None, t0);
        queue.upsert(StatusId::SaveError, "second", Severity::Warning, None, t0);

        assert_eq!(queue.text_for(StatusId::SaveError), Some("second"));
        assert_eq!(queue.messages.len(), 1);
    }

    #[test]
    fn test_clear_removes_only_the_named_message() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(StatusId::AudioWarning, "audio", Severity::Warning, None, t0);
        queue.upsert(StatusId::SaveError, "save", Severity::Warning, None, t0);

        queue.clear(StatusId::AudioWarning);

        assert!(queue.text_for(StatusId::AudioWarning).is_none());
        assert_eq!(queue.text_for(StatusId::SaveError), Some("save"));
    }

    #[test]
    fn test_message_expires_after_its_ttl() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(
            StatusId::ResumeWarning,
            "resuming",
            Severity::Info,
            Some(Duration::from_secs(4)),
            t0,
        );

        queue.tick(t0 + Duration::from_secs(2));
        assert!(
            queue.current().is_some(),
            "must still be visible before its ttl"
        );

        queue.tick(t0 + Duration::from_secs(5));
        assert!(
            queue.current().is_none(),
            "must expire once its ttl elapses"
        );
    }

    #[test]
    fn test_persistent_message_survives_ticks() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(
            StatusId::AudioWarning,
            "dead mic",
            Severity::Warning,
            None,
            t0,
        );

        queue.tick(t0 + Duration::from_secs(1000));

        assert_eq!(queue.current(), Some(("dead mic", Severity::Warning)));
    }

    #[test]
    fn test_rotates_through_multiple_messages_in_enqueue_order() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(StatusId::AudioWarning, "A", Severity::Warning, None, t0);
        queue.tick(t0);
        queue.upsert(StatusId::SaveError, "B", Severity::Warning, None, t0);

        assert_eq!(queue.current().map(|(t, _)| t), Some("A"));

        let t1 = t0 + StatusQueue::ROTATE_INTERVAL;
        queue.tick(t1);
        assert_eq!(
            queue.current().map(|(t, _)| t),
            Some("B"),
            "rotates to the next message"
        );

        let t2 = t1 + StatusQueue::ROTATE_INTERVAL;
        queue.tick(t2);
        assert_eq!(
            queue.current().map(|(t, _)| t),
            Some("A"),
            "cycles back around"
        );
    }

    #[test]
    fn test_single_message_does_not_rotate_away() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(StatusId::AudioWarning, "only", Severity::Warning, None, t0);

        queue.tick(t0 + StatusQueue::ROTATE_INTERVAL * 10);

        assert_eq!(queue.current().map(|(t, _)| t), Some("only"));
    }

    #[test]
    fn test_clearing_the_displayed_message_falls_back_to_remaining_one() {
        let t0 = Instant::now();
        let mut queue = StatusQueue::new();
        queue.upsert(StatusId::AudioWarning, "A", Severity::Warning, None, t0);
        queue.upsert(StatusId::SaveError, "B", Severity::Warning, None, t0);

        queue.clear(StatusId::AudioWarning);

        assert_eq!(queue.current().map(|(t, _)| t), Some("B"));
    }
}
