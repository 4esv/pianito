//! Piano profiling screen for measuring deviation of all 88 keys.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::audio::Partial;
use crate::tuning::notes::{Note, NOTES};
use crate::tuning::profile::PianoProfile;
use crate::tuning::temperament::Temperament;
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
    /// Target frequency for the current note. Kept in sync by App so the
    /// info panel shows the same target the meter's cents are computed from.
    target_freq: f32,
    /// In-tune tolerance in cents (from config; drives the meter zone).
    tolerance: f32,
    /// Partials measured from the audio window backing `current_freq`/
    /// `current_cents` (issue #22). Set by `App` alongside `update`, once a
    /// raw audio window is available to run the FFT analyzer against; empty
    /// until then, or after `clear`/`confirm_note`/`skip_note`/`go_back`.
    current_partials: Vec<Partial>,
    /// The profile being built.
    profile: PianoProfile,
}

impl ProfilingScreen {
    /// Create a new profiling screen.
    pub fn new() -> Self {
        Self {
            current_note_idx: 0,
            current_freq: None,
            current_cents: None,
            // NOTE: pre-sync default only; App overwrites this via
            // set_target_freq with the stretch/a4-adjusted target
            target_freq: Temperament::new().frequency(NOTES[0].midi),
            tolerance: 5.0,
            current_partials: Vec::new(),
            profile: PianoProfile::new(),
        }
    }

    /// Set the in-tune tolerance in cents.
    pub fn set_tolerance(&mut self, tolerance: f32) {
        self.tolerance = tolerance;
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
        self.current_partials = Vec::new();
    }

    /// Record the partial spectrum backing the current reading (issue #22).
    /// Called by `App` alongside `update`, once a raw audio window is
    /// available to analyze.
    pub fn set_current_partials(&mut self, partials: Vec<Partial>) {
        self.current_partials = partials;
    }

    /// The partials measured for the current reading, if any.
    pub fn current_partials(&self) -> &[Partial] {
        &self.current_partials
    }

    /// Set the target frequency for the current note.
    pub fn set_target_freq(&mut self, freq: f32) {
        self.target_freq = freq;
    }

    /// Get the target frequency for the current note.
    pub fn target_freq(&self) -> f32 {
        self.target_freq
    }

    /// Confirm the current note measurement.
    /// Returns true if profiling is now complete.
    ///
    /// Without a pitch reading this is a no-op: advancing silently would
    /// leave a hole that `TuningOrder::from_profile` treats as unknown/worst
    /// deviation and queues first (after the temperament octave). Use skip
    /// to pass over a note deliberately.
    pub fn confirm_note(&mut self) -> bool {
        let (Some(freq), Some(cents)) = (self.current_freq, self.current_cents) else {
            return false;
        };

        let note = self.current_note();
        let partials = std::mem::take(&mut self.current_partials);
        self.profile
            .record_note_with_partials(note.midi, freq, cents, partials);

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
        self.current_partials = Vec::new();

        self.is_complete()
    }

