//! Progress indicator component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Progress indicator showing current note position.
pub struct Progress {
    current: usize,
    total: usize,
    note_name: String,
}

impl Progress {
    /// Create a new progress indicator.
    pub fn new(current: usize, total: usize, note_name: impl Into<String>) -> Self {
        Self {
            current,
            total,
            note_name: note_name.into(),
        }
    }
}

impl Widget for Progress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement progress rendering
        let _ = (area, buf, self.current, self.total, &self.note_name);
    }
}
