//! Mode selection screen.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Selected tuning mode.
#[derive(Debug, Clone, Copy, Default)]
pub enum SelectedMode {
    #[default]
    QuickTune,
    ConcertPitch,
}

/// Mode selection screen.
pub struct ModeSelectScreen {
    selected: SelectedMode,
}

impl ModeSelectScreen {
    /// Create a new mode select screen.
    pub fn new() -> Self {
        Self {
            selected: SelectedMode::default(),
        }
    }

    /// Get the currently selected mode.
    pub fn selected(&self) -> SelectedMode {
        self.selected
    }

    /// Select the next mode.
    pub fn next(&mut self) {
        self.selected = match self.selected {
            SelectedMode::QuickTune => SelectedMode::ConcertPitch,
            SelectedMode::ConcertPitch => SelectedMode::QuickTune,
        };
    }
}

impl Default for ModeSelectScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &ModeSelectScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement mode select rendering
        let _ = (area, buf, self.selected);
    }
}
