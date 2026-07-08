//! Main application state machine.

use std::collections::HashSet;

use crossterm::event::KeyCode;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

use crate::config::EffectiveConfig;
use crate::tuning::order::TuningOrder;
use crate::tuning::profile::PianoProfile;
use crate::tuning::session::{Session, TuningMode};
use crate::tuning::stretch::StretchCurve;
use crate::tuning::temperament::Temperament;

use super::screens::{
    mode_select::SelectedMode, CalibrationScreen, CompleteScreen, ModeSelectScreen,
    ProfilingScreen, TuningScreen,
};
use super::theme::Theme;

/// Application screen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Mode selection screen.
    ModeSelect,
    /// Calibration (for quick tune).
    Calibration,
    /// Profiling all 88 keys.
    Profiling,
    /// Main tuning screen.
    Tuning,
    /// Session complete.
    Complete,
}

/// Main application.
pub struct App {
    /// Current state.
    state: AppState,
    /// Current session.
    session: Option<Session>,
    /// Should quit flag.
    should_quit: bool,
    /// Mode select screen.
    mode_select: ModeSelectScreen,
    /// Calibration screen.
    calibration: CalibrationScreen,
    /// Profiling screen (for profile mode).
    profiling: Option<ProfilingScreen>,
    /// Piano profile (from profiling mode).
    profile: Option<PianoProfile>,
    /// Tuning screen (created when tuning starts).
    tuning: Option<TuningScreen>,
    /// Complete screen (created when session ends).
    complete: Option<CompleteScreen>,
    /// Tuning order.
    tuning_order: TuningOrder,
    /// Temperament calculator.
    temperament: Temperament,
    /// Current note index in tuning order.
    current_note_idx: usize,
    /// Configured A4 reference (from CLI/config file) for new sessions.
    configured_a4: f32,
    /// In-tune tolerance in cents (from CLI/config file).
    tolerance: f32,
    /// Mode preselected in the menu (from CLI/config file); re-applied
    /// when the menu comes back after a completed session.
    default_mode: SelectedMode,
    /// Whether to beep when a note locks into the in-tune zone.
    beep_enabled: bool,
    /// A beep is due; the main loop drains this via `take_beep` (the App
    /// has no audio output of its own).
    beep_pending: bool,
    /// The current strike already reached the in-tune zone. Re-armed only
    /// on silence or a note/step change so the beep can't retrigger off
    /// its own sound picked up by the microphone.
    in_tune_latched: bool,
    /// Railsback stretch curve applied to targets (None = pure equal
    /// temperament).
    stretch: Option<StretchCurve>,
    /// Most recent save failure (session autosave or profile save),
    /// surfaced in the status line.
    save_error: Option<String>,
    /// Warning produced while resuming a session.
    resume_warning: Option<String>,
    /// Most recent audio-stream error reported by the capture backend.
    audio_warning: Option<String>,
}

impl App {
    /// Create a new application.
    pub fn new() -> Self {
        Self {
            state: AppState::ModeSelect,
            session: None,
            should_quit: false,
            mode_select: ModeSelectScreen::new(),
            calibration: CalibrationScreen::new(),
            profiling: None,
            profile: None,
            tuning: None,
            complete: None,
            tuning_order: TuningOrder::new(),
            temperament: Temperament::new(),
            current_note_idx: 0,
            configured_a4: 440.0,
            tolerance: 5.0,
            default_mode: SelectedMode::default(),
            beep_enabled: false,
            beep_pending: false,
            in_tune_latched: false,
            stretch: Some(StretchCurve::new()),
            save_error: None,
            resume_warning: None,
            audio_warning: None,
        }
    }

    /// Create a new application honoring the effective CLI/config settings:
    /// A4 reference, in-tune tolerance, lock beep, and the default mode
    /// preselected in the menu.
    pub fn with_config(config: &EffectiveConfig) -> Self {
        let mut app = Self::new();
        app.configured_a4 = config.a4;
        app.tolerance = config.tolerance;
        app.beep_enabled = config.beep;
        app.default_mode = if config.quick_mode {
            SelectedMode::QuickTune
        } else {
            SelectedMode::ConcertPitch
        };
        app.mode_select.select(app.default_mode);
        app
    }

