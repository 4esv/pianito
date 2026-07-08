//! Tuning-session state machine.
//!
//! `TuningFlow` owns the session-progression domain state that used to live
//! directly on `App`: which note is the current target, how far the
//! session has progressed, the lock-beep latch, and the target-frequency
//! math (temperament + optional Railsback stretch). `App` delegates to this
//! type and keeps only screen routing, key dispatch, and status rendering.

use std::collections::HashSet;

use super::notes::{Note, NOTE_COUNT};
use super::order::TuningOrder;
use super::session::{Session, TuningMode};
use super::stretch::StretchCurve;
use super::temperament::Temperament;

/// Data needed to display the current note; returned by
/// [`TuningFlow::current_note`], `None` once the session is finished.
pub struct CurrentNote {
    /// Display name (e.g. "F3").
    pub display_name: String,
    /// Position in the tuning order.
    pub note_index: usize,
    /// Total notes in the tuning order (always 88).
    pub total_notes: usize,
    /// Target frequency in Hz (temperament + optional stretch).
    pub target_freq: f32,
    /// Number of strings for this note (1, 2, or 3).
    pub string_count: u8,
    /// MIDI note number.
    pub midi: u8,
    /// Chromatic indices (midi - 21) of notes already completed, for the
    /// piano-progress overlay.
    pub completed_chromatic_indices: HashSet<usize>,
}

/// Owns the tuning-session state machine: the active [`Session`], the
/// [`TuningOrder`] it follows, the [`Temperament`] and optional
/// [`StretchCurve`] driving target frequencies, and the lock-beep latch.
pub struct TuningFlow {
    session: Session,
    tuning_order: TuningOrder,
    temperament: Temperament,
    stretch: Option<StretchCurve>,
    current_note_idx: usize,
    /// The current strike already reached the in-tune zone. Re-armed only
    /// on silence or a note/step change (see [`Self::rearm_beep_latch`]) so
    /// a beep can't retrigger off its own sound picked up by the
    /// microphone.
    in_tune_latched: bool,
}

impl TuningFlow {
    /// Start a brand-new session at the first note in `tuning_order`.
    pub fn new(
        mode: TuningMode,
        tuning_order: TuningOrder,
        temperament: Temperament,
        stretch: Option<StretchCurve>,
    ) -> Self {
        let session = Session::new(mode, temperament.a4());
        Self {
            session,
            tuning_order,
            temperament,
            stretch,
            current_note_idx: 0,
            in_tune_latched: false,
        }
    }

    /// Resume from a persisted session, picking up at
    /// `session.current_note_index`.
    pub fn resume(
        session: Session,
        tuning_order: TuningOrder,
        temperament: Temperament,
        stretch: Option<StretchCurve>,
    ) -> Self {
        let current_note_idx = session.current_note_index;
        Self {
            session,
            tuning_order,
            temperament,
            stretch,
            current_note_idx,
            in_tune_latched: false,
        }
    }

    /// The underlying session.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The underlying session, mutably.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Position in the tuning order (0-88; 88 means finished).
    pub fn current_note_index(&self) -> usize {
        self.current_note_idx
    }

    /// Whether every note has been visited (session finished).
    pub fn is_finished(&self) -> bool {
        self.current_note_idx >= NOTE_COUNT
    }

    /// Target frequency for a MIDI note: equal temperament plus the
    /// optional Railsback stretch.
    pub fn target_for_midi(&self, midi: u8) -> f32 {
        let base = self.temperament.frequency(midi);
        match &self.stretch {
            Some(curve) => curve.apply(base, midi),
            None => base,
        }
    }

    /// Convert a detected frequency to cents deviation from `target`.
    pub fn cents_from_target(&self, frequency: f32, target: f32) -> f32 {
        self.temperament.cents_from_target(frequency, target)
    }

