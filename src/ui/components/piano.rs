//! Reusable ASCII piano keyboard visualization.
//!
//! # Pattern
//!
//! Each octave follows the pattern `EWBWBWEWBWBWBWE` where:
//! - E = Edge (║)
//! - W = White key (space off, ▓ on)
//! - B = Black key (▒ off, █ on)
//!
//! ```text
//! Key : EWBWBWEWBWBWBWE
//! Note:  C#D#E F#G#A#B
//!         C D   F G A
//! ```

use std::collections::{HashMap, HashSet};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::ui::theme::Theme;

/// Characters for piano rendering.
pub mod chars {
    pub const EDGE: char = '║';
    pub const WHITE_OFF: char = ' ';
    pub const WHITE_ON: char = '▓';
    pub const BLACK_OFF: char = '▒';
    pub const BLACK_ON: char = '█';
    pub const BORDER_WHITE: char = '═';
    pub const BORDER_BLACK: char = '╩';
    pub const CORNER_LEFT: char = '╚';
    pub const CORNER_RIGHT: char = '╝';
}

/// A cell in the piano layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Vertical edge separator (between E-F and B-C, plus boundaries).
    Edge,
    /// White key with index relative to keyboard start.
    White(usize),
    /// Black key with index relative to keyboard start.
    Black(usize),
}

/// Piano keyboard visualization widget.
///
/// Generates a piano of `n` keys starting at any MIDI note.
/// Supports highlighting completed keys and marking the current key.
///
/// # Example
///
/// ```ignore
/// // Full 88-key piano with some highlighted keys
/// let highlighted: HashSet<usize> = [0, 2, 4].into_iter().collect();
/// let piano = Piano::new(21, 88)
///     .highlighted(highlighted)
///     .current(Some(5));
/// ```
pub struct Piano {
    /// Starting MIDI note (21 = A0, 108 = C8).
    start_midi: u8,
    /// Number of keys.
    num_keys: usize,
    /// Set of highlighted key indices (relative to start).
    highlighted: HashSet<usize>,
    /// Per-key deviations in cents (key index -> cents).
    /// Keys with deviations are colored by their deviation value.
    deviations: HashMap<usize, f32>,
    /// Currently active key (shown with accent color).
    current: Option<usize>,
    /// Color for highlighted keys.
    on_color: Color,
    /// Color for current key.
    current_color: Color,
    /// Whether this is a continuing segment (no right corner).
    continuing: bool,
}

impl Piano {
    /// Create a piano starting at `start_midi` with `num_keys` keys.
    pub fn new(start_midi: u8, num_keys: usize) -> Self {
        Self {
            start_midi,
            num_keys,
            highlighted: HashSet::new(),
            deviations: HashMap::new(),
            current: None,
            on_color: Color::Green,
            current_color: Color::Cyan,
            continuing: false,
        }
    }

    /// Create a full 88-key piano (A0 to C8).
    pub fn full() -> Self {
        Self::new(21, 88)
    }

    /// Create a single octave starting at the given MIDI note.
    pub fn octave(start_midi: u8) -> Self {
        Self::new(start_midi, 12)
    }

    /// Set highlighted (completed/on) keys.
    pub fn highlighted(mut self, keys: HashSet<usize>) -> Self {
        self.highlighted = keys;
        self
    }

    /// Set the currently active key.
    pub fn current(mut self, key: Option<usize>) -> Self {
        self.current = key;
        self
    }

    /// Set color for highlighted keys.
    pub fn on_color(mut self, color: Color) -> Self {
        self.on_color = color;
        self
    }

    /// Set color for current key.
    pub fn current_color(mut self, color: Color) -> Self {
        self.current_color = color;
        self
    }

    /// Mark as continuing (no right corner in border).
    pub fn continuing(mut self, cont: bool) -> Self {
        self.continuing = cont;
        self
    }

    /// Set per-key deviations for deviation-based coloring.
    /// Keys with deviations will be colored green/yellow/red based on cents.
    pub fn with_deviations(mut self, deviations: HashMap<usize, f32>) -> Self {
        self.deviations = deviations;
        self
    }

    /// Check if semitone (0-11, where 0=C) is a black key.
    #[inline]
    fn is_black(semitone: u8) -> bool {
        matches!(semitone, 1 | 3 | 6 | 8 | 10)
    }

    /// Check if semitone has an edge after it (E=4 or B=11).
    #[inline]
    fn has_edge_after(semitone: u8) -> bool {
        semitone == 4 || semitone == 11
    }

