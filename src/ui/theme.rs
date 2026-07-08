//! UI theme constants and colors.
//!
//! [`Theme`] is the single source every component reads colors from - no
//! `Color::` literal should appear outside this module (and `Piano`'s
//! defaults, which are threaded from here). That keeps `NO_COLOR` handling
//! and any future palette change in one place.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

/// Theme color mode.
///
/// [`ThemeMode::Monochrome`] activates when the `NO_COLOR` env var
/// (<https://no-color.org>) is set to a non-empty value: every fg color
/// collapses to the terminal default, while `Modifier` emphasis (bold, etc.)
/// is kept so structure survives without color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    /// Full color palette.
    Color,
    /// No fg colors; only `Modifier` emphasis.
    Monochrome,
}

impl ThemeMode {
    /// Resolve mode from a `NO_COLOR`-shaped value (`None` = unset). Pure -
    /// no env access - so the NO_COLOR contract is testable without touching
    /// process-global state.
    fn from_no_color_value(value: Option<&str>) -> Self {
        match value {
            Some(v) if !v.is_empty() => Self::Monochrome,
            _ => Self::Color,
        }
    }

    /// Detect mode from the real `NO_COLOR` env var.
    fn detect() -> Self {
        Self::from_no_color_value(std::env::var("NO_COLOR").ok().as_deref())
    }
}

/// Color theme for the application.
pub struct Theme;

impl Theme {
    /// In-tune color (within ±5 cents).
    pub const IN_TUNE: Color = Color::Green;
    /// Warning color (±5-15 cents).
    pub const WARNING: Color = Color::Yellow;
    /// Out of tune color (beyond ±15 cents).
    pub const OUT_OF_TUNE: Color = Color::Red;
    /// Border/title color. `Reset` (the terminal's default foreground)
    /// rather than a fixed White, which is invisible on light-background
    /// terminals.
    pub const BORDER: Color = Color::Reset;
    /// Muted/secondary text.
    pub const MUTED: Color = Color::DarkGray;
    /// Accent color.
    pub const ACCENT: Color = Color::Cyan;
    /// Background color.
    pub const BG: Color = Color::Reset;
    /// Selected item color.
    pub const SELECTED: Color = Color::Cyan;

    /// Active theme mode for this process, detected once from `NO_COLOR`.
    fn mode() -> ThemeMode {
        static MODE: OnceLock<ThemeMode> = OnceLock::new();
        *MODE.get_or_init(ThemeMode::detect)
    }

    /// Single source every color accessor below routes through: in
    /// [`ThemeMode::Monochrome`] any fg color collapses to
    /// [`Color::Reset`], so `NO_COLOR` truly emits no color.
    fn resolve(color: Color, mode: ThemeMode) -> Color {
        match mode {
            ThemeMode::Color => color,
            ThemeMode::Monochrome => Color::Reset,
        }
    }

    /// Resolve `color` for the active [`ThemeMode`]. Used by every style
    /// accessor here and by `Piano`'s color defaults, which are threaded
    /// from `Theme` rather than hardcoded.
    pub(crate) fn resolved(color: Color) -> Color {
        Self::resolve(color, Self::mode())
    }

    /// Style for in-tune indicator.
    pub fn in_tune() -> Style {
        Style::default().fg(Self::resolved(Self::IN_TUNE))
    }

    /// Style for warning indicator.
    pub fn warning() -> Style {
        Style::default().fg(Self::resolved(Self::WARNING))
    }

    /// Style for out-of-tune indicator.
    pub fn out_of_tune() -> Style {
        Style::default().fg(Self::resolved(Self::OUT_OF_TUNE))
    }

    /// Style for border.
    pub fn border() -> Style {
        Style::default().fg(Self::resolved(Self::BORDER))
    }

    /// Style for muted text.
    pub fn muted() -> Style {
        Style::default().fg(Self::resolved(Self::MUTED))
    }

    /// Style for accent text.
    pub fn accent() -> Style {
        Style::default().fg(Self::resolved(Self::ACCENT))
    }

    /// Style for selected item.
    pub fn selected() -> Style {
        Style::default()
            .fg(Self::resolved(Self::SELECTED))
            .add_modifier(Modifier::BOLD)
    }

    /// Style for title.
    pub fn title() -> Style {
        Style::default()
            .fg(Self::resolved(Self::BORDER))
            .add_modifier(Modifier::BOLD)
    }

    /// Get color based on cents deviation.
    pub fn color_for_cents(cents: f32) -> Color {
        let abs_cents = cents.abs();
        let raw = if abs_cents <= 5.0 {
            Self::IN_TUNE
        } else if abs_cents <= 15.0 {
            Self::WARNING
        } else {
            Self::OUT_OF_TUNE
        };
        Self::resolved(raw)
    }

