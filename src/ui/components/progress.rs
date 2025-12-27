//! Progress indicator component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Gauge, Widget},
};

use crate::ui::theme::Theme;

/// Progress indicator showing current note position.
pub struct Progress {
    current: usize,
    total: usize,
    note_name: String,
    phase_name: String,
}

impl Progress {
    /// Create a new progress indicator.
    pub fn new(
        current: usize,
        total: usize,
        note_name: impl Into<String>,
        phase_name: impl Into<String>,
    ) -> Self {
        Self {
            current,
            total,
            note_name: note_name.into(),
            phase_name: phase_name.into(),
        }
    }

    /// Get progress as a ratio (0.0 to 1.0).
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.current as f64 / self.total as f64
        }
    }
}

impl Widget for Progress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 || area.width < 20 {
            return;
        }

        // Header line: note name and progress
        let header = format!(
            "{} | {}/{} | {}",
            self.note_name, self.current + 1, self.total, self.phase_name
        );

        let header_style = Theme::title();
        buf.set_string(area.x, area.y, &header, header_style);

        // Progress bar on second line if space
        if area.height >= 2 {
            let bar_area = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            };

            let percent = (self.ratio() * 100.0) as u16;
            let gauge = Gauge::default()
                .ratio(self.ratio())
                .gauge_style(Theme::accent())
                .label(format!("{}%", percent));

            gauge.render(bar_area, buf);
        }
    }
}

/// Compact progress for header display.
pub struct CompactProgress {
    note_name: String,
    current: usize,
    total: usize,
}

impl CompactProgress {
    /// Create a compact progress indicator.
    pub fn new(note_name: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            note_name: note_name.into(),
            current,
            total,
        }
    }
}

impl Widget for CompactProgress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = format!("{} | {}/{}", self.note_name, self.current + 1, self.total);
        buf.set_string(area.x, area.y, &text, Theme::muted());
    }
}