    /// Build the cell layout for this keyboard.
    pub fn build_cells(&self) -> Vec<Cell> {
        let mut cells = Vec::with_capacity(self.num_keys + self.num_keys / 6 + 2);

        // Leading edge
        cells.push(Cell::Edge);

        for i in 0..self.num_keys {
            let midi = self.start_midi + i as u8;
            let semitone = midi % 12;

            if Self::is_black(semitone) {
                cells.push(Cell::Black(i));
            } else {
                cells.push(Cell::White(i));
            }

            // Add edge after E (4) or B (11) if more keys follow
            if i + 1 < self.num_keys && Self::has_edge_after(semitone) {
                cells.push(Cell::Edge);
            }
        }

        // Trailing edge
        cells.push(Cell::Edge);

        cells
    }

    /// Calculate the width needed for this keyboard.
    pub fn width(&self) -> usize {
        self.build_cells().len()
    }

    /// Get the starting MIDI note.
    pub fn start_midi(&self) -> u8 {
        self.start_midi
    }

    /// Get the number of keys.
    pub fn num_keys(&self) -> usize {
        self.num_keys
    }

    /// Convert a MIDI note to a relative key index.
    /// Returns None if the MIDI note is outside this keyboard's range.
    pub fn midi_to_index(&self, midi: u8) -> Option<usize> {
        if midi >= self.start_midi && (midi - self.start_midi) < self.num_keys as u8 {
            Some((midi - self.start_midi) as usize)
        } else {
            None
        }
    }

    /// Render to a vector of strings (for testing/debugging).
    pub fn render_to_strings(&self) -> Vec<String> {
        let cells = self.build_cells();
        (0..4)
            .map(|row| self.render_row_to_string(&cells, row))
            .collect()
    }

    /// Render a single row to a string.
    fn render_row_to_string(&self, cells: &[Cell], row: usize) -> String {
        let mut result = String::with_capacity(cells.len());

        for (col, cell) in cells.iter().enumerate() {
            let ch = match (cell, row) {
                // Rows 0-1: Top rows
                (Cell::Edge, 0..=1) => chars::EDGE,
                (Cell::Black(i), 0..=1) => {
                    if self.is_on(*i) {
                        chars::BLACK_ON
                    } else {
                        chars::BLACK_OFF
                    }
                }
                (Cell::White(i), 0..=1) => {
                    if self.is_on(*i) {
                        chars::WHITE_ON
                    } else {
                        chars::WHITE_OFF
                    }
                }

                // Row 2: Bottom row
                (Cell::Edge, 2) => chars::EDGE,
                (Cell::Black(_), 2) => chars::EDGE,
                (Cell::White(i), 2) => {
                    if self.is_on(*i) {
                        chars::WHITE_ON
                    } else {
                        chars::WHITE_OFF
                    }
                }

                // Row 3: Border
                (cell, 3) => {
                    if col == 0 {
                        chars::CORNER_LEFT
                    } else if col == cells.len() - 1 {
                        if self.continuing {
                            chars::BORDER_BLACK
                        } else {
                            chars::CORNER_RIGHT
                        }
                    } else {
                        match cell {
                            Cell::Edge | Cell::Black(_) => chars::BORDER_BLACK,
                            Cell::White(_) => chars::BORDER_WHITE,
                        }
                    }
                }

                _ => ' ',
            };
            result.push(ch);
        }

        result
    }

    /// Check if a key is "on" (highlighted, has deviation, or current).
    fn is_on(&self, index: usize) -> bool {
        self.highlighted.contains(&index)
            || self.deviations.contains_key(&index)
            || self.current == Some(index)
    }

    /// Get style for a key.
    fn key_style(&self, index: usize) -> Style {
        if self.current == Some(index) {
            Style::default().fg(self.current_color)
        } else if let Some(&cents) = self.deviations.get(&index) {
            // Color by deviation: green (in-tune), yellow (warning), red (out-of-tune)
            Style::default().fg(Theme::color_for_cents(cents))
        } else if self.highlighted.contains(&index) {
            Style::default().fg(self.on_color)
        } else {
            Style::default()
        }
    }