    /// Create app with an existing session (for resume). Tolerance and beep
    /// come from the config; the A4 reference comes from the session (the
    /// piano is mid-tune at that reference, so the config value must not
    /// silently retarget the remaining notes). `configured_a4` keeps the
    /// CLI/config value: it only feeds sessions started later from the menu.
    pub fn with_session(session: Session, config: &EffectiveConfig) -> Self {
        let mut app = Self::with_config(config);
        app.current_note_idx = session.current_note_index;
        app.temperament = Temperament::with_a4(session.a4_reference);

        // Profile sessions were ordered by measured deviation; resuming with
        // the traditional order would drop the user on an unrelated note and
        // re-queue notes already tuned.
        if session.mode == TuningMode::Profile {
            match Self::profile_for_session(&session) {
                Some(profile) => {
                    app.tuning_order = TuningOrder::from_profile(&profile);
                    app.profile = Some(profile);
                }
                None => {
                    // WARNING: without the saved profile the deviation order
                    // cannot be rebuilt; say so instead of switching silently.
                    app.resume_warning =
                        Some("Saved profile not found - resuming in traditional order".to_string());
                }
            }
        }

        app.session = Some(session);
        app.state = AppState::Tuning;
        app.setup_current_note();
        app
    }

    /// Find the profile that produced a Profile-mode session.
    fn profile_for_session(session: &Session) -> Option<PianoProfile> {
        let profiles = PianoProfile::list_all().ok()?;
        Self::find_matching_profile(session, &profiles).cloned()
    }

