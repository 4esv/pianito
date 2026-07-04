//! Main tuning screen.

use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::ui::components::instructions::TuningStep;
use crate::ui::components::{Instructions, Meter, Piano, Progress};
use crate::ui::theme::{Shortcuts, Theme};

/// Main tuning screen state.
pub struct TuningScreen {
    /// Current note name.
    note_name: String,
    /// Current note index in tuning order.
    note_index: usize,
    /// Chromatic note index (0=A0, 87=C8) for piano display.
    chromatic_index: usize,
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
    /// Current tuning step (for multi-string notes).
    tuning_step: Option<TuningStep>,
    /// Phase name for display.
    phase_name: String,
    /// Whether to show piano progress view.
    show_piano_progress: bool,
    /// Set of completed chromatic indices.
    completed_notes: HashSet<usize>,
    /// In-tune tolerance in cents (from config; drives the meter zone and
    /// direction hints).
    tolerance: f32,
}

impl TuningScreen {
    /// Create a new tuning screen.
    pub fn new(
        note_name: impl Into<String>,
        note_index: usize,
        total_notes: usize,
        target_freq: f32,
        string_count: u8,
        midi: u8,
    ) -> Self {
        // Use first_for_strings to get the starting step for bi/trichord notes
        let tuning_step = TuningStep::first_for_strings(string_count);

        let phase_name = if string_count == 3 {
            "Trichord".to_string()
        } else if string_count == 2 {
            "Bichord".to_string()
        } else {
            "Single".to_string()
        };

        // Chromatic index: 0=A0 (MIDI 21), 87=C8 (MIDI 108)
        let chromatic_index = (midi - 21) as usize;

        Self {
            note_name: note_name.into(),
            note_index,
            chromatic_index,
            total_notes,
            target_freq,
            detected_freq: None,
            cents_deviation: 0.0,
            string_count,
            tuning_step,
            phase_name,
            show_piano_progress: false,
            completed_notes: HashSet::new(),
            tolerance: 5.0,
        }
    }

    /// Set the in-tune tolerance in cents.
    pub fn set_tolerance(&mut self, tolerance: f32) {
        self.tolerance = tolerance;
    }

    /// Toggle piano progress display.
    pub fn toggle_piano_progress(&mut self) {
        self.show_piano_progress = !self.show_piano_progress;
    }