    /// Render a single row.
    fn render_row(&self, cells: &[Cell], row: usize, area: Rect, buf: &mut Buffer) {
        let y = area.y + row as u16;
        if y >= area.y + area.height {
            return;
        }

        let display_width = cells.len().min(area.width as usize);

        for (col, cell) in cells.iter().take(display_width).enumerate() {
            let x = area.x + col as u16;

            let (ch, style) = match (cell, row) {
                // Rows 0-1: Top rows (all keys visible)
                (Cell::Edge, 0..=1) => (chars::EDGE, Style::default()),
                (Cell::Black(i), 0..=1) => {
                    let ch = if self.is_on(*i) {
                        chars::BLACK_ON
                    } else {
                        chars::BLACK_OFF
                    };
                    (ch, self.key_style(*i))
                }
                (Cell::White(i), 0..=1) => {
                    let ch = if self.is_on(*i) {
                        chars::WHITE_ON
                    } else {
                        chars::WHITE_OFF
                    };
                    (ch, self.key_style(*i))
                }

                // Row 2: Bottom row (black keys become edges)
                (Cell::Edge, 2) => (chars::EDGE, Style::default()),
                (Cell::Black(_), 2) => (chars::EDGE, Style::default()),
                (Cell::White(i), 2) => {
                    let ch = if self.is_on(*i) {
                        chars::WHITE_ON
                    } else {
                        chars::WHITE_OFF
                    };
                    (ch, self.key_style(*i))
                }

                // Row 3: Border
                (cell, 3) => {
                    let ch = if col == 0 {
                        chars::CORNER_LEFT
                    } else if col == display_width - 1 {
                        if self.continuing {
                            chars::BORDER_BLACK // ╩ for continuation
                        } else {
                            chars::CORNER_RIGHT // ╝ for end
                        }
                    } else {
                        match cell {
                            Cell::Edge | Cell::Black(_) => chars::BORDER_BLACK,
                            Cell::White(_) => chars::BORDER_WHITE,
                        }
                    };
                    (ch, Style::default())
                }

                _ => (' ', Style::default()),
            };

            buf.set_string(x, y, ch.to_string(), style);
        }
    }
}

impl Widget for Piano {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 3 {
            return;
        }

        let cells = self.build_cells();

