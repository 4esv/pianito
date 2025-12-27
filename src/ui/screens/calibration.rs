//! Calibration screen for detecting piano's pitch center.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::Widget,
};

/// Calibration screen for initial A4 detection.
pub struct CalibrationScreen {
    detected_freq: Option<f32>,
    samples_collected: usize,
    target_samples: usize,
}

impl CalibrationScreen {
    /// Create a new calibration screen.
    pub fn new() -> Self {
        Self {
            detected_freq: None,
            samples_collected: 0,
            target_samples: 10,
        }
    }

    /// Update with a detected frequency.
    pub fn update(&mut self, freq: f32) {
        // TODO: Implement averaging logic
        self.detected_freq = Some(freq);
        self.samples_collected += 1;
    }

    /// Check if calibration is complete.
    pub fn is_complete(&self) -> bool {
        self.samples_collected >= self.target_samples
    }

    /// Get the final detected frequency.
    pub fn result(&self) -> Option<f32> {
        self.detected_freq
    }
}

impl Default for CalibrationScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &CalibrationScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // TODO: Implement calibration screen rendering
        let _ = (area, buf, self.detected_freq, self.samples_collected);
    }
}