    /// Get style based on cents deviation.
    pub fn style_for_cents(cents: f32) -> Style {
        Style::default().fg(Self::color_for_cents(cents))
    }
}

/// Box-drawing characters for the meter.
pub struct BoxChars;

impl BoxChars {
    /// Vertical bar characters for different fill levels (1/8 to 8/8).
    pub const BLOCKS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    /// Thin vertical line.
    pub const THIN_VERTICAL: char = '┊';
    /// Thick vertical line (center).
    pub const THICK_VERTICAL: char = '┃';
    /// Flat symbol.
    pub const FLAT: char = '♭';
    /// Sharp symbol.
    pub const SHARP: char = '♯';
    /// Left arrow.
    pub const LEFT_ARROW: char = '◀';
    /// Right arrow.
    pub const RIGHT_ARROW: char = '▶';

    /// Get block character for fill level (0.0 to 1.0).
    pub fn block_for_fill(fill: f32) -> char {
        let fill = fill.clamp(0.0, 1.0);
        let index = ((fill * 8.0) as usize).min(7);
        Self::BLOCKS[index]
    }
}

/// Keyboard shortcut hints.
pub struct Shortcuts;

impl Shortcuts {
    /// Space key hint.
    pub const SPACE: &'static str = "[Space]";
    /// S key hint.
    pub const SKIP: &'static str = "[S]";
    /// Q key hint.
    pub const QUIT: &'static str = "[Q]";
    /// B key hint.
    pub const BACK: &'static str = "[B]";
    /// P key hint.
    pub const PIANO: &'static str = "[P]";
    /// Enter key hint.
    pub const ENTER: &'static str = "[Enter]";
    /// Up/Down arrows hint.
    pub const ARROWS: &'static str = "[↑/↓]";

    /// Format a shortcut with its action.
    pub fn format(key: &str, action: &str) -> String {
        format!("{} {}", key, action)
    }
}

#[cfg(test)]
mod theme_mode_tests {
    use super::*;

    // NOTE: these test the pure `from_no_color_value` / `resolve` functions
    // directly rather than mutating the real `NO_COLOR` env var. `Theme`
    // caches its detected mode process-wide in a `OnceLock`, and `cargo
    // test` runs tests in threads within one process - a test that called
    // `std::env::set_var` would race every other test that reads
    // `Theme::mode()` (order-dependent, so flaky in CI). Testing the pure
    // logic exercises the exact same NO_COLOR contract deterministically.

    #[test]
    fn mode_is_color_when_no_color_is_absent_or_empty() {
        assert_eq!(ThemeMode::from_no_color_value(None), ThemeMode::Color);
        assert_eq!(ThemeMode::from_no_color_value(Some("")), ThemeMode::Color);
    }

    #[test]
    fn mode_is_monochrome_for_any_non_empty_no_color_value() {
        // no-color.org: presence (not content) of the value disables color.
        for value in ["1", "true", "0", "anything"] {
            assert_eq!(
                ThemeMode::from_no_color_value(Some(value)),
                ThemeMode::Monochrome,
                "NO_COLOR={value:?} should select Monochrome"
            );
        }
    }

    #[test]
    fn no_color_set_resolves_style_to_no_color() {
        // Given NO_COLOR set -> style resolves to no color.
        let mode = ThemeMode::from_no_color_value(Some("1"));
        assert_eq!(Theme::resolve(Theme::IN_TUNE, mode), Color::Reset);
    }

    #[test]
    fn resolver_passes_representative_colors_through_in_color_mode() {
        for color in [
            Theme::IN_TUNE,
            Theme::WARNING,
            Theme::OUT_OF_TUNE,
            Theme::ACCENT,
            Theme::MUTED,
            Theme::SELECTED,
        ] {
            assert_eq!(Theme::resolve(color, ThemeMode::Color), color);
        }
    }

    #[test]
    fn resolver_collapses_representative_colors_in_monochrome_mode() {
        for color in [
            Theme::IN_TUNE,
            Theme::WARNING,
            Theme::OUT_OF_TUNE,
            Theme::ACCENT,
            Theme::MUTED,
            Theme::SELECTED,
        ] {
            assert_eq!(Theme::resolve(color, ThemeMode::Monochrome), Color::Reset);
        }
    }

    #[test]
    fn border_and_title_default_to_reset_for_light_terminal_visibility() {
        assert_eq!(Theme::BORDER, Color::Reset);
    }
}
