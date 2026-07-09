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
use crate::ui::components::meter::MuteLevel;
use crate::ui::components::{Instructions, Meter, NoteGlyph, Piano, Progress};
use crate::ui::theme::{Shortcuts, Theme};

/// Progressive small-terminal degradation for the tuning screen (issue
/// #31). The full layout wants a lot of vertical real estate (the note
/// glyph header, the piano, capped instructions, a flexible meter, and
/// help text); rather than a single "too small" wall, each optional
/// element is dropped in turn - piano, then the large note glyph, then
/// instructions, then help - so the essential needle+note+cents stay
/// visible as long as there's room for a full-featured [`Meter`] (the
/// [`Meter`] widget itself degrades further below that, see `meter.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TuningLayoutPlan {
    show_piano: bool,
    show_glyph: bool,
    show_instructions: bool,
    show_help: bool,
}

impl TuningLayoutPlan {
    /// Row cost of the header when showing the large 5-row note glyph
    /// beside the progress line.
    const GLYPH_HEADER_ROWS: u16 = 5;
    /// Row cost of the header once the glyph is dropped - just the
    /// progress line's own compact 1-row form.
    const PLAIN_HEADER_ROWS: u16 = 1;
    /// Breathing room between the header and the piano, shown only when
    /// the piano itself is shown (issue #31: the other spacers from the
    /// original layout are cut entirely to buy back rows for the piano at
    /// 80x24).
    const HEADER_SPACER_ROWS: u16 = 1;
    /// [`Piano`] itself refuses to render below this height.
    const PIANO_ROWS: u16 = 4;
    /// Instructions capped at ~4 rows (issue #32 deferred item), instead
    /// of the `Min(6)` that used to let prose soak up the meter's space.
    const INSTRUCTIONS_ROWS: u16 = 4;
    const HELP_ROWS: u16 = 2;
    /// Rows [`Meter`] needs to show its full tick/needle/cents display;
    /// below this it falls back to a compact one-liner rather than a
    /// blank area, so starving it below this floor is a last resort, not
    /// a first one.
    const MIN_FULL_METER_ROWS: u16 = 5;

    /// Combinations to try, most generous first. Each is a strict subset
    /// of the previous one (never re-adds something an earlier, more
    /// generous combination dropped), which is what makes this a
    /// monotonic staircase rather than an arbitrary search.
    const COMBOS: [(bool, bool, bool, bool); 5] = [
        // (piano, glyph, instructions, help)
        (true, true, true, true),
        (false, true, true, true),
        (false, false, true, true),
        (false, false, false, true),
        (false, false, false, false),
    ];

    /// Decide which optional elements fit in `height` inner rows: the
    /// most generous combination that still leaves the meter its full
    /// height, or the leanest combination if none does.
    fn compute(height: u16) -> Self {
        for (piano, glyph, instructions, help) in Self::COMBOS {
            let fixed = Self::fixed_rows(piano, glyph, instructions, help);
            if height >= fixed.saturating_add(Self::MIN_FULL_METER_ROWS) {
                return Self {
                    show_piano: piano,
                    show_glyph: glyph,
                    show_instructions: instructions,
                    show_help: help,
                };
            }
        }
        let (piano, glyph, instructions, help) = Self::COMBOS[Self::COMBOS.len() - 1];
        Self {
            show_piano: piano,
            show_glyph: glyph,
            show_instructions: instructions,
            show_help: help,
        }
    }

    /// Total fixed (non-meter) rows this combination costs.
    fn fixed_rows(piano: bool, glyph: bool, instructions: bool, help: bool) -> u16 {
        let header = if glyph {
            Self::GLYPH_HEADER_ROWS
        } else {
            Self::PLAIN_HEADER_ROWS
        };
        let spacer = if piano { Self::HEADER_SPACER_ROWS } else { 0 };
        let piano_rows = if piano { Self::PIANO_ROWS } else { 0 };
        let instructions_rows = if instructions {
            Self::INSTRUCTIONS_ROWS
        } else {
            0
        };
        let help_rows = if help { Self::HELP_ROWS } else { 0 };
        header + spacer + piano_rows + instructions_rows + help_rows
    }
}

