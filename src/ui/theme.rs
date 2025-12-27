//! UI theme constants and colors.

use ratatui::style::Color;

/// Color theme for the application.
pub struct Theme;

impl Theme {
    /// In-tune color (within tolerance).
    pub const IN_TUNE: Color = Color::Green;
    /// Warning color (slightly out of tune).
    pub const WARNING: Color = Color::Yellow;
    /// Out of tune color.
    pub const OUT_OF_TUNE: Color = Color::Red;
    /// Border color.
    pub const BORDER: Color = Color::White;
    /// Muted/secondary text.
    pub const MUTED: Color = Color::DarkGray;
    /// Accent color.
    pub const ACCENT: Color = Color::Cyan;
}

/// Box-drawing characters for the meter.
pub struct BoxChars;

impl BoxChars {
    /// Vertical bar characters for different fill levels.
    pub const BLOCKS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    /// Thin vertical line.
    pub const THIN_VERTICAL: char = '┊';
    /// Thick vertical line (center).
    pub const THICK_VERTICAL: char = '┃';
}
