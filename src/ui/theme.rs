//! UI theme constants and colors.

use ratatui::style::{Color, Modifier, Style};

/// Color theme for the application.
pub struct Theme;

impl Theme {
    /// In-tune color (within ±5 cents).
    pub const IN_TUNE: Color = Color::Green;
    /// Warning color (±5-15 cents).
    pub const WARNING: Color = Color::Yellow;
    /// Out of tune color (beyond ±15 cents).
    pub const OUT_OF_TUNE: Color = Color::Red;
    /// Border color.
    pub const BORDER: Color = Color::White;
    /// Muted/secondary text.
    pub const MUTED: Color = Color::DarkGray;
    /// Accent color.
    pub const ACCENT: Color = Color::Cyan;
    /// Background color.
    pub const BG: Color = Color::Reset;
    /// Selected item color.
    pub const SELECTED: Color = Color::Cyan;

    /// Style for in-tune indicator.
    pub fn in_tune() -> Style {
        Style::default().fg(Self::IN_TUNE)
    }

    /// Style for warning indicator.
    pub fn warning() -> Style {
        Style::default().fg(Self::WARNING)
    }

    /// Style for out-of-tune indicator.
    pub fn out_of_tune() -> Style {
        Style::default().fg(Self::OUT_OF_TUNE)
    }

    /// Style for border.
    pub fn border() -> Style {
        Style::default().fg(Self::BORDER)
    }

    /// Style for muted text.
    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }

    /// Style for accent text.
    pub fn accent() -> Style {
        Style::default().fg(Self::ACCENT)
    }

    /// Style for selected item.
    pub fn selected() -> Style {
        Style::default()
            .fg(Self::SELECTED)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for title.
    pub fn title() -> Style {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    /// Get color based on cents deviation.
    pub fn color_for_cents(cents: f32) -> Color {
        let abs_cents = cents.abs();
        if abs_cents <= 5.0 {
            Self::IN_TUNE
        } else if abs_cents <= 15.0 {
            Self::WARNING
        } else {
            Self::OUT_OF_TUNE
        }
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
