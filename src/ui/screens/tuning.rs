//! Main tuning screen.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::ui::components::instructions::TuningStep;
use crate::ui::components::{Instructions, Meter, Progress};
use crate::ui::theme::{Shortcuts, Theme};

/// Main tuning screen state.
pub struct TuningScreen {
    /// Current note name.
    note_name: String,
    /// Current note index in tuning order.
    note_index: usize,
    /// Total notes to tune.
    total_notes: usize,
    /// Target frequency in Hz.
    target_freq: f32,
    /// Detected frequency (if any).
    detected_freq: Option<f32>,
    /// Cents deviation from target.
    cents_deviation: f32,
    /// Number of strings for this note.
    string_count: u8,
    /// Current tuning step (for trichords).
    tuning_step: Option<TuningStep>,
    /// Phase name for display.
    phase_name: String,
}

impl TuningScreen {
    /// Create a new tuning screen.
    pub fn new(
        note_name: impl Into<String>,
        note_index: usize,
        total_notes: usize,
        target_freq: f32,
        string_count: u8,
    ) -> Self {
        let tuning_step = if string_count == 3 {
            Some(TuningStep::MuteOuter)
        } else {
            None
        };

        let phase_name = if string_count == 3 {
            "Trichord".to_string()
        } else if string_count == 2 {
            "Bichord".to_string()
        } else {
            "Single".to_string()
        };

        Self {
            note_name: note_name.into(),
            note_index,
            total_notes,
            target_freq,
            detected_freq: None,
            cents_deviation: 0.0,
            string_count,
            tuning_step,
            phase_name,
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

    /// Get current cents deviation.
    pub fn cents(&self) -> f32 {
        self.cents_deviation
    }

    /// Check if this is a trichord note.
    pub fn is_trichord(&self) -> bool {
        self.string_count == 3
    }

    /// Get current tuning step.
    pub fn tuning_step(&self) -> Option<TuningStep> {
        self.tuning_step
    }

    /// Advance to next tuning step (for trichords).
    pub fn next_step(&mut self) -> bool {
        if let Some(step) = &self.tuning_step {
            if let Some(next) = step.next() {
                self.tuning_step = Some(next);
                return true;
            }
        }
        false
    }

    /// Check if note tuning is complete.
    pub fn is_complete(&self) -> bool {
        if self.string_count == 3 {
            self.tuning_step == Some(TuningStep::TuneRight)
                && self.cents_deviation.abs() <= 5.0
                && self.detected_freq.is_some()
        } else {
            self.cents_deviation.abs() <= 5.0 && self.detected_freq.is_some()
        }
    }

    /// Get note name.
    pub fn note_name(&self) -> &str {
        &self.note_name
    }

    /// Get target frequency.
    pub fn target_freq(&self) -> f32 {
        self.target_freq
    }
}

impl Widget for &TuningScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Main container
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(format!(" Tuning: {} ", self.note_name))
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 15 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Layout
        let chunks = Layout::vertical([
            Constraint::Length(2), // Progress bar
            Constraint::Length(1), // Spacer
            Constraint::Length(8), // Meter
            Constraint::Length(1), // Spacer
            Constraint::Min(6),    // Instructions
            Constraint::Length(2), // Help text
        ])
        .split(inner);

        // Progress indicator
        let progress = Progress::new(
            self.note_index,
            self.total_notes,
            &self.note_name,
            &self.phase_name,
        );
        progress.render(chunks[0], buf);

        // Cents meter
        let meter = if self.detected_freq.is_some() {
            Meter::new(self.cents_deviation)
        } else {
            Meter::listening()
        };
        meter.render(chunks[2], buf);

        // Instructions panel
        let instructions_area = chunks[4];
        if self.string_count == 3 {
            if let Some(step) = self.tuning_step {
                let instructions =
                    Instructions::trichord(step).with_direction_hint(self.cents_deviation);
                instructions.render(instructions_area, buf);
            }
        } else {
            let instructions = Instructions::simple().with_direction_hint(self.cents_deviation);
            instructions.render(instructions_area, buf);
        }

        // Help text
        let help_text = format!(
            "{} Confirm  {} Reference tone  {} Skip  {} Quit",
            Shortcuts::SPACE,
            Shortcuts::REFERENCE,
            Shortcuts::SKIP,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(chunks[5], buf);
    }
}