/// Resolved screen regions for a [`TuningLayoutPlan`], the actual
/// [`Layout`] split against `inner`. Kept separate from `TuningLayoutPlan`
/// so the plan's drop-order decision and the constraint resolution (issue
/// #32's flexible meter height) are each independently testable.
struct TuningAreas {
    header: Rect,
    piano: Option<Rect>,
    instructions: Option<Rect>,
    meter: Rect,
    help: Option<Rect>,
}

fn layout_areas(inner: Rect, plan: TuningLayoutPlan) -> TuningAreas {
    let mut constraints = Vec::with_capacity(6);
    constraints.push(Constraint::Length(if plan.show_glyph {
        TuningLayoutPlan::GLYPH_HEADER_ROWS
    } else {
        TuningLayoutPlan::PLAIN_HEADER_ROWS
    }));
    if plan.show_piano {
        constraints.push(Constraint::Length(TuningLayoutPlan::HEADER_SPACER_ROWS));
        constraints.push(Constraint::Length(TuningLayoutPlan::PIANO_ROWS));
    }
    if plan.show_instructions {
        constraints.push(Constraint::Length(TuningLayoutPlan::INSTRUCTIONS_ROWS));
    }
    // The meter gets whatever's left (issue #32 deferred item): a `Min`
    // constraint instead of a fixed `Length`, so extra vertical space
    // flows to it rather than to the instructions prose.
    constraints.push(Constraint::Min(0));
    if plan.show_help {
        constraints.push(Constraint::Length(TuningLayoutPlan::HELP_ROWS));
    }

    let chunks = Layout::vertical(constraints).split(inner);

    let mut idx = 0;
    let header = chunks[idx];
    idx += 1;
    let piano = if plan.show_piano {
        idx += 1; // skip the spacer
        let rect = chunks[idx];
        idx += 1;
        Some(rect)
    } else {
        None
    };
    let instructions = if plan.show_instructions {
        let rect = chunks[idx];
        idx += 1;
        Some(rect)
    } else {
        None
    };
    let meter = chunks[idx];
    idx += 1;
    let help = if plan.show_help {
        Some(chunks[idx])
    } else {
        None
    };

    TuningAreas {
        header,
        piano,
        instructions,
        meter,
        help,
    }
}

/// Minimum inner width/height below which nothing coherent fits even the
/// leanest [`TuningLayoutPlan`] (a [`Progress`] header needs 20 columns to
/// draw anything at all) - the only remaining hard wall, much smaller than
/// the old 40x19 one (issue #31).
const MIN_USABLE_WIDTH: u16 = 20;
const MIN_USABLE_HEIGHT: u16 = 3;

/// Offset that centers `content` units within `container` units, clamped
/// to zero rather than underflowing when the content doesn't fit (issue
/// #31 item 4: no more raw `x + width/2 - len/2` arithmetic).
fn centered_start(container: u16, content: u16) -> u16 {
    container.saturating_sub(content) / 2
}

