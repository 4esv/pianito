//! Main application state machine.

use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

use crate::audio::{PartialAnalyzer, PitchDetector, SilenceWatchdog};
use crate::config::EffectiveConfig;
use crate::tuning::flow::TuningFlow;
use crate::tuning::order::TuningOrder;
use crate::tuning::profile::PianoProfile;
use crate::tuning::session::{Session, TuningMode};
use crate::tuning::stretch::{StretchCurve, StretchMode};
use crate::tuning::temperament::Temperament;

use super::components::HelpOverlay;
use super::screens::{
    mode_select::SelectedMode, CalibrationScreen, CompleteScreen, ModeSelectScreen,
    ProfilingScreen, TuningScreen,
};
use super::status::{Severity, StatusId, StatusQueue};
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
    /// Tuning-session state machine (target note, progress, lock latch);
    /// `Some` only while a session is active (Tuning, and retained through
    /// Complete until `reset`).
    flow: Option<TuningFlow>,
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
    /// Temperament calculator. Drives calibration/profiling target math
    /// before a session (and its own `TuningFlow`) exists; a copy is
    /// handed to the flow when the session starts.
    temperament: Temperament,
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
    /// Configured stretch mode (from CLI/config file); resolved to `stretch`
    /// via [`StretchMode::resolve`] whenever the mode or the loaded profile
    /// changes.
    stretch_mode: StretchMode,
    /// Stretch curve applied to targets, resolved from `stretch_mode` (None
    /// = pure equal temperament). Same role as `temperament`: pre-session
    /// copy, handed to the flow when the session starts.
    stretch: Option<StretchCurve>,
    /// Multi-message status line (issue #34): audio warnings, save
    /// failures, resume notes, and the silence-watchdog hint each keep
    /// their own slot instead of one overwriting another.
    status: StatusQueue,
    /// Tracks time since the last mic read that carried real signal, to
    /// surface a "check your microphone" hint on a dead (but error-free)
    /// input stream — e.g. a macOS TCC permission denial, which delivers
    /// silent zero samples rather than a `cpal` error (issue #34). Anchored
    /// to `Instant::now()` at construction, so the idle clock starts
    /// counting from app startup.
    watchdog: SilenceWatchdog,
    /// '?' help overlay. Checked before any screen-specific key handling so
    /// its input never leaks through to the screen underneath.
    help: HelpOverlay,
    /// Sample rate of the active audio input. Defaults to a placeholder
    /// until `main` calls `set_sample_rate` once the real device is open;
    /// only used to size the partial analyzer's register window (issue #22).
    sample_rate: u32,
    /// FFT partial analyzer used to capture each profiled note's overtone
    /// structure at confirm time (issue #22). Rebuilt by `set_sample_rate`
    /// so its bin-width math matches the real device rate.
    partial_analyzer: PartialAnalyzer,
}

impl App {
    /// Placeholder sample rate before `set_sample_rate` is called with the
    /// real device rate. Never reaches a real analysis: `main` sets the
    /// actual rate before the capture loop starts.
    const DEFAULT_SAMPLE_RATE: u32 = 44_100;

    /// Create a new application.
    pub fn new() -> Self {
        Self {
            state: AppState::ModeSelect,
            flow: None,
            should_quit: false,
            mode_select: ModeSelectScreen::new(),
            calibration: CalibrationScreen::new(),
            profiling: None,
            profile: None,
            tuning: None,
            complete: None,
            temperament: Temperament::new(),
            configured_a4: 440.0,
            tolerance: 5.0,
            default_mode: SelectedMode::default(),
            beep_enabled: false,
            beep_pending: false,
            stretch_mode: StretchMode::default(),
            stretch: Some(StretchCurve::railsback_default()),
            status: StatusQueue::new(),
            watchdog: SilenceWatchdog::new(SilenceWatchdog::DEFAULT_TIMEOUT, Instant::now()),
            help: HelpOverlay::new(),
            sample_rate: Self::DEFAULT_SAMPLE_RATE,
            partial_analyzer: PartialAnalyzer::new(Self::DEFAULT_SAMPLE_RATE),
        }
    }

