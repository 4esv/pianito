//! Session complete summary screen.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

use crate::tuning::session::CompletedNote;

/// Session complete screen with summary.
pub struct CompleteScreen {
    completed_notes: Vec<CompletedNote>,
    avg_deviation: f32,
}

impl CompleteScreen {
    /// Create a new complete screen.
    pub fn new(completed_notes: Vec<CompletedNote>) -> Self {
        let avg_deviation = if completed_notes.is_empty() {
            0.0
        } else {
            let sum: f32 = completed_notes.iter().map(|n| n.final_cents.abs()).sum();
            sum / completed_notes.len() as f32
        };

        Self {
            completed_notes,
            avg_deviation,
        }
    }
}

impl Widget for &CompleteScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement complete screen rendering
        let _ = (area, buf, &self.completed_notes, self.avg_deviation);
    }
}