/// Render a centered "too small" message, with the required and current
/// dimensions (issue #31 item 3), instead of a bare left-aligned string.
fn render_too_small_message(inner: Rect, buf: &mut Buffer) {
    let lines = [
        "Terminal too small".to_string(),
        format!(
            "Need at least {MIN_USABLE_WIDTH}x{MIN_USABLE_HEIGHT}, have {}x{}",
            inner.width, inner.height
        ),
    ];

    let start_y = inner.y + centered_start(inner.height, lines.len() as u16);
    for (i, line) in lines.iter().enumerate() {
        let y = start_y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let x = inner.x + centered_start(inner.width, line.len() as u16);
        buf.set_string(x, y, line, Theme::warning());
    }
}

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
    /// Level shown by the muting-step VU indicator (issue #32 deferred
    /// item), `0.0..=1.0`. Fed from pitch-detection confidence while
    /// muting - see [`Self::set_mute_level`].
    mute_level: f32,
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
            mute_level: 0.0,
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
        self.mute_level = 0.0;
    }

    /// Get current cents deviation.
    pub fn cents(&self) -> f32 {
        self.cents_deviation
    }

    /// Set the level shown by the muting-step VU indicator (issue #32
    /// deferred item), clamped to `0.0..=1.0`. Only meaningful (i.e.
    /// rendered) while [`Self::shows_mute_level`] is true.
    pub fn set_mute_level(&mut self, level: f32) {
        self.mute_level = level.clamp(0.0, 1.0);
    }

    /// The muting-step VU indicator's current level.
    pub fn mute_level(&self) -> f32 {
        self.mute_level
    }

    /// Whether the meter area should show the mute-step level/VU indicator
    /// instead of the normal cents meter: true only during a muting step
    /// (there is no pitch *target* to plot while damping strings), false
    /// otherwise - including for monochord notes, which have no steps at
    /// all.
    pub fn shows_mute_level(&self) -> bool {
        self.tuning_step.map(|s| s.is_muting()).unwrap_or(false)
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

        // The only remaining hard wall (issue #31): below this, nothing
        // coherent fits even the leanest layout plan. Everything above it
        // degrades progressively instead - see `TuningLayoutPlan`.
        if inner.width < MIN_USABLE_WIDTH || inner.height < MIN_USABLE_HEIGHT {
            render_too_small_message(inner, buf);
            return;
        }

        let plan = TuningLayoutPlan::compute(inner.height);
        let areas = layout_areas(inner, plan);

        // Header: the current note as a large glyph (issue #32 - it used to
        // be the smallest text on screen, the border title) beside the
        // progress indicator. Dropped first at small heights (`plan`), and
        // also skipped below a width that would otherwise starve `Progress`
        // (which itself needs >=20 columns), even when `plan` kept it.
        let glyph_width = NoteGlyph::new(&self.note_name).width();
        let glyph_style = if self.lock_anim.is_locked() {
            Theme::in_tune().add_modifier(Modifier::BOLD)
        } else {
            Theme::accent()
        };
        let progress_area = if plan.show_glyph && areas.header.width >= glyph_width + 2 + 20 {
            let chunks = Layout::horizontal([
                Constraint::Length(glyph_width),
                Constraint::Length(2), // Gap
                Constraint::Min(20),
            ])
            .split(areas.header);
            NoteGlyph::new(&self.note_name)
                .style(glyph_style)
                .render(chunks[0], buf);
            chunks[2]
        } else {
            areas.header
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
        if let Some(piano_area) = areas.piano {
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
        if let Some(instructions_area) = areas.instructions {
            if let Some(step) = self.tuning_step {
                // Multi-string note (bichord or trichord)
                let instructions = if self.shows_mute_level() {
                    // Don't show direction hints during muting
                    Instructions::for_step(step, self.string_count)
                } else {
                    Instructions::for_step(step, self.string_count)
                        .with_direction_hint(self.cents_deviation, self.tolerance)
                };
                instructions.render(instructions_area, buf);
            } else {
                // Monochord note - simple instruction
                let instructions = Instructions::simple()
                    .with_direction_hint(self.cents_deviation, self.tolerance);
                instructions.render(instructions_area, buf);
            }
        }

        // Meter area: the muting-step level/VU indicator (issue #32
        // deferred item - there's no pitch target to plot while damping
        // strings, but the area shouldn't just sit blank), or the normal
        // cents meter otherwise.
        if self.shows_mute_level() {
            MuteLevel::new(self.mute_level).render(areas.meter, buf);
        } else {
            let meter = if self.detected_freq.is_some() {
                Meter::new(self.cents_deviation)
                    .tolerance(self.tolerance)
                    .smoothed(self.needle_trail.smoothed())
                    .trail(self.needle_trail.trail())
                    .flashing(self.lock_anim.is_flashing())
            } else {
                Meter::listening().tolerance(self.tolerance)
            };
            meter.render(areas.meter, buf);
        }

        // Help text
        if let Some(help_area) = areas.help {
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
    fn test_stock_80x24_shows_everything_including_the_piano() {
        // Issue #31: a stock 80x24 terminal (inner 78x22) used to drop the
        // piano entirely (the old full layout needed >=28 rows). The
        // tightened layout (capped instructions, flexible meter, single
        // header spacer) must now fit piano + glyph + instructions + meter
        // + help all at once at this size.
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 24);

        assert!(!rendered.contains("Terminal too small"));
        assert!(rendered.contains("Listening..."), "meter must be visible");
        assert!(rendered.contains("Confirm"), "help line must be visible");
        assert!(rendered.contains('╚'), "piano must survive at 80x24");
        assert!(
            rendered.contains('█'),
            "note glyph must also survive at 80x24"
        );
    }

    #[test]
    fn test_full_terminal_shows_piano() {
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 110, 30);

        assert!(rendered.contains('╚'), "piano visible");
        assert!(rendered.contains("Listening..."));
    }

    #[test]
    fn test_moderately_small_terminal_degrades_instead_of_a_hard_wall() {
        // Issue #31's flagged regression: 18-20 *inner* rows used to hit
        // the "too small" wall. This is even smaller (inner ~8 rows) and
        // must still render real content, not a message.
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 10);

        assert!(!rendered.contains("Terminal too small"));
        assert!(
            rendered.contains("Listening..."),
            "meter must still be visible when everything else is dropped"
        );
    }

    #[test]
    fn test_extremely_narrow_terminal_still_shows_the_message() {
        // Inner width 19, just below the 20-column wall; wide enough
        // overall (21 cols) that the first line isn't clipped.
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 21, 25);

        assert!(rendered.contains("Terminal too small"));
        assert!(
            rendered.contains("Need at least"),
            "message should state the required dimensions"
        );
    }

    #[test]
    fn test_too_small_message_reports_current_dimensions() {
        // Inner height 2, below the 3-row wall; plenty of width so the
        // message renders without clipping.
        let screen = monochord_screen();
        let rendered = render_to_string(&screen, 80, 4);
        assert!(
            rendered.contains("78x2"),
            "expected the actual inner dimensions in the message:\n{rendered}"
        );
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
            (19, 20), // just under the width gate
            (20, 2),  // just under the height gate
            (20, 3),  // exactly at both gates
            (40, 8),
            (40, 12),
            (40, 16),
            (60, 21),  // just below the full-plan floor
            (78, 22),  // stock 80x24 minus border
            (108, 28), // old full layout threshold
            (200, 60),
        ] {
            let _ = render_to_string(&screen, w, h);
        }
    }

    // -- graceful small-terminal degradation (issue #31) --

    mod layout_plan {
        use super::*;

        #[test]
        fn test_generous_height_shows_everything() {
            let plan = TuningLayoutPlan::compute(40);
            assert_eq!(
                plan,
                TuningLayoutPlan {
                    show_piano: true,
                    show_glyph: true,
                    show_instructions: true,
                    show_help: true,
                }
            );
        }

        #[test]
        fn test_stock_80x24_inner_height_keeps_everything() {
            // 80x24 terminal -> inner height 22 (2 rows for the border).
            // The whole point of issue #31: the piano must survive here.
            let plan = TuningLayoutPlan::compute(22);
            assert!(plan.show_piano, "piano must survive at 80x24");
            assert!(plan.show_glyph);
            assert!(plan.show_instructions);
            assert!(plan.show_help);
        }

        #[test]
        fn test_drops_piano_first() {
            // Enough for glyph+instructions+help+full meter, not for piano too.
            let plan = TuningLayoutPlan::compute(20);
            assert!(!plan.show_piano);
            assert!(plan.show_glyph);
            assert!(plan.show_instructions);
            assert!(plan.show_help);
        }

        #[test]
        fn test_drops_glyph_next() {
            let plan = TuningLayoutPlan::compute(15);
            assert!(!plan.show_piano);
            assert!(!plan.show_glyph);
            assert!(plan.show_instructions);
            assert!(plan.show_help);
        }

        #[test]
        fn test_drops_instructions_next() {
            let plan = TuningLayoutPlan::compute(11);
            assert!(!plan.show_piano);
            assert!(!plan.show_glyph);
            assert!(!plan.show_instructions);
            assert!(plan.show_help);
        }

        #[test]
        fn test_drops_help_last() {
            let plan = TuningLayoutPlan::compute(7);
            assert!(!plan.show_piano);
            assert!(!plan.show_glyph);
            assert!(!plan.show_instructions);
            assert!(!plan.show_help);
        }

        #[test]
        fn test_leanest_plan_used_below_its_own_floor_without_panicking() {
            // Height 0: even the leanest combination needs more than this,
            // but `compute` must still return *something* rather than panic.
            let plan = TuningLayoutPlan::compute(0);
            assert_eq!(
                plan,
                TuningLayoutPlan {
                    show_piano: false,
                    show_glyph: false,
                    show_instructions: false,
                    show_help: false,
                }
            );
        }

        #[test]
        fn test_dropping_is_monotonic_as_height_shrinks() {
            // Once something is dropped at some height, it must stay dropped
            // at every smaller height too (no flickering back in).
            let heights = [40, 22, 21, 20, 19, 16, 15, 12, 11, 8, 7, 6, 3, 0];
            let mut prev = TuningLayoutPlan::compute(heights[0]);
            for &h in &heights[1..] {
                let plan = TuningLayoutPlan::compute(h);
                assert!(
                    !plan.show_piano || prev.show_piano,
                    "piano reappeared at h={h}"
                );
                assert!(
                    !plan.show_glyph || prev.show_glyph,
                    "glyph reappeared at h={h}"
                );
                assert!(
                    !plan.show_instructions || prev.show_instructions,
                    "instructions reappeared at h={h}"
                );
                assert!(
                    !plan.show_help || prev.show_help,
                    "help reappeared at h={h}"
                );
                prev = plan;
            }
        }
    }

    mod layout_areas_tests {
        use super::*;

        #[test]
        fn test_meter_absorbs_extra_space_beyond_the_fixed_elements() {
            // Issue #32 deferred item: the meter gets a flexible `Min`
            // constraint instead of a fixed height, so extra room grows it
            // rather than the instructions panel.
            let plan = TuningLayoutPlan {
                show_piano: false,
                show_glyph: true,
                show_instructions: true,
                show_help: true,
            };
            let small = layout_areas(Rect::new(0, 0, 40, 16), plan);
            let large = layout_areas(Rect::new(0, 0, 40, 26), plan);

            assert_eq!(small.header.height, 5);
            assert_eq!(small.instructions.unwrap().height, 4);
            assert_eq!(large.header.height, 5, "header stays fixed");
            assert_eq!(
                large.instructions.unwrap().height,
                4,
                "instructions stay capped, not absorbing the extra space"
            );
            assert!(
                large.meter.height > small.meter.height,
                "the meter should grow with the extra space: {} vs {}",
                large.meter.height,
                small.meter.height
            );
        }

        #[test]
        fn test_dropped_elements_have_no_area() {
            let plan = TuningLayoutPlan {
                show_piano: false,
                show_glyph: false,
                show_instructions: false,
                show_help: false,
            };
            let areas = layout_areas(Rect::new(0, 0, 40, 10), plan);
            assert!(areas.piano.is_none());
            assert!(areas.instructions.is_none());
            assert!(areas.help.is_none());
            assert_eq!(
                areas.header.height, 1,
                "plain 1-row header without the glyph"
            );
        }

        #[test]
        fn test_piano_area_gets_its_required_four_rows() {
            let plan = TuningLayoutPlan {
                show_piano: true,
                show_glyph: true,
                show_instructions: true,
                show_help: true,
            };
            let areas = layout_areas(Rect::new(0, 0, 40, 22), plan);
            assert_eq!(areas.piano.unwrap().height, 4);
        }
    }

    mod too_small_message {
        use super::*;

        #[test]
        fn test_centered_start_centers_when_content_fits() {
            assert_eq!(centered_start(80, 20), 30);
        }

        #[test]
        fn test_centered_start_saturates_instead_of_underflowing() {
            // Content wider than the container must not panic or wrap
            // around via unsigned underflow.
            assert_eq!(centered_start(5, 20), 0);
        }

        #[test]
        fn test_centered_start_zero_container_does_not_panic() {
            assert_eq!(centered_start(0, 0), 0);
        }
    }

    // -- muting-step level/VU indicator (issue #32 deferred item) --

    fn bichord_screen() -> TuningScreen {
        // D#2, 2 strings: starts at the MuteBichord step.
        TuningScreen::new("D#2", 10, 88, 77.8, 2, 39)
    }

    #[test]
    fn test_shows_mute_level_true_during_muting_step() {
        let screen = bichord_screen();
        assert_eq!(screen.tuning_step(), Some(TuningStep::MuteBichord));
        assert!(screen.shows_mute_level());
    }

    #[test]
    fn test_shows_mute_level_false_once_past_the_muting_step() {
        let mut screen = bichord_screen();
        assert!(screen.next_step(), "must advance to TuneBichord");
        assert_eq!(screen.tuning_step(), Some(TuningStep::TuneBichord));
        assert!(!screen.shows_mute_level());
    }

    #[test]
    fn test_shows_mute_level_false_for_monochord_notes() {
        let screen = monochord_screen();
        assert_eq!(screen.tuning_step(), None);
        assert!(!screen.shows_mute_level());
    }

    #[test]
    fn test_mute_level_defaults_to_zero() {
        let screen = bichord_screen();
        assert_eq!(screen.mute_level(), 0.0);
    }

    #[test]
    fn test_set_mute_level_clamps_to_unit_range() {
        let mut screen = bichord_screen();
        screen.set_mute_level(1.5);
        assert_eq!(screen.mute_level(), 1.0);
        screen.set_mute_level(-0.5);
        assert_eq!(screen.mute_level(), 0.0);
        screen.set_mute_level(0.42);
        assert_eq!(screen.mute_level(), 0.42);
    }

    #[test]
    fn test_clear_resets_mute_level() {
        let mut screen = bichord_screen();
        screen.set_mute_level(0.8);
        screen.clear();
        assert_eq!(screen.mute_level(), 0.0);
    }

    #[test]
    fn test_muting_step_renders_level_indicator_not_a_blank_meter() {
        let mut screen = bichord_screen();
        screen.set_mute_level(0.7);
        let rendered = render_to_string(&screen, 80, 24);

        assert!(
            !rendered.contains("Listening..."),
            "the normal meter must not render during a muting step"
        );
        assert!(
            rendered.contains("Muting"),
            "expected the level indicator's label in the meter area, got:\n{rendered}"
        );
    }

    #[test]
    fn test_tune_step_renders_normal_meter_not_the_level_indicator() {
        let mut screen = bichord_screen();
        screen.next_step(); // TuneBichord: no longer muting
        let rendered = render_to_string(&screen, 80, 24);

        assert!(rendered.contains("Listening..."), "normal meter expected");
    }
}
