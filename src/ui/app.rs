//! Main application state machine.

use crossterm::event::KeyCode;
use ratatui::Frame;

use crate::tuning::order::TuningOrder;
use crate::tuning::session::{Session, TuningMode};
use crate::tuning::temperament::Temperament;

use super::screens::{
    mode_select::SelectedMode, CalibrationScreen, CompleteScreen, ModeSelectScreen, TuningScreen,
};

/// Application screen state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Mode selection screen.
    ModeSelect,
    /// Calibration (for quick tune).
    Calibration,
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
    /// Whether reference tone is playing.
    playing_reference: bool,
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
            tuning: None,
            complete: None,
            tuning_order: TuningOrder::new(),
            temperament: Temperament::new(),
            current_note_idx: 0,
            playing_reference: false,
        }
    }

    /// Create app with an existing session (for resume).
    pub fn with_session(session: Session) -> Self {
        let mut app = Self::new();
        app.current_note_idx = session.current_note_index;
        app.temperament = Temperament::with_a4(session.a4_reference);
        app.session = Some(session);
        app.state = AppState::Tuning;
        app.setup_current_note();
        app
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

    /// Check if reference tone is playing.
    pub fn is_playing_reference(&self) -> bool {
        self.playing_reference
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
            AppState::Tuning => self.handle_tuning_key(key),
            AppState::Complete => self.handle_complete_key(key),
        }
    }

    fn handle_mode_select_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Up | KeyCode::Down | KeyCode::Tab => {
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
                // Skip calibration, use 440 Hz
                self.temperament = Temperament::new();
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
            KeyCode::Char('r') | KeyCode::Char('R') => {
                // Toggle reference tone
                self.playing_reference = !self.playing_reference;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                // Skip current note
                self.skip_note();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                // Save session before quitting
                if let Some(session) = &self.session {
                    let _ = session.save();
                }
                self.quit();
            }
            _ => {}
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
        };

        match mode {
            TuningMode::Quick => {
                self.state = AppState::Calibration;
                self.calibration.reset();
            }
            TuningMode::Concert => {
                self.temperament = Temperament::new();
                self.start_tuning();
            }
        }
    }

    /// Start tuning after calibration.
    fn start_tuning(&mut self) {
        let mode = match self.mode_select.selected() {
            SelectedMode::QuickTune => TuningMode::Quick,
            SelectedMode::ConcertPitch => TuningMode::Concert,
        };

        self.session = Some(Session::new(mode, self.temperament.a4()));
        self.current_note_idx = 0;
        self.state = AppState::Tuning;
        self.setup_current_note();
    }

    /// Set up the tuning screen for the current note.
    fn setup_current_note(&mut self) {
        if self.current_note_idx >= 88 {
            self.finish_session();
            return;
        }

        if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
            let target_freq = self.temperament.frequency(note.midi);

            self.tuning = Some(TuningScreen::new(
                note.display_name(),
                self.current_note_idx,
                88,
                target_freq,
                note.strings,
            ));
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
            AppState::Tuning => {
                if let Some(tuning) = &mut self.tuning {
                    if confidence > 0.6 {
                        let target = tuning.target_freq();
                        let cents = self.temperament.cents_from_target(freq, target);
                        tuning.update(freq, cents);
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
        match self.state {
            AppState::Calibration => {
                self.calibration.clear();
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
            // For trichords, advance through steps
            if tuning.is_trichord() && tuning.next_step() {
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

    /// Skip current note.
    fn skip_note(&mut self) {
        // Record as skipped (0 cents)
        if let Some(session) = &mut self.session {
            if let Some(note) = self.tuning_order.note_at(self.current_note_idx) {
                session.complete_note(note.display_name(), 0.0);
            }
        }

        self.advance_to_next_note();
    }

    /// Advance to the next note.
    fn advance_to_next_note(&mut self) {
        self.current_note_idx += 1;
        self.playing_reference = false;

        if self.current_note_idx >= 88 {
            self.finish_session();
        } else {
            self.setup_current_note();

            // Update session progress
            if let Some(session) = &mut self.session {
                session.current_note_index = self.current_note_idx;
                let _ = session.save();
            }
        }
    }

    /// Finish the tuning session.
    fn finish_session(&mut self) {
        if let Some(session) = self.session.take() {
            let completed_notes = session.completed_notes.clone();
            self.complete = Some(CompleteScreen::new(completed_notes));
        } else {
            self.complete = Some(CompleteScreen::new(Vec::new()));
        }
        self.state = AppState::Complete;
    }

    /// Reset to start a new session.
    fn reset(&mut self) {
        self.state = AppState::ModeSelect;
        self.session = None;
        self.tuning = None;
        self.complete = None;
        self.current_note_idx = 0;
        self.playing_reference = false;
        self.mode_select = ModeSelectScreen::new();
        self.calibration = CalibrationScreen::new();
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
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
