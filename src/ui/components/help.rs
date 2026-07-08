//! '?' help overlay: a centered modal listing keybindings, grouped by
//! screen. Self-contained state machine + rendering; `App` only forwards
//! keys to it and asks whether it's open.

use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::ui::theme::Theme;

/// Overlay open/closed state. `?` toggles it open from anywhere; while
/// open, every key closes it and is consumed (must not leak to the screen
/// underneath).
#[derive(Debug, Default)]
pub struct HelpOverlay {
    open: bool,
}

impl HelpOverlay {
    /// Create a new, closed overlay.
    pub fn new() -> Self {
        Self { open: false }
    }

    /// Whether the overlay is currently shown.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Feed a keypress to the overlay. Returns `true` if the overlay
    /// consumed it, in which case the caller must not forward the key
    /// anywhere else.
    ///
    /// - closed + `?` -> opens, consumed.
    /// - open + anything (including `?` or `Esc`) -> closes, consumed. This
    ///   is what keeps a dismissing keystroke like `s` or `q` from also
    ///   skipping a note or quitting the screen underneath.
    /// - closed + anything else -> not consumed, falls through to the
    ///   caller's normal key handling.
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        if self.open {
            self.open = false;
            true
        } else if key == KeyCode::Char('?') {
            self.open = true;
            true
        } else {
            false
        }
    }
}

/// One row in a keybinding group: (key hint, action).
const GLOBAL: &[(&str, &str)] = &[("?", "Toggle this help"), ("any key", "Close this help")];

const MODE_SELECT: &[(&str, &str)] = &[
    ("↑/↓", "Navigate modes"),
    ("Tab", "Same as ↓ (hidden, but pressable)"),
    ("Enter", "Start session"),
    ("Q / Esc", "Quit"),
];

const CALIBRATION: &[(&str, &str)] = &[
    ("S", "Skip calibration (use configured A4)"),
    ("Q / Esc", "Quit"),
];

const PROFILING: &[(&str, &str)] = &[
    ("Space", "Confirm note"),
    ("B", "Back"),
    ("S", "Skip note"),
    ("Q / Esc", "Quit"),
];

const TUNING: &[(&str, &str)] = &[
    ("Space", "Confirm note/step"),
    ("B", "Back"),
    ("P", "Toggle piano progress"),
    ("S", "Skip note"),
    ("Q / Esc", "Quit (autosaves session)"),
];

const COMPLETE: &[(&str, &str)] = &[("Enter", "New session"), ("Q / Esc", "Quit")];

/// Left/right column split so the whole reference fits a stock 80x24
/// terminal without scrolling (a single column of every group runs to ~25
/// lines - taller than the screen).
const LEFT_COLUMN: &[(&str, &[(&str, &str)])] = &[
    ("Global", GLOBAL),
    ("Mode Select", MODE_SELECT),
    ("Calibration", CALIBRATION),
];
const RIGHT_COLUMN: &[(&str, &[(&str, &str)])] = &[
    ("Profiling", PROFILING),
    ("Tuning", TUNING),
    ("Complete", COMPLETE),
];

/// CLI flags aren't keybindings, but the issue calls out that they belong
/// on the same reference surface as the mode-dependent keys.
const CLI_FLAGS: &str = "CLI flags (see --help): --a4 <hz>, --beep, --quick, --resume. \
    Subcommands: analyze, reference, history, reset.";

impl Widget for &HelpOverlay {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.open {
            return;
        }

