//! Piano profiling screen for measuring deviation of all 88 keys.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tuning::notes::{Note, NOTES};
use crate::tuning::profile::PianoProfile;
use crate::ui::components::{Meter, Piano, Progress};
use crate::ui::theme::{Shortcuts, Theme};

/// Profiling screen for measuring all 88 keys sequentially.
pub struct ProfilingScreen {
    /// Current note index (0-87, chromatic order A0→C8).
    current_note_idx: usize,
    /// Current detected frequency.
    current_freq: Option<f32>,
    /// Current cents deviation.
    current_cents: Option<f32>,
    /// The profile being built.
    profile: PianoProfile,
    /// Whether to show the piano progress view.
    show_piano: bool,
}

impl ProfilingScreen {
    /// Create a new profiling screen.
    pub fn new() -> Self {
        Self {
            current_note_idx: 0,
            current_freq: None,
            current_cents: None,
            profile: PianoProfile::new(),
            show_piano: true,
        }
    }

    /// Get the current note to profile.
    pub fn current_note(&self) -> &'static Note {
        &NOTES[self.current_note_idx]
    }

    /// Get the current note index.
    pub fn current_note_idx(&self) -> usize {
        self.current_note_idx
    }

    /// Update with detected pitch.
    pub fn update(&mut self, freq: f32, cents: f32) {
        self.current_freq = Some(freq);
        self.current_cents = Some(cents);
    }

    /// Clear detected pitch (silence).
    pub fn clear(&mut self) {
        self.current_freq = None;
        self.current_cents = None;
    }

    /// Confirm the current note measurement.
    /// Returns true if profiling is now complete.
    pub fn confirm_note(&mut self) -> bool {
        if let (Some(freq), Some(cents)) = (self.current_freq, self.current_cents) {
            let note = self.current_note();
            self.profile.record_note(note.midi, freq, cents);
        }

        self.current_note_idx += 1;
        self.current_freq = None;
        self.current_cents = None;

        self.is_complete()
    }

    /// Skip the current note without recording.
    /// Returns true if profiling is now complete.
    pub fn skip_note(&mut self) -> bool {
        self.current_note_idx += 1;
        self.current_freq = None;
        self.current_cents = None;

        self.is_complete()
    }

    /// Go back to the previous note.
    pub fn go_back(&mut self) {
        if self.current_note_idx > 0 {
            self.current_note_idx -= 1;
            self.current_freq = None;
            self.current_cents = None;
        }
    }

    /// Check if profiling is complete (all 88 notes visited).
    pub fn is_complete(&self) -> bool {
        self.current_note_idx >= 88
    }

    /// Take the completed profile.
    pub fn take_profile(self) -> PianoProfile {
        self.profile
    }

    /// Get a reference to the profile.
    pub fn profile(&self) -> &PianoProfile {
        &self.profile
    }

    /// Get progress as (current, total).
    pub fn progress(&self) -> (usize, usize) {
        (self.current_note_idx, 88)
    }

    /// Toggle piano display.
    pub fn toggle_piano(&mut self) {
        self.show_piano = !self.show_piano;
    }
}

impl Default for ProfilingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &ProfilingScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let note = self.current_note();
        let title = format!(" Profile: {} ", note.display_name());

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(title)
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
            Constraint::Length(4), // Piano visualization
            Constraint::Length(1), // Spacer
            Constraint::Length(4), // Note info
            Constraint::Length(1), // Spacer
            Constraint::Length(8), // Meter
            Constraint::Length(2), // Help text
        ])
        .split(inner);

        // Progress indicator
        let (completed, total) = self.progress();
        let progress = Progress::new(completed, total, note.display_name(), "Profiling");
        progress.render(chunks[0], buf);

        // Piano visualization with profiled notes colored by deviation
        let deviations: HashMap<usize, f32> = self
            .profile
            .notes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| n.as_ref().map(|note| (i, note.cents)))
            .collect();

        let piano = Piano::full()
            .with_deviations(deviations)
            .current(Some(self.current_note_idx));
        piano.render(chunks[2], buf);

        // Note info panel
        render_note_info(note, &self.profile, chunks[4], buf);

        // Cents meter
        if let Some(cents) = self.current_cents {
            let meter = Meter::new(cents);
            meter.render(chunks[6], buf);
        } else {
            let meter = Meter::listening();
            meter.render(chunks[6], buf);
        }

        // Help text
        let help_text = format!(
            "{} Confirm  {} Back  {} Skip  {} Quit",
            Shortcuts::SPACE,
            Shortcuts::BACK,
            Shortcuts::SKIP,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(chunks[7], buf);
    }
}

/// Render note info panel.
fn render_note_info(note: &Note, profile: &PianoProfile, area: Rect, buf: &mut Buffer) {
    if area.height < 3 {
        return;
    }

    // Note name and target frequency
    let target_freq = 440.0 * 2_f32.powf((note.midi as f32 - 69.0) / 12.0);
    let info_line = format!(
        "{}  Target: {:.1} Hz  Strings: {}",
        note.display_name(),
        target_freq,
        note.strings
    );

    let info = Paragraph::new(info_line)
        .style(Theme::accent())
        .alignment(Alignment::Center);

    let info_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    info.render(info_area, buf);

    // Profile summary
    let (completed, total) = profile.progress();
    let avg_deviation = profile.average_deviation();
    let summary = format!(
        "Profiled: {}/{}  Avg deviation: {:.1} cents",
        completed, total, avg_deviation
    );

    let summary_para = Paragraph::new(summary)
        .style(Theme::muted())
        .alignment(Alignment::Center);

    let summary_area = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: 1,
    };
    summary_para.render(summary_area, buf);
}