    /// Set the completed notes for progress display.
    pub fn set_completed_notes(&mut self, completed: HashSet<usize>) {
        self.completed_notes = completed;
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

    /// Check if this note has multiple strings (bichord or trichord).
    pub fn is_multi_string(&self) -> bool {
        self.string_count >= 2
    }

    /// Get current tuning step.
    pub fn tuning_step(&self) -> Option<TuningStep> {
        self.tuning_step
    }

    /// Advance to next tuning step (for multi-string notes).
    pub fn next_step(&mut self) -> bool {
        if let Some(step) = &self.tuning_step {
            if let Some(next) = step.next() {
                self.tuning_step = Some(next);
                return true;
            }
        }
        false
    }

    /// Go back to previous tuning step.
    pub fn prev_step(&mut self) -> bool {
        if let Some(step) = &self.tuning_step {
            if let Some(prev) = step.prev() {
                self.tuning_step = Some(prev);
                return true;
            }
        }
        false
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

        // Minimum for the reduced layout below (progress + instructions +
        // meter + help)
        if inner.height < 16 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Check if we're in muting step (don't show meter or hints)
        let is_muting_step = self.tuning_step.map(|s| s.is_muting()).unwrap_or(false);

        // Full layout needs 25 rows; below that, drop the piano and spacers
        // so the meter (the core feedback) keeps its full height.
        let (progress_area, piano_area, instructions_area, meter_area, help_area) =
            if inner.height >= 25 {
                let chunks = Layout::vertical([
                    Constraint::Length(2), // Progress bar
                    Constraint::Length(1), // Spacer
                    Constraint::Length(4), // Piano visualization
                    Constraint::Length(1), // Spacer
                    Constraint::Min(6),    // Instructions
                    Constraint::Length(1), // Spacer
                    Constraint::Length(8), // Meter (hidden during muting)
                    Constraint::Length(2), // Help text
                ])
                .split(inner);
                (chunks[0], Some(chunks[2]), chunks[4], chunks[6], chunks[7])
            } else {
                let chunks = Layout::vertical([
                    Constraint::Length(2), // Progress bar
                    Constraint::Min(4),    // Instructions
                    Constraint::Length(8), // Meter (hidden during muting)
                    Constraint::Length(2), // Help text
                ])
                .split(inner);
                (chunks[0], None, chunks[1], chunks[2], chunks[3])
            };

        // Progress indicator
        let progress = Progress::new(
            self.note_index,
            self.total_notes,
            &self.note_name,
            &self.phase_name,
        );
        progress.render(progress_area, buf);

        // Piano visualization (full 88-key piano, A0=MIDI 21)
        if let Some(piano_area) = piano_area {
            let piano = if self.show_piano_progress {
                Piano::full()
                    .highlighted(self.completed_notes.clone())
                    .current(Some(self.chromatic_index))
            } else {
                Piano::full().current(Some(self.chromatic_index))
            };
            piano.render(piano_area, buf);
        }

        // Instructions panel
        if let Some(step) = self.tuning_step {
            // Multi-string note (bichord or trichord)
            let instructions = if is_muting_step {
                // Don't show direction hints during muting
                Instructions::for_step(step, self.string_count)
            } else {
                Instructions::for_step(step, self.string_count)
                    .with_direction_hint(self.cents_deviation, self.tolerance)
            };
            instructions.render(instructions_area, buf);
        } else {
            // Monochord note - simple instruction
            let instructions =
                Instructions::simple().with_direction_hint(self.cents_deviation, self.tolerance);
            instructions.render(instructions_area, buf);
        }

        // Cents meter (hidden during muting step)
        if !is_muting_step {
            let meter = if self.detected_freq.is_some() {
                Meter::new(self.cents_deviation).tolerance(self.tolerance)
            } else {
                Meter::listening().tolerance(self.tolerance)
            };
            meter.render(meter_area, buf);
        }

        // Help text
        let help_text = format!(
            "{} Confirm  {} Back  {} Progress  {} Skip  {} Quit",
            Shortcuts::SPACE,
            Shortcuts::BACK,
            Shortcuts::PIANO,
            Shortcuts::SKIP,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(help_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(screen: &TuningScreen, width: u16, height: u16) -> String {
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

    fn monochord_screen() -> TuningScreen {
        // A0 (MIDI 21) is a single-string note: no muting step, meter visible
        TuningScreen::new("A0", 87, 88, 27.5, 1, 21)
    }

    #[test]
    fn test_small_terminal_keeps_meter_and_help() {
        // Stock 80x24 terminal: inner is 78x22, below the 25-row full layout
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 24);

        assert!(!rendered.contains("Terminal too small"));
        assert!(rendered.contains("Listening..."), "meter must be visible");
        assert!(rendered.contains("Confirm"), "help line must be visible");
        assert!(!rendered.contains('╚'), "piano dropped at this height");
    }

    #[test]
    fn test_full_terminal_shows_piano() {
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 110, 30);

        assert!(rendered.contains('╚'), "piano visible");
        assert!(rendered.contains("Listening..."));
    }

    #[test]
    fn test_too_small_terminal_shows_message() {
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 10);
        assert!(rendered.contains("Terminal too small"));
    }

    #[test]
    fn test_configured_tolerance_widens_in_tune_zone() {
        // +8 cents: out of tune at the default 5-cent tolerance...
        let mut screen = monochord_screen();
        screen.update(27.63, 8.0);
        let rendered = render_to_string(&screen, 80, 24);
        assert!(rendered.contains("Loosen"), "hint shown outside tolerance");

        // ...but in tune once the configured tolerance is 10 cents
        screen.set_tolerance(10.0);
        let rendered = render_to_string(&screen, 80, 24);
        assert!(
            !rendered.contains("Loosen") && !rendered.contains("Tighten"),
            "no hint inside the configured tolerance"
        );
    }
}