    /// Match a session to its profile: prefer the explicit `profile_id` link
    /// (set on every session since it was introduced); fall back to "the
    /// most recent profile saved at or before the session was created" for
    /// sessions saved before the link existed (the profile is written
    /// immediately before the session starts, so recency alone used to be
    /// the only signal).
    ///
    /// Split out from `profile_for_session` so the matching logic can be
    /// unit-tested without touching the real user data dir that
    /// `PianoProfile::list_all` reads from. `profiles` must be sorted
    /// most-recent-first, as `list_all` returns them.
    fn find_matching_profile<'a>(
        session: &Session,
        profiles: &'a [PianoProfile],
    ) -> Option<&'a PianoProfile> {
        match &session.profile_id {
            Some(profile_id) => profiles.iter().find(|p| &p.id == profile_id),
            None => profiles.iter().find(|p| p.created_at <= session.created_at),
        }
    }

    /// Enable or disable stretch tuning (Railsback curve).
    pub fn set_stretch(&mut self, enabled: bool) {
        self.stretch = enabled.then(StretchCurve::new);
    }

    /// Error from the most recent save (session or profile), if any. Also
    /// rendered in the status line; available for printing after terminal
    /// restore.
    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    /// Record an audio-stream error (e.g. microphone unplugged) for display
    /// in the status line. The main loop polls `MicCapture::take_error` and
    /// parks the message here instead of writing to stderr while the
    /// terminal is in raw mode.
    pub fn set_audio_warning(&mut self, msg: String) {
        self.audio_warning = Some(msg);
    }

    /// Drain the pending beep request. Returns true at most once per lock:
    /// the main loop polls this each iteration and plays the confirmation
    /// tone (the App itself never touches the audio output).
    pub fn take_beep(&mut self) -> bool {
        std::mem::take(&mut self.beep_pending)
    }

    /// Get current state.
    pub fn state(&self) -> AppState {
        self.state
    }

    /// Check if the app should quit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Request quit.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Get current session.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Get mutable session.
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// Get target frequency for current note.
    pub fn current_target_freq(&self) -> Option<f32> {
        self.tuning.as_ref().map(|t| t.target_freq())
    }

    /// Handle key press event.
    pub fn handle_key(&mut self, key: KeyCode) {
        match self.state {
            AppState::ModeSelect => self.handle_mode_select_key(key),
            AppState::Calibration => self.handle_calibration_key(key),
            AppState::Profiling => self.handle_profiling_key(key),
            AppState::Tuning => self.handle_tuning_key(key),
            AppState::Complete => self.handle_complete_key(key),
        }
    }

    fn handle_mode_select_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up => {
                self.mode_select.prev();
            }
            KeyCode::Down | KeyCode::Tab => {
                self.mode_select.next();
            }
            KeyCode::Enter => {
                self.start_session();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.quit();
            }
            _ => {}
        }
    }

    fn handle_calibration_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip calibration, use the configured A4 reference
                self.temperament = Temperament::with_a4(self.configured_a4);
                self.start_tuning();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.quit();
            }
            _ => {}
        }
    }

    fn handle_tuning_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char(' ') => {
                // Confirm current note/step
                self.confirm_note();
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                // Go back to previous step or note
                self.go_back();
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Toggle piano progress display
                self.toggle_piano_progress();
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip current note
                self.skip_note();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                // Save session before quitting; a failure is kept in
                // save_error so the caller can report it after restore
                self.autosave_session();
                self.quit();
            }
            _ => {}
        }
    }

    /// Toggle piano progress display.
    fn toggle_piano_progress(&mut self) {
        if let Some(tuning) = &mut self.tuning {
            tuning.toggle_piano_progress();
        }
    }

    fn handle_complete_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter => {
                // Start new session
                self.reset();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.quit();
            }
            _ => {}
        }
    }

    /// Start a new tuning session based on selected mode.
    fn start_session(&mut self) {
        let mode = match self.mode_select.selected() {
            SelectedMode::QuickTune => TuningMode::Quick,
            SelectedMode::ConcertPitch => TuningMode::Concert,
            SelectedMode::Profile => TuningMode::Profile,
        };

        match mode {
            TuningMode::Quick => {
                self.state = AppState::Calibration;
                self.calibration.reset();
            }
            TuningMode::Concert => {
                self.temperament = Temperament::with_a4(self.configured_a4);
                self.start_tuning();
            }
            TuningMode::Profile => {
                self.start_profiling();
            }
        }
    }

    /// Start the profiling phase.
    fn start_profiling(&mut self) {
        self.temperament = Temperament::with_a4(self.configured_a4);
        let mut profiling = ProfilingScreen::new();
        profiling.set_tolerance(self.tolerance);
        self.profiling = Some(profiling);
        self.state = AppState::Profiling;
        self.sync_profiling_target();
    }

    /// Handle key press in profiling state.
    fn handle_profiling_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char(' ') => {
                // Confirm current note (no-op without a pitch reading)
                if let Some(profiling) = &mut self.profiling {
                    if profiling.confirm_note() {
                        self.finish_profiling();
                    } else {
                        self.sync_profiling_target();
                    }
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                // Go back to previous note
                if let Some(profiling) = &mut self.profiling {
                    profiling.go_back();
                }
                self.sync_profiling_target();
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip current note
                if let Some(profiling) = &mut self.profiling {
                    if profiling.skip_note() {
                        self.finish_profiling();
                    } else {
                        self.sync_profiling_target();
                    }
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.quit();
            }
            _ => {}
        }
    }

    /// Push the current note's target into the profiling screen so the info
    /// panel and the meter read from the same number.
    fn sync_profiling_target(&mut self) {
        let midi = match &self.profiling {
            Some(profiling) if !profiling.is_complete() => profiling.current_note().midi,
            _ => return,
        };
        let target = self.target_for_midi(midi);
        if let Some(profiling) = &mut self.profiling {
            profiling.set_target_freq(target);
        }
    }

    /// Target frequency for a MIDI note: equal temperament plus the optional
    /// Railsback stretch.
    fn target_for_midi(&self, midi: u8) -> f32 {
        let base = self.temperament.frequency(midi);
        match &self.stretch {
            Some(curve) => curve.apply(base, midi),
            None => base,
        }
    }

    /// Finish profiling and transition to tuning.
    fn finish_profiling(&mut self) {
        if let Some(profiling) = self.profiling.take() {
            let profile = profiling.take_profile();

            // Save the profile; continue tuning either way. NOTE: no
            // eprintln! here — the TUI owns the terminal, so the failure is
            // parked in save_error for the status line (and for printing
            // after restore on quit).
            if let Err(e) = profile.save() {
                self.save_error = Some(format!("Failed to save profile: {}", e));
            }

            // Create tuning order based on profile deviations
            self.tuning_order = TuningOrder::from_profile(&profile);
            self.profile = Some(profile);

            // Start tuning with the profile-based order
            self.start_tuning();
        }
    }

    /// Start tuning after calibration.
    fn start_tuning(&mut self) {
        let mode = match self.mode_select.selected() {
            SelectedMode::QuickTune => TuningMode::Quick,
            SelectedMode::ConcertPitch => TuningMode::Concert,
            SelectedMode::Profile => TuningMode::Profile,
        };

        let mut session = Session::new(mode, self.temperament.a4());
        // Profile mode: link the session to the profile that ordered it, so
        // resuming can't reattach a different piano's profile (see
        // `find_matching_profile`). `self.profile` is set by
        // `finish_profiling` before this runs.
        if let Some(profile) = &self.profile {
            session.profile_id = Some(profile.id.clone());
        }
        self.session = Some(session);
        self.current_note_idx = 0;
        self.state = AppState::Tuning;
        self.setup_current_note();
    }

    /// Set up the tuning screen for the current note.
    fn setup_current_note(&mut self) {
        // New note, new strike: re-arm the lock beep
        self.in_tune_latched = false;

        if self.current_note_idx >= 88 {
            self.finish_session();
            return;
        }

        if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
            let target_freq = self.target_for_midi(note.midi);

            // Collect completed chromatic indices from session (midi - 21)
            let completed_notes: HashSet<usize> = if let Some(session) = &self.session {
                session
                    .completed_notes
                    .iter()
                    .filter_map(|cn| {
                        // Look up note by name to get its midi, then convert to chromatic index
                        crate::tuning::notes::Note::from_name(&cn.note)
                            .map(|n| (n.midi - 21) as usize)
                    })
                    .collect()
            } else {
                HashSet::new()
            };

            let mut tuning = TuningScreen::new(
                note.display_name(),
                self.current_note_idx,
                88,
                target_freq,
                note.strings,
                note.midi,
            );
            tuning.set_completed_notes(completed_notes);
            tuning.set_tolerance(self.tolerance);
            self.tuning = Some(tuning);
        }
    }

    /// Update with detected pitch.
    pub fn update_pitch(&mut self, freq: f32, confidence: f32) {
        match self.state {
            AppState::Calibration => {
                if confidence > 0.8 {
                    self.calibration.update(freq);
                    if self.calibration.is_complete() {
                        if let Some(a4) = self.calibration.result() {
                            self.temperament = Temperament::with_a4(a4);
                        }
                        self.start_tuning();
                    }
                }
            }
            AppState::Profiling => {
                if let Some(profiling) = &mut self.profiling {
                    if confidence > 0.6 {
                        // Use the screen's synced target so meter and info
                        // panel agree
                        let target = profiling.target_freq();
                        let cents = self.temperament.cents_from_target(freq, target);
                        profiling.update(freq, cents);
                    } else {
                        profiling.clear();
                    }
                }
            }
            AppState::Tuning => {
                if let Some(tuning) = &mut self.tuning {
                    if confidence > 0.6 {
                        let target = tuning.target_freq();
                        let cents = self.temperament.cents_from_target(freq, target);
                        tuning.update(freq, cents);

                        // Lock beep: fire once when the strike first enters
                        // the in-tune zone. The latch is NOT re-armed by an
                        // out-of-tolerance reading — the microphone may pick
                        // up the beep itself and the retrigger would loop.
                        let is_muting =
                            tuning.tuning_step().map(|s| s.is_muting()).unwrap_or(false);
                        if !is_muting && cents.abs() <= self.tolerance && !self.in_tune_latched {
                            self.in_tune_latched = true;
                            if self.beep_enabled {
                                self.beep_pending = true;
                            }
                        }
                    } else {
                        tuning.clear();
                    }
                }
            }
            _ => {}
        }
    }

    /// Clear pitch detection (silence).
    pub fn clear_pitch(&mut self) {
        // Silence between strikes re-arms the lock beep
        self.in_tune_latched = false;
        match self.state {
            AppState::Calibration => {
                self.calibration.clear();
            }
            AppState::Profiling => {
                if let Some(profiling) = &mut self.profiling {
                    profiling.clear();
                }
            }
            AppState::Tuning => {
                if let Some(tuning) = &mut self.tuning {
                    tuning.clear();
                }
            }
            _ => {}
        }
    }

    /// Confirm current note is tuned.
    fn confirm_note(&mut self) {
        if let Some(tuning) = &mut self.tuning {
            // For multi-string notes (bichord/trichord), advance through steps
            if tuning.is_multi_string() && tuning.next_step() {
                // New step, new strike: re-arm the lock beep
                self.in_tune_latched = false;
                return;
            }

            // Record completion
            if let Some(session) = &mut self.session {
                if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
                    session.complete_note(note.display_name(), tuning.cents());
                }
            }

            self.advance_to_next_note();
        }
    }

    /// Go back to previous step or previous note.
    fn go_back(&mut self) {
        // Try to go to previous step first
        if let Some(tuning) = &mut self.tuning {
            if tuning.prev_step() {
                self.in_tune_latched = false;
                return;
            }
        }

        // Otherwise go to previous note
        if self.current_note_idx > 0 {
            self.current_note_idx -= 1;

            // Drop the stale completion record so re-confirming the note
            // doesn't duplicate it (and doesn't show it as already done)
            if let Some(session) = &mut self.session {
                session.current_note_index = self.current_note_idx;
                if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
                    let name = note.display_name();
                    session.completed_notes.retain(|cn| cn.note != name);
                }
            }
            self.autosave_session();

            self.setup_current_note();

            // For multi-string notes, go to last step
            if let Some(tuning) = &mut self.tuning {
                while tuning.next_step() {}
            }
        }
    }

    /// Skip current note.
    fn skip_note(&mut self) {
        // Advance without recording a completion: a skipped note is untuned
        // and must not count as 0.0 cents in progress or history
        if let Some(session) = &mut self.session {
            session.skip_note();
        }

        self.advance_to_next_note();
    }

    /// Advance to the next note.
    fn advance_to_next_note(&mut self) {
        self.current_note_idx += 1;

        // Persist progress, including the final advance to 88, so a finished
        // session is stored complete and --resume doesn't reopen it
        if let Some(session) = &mut self.session {
            session.current_note_index = self.current_note_idx;
        }
        self.autosave_session();

        if self.current_note_idx >= 88 {
            self.finish_session();
        } else {
            self.setup_current_note();
        }
    }

    /// Finish the tuning session. The session is kept (not taken) so the
    /// completed state remains inspectable until reset.
    fn finish_session(&mut self) {
        let completed_notes = self
            .session
            .as_ref()
            .map(|s| s.completed_notes.clone())
            .unwrap_or_default();
        self.complete = Some(CompleteScreen::new(completed_notes));
        self.state = AppState::Complete;
    }

    /// Persist the current session, capturing failures for display instead
    /// of discarding them.
    fn autosave_session(&mut self) {
        // NOTE: unit tests drive the tuning flow in-memory; don't write to
        // the user's real data dir from the test suite
        if cfg!(test) {
            return;
        }
        let result = match &self.session {
            Some(session) => session.save(),
            None => return,
        };
        match result {
            Ok(()) => self.save_error = None,
            Err(e) => self.save_error = Some(format!("Session save failed: {}", e)),
        }
    }

    /// Reset to start a new session.
    fn reset(&mut self) {
        self.state = AppState::ModeSelect;
        self.session = None;
        self.profiling = None;
        self.profile = None;
        self.tuning = None;
        self.complete = None;
        self.current_note_idx = 0;
        self.tuning_order = TuningOrder::new();
        self.mode_select = ModeSelectScreen::new();
        self.mode_select.select(self.default_mode);
        self.calibration = CalibrationScreen::new();
        self.save_error = None;
        self.resume_warning = None;
        self.beep_pending = false;
        self.in_tune_latched = false;
        // NOTE: tolerance and beep_enabled are config-scoped and survive
        // reset, like configured_a4.
        // NOTE: audio_warning is deliberately kept across reset — a dead
        // mic stays dead in the next session and cpal fatal stream errors
        // fire only once, so clearing here would hide the only sign of it.
    }

    /// Render the current screen.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        match self.state {
            AppState::ModeSelect => {
                frame.render_widget(&self.mode_select, area);
            }
            AppState::Calibration => {
                frame.render_widget(&self.calibration, area);
            }
            AppState::Profiling => {
                if let Some(profiling) = &self.profiling {
                    frame.render_widget(profiling, area);
                }
            }
            AppState::Tuning => {
                if let Some(tuning) = &self.tuning {
                    frame.render_widget(tuning, area);
                }
            }
            AppState::Complete => {
                if let Some(complete) = &self.complete {
                    frame.render_widget(complete, area);
                }
            }
        }

        // Status line for audio errors / save failures / resume warnings,
        // drawn over the bottom border of every screen so it can't be
        // missed (audio errors can occur in any state)
        let status = self
            .audio_warning
            .as_deref()
            .or(self.save_error.as_deref())
            .or(self.resume_warning.as_deref());
        if let Some(msg) = status {
            if area.height > 2 && area.width > 4 {
                let status_area = Rect::new(
                    area.x + 2,
                    area.y + area.height - 1,
                    area.width.saturating_sub(4),
                    1,
                );
                let warning = Paragraph::new(msg).style(Theme::warning());
                frame.render_widget(warning, status_area);
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(a4: f32) -> EffectiveConfig {
        EffectiveConfig {
            a4,
            tolerance: 5.0,
            beep: false,
            quick_mode: false,
            resume: false,
        }
    }

    /// App in Tuning state with a Concert session (no disk I/O in tests:
    /// autosave_session is a no-op under cfg(test)).
    fn concert_app() -> App {
        let mut app = App::new();
        app.mode_select.next(); // QuickTune -> ConcertPitch
        app.start_session();
        assert_eq!(app.state(), AppState::Tuning);
        app
    }

    /// Confirm through all steps of the current note until it's recorded.
    fn confirm_note_fully(app: &mut App) {
        let before = app.session.as_ref().unwrap().completed_notes.len();
        while app.session.as_ref().unwrap().completed_notes.len() == before {
            app.confirm_note();
        }
    }

    #[test]
    fn test_mode_select_up_wraps_backward() {
        let mut app = App::new();

        app.handle_key(KeyCode::Up);
        assert_eq!(app.mode_select.selected(), SelectedMode::Profile);

        app.handle_key(KeyCode::Down);
        assert_eq!(app.mode_select.selected(), SelectedMode::QuickTune);
    }

    #[test]
    fn test_configured_a4_used_for_concert_session() {
        // with_config preselects ConcertPitch (quick_mode: false)
        let mut app = App::with_config(&test_config(442.0));
        app.start_session();

        assert!((app.temperament.a4() - 442.0).abs() < f32::EPSILON);
        let session = app.session.as_ref().unwrap();
        assert!((session.a4_reference - 442.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_configured_a4_used_for_profile_session() {
        let mut app = App::with_config(&test_config(442.0));
        app.mode_select.select(SelectedMode::Profile);
        app.start_session();

        assert_eq!(app.state(), AppState::Profiling);
        assert!((app.temperament.a4() - 442.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_configured_a4_used_when_skipping_calibration() {
        let config = EffectiveConfig {
            quick_mode: true,
            ..test_config(438.0)
        };
        let mut app = App::with_config(&config);
        app.start_session(); // QuickTune -> Calibration
        assert_eq!(app.state(), AppState::Calibration);

        app.handle_key(KeyCode::Char('s'));
        assert!((app.temperament.a4() - 438.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_skip_note_is_not_recorded_as_completed() {
        let mut app = concert_app();
        app.skip_note();

        let session = app.session.as_ref().unwrap();
        assert!(
            session.completed_notes.is_empty(),
            "skipped notes must not count as tuned at 0.0 cents"
        );
        assert_eq!(session.current_note_index, 1);
        assert_eq!(app.current_note_idx, 1);
    }

    #[test]
    fn test_go_back_removes_stale_completion() {
        let mut app = concert_app();
        confirm_note_fully(&mut app);
        assert_eq!(app.session.as_ref().unwrap().completed_notes.len(), 1);

        app.go_back();
        assert_eq!(app.current_note_idx, 0);
        assert!(
            app.session.as_ref().unwrap().completed_notes.is_empty(),
            "record removed so re-confirming can't duplicate it"
        );

        confirm_note_fully(&mut app);
        assert_eq!(
            app.session.as_ref().unwrap().completed_notes.len(),
            1,
            "re-confirm must not create a duplicate entry"
        );
    }

    #[test]
    fn test_finished_session_is_marked_complete() {
        let mut app = concert_app();

        // Jump to the last note
        app.current_note_idx = 87;
        if let Some(session) = app.session.as_mut() {
            session.current_note_index = 87;
        }
        app.setup_current_note();
        confirm_note_fully(&mut app);

        assert_eq!(app.state(), AppState::Complete);
        let session = app.session.as_ref().expect("session retained on finish");
        assert_eq!(session.current_note_index, 88);
        assert!(
            session.is_complete(),
            "a finished session must not be resumable"
        );
    }

    #[test]
    fn test_stretch_applied_to_targets() {
        let app = App::new();

        // Railsback: bass flat of pure ET, treble sharp
        assert!(app.target_for_midi(21) < app.temperament.frequency(21));
        assert!(app.target_for_midi(108) > app.temperament.frequency(108));

        // Small near the temperament zone
        let a4_delta = (app.target_for_midi(69) - app.temperament.frequency(69)).abs();
        assert!(a4_delta < 1.0);
    }

    #[test]
    fn test_stretch_can_be_disabled() {
        let mut app = App::new();
        app.set_stretch(false);
        assert_eq!(app.target_for_midi(21), app.temperament.frequency(21));
        assert_eq!(app.target_for_midi(108), app.temperament.frequency(108));
    }

    #[test]
    fn test_resume_uses_session_a4_and_index() {
        let mut session = Session::new(TuningMode::Concert, 441.0);
        session.current_note_index = 5;

        let app = App::with_session(session, &test_config(440.0));
        assert_eq!(app.state(), AppState::Tuning);
        assert_eq!(app.current_note_idx, 5);
        assert!((app.temperament.a4() - 441.0).abs() < f32::EPSILON);
        // configured_a4 stays config-scoped: a new session started after
        // this resume must not inherit the resumed session's A4
        assert!((app.configured_a4 - 440.0).abs() < f32::EPSILON);
        assert!(app.resume_warning.is_none());
    }

    #[test]
    fn test_start_tuning_links_profile_id_when_profile_present() {
        let mut app = App::with_config(&test_config(440.0));
        app.mode_select.select(SelectedMode::Profile);
        let profile = PianoProfile::new();
        let profile_id = profile.id.clone();
        app.profile = Some(profile);

        app.start_tuning();

        let session = app.session.as_ref().expect("session created");
        assert_eq!(session.profile_id.as_deref(), Some(profile_id.as_str()));
    }

    #[test]
    fn test_start_tuning_leaves_profile_id_none_without_profile() {
        let app = concert_app(); // Concert mode: no profile involved
        let session = app.session.as_ref().expect("session created");
        assert!(session.profile_id.is_none());
    }

    /// Build a minimal profile with a given id/created_at for matching tests.
    fn test_profile(id: &str, created_at: chrono::DateTime<chrono::Utc>) -> PianoProfile {
        let mut profile = PianoProfile::new();
        profile.id = id.to_string();
        profile.created_at = created_at;
        profile
    }

    #[test]
    fn test_find_matching_profile_prefers_explicit_link_over_recency() {
        // Regression for #21: profile A, then profile B (more recent), then
        // resume A's session must reattach A - not silently pick B.
        let now = chrono::Utc::now();
        let profile_a = test_profile("profile-a", now - chrono::Duration::minutes(10));
        let profile_b = test_profile("profile-b", now - chrono::Duration::minutes(1));
        // list_all() returns most-recent-first.
        let profiles = vec![profile_b, profile_a];

        let mut session = Session::new(TuningMode::Profile, 440.0);
        session.created_at = now;
        session.profile_id = Some("profile-a".to_string());

        let found = App::find_matching_profile(&session, &profiles).expect("profile found");
        assert_eq!(
            found.id, "profile-a",
            "explicit link must win over the more recent profile"
        );
    }

    #[test]
    fn test_find_matching_profile_falls_back_to_recency_heuristic_without_link() {
        // Legacy sessions (saved before profile_id existed) keep the old
        // "most recent profile at or before the session" behavior.
        let now = chrono::Utc::now();
        let profile_a = test_profile("profile-a", now - chrono::Duration::minutes(10));
        let profile_b = test_profile("profile-b", now - chrono::Duration::minutes(1));
        let profiles = vec![profile_b, profile_a];

        let mut session = Session::new(TuningMode::Profile, 440.0);
        session.created_at = now;
        session.profile_id = None;

        let found = App::find_matching_profile(&session, &profiles).expect("profile found");
        assert_eq!(
            found.id, "profile-b",
            "legacy sessions use the most-recent-at-or-before heuristic"
        );
    }

    #[test]
    fn test_find_matching_profile_returns_none_when_linked_profile_missing() {
        // A dangling profile_id (e.g. the profile was deleted) must not
        // silently fall back to the heuristic and reattach the wrong piano.
        let now = chrono::Utc::now();
        let profiles = vec![test_profile(
            "profile-b",
            now - chrono::Duration::minutes(1),
        )];

        let mut session = Session::new(TuningMode::Profile, 440.0);
        session.created_at = now;
        session.profile_id = Some("missing-profile".to_string());

        assert!(App::find_matching_profile(&session, &profiles).is_none());
    }

    /// Config-driven app in Tuning state on the first note (F3, a bichord),
    /// advanced past the mute step so the meter is live.
    fn beeping_app(tolerance: f32) -> App {
        let config = EffectiveConfig {
            tolerance,
            beep: true,
            ..test_config(440.0)
        };
        let mut app = App::with_config(&config); // ConcertPitch preselected
        app.start_session();
        assert_eq!(app.state(), AppState::Tuning);
        app.handle_key(KeyCode::Char(' ')); // past the bichord mute step
        app
    }

    /// Frequency `cents` sharp of `freq`.
    fn sharp(freq: f32, cents: f32) -> f32 {
        freq * 2f32.powf(cents / 1200.0)
    }

    #[test]
    fn test_with_config_preselects_default_mode() {
        let app = App::with_config(&test_config(440.0));
        assert_eq!(app.mode_select.selected(), SelectedMode::ConcertPitch);

        let config = EffectiveConfig {
            quick_mode: true,
            ..test_config(440.0)
        };
        let app = App::with_config(&config);
        assert_eq!(app.mode_select.selected(), SelectedMode::QuickTune);
    }

    #[test]
    fn test_reset_restores_configured_default_mode() {
        let config = EffectiveConfig {
            quick_mode: true,
            ..test_config(440.0)
        };
        let mut app = App::with_config(&config);
        app.mode_select.select(SelectedMode::Profile);
        app.reset();
        assert_eq!(app.mode_select.selected(), SelectedMode::QuickTune);
    }

    #[test]
    fn test_beep_fires_once_per_strike_on_lock() {
        let mut app = beeping_app(5.0);
        let target = app.current_target_freq().unwrap();

        // Out of tune: no beep
        app.update_pitch(sharp(target, 30.0), 0.9);
        assert!(!app.take_beep());

        // Pulls into the zone: exactly one beep
        app.update_pitch(sharp(target, 1.0), 0.9);
        assert!(app.take_beep());
        app.update_pitch(target, 0.9);
        assert!(!app.take_beep(), "still locked: no retrigger");

        // Drifting out and back WITHOUT silence must not re-beep — the
        // microphone may be hearing the beep itself
        app.update_pitch(sharp(target, 30.0), 0.9);
        app.update_pitch(target, 0.9);
        assert!(!app.take_beep());

        // Silence re-arms; the next in-tune strike beeps again
        app.clear_pitch();
        app.update_pitch(target, 0.9);
        assert!(app.take_beep());
    }

    #[test]
    fn test_no_beep_when_disabled() {
        let mut app = App::with_config(&test_config(440.0)); // beep: false
        app.start_session();
        app.handle_key(KeyCode::Char(' '));

        let target = app.current_target_freq().unwrap();
        app.update_pitch(target, 0.9);
        assert!(!app.take_beep());
    }

    #[test]
    fn test_no_beep_during_muting_step() {
        let mut app = beeping_app(5.0);
        app.handle_key(KeyCode::Char('b')); // back to the mute step

        let target = app.current_target_freq().unwrap();
        app.update_pitch(target, 0.9);
        assert!(!app.take_beep());
    }

    #[test]
    fn test_configured_tolerance_drives_lock() {
        // +10 cents locks at tolerance 20 but not at 5
        let mut app = beeping_app(20.0);
        let target = app.current_target_freq().unwrap();
        app.update_pitch(sharp(target, 10.0), 0.9);
        assert!(app.take_beep());

        let mut app = beeping_app(5.0);
        let target = app.current_target_freq().unwrap();
        app.update_pitch(sharp(target, 10.0), 0.9);
        assert!(!app.take_beep());
    }

    #[test]
    fn test_profiling_target_stays_synced_with_note() {
        let mut app = App::new();
        app.mode_select.next();
        app.mode_select.next(); // Profile
        app.start_session();

        let profiling = app.profiling.as_ref().unwrap();
        let expected = app.target_for_midi(profiling.current_note().midi);
        assert!((profiling.target_freq() - expected).abs() < f32::EPSILON);

        // Skipping advances the note; the target must follow
        app.handle_key(KeyCode::Char('s'));
        let profiling = app.profiling.as_ref().unwrap();
        let expected = app.target_for_midi(profiling.current_note().midi);
        assert!((profiling.target_freq() - expected).abs() < f32::EPSILON);
    }
}
