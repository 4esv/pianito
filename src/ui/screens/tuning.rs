//! Main tuning screen.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Main tuning screen state.
pub struct TuningScreen {
    note_name: String,
    note_index: usize,
    total_notes: usize,
    target_freq: f32,
    detected_freq: Option<f32>,
    cents_deviation: f32,
}

impl TuningScreen {
    /// Create a new tuning screen.
    pub fn new(note_name: impl Into<String>, note_index: usize, target_freq: f32) -> Self {
        Self {
            note_name: note_name.into(),
            note_index,
            total_notes: 88,
            target_freq,
            detected_freq: None,
            cents_deviation: 0.0,
        }
    }

    /// Update with detected pitch.
    pub fn update(&mut self, freq: f32, cents: f32) {
        self.detected_freq = Some(freq);
        self.cents_deviation = cents;
    }

    /// Clear detected pitch (silence/no detection).
    pub fn clear(&mut self) {
        self.detected_freq = None;
        self.cents_deviation = 0.0;
    }
}

impl Widget for &TuningScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement tuning screen rendering
        let _ = (
            area,
            buf,
            &self.note_name,
            self.note_index,
            self.total_notes,
            self.target_freq,
            self.detected_freq,
            self.cents_deviation,
        );
    }
}