    /// Create a new application honoring the effective CLI/config settings:
    /// A4 reference, in-tune tolerance, lock beep, stretch mode, and the
    /// default mode preselected in the menu.
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
        app.set_stretch_mode(config.stretch);
        app
    }

    /// Create app with an existing session (for resume). Tolerance and beep
    /// come from the config; the A4 reference comes from the session (the
    /// piano is mid-tune at that reference, so the config value must not
    /// silently retarget the remaining notes). `configured_a4` keeps the
    /// CLI/config value: it only feeds sessions started later from the menu.
    pub fn with_session(session: Session, config: &EffectiveConfig) -> Self {
        let mut app = Self::with_config(config);
        app.temperament = Temperament::with_a4(session.a4_reference);

        // Profile sessions were ordered by measured deviation; resuming with
        // the traditional order would drop the user on an unrelated note and
        // re-queue notes already tuned.
        let mut tuning_order = TuningOrder::new();
        if session.mode == TuningMode::Profile {
            match Self::profile_for_session(&session) {
                Some(profile) => {
                    tuning_order = TuningOrder::from_profile(&profile);
                    app.profile = Some(profile);
                }
                None => {
                    // WARNING: without the saved profile the deviation order
                    // cannot be rebuilt; say so instead of switching silently.
                    app.status.upsert(
                        StatusId::ResumeWarning,
                        "Saved profile not found - resuming in traditional order",
                        Severity::Info,
                        None,
                        Instant::now(),
                    );
                }
            }
            app.recompute_stretch();
        }

        app.flow = Some(TuningFlow::resume(
            session,
            tuning_order,
            app.temperament,
            app.stretch.clone(),
        ));
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

    /// Set the stretch mode and re-resolve the active curve from it (and
    /// the currently loaded profile, if any - see [`StretchMode::resolve`]).
    pub fn set_stretch_mode(&mut self, mode: StretchMode) {
        self.stretch_mode = mode;
        self.recompute_stretch();
    }

    /// Re-resolve `stretch` from `stretch_mode` and `profile`. Called
    /// whenever either changes: directly from `set_stretch_mode`, and after
    /// `profile` goes from `None` to `Some` (`finish_profiling`, and
    /// `with_session` resuming a profile-linked session) - moments
    /// `with_config`'s initial resolve couldn't have accounted for.
    fn recompute_stretch(&mut self) {
        self.stretch = self.stretch_mode.resolve(self.profile.as_ref());
    }

    /// Error from the most recent save (session or profile), if any. Also
    /// rendered in the status line; available for printing after terminal
    /// restore.
    pub fn save_error(&self) -> Option<&str> {
        self.status.text_for(StatusId::SaveError)
    }

    /// Record an audio-stream error (e.g. microphone unplugged) for display
    /// in the status line. The main loop forwards the pitch worker's
    /// `take_audio_warning` here instead of writing to stderr while the
    /// terminal is in raw mode.
    pub fn set_audio_warning(&mut self, msg: String) {
        self.status.upsert(
            StatusId::AudioWarning,
            msg,
            Severity::Warning,
            None,
            Instant::now(),
        );
    }

    /// Record the pitch worker's latest mic-read signal state (issue #34/#28):
    /// feeds the silence watchdog with whether the most recent read carried
    /// real signal, independent of whether a pitch was actually detected in
    /// it (a quiet moment between strikes is not a dead mic). A signal-bearing
    /// read re-arms the idle timer; a run of exact-zero reads (a dead but
    /// error-free stream) lets it elapse.
    pub fn observe_audio_signal(&mut self, has_signal: bool, now: Instant) {
        self.watchdog.observe(has_signal, now);
    }

    /// Per-render-tick upkeep for time-driven state: raises or clears the
    /// silence-watchdog hint, and expires/rotates the status queue. Runs at
    /// render cadence so it advances even on ticks with no pitch event
    /// (`now` is explicit so both are deterministic under test).
    pub fn tick(&mut self, now: Instant) {
        if self.watchdog.is_idle(now) {
            self.status.upsert(
                StatusId::SilenceWatchdog,
                "No audio input detected - check microphone permissions \
                 (System Settings > Privacy > Microphone)",
                Severity::Warning,
                None,
                now,
            );
        } else {
            self.status.clear(StatusId::SilenceWatchdog);
        }
        self.status.tick(now);
    }

    /// Set the sample rate of the active audio input (called once by `main`
    /// after the mic opens). Rebuilds the partial analyzer so its internal
    /// bin-width math matches the real device rate (issue #22).
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.partial_analyzer = PartialAnalyzer::new(sample_rate);
    }

    /// Confidence floor below which a pitch reading is treated as noise
    /// during profiling. Mirrors the floor `update_pitch`'s Profiling arm
    /// gates the frequency/cents reading on, so partial capture describes
    /// the same reading rather than a rejected one.
    const PROFILING_CONFIDENCE_FLOOR: f32 = 0.6;

    /// Capture the partial spectrum backing the current profiling reading
    /// (issue #22): analyzes `samples` (the raw audio window that produced
    /// `freq`) so confirming the note can persist `Vec<Partial>` alongside
    /// it, for the future inharmonicity fit (issue #23).
    ///
    /// No-op outside `Profiling`, or below [`Self::PROFILING_CONFIDENCE_FLOOR`].
    pub fn capture_profiling_partials(&mut self, freq: f32, confidence: f32, samples: &[f32]) {
        if confidence <= Self::PROFILING_CONFIDENCE_FLOOR || self.state != AppState::Profiling {
            return;
        }
        let Some(profiling) = &mut self.profiling else {
            return;
        };

        // Bass notes get the whole window; treble is trimmed to its shorter
        // register window (the same tiering `PitchDetector::detect_for_target`
        // uses) so a long buffer's decayed tail or the next strike doesn't
        // dilute the FFT.
        let window = PitchDetector::window_for_target(profiling.target_freq());
        let want = (self.sample_rate as f64 * window.as_secs_f64()).round() as usize;
        let trimmed = if want > 0 && samples.len() > want {
            &samples[samples.len() - want..]
        } else {
            samples
        };

        let partials = self.partial_analyzer.analyze(trimmed, freq);
        profiling.set_current_partials(partials);
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
        self.flow.as_ref().map(TuningFlow::session)
    }

    /// Get mutable session.
    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.flow.as_mut().map(TuningFlow::session_mut)
    }

    /// Get target frequency for current note.
    pub fn current_target_freq(&self) -> Option<f32> {
        self.tuning.as_ref().map(|t| t.target_freq())
    }

    /// Handle key press event. The help overlay gets first look at every
    /// key, on every screen: `?` opens it, and while open it swallows
    /// whatever key closes it, so that key never also acts on the screen
    /// underneath (e.g. `s` closing the overlay must not also skip a note).
    pub fn handle_key(&mut self, key: KeyCode) {
        if self.help.handle_key(key) {
            return;
        }

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
                self.start_tuning(TuningMode::Quick, TuningOrder::new());
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
        let mode: TuningMode = self.mode_select.selected().into();

        match mode {
            TuningMode::Quick => {
                self.state = AppState::Calibration;
                self.calibration.reset();
            }
            TuningMode::Concert => {
                self.temperament = Temperament::with_a4(self.configured_a4);
                self.start_tuning(TuningMode::Concert, TuningOrder::new());
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
        profiling.set_a4_reference(self.configured_a4);
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
                self.status.upsert(
                    StatusId::SaveError,
                    format!("Failed to save profile: {}", e),
                    Severity::Warning,
                    None,
                    Instant::now(),
                );
            }

            // Tuning order based on profile deviations.
            let tuning_order = TuningOrder::from_profile(&profile);
            self.profile = Some(profile);
            self.recompute_stretch();

            self.start_tuning(TuningMode::Profile, tuning_order);
        }
    }

    /// Start a tuning session in `mode` following `tuning_order`. The mode
    /// is passed in explicitly rather than re-derived from the mode-select
    /// widget: this state can be reached after profiling, where the widget
    /// no longer reflects the mode actually in progress.
    fn start_tuning(&mut self, mode: TuningMode, tuning_order: TuningOrder) {
        let mut flow = TuningFlow::new(mode, tuning_order, self.temperament, self.stretch.clone());
        // Profile mode: link the session to the profile that ordered it, so
        // resuming can't reattach a different piano's profile (see
        // `find_matching_profile`). `self.profile` is set by
        // `finish_profiling` before this runs; Quick/Concert leave it None.
        if let Some(profile) = &self.profile {
            flow.session_mut().profile_id = Some(profile.id.clone());
        }
        self.flow = Some(flow);
        self.state = AppState::Tuning;
        self.setup_current_note();
    }

    /// Set up the tuning screen for the current note, or transition to the
    /// complete screen once the flow has run out of notes.
    fn setup_current_note(&mut self) {
        let is_finished = match &self.flow {
            Some(flow) => flow.is_finished(),
            None => return,
        };

        if is_finished {
            self.finish_session();
            return;
        }

        let current = self.flow.as_ref().and_then(TuningFlow::current_note);
        if let Some(current) = current {
            let mut tuning = TuningScreen::new(
                current.display_name,
                current.note_index,
                current.total_notes,
                current.target_freq,
                current.string_count,
                current.midi,
            );
            tuning.set_completed_notes(current.completed_chromatic_indices);
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
                        self.start_tuning(TuningMode::Quick, TuningOrder::new());
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
                        profiling.update(freq, cents, confidence);
                    } else {
                        profiling.clear();
                    }
                }
            }
            AppState::Tuning => {
                if let Some(tuning) = &mut self.tuning {
                    if confidence > 0.6 {
                        let target = tuning.target_freq();
                        let cents = match &self.flow {
                            Some(flow) => flow.cents_from_target(freq, target),
                            None => 0.0,
                        };
                        tuning.update(freq, cents);

                        // Lock beep: fire once when the strike first enters
                        // the in-tune zone. The latch is NOT re-armed by an
                        // out-of-tolerance reading — the microphone may pick
                        // up the beep itself and the retrigger would loop.
                        let is_muting =
                            tuning.tuning_step().map(|s| s.is_muting()).unwrap_or(false);
                        let locked = match &mut self.flow {
                            Some(flow) => flow.observe_lock(cents, self.tolerance, is_muting),
                            None => false,
                        };
                        if locked && self.beep_enabled {
                            self.beep_pending = true;
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
        if let Some(flow) = &mut self.flow {
            flow.rearm_beep_latch();
        }
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
        let Some(tuning) = &mut self.tuning else {
            return;
        };

        // For multi-string notes (bichord/trichord), advance through steps
        if tuning.is_multi_string() && tuning.next_step() {
            // New step, new strike: re-arm the lock beep
            if let Some(flow) = &mut self.flow {
                flow.rearm_beep_latch();
            }
            return;
        }

        let cents = tuning.cents();
        let finished = self
            .flow
            .as_mut()
            .map(|flow| flow.confirm_current_note(cents));
        self.after_note_advance(finished);
    }

    /// Go back to previous step or previous note.
    fn go_back(&mut self) {
        // Try to go to previous step first
        if let Some(tuning) = &mut self.tuning {
            if tuning.prev_step() {
                if let Some(flow) = &mut self.flow {
                    flow.rearm_beep_latch();
                }
                return;
            }
        }

        // Otherwise go to previous note
        let went_back = self
            .flow
            .as_mut()
            .map(|flow| flow.go_back_note())
            .unwrap_or(false);
        if went_back {
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
        let finished = self.flow.as_mut().map(|flow| flow.skip_current_note());
        self.after_note_advance(finished);
    }

    /// Persist progress after a note advances (confirm or skip) and route
    /// to the next screen: the tuning screen for the next note, or the
    /// complete screen once `flow` reports the session finished. No-op if
    /// there was no active flow to advance.
    fn after_note_advance(&mut self, finished: Option<bool>) {
        let Some(finished) = finished else {
            return;
        };
        // Persist progress, including the final advance to 88, so a
        // finished session is stored complete and --resume doesn't reopen
        // it.
        self.autosave_session();
        if finished {
            self.finish_session();
        } else {
            self.setup_current_note();
        }
    }

    /// Finish the tuning session. The session is kept (not taken) so the
    /// completed state remains inspectable until reset.
    fn finish_session(&mut self) {
        let completed_notes = self
            .flow
            .as_ref()
            .map(|flow| flow.session().completed_notes.clone())
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
        let result = match &self.flow {
            Some(flow) => flow.session().save(),
            None => return,
        };
        match result {
            Ok(()) => self.status.clear(StatusId::SaveError),
            Err(e) => self.status.upsert(
                StatusId::SaveError,
                format!("Session save failed: {}", e),
                Severity::Warning,
                None,
                Instant::now(),
            ),
        }
    }

    /// Reset to start a new session.
    fn reset(&mut self) {
        self.state = AppState::ModeSelect;
        self.flow = None;
        self.profiling = None;
        self.profile = None;
        self.tuning = None;
        self.complete = None;
        self.mode_select = ModeSelectScreen::new();
        self.mode_select.select(self.default_mode);
        self.calibration = CalibrationScreen::new();
        self.status.clear(StatusId::SaveError);
        self.status.clear(StatusId::ResumeWarning);
        self.beep_pending = false;
        // NOTE: tolerance and beep_enabled are config-scoped and survive
        // reset, like configured_a4.
        // NOTE: AudioWarning and SilenceWatchdog are deliberately kept
        // across reset — a dead mic stays dead in the next session and cpal
        // fatal stream errors fire only once, so clearing here would hide
        // the only sign of it.
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

        // Status line: audio errors / save failures / resume notes / the
        // silence-watchdog hint each keep their own slot in `status` and
        // rotate through, drawn over the bottom border of every screen so
        // it can't be missed (these can occur in any state).
        if let Some((msg, severity)) = self.status.current() {
            if area.height > 2 && area.width > 4 {
                let status_area = Rect::new(
                    area.x + 2,
                    area.y + area.height - 1,
                    area.width.saturating_sub(4),
                    1,
                );
                let style = match severity {
                    Severity::Warning => Theme::warning(),
                    Severity::Info => Theme::muted(),
                };
                let status_line = Paragraph::new(msg).style(style);
                frame.render_widget(status_line, status_area);
            }
        }

        // Help overlay renders last so it sits on top of the screen and the
        // status line, on any state.
        if self.help.is_open() {
            frame.render_widget(&self.help, area);
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
    use std::time::Duration;

    use super::*;
    use crate::audio::TestAudioSource;

    fn test_config(a4: f32) -> EffectiveConfig {
        EffectiveConfig {
            a4,
            tolerance: 5.0,
            beep: false,
            quick_mode: false,
            resume: false,
            stretch: StretchMode::Railsback,
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
        let before = app.flow.as_ref().unwrap().session().completed_notes.len();
        while app.flow.as_ref().unwrap().session().completed_notes.len() == before {
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
        let session = app.flow.as_ref().unwrap().session();
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

        let flow = app.flow.as_ref().unwrap();
        assert!(
            flow.session().completed_notes.is_empty(),
            "skipped notes must not count as tuned at 0.0 cents"
        );
        assert_eq!(flow.session().current_note_index, 1);
        assert_eq!(flow.current_note_index(), 1);
    }

    #[test]
    fn test_go_back_removes_stale_completion() {
        let mut app = concert_app();
        confirm_note_fully(&mut app);
        assert_eq!(
            app.flow.as_ref().unwrap().session().completed_notes.len(),
            1
        );

        app.go_back();
        assert_eq!(app.flow.as_ref().unwrap().current_note_index(), 0);
        assert!(
            app.flow
                .as_ref()
                .unwrap()
                .session()
                .completed_notes
                .is_empty(),
            "record removed so re-confirming can't duplicate it"
        );

        confirm_note_fully(&mut app);
        assert_eq!(
            app.flow.as_ref().unwrap().session().completed_notes.len(),
            1,
            "re-confirm must not create a duplicate entry"
        );
    }

    #[test]
    fn test_finished_session_is_marked_complete() {
        let mut app = concert_app();

        // Skip through to the last note (index 87) via the real state
        // machine rather than poking internals directly.
        for _ in 0..87 {
            app.skip_note();
        }
        confirm_note_fully(&mut app);

        assert_eq!(app.state(), AppState::Complete);
        let session = app
            .flow
            .as_ref()
            .expect("session retained on finish")
            .session();
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
        app.set_stretch_mode(StretchMode::Off);
        assert_eq!(app.target_for_midi(21), app.temperament.frequency(21));
        assert_eq!(app.target_for_midi(108), app.temperament.frequency(108));
    }

    #[test]
    fn test_with_config_off_yields_no_stretch() {
        let config = EffectiveConfig {
            stretch: StretchMode::Off,
            ..test_config(440.0)
        };
        let app = App::with_config(&config);
        assert!(app.stretch.is_none());
    }

    #[test]
    fn test_with_config_railsback_yields_railsback_curve() {
        let config = EffectiveConfig {
            stretch: StretchMode::Railsback,
            ..test_config(440.0)
        };
        let app = App::with_config(&config);
        assert_eq!(
            app.stretch.as_ref().unwrap().offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_with_config_profile_without_loaded_profile_falls_back_to_railsback() {
        let config = EffectiveConfig {
            stretch: StretchMode::Profile,
            ..test_config(440.0)
        };
        let app = App::with_config(&config);
        assert_eq!(
            app.stretch.as_ref().unwrap().offset_cents(21),
            StretchCurve::railsback_default().offset_cents(21)
        );
    }

    #[test]
    fn test_profile_mode_uses_measured_curve_once_profile_is_loaded() {
        let config = EffectiveConfig {
            stretch: StretchMode::Profile,
            ..test_config(440.0)
        };
        let mut app = App::with_config(&config);

        let mut profile = PianoProfile::new();
        profile.record_note(21, 27.0, -12.5); // A0, measured 12.5 cents flat
        app.profile = Some(profile);
        app.recompute_stretch();

        assert_eq!(app.stretch.as_ref().unwrap().offset_cents(21), -12.5);
        // A key the profile never measured stays at 0, not the Railsback value.
        assert_eq!(app.stretch.as_ref().unwrap().offset_cents(108), 0.0);
    }

    #[test]
    fn test_resume_uses_session_a4_and_index() {
        let mut session = Session::new(TuningMode::Concert, 441.0);
        session.current_note_index = 5;

        let app = App::with_session(session, &test_config(440.0));
        assert_eq!(app.state(), AppState::Tuning);
        assert_eq!(app.flow.as_ref().unwrap().current_note_index(), 5);
        assert!((app.temperament.a4() - 441.0).abs() < f32::EPSILON);
        // configured_a4 stays config-scoped: a new session started after
        // this resume must not inherit the resumed session's A4
        assert!((app.configured_a4 - 440.0).abs() < f32::EPSILON);
        assert!(app.status.text_for(StatusId::ResumeWarning).is_none());
    }

    #[test]
    fn test_start_tuning_links_profile_id_when_profile_present() {
        let mut app = App::with_config(&test_config(440.0));
        app.mode_select.select(SelectedMode::Profile);
        let profile = PianoProfile::new();
        let profile_id = profile.id.clone();
        app.profile = Some(profile);

        app.start_tuning(TuningMode::Profile, TuningOrder::new());

        let session = app.flow.as_ref().expect("session created").session();
        assert_eq!(session.profile_id.as_deref(), Some(profile_id.as_str()));
    }

    #[test]
    fn test_start_tuning_leaves_profile_id_none_without_profile() {
        let app = concert_app(); // Concert mode: no profile involved
        let session = app.flow.as_ref().expect("session created").session();
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
    fn test_question_mark_opens_help_overlay_from_tuning() {
        let mut app = concert_app();
        assert!(!app.help.is_open());

        app.handle_key(KeyCode::Char('?'));
        assert!(app.help.is_open());
    }

    #[test]
    fn test_key_that_closes_overlay_does_not_leak_to_tuning_screen() {
        // Regression for #29: while the overlay is open, the keystroke that
        // dismisses it must not also act on the screen underneath.
        let mut app = concert_app();
        let index_before = app.flow.as_ref().unwrap().current_note_index();

        app.handle_key(KeyCode::Char('?'));
        assert!(app.help.is_open());

        app.handle_key(KeyCode::Char('s')); // would skip the note if it leaked
        assert!(!app.help.is_open(), "key must close the overlay");
        assert_eq!(
            app.flow.as_ref().unwrap().current_note_index(),
            index_before,
            "the closing keystroke must not also skip the note"
        );

        // Overlay is closed now, so 's' works normally again.
        app.handle_key(KeyCode::Char('s'));
        assert_eq!(
            app.flow.as_ref().unwrap().current_note_index(),
            index_before + 1
        );
    }

    #[test]
    fn test_esc_closes_overlay_without_quitting() {
        let mut app = concert_app();
        app.handle_key(KeyCode::Char('?'));

        app.handle_key(KeyCode::Esc);
        assert!(!app.help.is_open());
        assert!(!app.should_quit(), "Esc must close the overlay, not quit");
    }

    #[test]
    fn test_question_mark_toggles_overlay_from_mode_select() {
        let mut app = App::new();
        app.handle_key(KeyCode::Char('?'));
        assert!(app.help.is_open());

        app.handle_key(KeyCode::Char('?'));
        assert!(!app.help.is_open());
        // The mode-select screen never saw either '?' as a key of its own.
        assert_eq!(app.mode_select.selected(), SelectedMode::QuickTune);
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

    // ---- Partial spectra capture during profiling (issue #22) -------------

    const TEST_SAMPLE_RATE: u32 = 44_100;

    /// App in Profiling state with a known sample rate.
    fn profiling_app() -> App {
        let mut app = App::with_config(&test_config(440.0));
        app.mode_select.select(SelectedMode::Profile);
        app.start_session();
        app.set_sample_rate(TEST_SAMPLE_RATE);
        assert_eq!(app.state(), AppState::Profiling);
        app
    }

    #[test]
    fn test_capture_profiling_partials_matches_analyzer_output() {
        let mut app = profiling_app();

        // A0 is deep bass: window_for_target keeps the whole (short, in this
        // test) buffer, so what's captured must equal the analyzer run
        // directly on the same samples.
        let f0 = 27.5;
        let b = 0.009;
        let src = TestAudioSource::inharmonic(
            f0,
            b,
            &[(1, 0.05), (2, 1.0), (3, 0.9), (4, 0.7), (5, 0.5)],
            0.3,
            TEST_SAMPLE_RATE,
        );

        app.capture_profiling_partials(f0, 0.9, src.samples());

        let expected = PartialAnalyzer::new(TEST_SAMPLE_RATE).analyze(src.samples(), f0);
        assert!(!expected.is_empty(), "fixture must yield partials");

        let profiling = app.profiling.as_ref().unwrap();
        assert_eq!(profiling.current_partials(), expected.as_slice());
    }

    #[test]
    fn test_capture_profiling_partials_ignored_outside_profiling() {
        let mut app = concert_app();
        app.set_sample_rate(TEST_SAMPLE_RATE);
        let src = TestAudioSource::inharmonic(440.0, 0.0, &[(1, 1.0)], 0.1, TEST_SAMPLE_RATE);

        // Must not panic; Tuning state has no profiling screen to record into.
        app.capture_profiling_partials(440.0, 0.9, src.samples());
        assert!(app.profiling.is_none());
    }

    #[test]
    fn test_capture_profiling_partials_below_confidence_floor_is_noop() {
        let mut app = profiling_app();
        let src =
            TestAudioSource::inharmonic(27.5, 0.009, &[(2, 1.0), (3, 0.8)], 0.3, TEST_SAMPLE_RATE);

        app.capture_profiling_partials(27.5, 0.5, src.samples());

        let profiling = app.profiling.as_ref().unwrap();
        assert!(profiling.current_partials().is_empty());
    }

    #[test]
    fn test_capture_profiling_partials_trims_to_register_window() {
        let mut app = profiling_app();
        // Force a treble target (window_for_target trims to 60ms above C6)
        // regardless of which note is actually "current".
        app.profiling.as_mut().unwrap().set_target_freq(2000.0);

        let f0 = 2000.0;
        let src =
            TestAudioSource::inharmonic(f0, 0.0, &[(1, 1.0), (2, 0.5)], 0.3, TEST_SAMPLE_RATE);
        let samples = src.samples();

        app.capture_profiling_partials(f0, 0.9, samples);

        let want = (TEST_SAMPLE_RATE as f64 * 0.06).round() as usize;
        assert_ne!(
            want,
            samples.len(),
            "fixture must be long enough to actually exercise trimming"
        );
        let trimmed = &samples[samples.len() - want..];
        let expected = PartialAnalyzer::new(TEST_SAMPLE_RATE).analyze(trimmed, f0);

        let profiling = app.profiling.as_ref().unwrap();
        assert_eq!(profiling.current_partials(), expected.as_slice());
    }

    // ---- Silence watchdog + multi-message status line (issue #34) --------

    /// A fresh app plus an `Instant` guaranteed to be at or after the
    /// watchdog's internal `Instant::now()` anchor from construction (taken
    /// *after* `App::new()` returns) — so timeouts measured from it never
    /// undercount the elapsed time relative to that anchor.
    fn app_with_now() -> (App, Instant) {
        let app = App::new();
        (app, Instant::now())
    }

    #[test]
    fn test_watchdog_hint_appears_after_timeout_with_no_signal() {
        let (mut app, t0) = app_with_now();

        // Ticks alone (no pitch events at all) must be enough to raise the
        // hint — the render loop advances independent of pitch arrival.
        app.tick(t0);
        assert!(app.status.current().is_none(), "must not warn immediately");

        app.tick(t0 + SilenceWatchdog::DEFAULT_TIMEOUT);
        let (msg, severity) = app.status.current().expect("hint must be shown once idle");
        assert!(msg.contains("microphone permissions"));
        assert_eq!(severity, Severity::Warning);
    }

    #[test]
    fn test_watchdog_hint_clears_once_signal_returns() {
        let (mut app, t0) = app_with_now();

        app.tick(t0 + SilenceWatchdog::DEFAULT_TIMEOUT);
        assert!(app.status.current().is_some());

        let t1 = t0 + SilenceWatchdog::DEFAULT_TIMEOUT + Duration::from_secs(1);
        app.observe_audio_signal(true, t1);
        app.tick(t1);

        assert!(app.status.current().is_none(), "signal must clear the hint");
    }

    #[test]
    fn test_silent_reads_do_not_reset_the_watchdog() {
        // A read with no signal (dead mic) must not itself count as
        // "activity" and postpone the hint.
        let (mut app, t0) = app_with_now();

        app.observe_audio_signal(false, t0 + Duration::from_secs(5));
        app.tick(t0 + SilenceWatchdog::DEFAULT_TIMEOUT);

        assert!(app.status.current().is_some());
    }

    #[test]
    fn test_multiple_status_messages_rotate_without_masking_each_other() {
        // Regression for the old single-Option priority chain: a persistent
        // audio warning used to permanently mask a save error.
        let t0 = Instant::now();
        let mut app = App::new();
        app.set_audio_warning("mic unplugged".to_string());
        app.status.upsert(
            StatusId::SaveError,
            "save failed",
            Severity::Warning,
            None,
            t0,
        );

        assert_eq!(app.save_error(), Some("save failed"));
        assert!(
            app.status.text_for(StatusId::AudioWarning).is_some(),
            "the audio warning must still be enqueued, not overwritten"
        );

        // Both are reachable via rotation, not just the first-set one.
        let mut seen = std::collections::HashSet::new();
        let mut now = t0;
        for _ in 0..4 {
            app.tick(now);
            if let Some((msg, _)) = app.status.current() {
                seen.insert(msg.to_string());
            }
            now += StatusQueue::ROTATE_INTERVAL;
        }
        assert!(seen.contains("mic unplugged"));
        assert!(seen.contains("save failed"));
    }
}