    /// Go back to the previous note.
    pub fn go_back(&mut self) {
        if self.current_note_idx > 0 {
            self.current_note_idx -= 1;
            self.current_freq = None;
            self.current_cents = None;
            self.current_partials = Vec::new();
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

        // Minimum for the reduced layout below (progress + info + meter + help)
        if inner.height < 16 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Full layout needs 23 rows; below that, drop the piano and spacers
        // so the meter (the core feedback) keeps its full height.
        let (progress_area, piano_area, info_area, meter_area, help_area) = if inner.height >= 23 {
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
            (chunks[0], Some(chunks[2]), chunks[4], chunks[6], chunks[7])
        } else {
            let chunks = Layout::vertical([
                Constraint::Length(2), // Progress bar
                Constraint::Length(4), // Note info
                Constraint::Length(8), // Meter
                Constraint::Min(0),    // Filler
                Constraint::Length(2), // Help text
            ])
            .split(inner);
            (chunks[0], None, chunks[1], chunks[2], chunks[4])
        };

        // Progress indicator
        let (completed, total) = self.progress();
        let progress = Progress::new(completed, total, note.display_name(), "Profiling");
        progress.render(progress_area, buf);

        // Piano visualization with profiled notes colored by deviation
        if let Some(piano_area) = piano_area {
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
            piano.render(piano_area, buf);
        }

        // Note info panel
        render_note_info(note, self.target_freq, &self.profile, info_area, buf);

        // Cents meter
        if let Some(cents) = self.current_cents {
            let meter = Meter::new(cents).tolerance(self.tolerance);
            meter.render(meter_area, buf);
        } else {
            let meter = Meter::listening().tolerance(self.tolerance);
            meter.render(meter_area, buf);
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
        help.render(help_area, buf);
    }
}

/// Render note info panel.
fn render_note_info(
    note: &Note,
    target_freq: f32,
    profile: &PianoProfile,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height < 3 {
        return;
    }

    // Note name and target frequency (as synced from App's temperament)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(screen: &ProfilingScreen, width: u16, height: u16) -> String {
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

    #[test]
    fn test_confirm_without_reading_is_noop() {
        let mut screen = ProfilingScreen::new();

        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 0, "must not advance");
        assert_eq!(screen.profile().progress(), (0, 88), "must not record");
    }

    #[test]
    fn test_confirm_records_at_current_index() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);

        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 1);
        assert_eq!(screen.profile().progress(), (1, 88));

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert_eq!(recorded.midi, 21);
        assert!((recorded.cents - 3.0).abs() < 0.01);
    }

    fn sample_partials() -> Vec<Partial> {
        vec![
            Partial {
                n: 1,
                freq_hz: 27.6,
                amplitude: 0.2,
            },
            Partial {
                n: 2,
                freq_hz: 55.3,
                amplitude: 1.0,
            },
        ]
    }

    #[test]
    fn test_confirm_persists_current_partials() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);
        screen.set_current_partials(sample_partials());

        assert_eq!(screen.current_partials(), sample_partials().as_slice());
        screen.confirm_note();

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert_eq!(recorded.partials, sample_partials());
    }

    #[test]
    fn test_confirm_without_partials_records_empty() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);
        screen.confirm_note();

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert!(recorded.partials.is_empty());
    }

    #[test]
    fn test_confirm_does_not_leak_partials_into_next_note() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);
        screen.set_current_partials(sample_partials());
        screen.confirm_note();

        // Next note's reading arrives with no partials set yet.
        screen.update(55.0, -1.0);
        screen.confirm_note();

        let recorded = screen.profile().notes[1].as_ref().expect("A#0 recorded");
        assert!(
            recorded.partials.is_empty(),
            "stale partials from the previous note must not carry over"
        );
    }

    #[test]
    fn test_clear_resets_partials() {
        let mut screen = ProfilingScreen::new();
        screen.set_current_partials(sample_partials());
        screen.clear();
        assert!(screen.current_partials().is_empty());
    }

    #[test]
    fn test_skip_note_resets_partials() {
        let mut screen = ProfilingScreen::new();
        screen.set_current_partials(sample_partials());
        screen.skip_note();
        assert!(screen.current_partials().is_empty());
    }

    #[test]
    fn test_go_back_resets_partials() {
        let mut screen = ProfilingScreen::new();
        screen.skip_note();
        screen.set_current_partials(sample_partials());
        screen.go_back();
        assert!(screen.current_partials().is_empty());
    }

    #[test]
    fn test_confirm_clears_reading_for_next_note() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);
        screen.confirm_note();

        // The old reading must not leak into the next note
        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 1);
    }

    #[test]
    fn test_skip_advances_without_recording() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0);

        assert!(!screen.skip_note());
        assert_eq!(screen.current_note_idx(), 1);
        assert_eq!(screen.profile().progress(), (0, 88));
    }

    #[test]
    fn test_go_back_at_zero_stays_at_zero() {
        let mut screen = ProfilingScreen::new();
        screen.go_back();
        assert_eq!(screen.current_note_idx(), 0);
    }

    #[test]
    fn test_go_back_steps_to_previous_note() {
        let mut screen = ProfilingScreen::new();
        screen.skip_note();
        screen.skip_note();
        screen.go_back();
        assert_eq!(screen.current_note_idx(), 1);
    }

    #[test]
    fn test_confirming_all_88_notes_completes() {
        let mut screen = ProfilingScreen::new();

        for i in 0..88 {
            screen.update(440.0, 1.0);
            let done = screen.confirm_note();
            assert_eq!(done, i == 87, "only the 88th confirm completes");
        }

        assert!(screen.is_complete());
        assert_eq!(screen.profile().progress(), (88, 88));
        assert!(screen.profile().is_complete());
    }

    #[test]
    fn test_target_freq_roundtrip() {
        let mut screen = ProfilingScreen::new();
        screen.set_target_freq(27.3);
        assert!((screen.target_freq() - 27.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_small_terminal_drops_piano_keeps_meter() {
        // Stock 80x24 terminal: inner is 78x22, below the 23-row full layout
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 80, 24);

        assert!(!rendered.contains("Terminal too small"));
        assert!(rendered.contains("Listening..."), "meter must be visible");
        assert!(rendered.contains("Profiled: 0/88"), "info must be visible");
        assert!(!rendered.contains('╚'), "piano dropped at this height");
    }

    #[test]
    fn test_full_terminal_shows_piano() {
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 110, 30);

        assert!(rendered.contains('╚'), "piano visible");
        assert!(rendered.contains("Listening..."));
    }

    #[test]
    fn test_too_small_terminal_shows_message() {
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 80, 10);
        assert!(rendered.contains("Terminal too small"));
    }
}
