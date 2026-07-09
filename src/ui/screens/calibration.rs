//! Calibration screen for detecting piano's pitch center.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Gauge, Paragraph, Widget},
};

use crate::ui::theme::{Shortcuts, Theme};

/// Calibration screen for initial A4 detection.
pub struct CalibrationScreen {
    /// Collected frequency samples.
    samples: Vec<f32>,
    /// Target number of samples.
    target_samples: usize,
    /// Current detected frequency (most recent).
    current_freq: Option<f32>,
    /// Whether we're actively listening.
    listening: bool,
}

impl CalibrationScreen {
    /// Create a new calibration screen.
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            target_samples: 10,
            current_freq: None,
            listening: true,
        }
    }

    /// Update with a detected frequency.
    pub fn update(&mut self, freq: f32) {
        // Only accept frequencies in reasonable A4 range (400-480 Hz)
        if (400.0..=480.0).contains(&freq) {
            self.current_freq = Some(freq);
            self.samples.push(freq);
        }
    }

    /// Clear current detection (no pitch detected).
    pub fn clear(&mut self) {
        self.current_freq = None;
    }

    /// Check if calibration is complete.
    pub fn is_complete(&self) -> bool {
        self.samples.len() >= self.target_samples
    }

    /// Get the final calibrated A4 frequency (average of samples).
    pub fn result(&self) -> Option<f32> {
        if self.samples.is_empty() {
            None
        } else {
            let sum: f32 = self.samples.iter().sum();
            Some(sum / self.samples.len() as f32)
        }
    }

    /// Get progress ratio (0.0 to 1.0).
    pub fn progress(&self) -> f64 {
        self.samples.len() as f64 / self.target_samples as f64
    }

    /// Get current frequency if detecting.
    pub fn current_freq(&self) -> Option<f32> {
        self.current_freq
    }

    /// Set listening state.
    pub fn set_listening(&mut self, listening: bool) {
        self.listening = listening;
    }

    /// Reset calibration.
    pub fn reset(&mut self) {
        self.samples.clear();
        self.current_freq = None;
        self.listening = true;
    }
}

impl Default for CalibrationScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &CalibrationScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Main container
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(" Calibration ")
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 10 || inner.width < 30 {
            // NOTE: set_string clips on x but indexes y unchecked, so guard
            // against a zero-height/zero-width inner rect (e.g. a one-row pane).
            if inner.height > 0 && inner.width > 0 {
                let msg = "Terminal too small";
                buf.set_string(inner.x, inner.y, msg, Theme::warning());
            }
            return;
        }

        // Layout
        let chunks = Layout::vertical([
            Constraint::Length(2), // Instructions
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // Current pitch display
            Constraint::Length(1), // Spacer
            Constraint::Length(3), // Progress bar
            Constraint::Min(2),    // Spacer
            Constraint::Length(2), // Help text
        ])
        .split(inner);

        // Instructions
        let instruction = Paragraph::new("Play A4 (the A above middle C) and hold the key")
            .style(Theme::title())
            .alignment(Alignment::Center);
        instruction.render(chunks[0], buf);

        // Current pitch display
        let pitch_area = chunks[2];
        if let Some(freq) = self.current_freq {
            let deviation = freq - 440.0;
            let style = Theme::style_for_cents(deviation * 4.0); // Approximate cents

            let freq_text = format!("{:.1} Hz", freq);
            let deviation_text = format!("({:+.1} Hz from 440)", deviation);

            let freq_x =
                (pitch_area.x + pitch_area.width / 2).saturating_sub(freq_text.len() as u16 / 2);
            buf.set_string(freq_x, pitch_area.y, &freq_text, style);

            let dev_x = (pitch_area.x + pitch_area.width / 2)
                .saturating_sub(deviation_text.len() as u16 / 2);
            buf.set_string(dev_x, pitch_area.y + 1, &deviation_text, Theme::muted());
        } else {
            let listening_text = if self.listening {
                "Listening..."
            } else {
                "No pitch detected"
            };
            let x = (pitch_area.x + pitch_area.width / 2)
                .saturating_sub(listening_text.len() as u16 / 2);
            buf.set_string(x, pitch_area.y, listening_text, Theme::muted());
        }

        // Progress bar
        let progress_area = chunks[4];
        let percent = (self.progress() * 100.0) as u16;
        let label = format!("Samples: {}/{}", self.samples.len(), self.target_samples);

        // Progress label
        let label_x =
            (progress_area.x + progress_area.width / 2).saturating_sub(label.len() as u16 / 2);
        buf.set_string(label_x, progress_area.y, &label, Theme::muted());

        // Progress bar
        if progress_area.height >= 2 {
            let bar_area = Rect {
                x: progress_area.x + 2,
                y: progress_area.y + 1,
                width: progress_area.width.saturating_sub(4),
                height: 1,
            };
            let gauge = Gauge::default()
                .ratio(self.progress())
                .gauge_style(Theme::accent())
                .label(format!("{}%", percent));
            gauge.render(bar_area, buf);
        }

        // Help text
        let help_text = format!(
            "{} Skip calibration (use 440 Hz)  {} Quit",
            Shortcuts::SKIP,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(chunks[6], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(screen: &CalibrationScreen, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        screen.render(area, &mut buf);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // -- centering arithmetic must not underflow (issue #31 item 4) --

    #[test]
    fn test_render_smoke_across_sizes_does_not_panic() {
        let mut screen = CalibrationScreen::new();
        screen.update(440.0);

        for (w, h) in [
            (0, 0),
            (1, 1),
            (2, 0),  // wide-but-zero-height: fallback y is out of buffer
            (2, 1),  // wide-but-one-row: inner.y == 1 is out of a 1-row buffer
            (30, 1), // realistic dragged-thin pane
            (80, 1), // one-row terminal / tmux split
            (10, 5),
            (29, 10), // just under the width gate
            (30, 9),  // just under the height gate
            (30, 10), // exactly at both gates
            (80, 24),
            (200, 60),
        ] {
            let _ = render_to_string(&screen, w, h);
        }
    }

    #[test]
    fn test_current_pitch_display_survives_at_the_gate_boundary() {
        // Outer 32x12 -> inner 30x10, exactly at the "too small" gate.
        let mut screen = CalibrationScreen::new();
        screen.update(442.3);
        let rendered = render_to_string(&screen, 32, 12);
        assert!(rendered.contains("Hz"), "expected the frequency readout");
    }
}
