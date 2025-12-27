//! Session complete summary screen.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tuning::session::CompletedNote;
use crate::ui::theme::{Shortcuts, Theme};

/// Session complete screen with summary.
pub struct CompleteScreen {
    /// Completed notes from the session.
    completed_notes: Vec<CompletedNote>,
    /// Average absolute deviation in cents.
    avg_deviation: f32,
    /// Notes within tolerance (±5 cents).
    notes_in_tune: usize,
    /// Notes with warning (±5-15 cents).
    notes_warning: usize,
    /// Notes out of tune (>±15 cents).
    notes_out_of_tune: usize,
    /// Total tuning duration.
    duration_secs: u64,
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

        let notes_in_tune = completed_notes
            .iter()
            .filter(|n| n.final_cents.abs() <= 5.0)
            .count();

        let notes_warning = completed_notes
            .iter()
            .filter(|n| n.final_cents.abs() > 5.0 && n.final_cents.abs() <= 15.0)
            .count();

        let notes_out_of_tune = completed_notes
            .iter()
            .filter(|n| n.final_cents.abs() > 15.0)
            .count();

        Self {
            completed_notes,
            avg_deviation,
            notes_in_tune,
            notes_warning,
            notes_out_of_tune,
            duration_secs: 0,
        }
    }

    /// Set the session duration.
    pub fn with_duration(mut self, secs: u64) -> Self {
        self.duration_secs = secs;
        self
    }

    /// Get the number of completed notes.
    pub fn note_count(&self) -> usize {
        self.completed_notes.len()
    }

    /// Get average deviation.
    pub fn avg_deviation(&self) -> f32 {
        self.avg_deviation
    }
}

impl Widget for &CompleteScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Main container
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(" Tuning Complete! ")
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 12 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Layout
        let chunks = Layout::vertical([
            Constraint::Length(3), // Title/congrats
            Constraint::Length(1), // Spacer
            Constraint::Length(6), // Summary stats
            Constraint::Length(1), // Spacer
            Constraint::Min(4),    // Quality breakdown
            Constraint::Length(2), // Help text
        ])
        .split(inner);

        // Congratulations message
        let quality = if self.avg_deviation <= 3.0 {
            ("Excellent tuning!", Theme::in_tune())
        } else if self.avg_deviation <= 8.0 {
            ("Good tuning!", Theme::in_tune())
        } else if self.avg_deviation <= 15.0 {
            ("Acceptable tuning", Theme::warning())
        } else {
            ("Tuning needs improvement", Theme::out_of_tune())
        };

        let congrats = Paragraph::new(quality.0)
            .style(quality.1)
            .alignment(Alignment::Center);
        congrats.render(chunks[0], buf);

        // Summary stats
        let stats_area = chunks[2];
        let stats = [
            format!("Notes tuned: {}", self.completed_notes.len()),
            format!("Average deviation: {:.1} cents", self.avg_deviation),
            format!(
                "Duration: {}:{:02}",
                self.duration_secs / 60,
                self.duration_secs % 60
            ),
        ];

        for (i, stat) in stats.iter().enumerate() {
            let y = stats_area.y + i as u16;
            if y < stats_area.y + stats_area.height {
                let x = stats_area.x + stats_area.width / 2 - stat.len() as u16 / 2;
                buf.set_string(x, y, stat, Theme::muted());
            }
        }

        // Quality breakdown
        let breakdown_area = chunks[4];
        let breakdown_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::muted())
            .title(" Breakdown ")
            .title_style(Theme::muted());

        let breakdown_inner = breakdown_block.inner(breakdown_area);
        breakdown_block.render(breakdown_area, buf);

        if breakdown_inner.height >= 3 {
            let in_tune_text = format!("● In tune (±5¢): {}", self.notes_in_tune);
            let warning_text = format!("● Warning (±5-15¢): {}", self.notes_warning);
            let out_text = format!("● Out of tune (>±15¢): {}", self.notes_out_of_tune);

            buf.set_string(
                breakdown_inner.x + 2,
                breakdown_inner.y,
                &in_tune_text,
                Theme::in_tune(),
            );
            if breakdown_inner.height >= 2 {
                buf.set_string(
                    breakdown_inner.x + 2,
                    breakdown_inner.y + 1,
                    &warning_text,
                    Theme::warning(),
                );
            }
            if breakdown_inner.height >= 3 {
                buf.set_string(
                    breakdown_inner.x + 2,
                    breakdown_inner.y + 2,
                    &out_text,
                    Theme::out_of_tune(),
                );
            }
        }

        // Help text
        let help_text = format!("{} New session  {} Quit", Shortcuts::ENTER, Shortcuts::QUIT);
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(chunks[5], buf);
    }
}
