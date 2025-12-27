//! Coaching instructions component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Step in the tuning process.
#[derive(Debug, Clone, Copy)]
pub enum TuningStep {
    /// Mute outer strings.
    MuteOuter,
    /// Tune center string.
    TuneCenter,
    /// Tune left string to unison.
    TuneLeft,
    /// Tune right string to unison.
    TuneRight,
}

impl TuningStep {
    /// Get step number (1-4).
    pub fn number(&self) -> u8 {
        match self {
            Self::MuteOuter => 1,
            Self::TuneCenter => 2,
            Self::TuneLeft => 3,
            Self::TuneRight => 4,
        }
    }

    /// Get instruction text.
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::MuteOuter => "Mute the outer strings with felt strip or rubber mutes",
            Self::TuneCenter => "Tune center string to target pitch",
            Self::TuneLeft => "Unmute left string, tune to unison with center",
            Self::TuneRight => "Unmute right string, tune to unison with center",
        }
    }
}

/// Instructions panel for coaching the user.
pub struct Instructions {
    step: TuningStep,
    total_steps: u8,
    direction_hint: Option<&'static str>,
}

impl Instructions {
    /// Create a new instructions panel.
    pub fn new(step: TuningStep, total_steps: u8) -> Self {
        Self {
            step,
            total_steps,
            direction_hint: None,
        }
    }

    /// Set a direction hint (e.g., "Turn CLOCKWISE").
    pub fn direction_hint(mut self, hint: &'static str) -> Self {
        self.direction_hint = Some(hint);
        self
    }
}

impl Widget for Instructions {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement instructions rendering
        let _ = (area, buf, self.step, self.total_steps, self.direction_hint);
    }
}