    /// Data for the current note; `None` once the session is finished.
    pub fn current_note(&self) -> Option<CurrentNote> {
        let note = self.tuning_order.note_at(self.current_note_idx)?;
        let target_freq = self.target_for_midi(note.midi);

        // Chromatic indices (midi - 21) of notes already completed, looked
        // up by name since that's all a `CompletedNote` stores.
        let completed_chromatic_indices = self
            .session
            .completed_notes
            .iter()
            .filter_map(|cn| Note::from_name(&cn.note).map(|n| (n.midi - 21) as usize))
            .collect();

        Some(CurrentNote {
            display_name: note.display_name(),
            note_index: self.current_note_idx,
            total_notes: self.tuning_order.len(),
            target_freq,
            string_count: note.strings,
            midi: note.midi,
            completed_chromatic_indices,
        })
    }

    /// Re-arm the lock-beep latch: called on a new note/step, or on
    /// silence between strikes.
    pub fn rearm_beep_latch(&mut self) {
        self.in_tune_latched = false;
    }

    /// Observe a pitch reading against `tolerance`. Returns `true` at most
    /// once per strike — the first reading that lands inside the in-tune
    /// zone — so callers can fire a confirmation beep exactly once. Not
    /// re-armed by an out-of-tolerance reading (only by
    /// [`Self::rearm_beep_latch`]) so the microphone picking up the beep
    /// itself can't retrigger it.
    pub fn observe_lock(&mut self, cents: f32, tolerance: f32, is_muting: bool) -> bool {
        if !is_muting && cents.abs() <= tolerance && !self.in_tune_latched {
            self.in_tune_latched = true;
            true
        } else {
            false
        }
    }

