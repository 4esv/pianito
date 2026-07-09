//! Main tuning screen.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Modifier,
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::ui::components::animation::{LockAnimation, NeedleTrail};
use crate::ui::components::instructions::TuningStep;
use crate::ui::components::{Instructions, Meter, NoteGlyph, Piano, Progress};
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
    /// Needle smoothing + fading trail (issue #32), advanced once per
    /// render tick by [`Self::advance_animation`] regardless of whether a
    /// new pitch reading arrived this tick.
    needle_trail: NeedleTrail,
    /// Lock-flash state machine (issue #32): flashes when the reading
    /// settles into the in-tune zone, then holds until it drifts back out.
    lock_anim: LockAnimation,
    /// Timestamp of the last `advance_animation` call, so `dt` can be
    /// derived instead of assuming a fixed frame interval (`None` before
    /// the first tick since this note started).
    last_tick: Option<Instant>,
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
            needle_trail: NeedleTrail::new(),
            lock_anim: LockAnimation::new(),
            last_tick: None,
        }
    }

    /// Set the in-tune tolerance in cents.
    pub fn set_tolerance(&mut self, tolerance: f32) {
        self.tolerance = tolerance;
    }

    /// Advance frame-driven animation state (issue #32): the needle's
    /// smoothing/trail and the lock-flash state machine. Called once per
    /// render tick regardless of whether a new pitch reading arrived this
    /// tick, so the needle keeps interpolating between the sparser raw
    /// detections (issue #28's ~60fps loop vs. ~10Hz pitch updates).
    pub fn advance_animation(&mut self, now: Instant) {
        let dt = match self.last_tick {
            Some(prev) => now.saturating_duration_since(prev),
            None => Duration::ZERO,
        };
        self.last_tick = Some(now);

        self.needle_trail.update(self.cents_deviation, dt);

        let is_muting = self.tuning_step.map(|s| s.is_muting()).unwrap_or(false);
        let in_tune = !is_muting
            && self.detected_freq.is_some()
            && self.cents_deviation.abs() <= self.tolerance;
        self.lock_anim.update(in_tune);
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

        // Minimum for the reduced layout below (header + instructions +
        // meter + help)
        if inner.height < 19 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Check if we're in muting step (don't show meter or hints)
        let is_muting_step = self.tuning_step.map(|s| s.is_muting()).unwrap_or(false);

        // Full layout needs 28 rows; below that, drop the piano and spacers
        // so the meter (the core feedback) keeps its full height.
        let (header_area, piano_area, instructions_area, meter_area, help_area) =
            if inner.height >= 28 {
                let chunks = Layout::vertical([
                    Constraint::Length(5), // Header: note glyph + progress
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
                    Constraint::Length(5), // Header: note glyph + progress
                    Constraint::Min(4),    // Instructions
                    Constraint::Length(8), // Meter (hidden during muting)
                    Constraint::Length(2), // Help text
                ])
                .split(inner);
                (chunks[0], None, chunks[1], chunks[2], chunks[3])
            };

        // Header: the current note as a large glyph (issue #32 - it used to
        // be the smallest text on screen, the border title) beside the
        // progress indicator. The glyph is skipped below a width that would
        // otherwise starve `Progress` (which itself needs >=20 columns).
        let glyph_width = NoteGlyph::new(&self.note_name).width();
        let glyph_style = if self.lock_anim.is_locked() {
            Theme::in_tune().add_modifier(Modifier::BOLD)
        } else {
            Theme::accent()
        };
        let progress_area = if header_area.width >= glyph_width + 2 + 20 {
            let chunks = Layout::horizontal([
                Constraint::Length(glyph_width),
                Constraint::Length(2), // Gap
                Constraint::Min(20),
            ])
            .split(header_area);
            NoteGlyph::new(&self.note_name)
                .style(glyph_style)
                .render(chunks[0], buf);
            chunks[2]
        } else {
            header_area
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
                Meter::new(self.cents_deviation)
                    .tolerance(self.tolerance)
                    .smoothed(self.needle_trail.smoothed())
                    .trail(self.needle_trail.trail())
                    .flashing(self.lock_anim.is_flashing())
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
        // Stock 80x24 terminal: inner is 78x22, below the 28-row full layout
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

    // -- hero screen: note glyph, needle animation, lock flash (issue #32) --

    #[test]
    fn test_note_glyph_renders_in_the_header() {
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 24);
        assert!(
            rendered.contains('█'),
            "expected the large note glyph's block characters in the header"
        );
    }

    /// Whether any glyph-ink cell ('█') in the rendered screen carries the
    /// `BOLD` modifier - the note glyph's "locked" style, distinguishing it
    /// from the plain accent style used before a lock. Checked via the
    /// modifier (not color) so the assertion holds under `NO_COLOR`, where
    /// every fg color collapses to the same `Reset`.
    fn any_glyph_ink_is_bold(screen: &TuningScreen, width: u16, height: u16) -> bool {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        screen.render(area, &mut buf);
        (0..height).any(|y| {
            (0..width).any(|x| {
                let cell = &buf[(x, y)];
                cell.symbol() == "█" && cell.style().add_modifier.contains(Modifier::BOLD)
            })
        })
    }

    #[test]
    fn test_note_glyph_is_not_bold_before_any_lock() {
        let mut screen = monochord_screen();
        screen.update(27.5, 2.0); // within default 5-cent tolerance, but no ticks yet
        assert!(!any_glyph_ink_is_bold(&screen, 80, 24));
    }

    #[test]
    fn test_note_glyph_turns_bold_once_locked() {
        let mut screen = monochord_screen();
        screen.update(27.5, 2.0); // within default 5-cent tolerance

        let mut now = Instant::now();
        for _ in 0..(LockAnimation::FLASH_FRAMES as u32 + 2) {
            now += Duration::from_millis(16);
            screen.advance_animation(now);
        }

        assert!(
            any_glyph_ink_is_bold(&screen, 80, 24),
            "glyph must switch to the locked style once the lock holds"
        );
    }

    #[test]
    fn test_advance_animation_does_not_panic_without_a_prior_tick() {
        // Guards the `last_tick == None` first-call path.
        let mut screen = monochord_screen();
        screen.update(27.5, 2.0);
        screen.advance_animation(Instant::now());
    }

    #[test]
    fn test_render_smoke_across_sizes_does_not_panic() {
        let mut screen = monochord_screen();
        screen.update(27.63, 3.0);
        screen.advance_animation(Instant::now());

        for (w, h) in [
            (0, 0),
            (1, 1),
            (10, 5),
            (39, 19),  // just under the width gate
            (40, 18),  // just under the height gate
            (40, 19),  // exactly at both gates
            (78, 22),  // stock 80x24 minus border
            (108, 28), // full layout threshold
            (200, 60),
        ] {
            let _ = render_to_string(&screen, w, h);
        }
    }
}