        let popup = centered_rect(area, 76, 20);
        Clear.render(popup, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(" Help ")
            .title_style(Theme::title());
        let inner = block.inner(popup);
        block.render(popup, buf);

        if inner.height < 4 || inner.width < 20 {
            return;
        }

        let [columns_area, footer_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(columns_area);

        render_column(LEFT_COLUMN, left_area, buf);
        render_column(RIGHT_COLUMN, right_area, buf);

        Paragraph::new(Line::from(Span::styled(CLI_FLAGS, Theme::muted())))
            .wrap(Wrap { trim: false })
            .render(footer_area, buf);
    }
}

/// Render one column's worth of keybinding groups (heading + its bindings,
/// stacked), word-wrapping within `area`.
fn render_column(groups: &[(&str, &[(&str, &str)])], area: Rect, buf: &mut Buffer) {
    let mut lines: Vec<Line> = Vec::new();
    for (heading, bindings) in groups {
        lines.push(Line::from(Span::styled(*heading, Theme::accent())));
        for (key, action) in *bindings {
            lines.push(Line::from(format!("  {:<8}{}", key, action)));
        }
    }
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buf);
}

/// A rect of `width` columns by `height` rows, centered in `area` and
/// clamped so it never exceeds it (small terminals just get the whole
/// area).
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render smoke check (not TDD - pure draw calls aren't test-first
    /// here, only the state machine above is). Confirms the groups and the
    /// hidden Tab/CLI pointers the issue calls out actually reach the
    /// buffer.
    fn render_to_string(overlay: &HelpOverlay, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);
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
    fn test_closed_overlay_renders_nothing() {
        let overlay = HelpOverlay::new();
        let rendered = render_to_string(&overlay, 80, 24);
        assert!(rendered.trim().is_empty());
    }

    #[test]
    fn test_open_overlay_renders_groups_and_hidden_bindings() {
        let mut overlay = HelpOverlay::new();
        overlay.handle_key(KeyCode::Char('?'));
        let rendered = render_to_string(&overlay, 80, 24);

        assert!(rendered.contains("Help"));
        assert!(rendered.contains("Mode Select"));
        assert!(rendered.contains("Tuning"));
        assert!(
            rendered.contains("Tab"),
            "hidden Tab binding must be documented"
        );
        assert!(
            rendered.contains("CLI flags"),
            "CLI-flag pointers must be documented"
        );
    }

    #[test]
    fn test_closed_ignores_non_toggle_keys() {
        let mut overlay = HelpOverlay::new();
        assert!(!overlay.handle_key(KeyCode::Char('s')));
        assert!(!overlay.is_open());
    }

    #[test]
    fn test_question_mark_opens_when_closed() {
        let mut overlay = HelpOverlay::new();
        assert!(overlay.handle_key(KeyCode::Char('?')));
        assert!(overlay.is_open());
    }

    #[test]
    fn test_any_key_closes_when_open() {
        let mut overlay = HelpOverlay::new();
        overlay.handle_key(KeyCode::Char('?'));
        assert!(overlay.is_open());

        assert!(
            overlay.handle_key(KeyCode::Char('x')),
            "key must be consumed, not leaked to the screen underneath"
        );
        assert!(!overlay.is_open());
    }

    #[test]
    fn test_esc_closes_when_open() {
        let mut overlay = HelpOverlay::new();
        overlay.handle_key(KeyCode::Char('?'));

        assert!(overlay.handle_key(KeyCode::Esc));
        assert!(!overlay.is_open());
    }

    #[test]
    fn test_question_mark_again_toggles_closed() {
        let mut overlay = HelpOverlay::new();
        overlay.handle_key(KeyCode::Char('?'));

        assert!(overlay.handle_key(KeyCode::Char('?')));
        assert!(!overlay.is_open());
    }

    #[test]
    fn test_every_key_while_open_is_consumed() {
        // Input must not leak to the tuning screen while the overlay is
        // shown, regardless of which key closes it.
        for key in [
            KeyCode::Char('s'),
            KeyCode::Char('q'),
            KeyCode::Char('p'),
            KeyCode::Char(' '),
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::Esc,
        ] {
            let mut overlay = HelpOverlay::new();
            overlay.handle_key(KeyCode::Char('?'));
            assert!(overlay.handle_key(key), "{:?} must be consumed", key);
            assert!(!overlay.is_open());
        }
    }
}