    /// Record the current note complete and advance. Returns `true` once
    /// the session is finished.
    pub fn confirm_current_note(&mut self, final_cents: f32) -> bool {
        if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
            self.session.complete_note(note.display_name(), final_cents);
        }
        self.advance()
    }

    /// Skip the current note without recording a completion (it must not
    /// count as 0.0 cents in progress or history). Returns `true` once the
    /// session is finished.
    pub fn skip_current_note(&mut self) -> bool {
        self.session.skip_note();
        self.advance()
    }

    /// Advance to the next note, persisting progress on the session
    /// (including the final advance to 88, so a finished session is stored
    /// complete and doesn't get reopened on resume).
    fn advance(&mut self) -> bool {
        self.current_note_idx += 1;
        self.session.current_note_index = self.current_note_idx;
        self.in_tune_latched = false;
        self.is_finished()
    }

    /// Step back to the previous note (no-op, returns `false`, at note 0).
    /// Drops that note's stale completion record so re-confirming it can't
    /// duplicate it.
    pub fn go_back_note(&mut self) -> bool {
        if self.current_note_idx == 0 {
            return false;
        }
        self.current_note_idx -= 1;
        self.session.current_note_index = self.current_note_idx;
        if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
            let name = note.display_name();
            self.session.completed_notes.retain(|cn| cn.note != name);
        }
        self.in_tune_latched = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow() -> TuningFlow {
        TuningFlow::new(
            TuningMode::Concert,
            TuningOrder::new(),
            Temperament::with_a4(440.0),
            Some(StretchCurve::railsback_default()),
        )
    }

    #[test]
    fn test_new_starts_at_first_note() {
        let flow = flow();
        assert_eq!(flow.current_note_index(), 0);
        assert!(!flow.is_finished());
        assert_eq!(flow.session().current_note_index, 0);
        assert_eq!(flow.session().a4_reference, 440.0);
    }

    #[test]
    fn test_resume_picks_up_at_saved_index() {
        let mut session = Session::new(TuningMode::Concert, 441.0);
        session.current_note_index = 5;

        let flow = TuningFlow::resume(
            session,
            TuningOrder::new(),
            Temperament::with_a4(441.0),
            None,
        );
        assert_eq!(flow.current_note_index(), 5);
        assert!(!flow.is_finished());
    }

    #[test]
    fn test_confirm_current_note_advances_and_records() {
        let mut flow = flow();
        let finished = flow.confirm_current_note(1.5);

        assert!(!finished);
        assert_eq!(flow.current_note_index(), 1);
        assert_eq!(flow.session().current_note_index, 1);
        assert_eq!(flow.session().completed_notes.len(), 1);
        assert_eq!(flow.session().completed_notes[0].final_cents, 1.5);
    }

    #[test]
    fn test_skip_current_note_advances_without_recording() {
        let mut flow = flow();
        let finished = flow.skip_current_note();

        assert!(!finished);
        assert_eq!(flow.current_note_index(), 1);
        assert!(
            flow.session().completed_notes.is_empty(),
            "skipped notes must not count as tuned at 0.0 cents"
        );
    }

    #[test]
    fn test_confirming_last_note_finishes_session() {
        let mut flow = flow();
        for _ in 0..87 {
            assert!(!flow.skip_current_note());
        }
        assert_eq!(flow.current_note_index(), 87);

        let finished = flow.confirm_current_note(0.0);
        assert!(finished);
        assert!(flow.is_finished());
        assert_eq!(flow.session().current_note_index, 88);
        assert!(flow.session().is_complete());
    }

    #[test]
    fn test_go_back_note_removes_stale_completion() {
        let mut flow = flow();
        flow.confirm_current_note(2.0);
        assert_eq!(flow.session().completed_notes.len(), 1);

        let went_back = flow.go_back_note();
        assert!(went_back);
        assert_eq!(flow.current_note_index(), 0);
        assert!(
            flow.session().completed_notes.is_empty(),
            "record removed so re-confirming can't duplicate it"
        );
    }

    #[test]
    fn test_go_back_note_at_zero_is_noop() {
        let mut flow = flow();
        assert!(!flow.go_back_note());
        assert_eq!(flow.current_note_index(), 0);
    }

    #[test]
    fn test_lock_latch_fires_once_then_requires_rearm() {
        let mut flow = flow();

        // Out of tune: no lock.
        assert!(!flow.observe_lock(30.0, 5.0, false));
        // In tune: locks exactly once.
        assert!(flow.observe_lock(1.0, 5.0, false));
        assert!(
            !flow.observe_lock(0.0, 5.0, false),
            "still locked: no retrigger"
        );

        // Silence re-arms; the next in-tune reading locks again.
        flow.rearm_beep_latch();
        assert!(flow.observe_lock(0.0, 5.0, false));
    }

    #[test]
    fn test_lock_latch_ignores_muting_step() {
        let mut flow = flow();
        assert!(!flow.observe_lock(0.0, 5.0, true));
    }

    #[test]
    fn test_target_for_midi_applies_stretch() {
        let flow = flow();
        let temperament = Temperament::with_a4(440.0);
        assert!(
            flow.target_for_midi(21) < temperament.frequency(21),
            "bass stretched flat"
        );
        assert!(
            flow.target_for_midi(108) > temperament.frequency(108),
            "treble stretched sharp"
        );
    }

    #[test]
    fn test_target_for_midi_without_stretch_matches_temperament() {
        let flow = TuningFlow::new(
            TuningMode::Concert,
            TuningOrder::new(),
            Temperament::with_a4(440.0),
            None,
        );
        let temperament = Temperament::with_a4(440.0);
        assert_eq!(flow.target_for_midi(21), temperament.frequency(21));
        assert_eq!(flow.target_for_midi(108), temperament.frequency(108));
    }

    #[test]
    fn test_current_note_reflects_completed_notes() {
        let mut flow = flow();
        flow.confirm_current_note(0.5);

        let current = flow.current_note().expect("session not finished");
        assert_eq!(current.note_index, 1);
        assert_eq!(current.total_notes, 88);
        assert_eq!(
            current.completed_chromatic_indices.len(),
            1,
            "first note's completion should show up in the progress set"
        );
    }

    #[test]
    fn test_current_note_is_none_once_finished() {
        let mut flow = flow();
        for _ in 0..88 {
            flow.skip_current_note();
        }
        assert!(flow.is_finished());
        assert!(flow.current_note().is_none());
    }
}
