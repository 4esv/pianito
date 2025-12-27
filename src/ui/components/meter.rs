//! Cents deviation meter component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Cents deviation meter for visualizing pitch accuracy.
pub struct Meter {
    /// Current cents deviation from target.
    cents: f32,
    /// Tolerance threshold in cents.
    tolerance: f32,
}

impl Meter {
    /// Create a new meter.
    pub fn new(cents: f32) -> Self {
        Self {
            cents,
            tolerance: 5.0,
        }
    }

    /// Set the tolerance threshold.
    pub fn tolerance(mut self, tolerance: f32) -> Self {
        self.tolerance = tolerance;
        self
    }
}

impl Widget for Meter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement meter rendering
        let _ = (area, buf, self.cents, self.tolerance);
    }
}