        for row in 0..4 {
            self.render_row(&cells, row, area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_one_octave_off() {
        // C4 = MIDI 60, one octave
        let piano = Piano::new(60, 12);
        let rows = piano.render_to_strings();

        // Expected pattern from spec:
        // Key : EWBWBWEWBWBWBWE
        // OFF : ║ ▒ ▒ ║ ▒ ▒ ▒ ║
        assert_eq!(rows[0], "║ ▒ ▒ ║ ▒ ▒ ▒ ║");
        assert_eq!(rows[1], "║ ▒ ▒ ║ ▒ ▒ ▒ ║");
        assert_eq!(rows[2], "║ ║ ║ ║ ║ ║ ║ ║");
        assert_eq!(rows[3], "╚═╩═╩═╩═╩═╩═╩═╝");
    }

    #[test]
    fn test_render_one_octave_all_on() {
        // All keys highlighted
        let highlighted: HashSet<usize> = (0..12).collect();
        let piano = Piano::new(60, 12).highlighted(highlighted);
        let rows = piano.render_to_strings();

        // Expected pattern from spec:
        // ON  : ║▓█▓█▓║▓█▓█▓█▓║
        assert_eq!(rows[0], "║▓█▓█▓║▓█▓█▓█▓║");
        assert_eq!(rows[1], "║▓█▓█▓║▓█▓█▓█▓║");
        assert_eq!(rows[2], "║▓║▓║▓║▓║▓║▓║▓║");
        assert_eq!(rows[3], "╚═╩═╩═╩═╩═╩═╩═╝");
    }

    #[test]
    fn test_render_partial_on() {
        // Only C and E highlighted (indices 0 and 4)
        let highlighted: HashSet<usize> = [0, 4].into_iter().collect();
        let piano = Piano::new(60, 12).highlighted(highlighted);
        let rows = piano.render_to_strings();

        // C=on, C#=off, D=off, D#=off, E=on, ...
        assert_eq!(rows[0], "║▓▒ ▒▓║ ▒ ▒ ▒ ║");
        assert_eq!(rows[2], "║▓║ ║▓║ ║ ║ ║ ║");
    }

    #[test]
    fn test_render_continuing() {
        let piano = Piano::new(60, 12).continuing(true);
        let rows = piano.render_to_strings();

        // Last char should be ╩ not ╝
        assert!(rows[3].ends_with('╩'));
    }

    #[test]
    fn test_midi_to_index() {
        let piano = Piano::new(60, 12); // C4 to B4

        assert_eq!(piano.midi_to_index(60), Some(0)); // C4
        assert_eq!(piano.midi_to_index(71), Some(11)); // B4
        assert_eq!(piano.midi_to_index(59), None); // B3 (before range)
        assert_eq!(piano.midi_to_index(72), None); // C5 (after range)
    }

    #[test]
    fn test_is_black() {
        // C=0, C#=1, D=2, D#=3, E=4, F=5, F#=6, G=7, G#=8, A=9, A#=10, B=11
        assert!(!Piano::is_black(0)); // C
        assert!(Piano::is_black(1)); // C#
        assert!(!Piano::is_black(2)); // D
        assert!(Piano::is_black(3)); // D#
        assert!(!Piano::is_black(4)); // E
        assert!(!Piano::is_black(5)); // F
        assert!(Piano::is_black(6)); // F#
        assert!(!Piano::is_black(7)); // G
        assert!(Piano::is_black(8)); // G#
        assert!(!Piano::is_black(9)); // A
        assert!(Piano::is_black(10)); // A#
        assert!(!Piano::is_black(11)); // B
    }

    #[test]
    fn test_has_edge_after() {
        // Edges after E (4) and B (11)
        assert!(!Piano::has_edge_after(0));
        assert!(!Piano::has_edge_after(3));
        assert!(Piano::has_edge_after(4)); // E
        assert!(!Piano::has_edge_after(5));
        assert!(Piano::has_edge_after(11)); // B
    }

    #[test]
    fn test_one_octave_from_c() {
        // C4 = MIDI 60, one octave = 12 keys
        let piano = Piano::new(60, 12);
        let cells = piano.build_cells();

        // Pattern: E W B W B W E W B W B W B W E
        // That's 15 cells for a complete octave
        assert_eq!(cells.len(), 15);

        assert_eq!(cells[0], Cell::Edge); // Leading edge
        assert_eq!(cells[1], Cell::White(0)); // C
        assert_eq!(cells[2], Cell::Black(1)); // C#
        assert_eq!(cells[3], Cell::White(2)); // D
        assert_eq!(cells[4], Cell::Black(3)); // D#
        assert_eq!(cells[5], Cell::White(4)); // E
        assert_eq!(cells[6], Cell::Edge); // E-F edge
        assert_eq!(cells[7], Cell::White(5)); // F
        assert_eq!(cells[8], Cell::Black(6)); // F#
        assert_eq!(cells[9], Cell::White(7)); // G
        assert_eq!(cells[10], Cell::Black(8)); // G#
        assert_eq!(cells[11], Cell::White(9)); // A
        assert_eq!(cells[12], Cell::Black(10)); // A#
        assert_eq!(cells[13], Cell::White(11)); // B
        assert_eq!(cells[14], Cell::Edge); // Trailing edge
    }

    #[test]
    fn test_partial_octave() {
        // Just C D E (3 white keys, 2 black keys)
        let piano = Piano::new(60, 5); // C, C#, D, D#, E
        let cells = piano.build_cells();

        // E W B W B W E = 7 cells
        assert_eq!(cells.len(), 7);
        assert_eq!(cells[0], Cell::Edge);
        assert_eq!(cells[1], Cell::White(0)); // C
        assert_eq!(cells[2], Cell::Black(1)); // C#
        assert_eq!(cells[3], Cell::White(2)); // D
        assert_eq!(cells[4], Cell::Black(3)); // D#
        assert_eq!(cells[5], Cell::White(4)); // E
        assert_eq!(cells[6], Cell::Edge); // Trailing (no internal edge since F not included)
    }

    #[test]
    fn test_starting_on_a() {
        // A0 = MIDI 21, typical piano start
        let piano = Piano::new(21, 3); // A, A#, B
        let cells = piano.build_cells();

        // E W B W E = 5 cells (edge after B since it's semitone 11)
        // Wait, B is the last key so no internal edge, just trailing
        // Actually: A=9, A#=10, B=11. B has edge_after but B is last key, so no internal edge
        // Result: E W B W E = 5 cells
        assert_eq!(cells.len(), 5);
        assert_eq!(cells[0], Cell::Edge);
        assert_eq!(cells[1], Cell::White(0)); // A (semitone 9)
        assert_eq!(cells[2], Cell::Black(1)); // A# (semitone 10)
        assert_eq!(cells[3], Cell::White(2)); // B (semitone 11)
        assert_eq!(cells[4], Cell::Edge); // Trailing
    }

    #[test]
    fn test_cross_octave() {
        // B to C crossing: B3, C4 = MIDI 59, 60
        let piano = Piano::new(59, 2);
        let cells = piano.build_cells();

        // B (11) has edge after, C follows
        // E W E W E = 5 cells
        assert_eq!(cells.len(), 5);
        assert_eq!(cells[0], Cell::Edge);
        assert_eq!(cells[1], Cell::White(0)); // B
        assert_eq!(cells[2], Cell::Edge); // B-C gap
        assert_eq!(cells[3], Cell::White(1)); // C
        assert_eq!(cells[4], Cell::Edge);
    }
}
