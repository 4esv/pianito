//! Main application state machine.

use crate::tuning::session::Session;

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
    state: AppState,
    session: Option<Session>,
    should_quit: bool,
}

impl App {
    /// Create a new application.
    pub fn new() -> Self {
        Self {
            state: AppState::ModeSelect,
            session: None,
            should_quit: false,
        }
    }

    /// Get current state.
    pub fn state(&self) -> AppState {
        self.state
    }

    /// Set the state.
    pub fn set_state(&mut self, state: AppState) {
        self.state = state;
    }

    /// Get the current session.
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Set the session.
    pub fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    /// Check if the app should quit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Request quit.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
