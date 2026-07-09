//! Large multi-row note glyph - the hero visual for the tuning screen
//! (issue #32).
//!
//! The note being tuned - the thing a tuner glances at constantly - used to
//! be the smallest text on screen (the border title). This renders it as a
//! blocky 5-row glyph instead, using a tiny embedded 5x5 pixel font covering
//! exactly what [`crate::tuning::notes::Note::display_name`] can produce:
//! the letters A-G, `#`, and an octave digit 0-8.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

/// Glyph cell height in rows.
const GLYPH_HEIGHT: usize = 5;
/// Glyph cell width in columns.
const GLYPH_WIDTH: usize = 5;
/// Columns of blank space between adjacent glyphs.
const GLYPH_GAP: usize = 1;

/// 5x5 pixel font for the characters a note name can contain. `#` marks a
/// filled cell; anything else is blank. Unrecognized characters return
/// `None` and are skipped entirely (no blank placeholder), so stray input
/// can't desync the glyph spacing.
fn glyph_bitmap(ch: char) -> Option<[&'static str; GLYPH_HEIGHT]> {
    Some(match ch {
        'A' => [".###.", "#...#", "#####", "#...#", "#...#"],
        'B' => ["####.", "#...#", "####.", "#...#", "####."],
        'C' => [".####", "#....", "#....", "#....", ".####"],
        'D' => ["####.", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "###..", "#....", "#####"],
        'F' => ["#####", "#....", "###..", "#....", "#...."],
        'G' => [".####", "#....", "#..##", "#...#", ".####"],
        '#' => [".#.#.", "#####", ".#.#.", "#####", ".#.#."],
        '0' => [".###.", "#...#", "#...#", "#...#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", ".###."],
        '2' => [".###.", "#...#", "..##.", ".#...", "#####"],
        '3' => [".###.", "#...#", "..##.", "#...#", ".###."],
        '4' => ["#..#.", "#..#.", "#####", "...#.", "...#."],
        '5' => ["#####", "#....", "####.", "....#", "####."],
        '6' => [".###.", "#....", "####.", "#...#", ".###."],
        '7' => ["#####", "....#", "...#.", "..#..", "..#.."],
        '8' => [".###.", "#...#", ".###.", "#...#", ".###."],
        _ => return None,
    })
}

/// Rendered width in columns for `text` (a note display name, e.g.
/// `"C#4"`), including inter-glyph gaps. Characters with no glyph are
/// skipped rather than reserving blank space for them.
pub fn glyph_text_width(text: &str) -> u16 {
    let known = text.chars().filter(|c| glyph_bitmap(*c).is_some()).count();
    if known == 0 {
        return 0;
    }
    (known * GLYPH_WIDTH + (known - 1) * GLYPH_GAP) as u16
}

/// Large blocky rendering of a note name (e.g. `"A4"`, `"C#4"`) - the hero
/// visual a tuner glances at constantly (issue #32).
pub struct NoteGlyph<'a> {
    text: &'a str,
    style: Style,
}

impl<'a> NoteGlyph<'a> {
    /// A glyph rendering `text` in the default style.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            style: Style::default(),
        }
    }

    /// Set the glyph's style (color/emphasis).
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Rendered width in columns, for callers sizing a layout split around
    /// this glyph before rendering it.
    pub fn width(&self) -> u16 {
        glyph_text_width(self.text)
    }
}

impl Widget for NoteGlyph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let rows = (area.height as usize).min(GLYPH_HEIGHT);

        let mut x = area.x;
        for ch in self.text.chars() {
            let Some(bitmap) = glyph_bitmap(ch) else {
                continue;
            };
            if x + GLYPH_WIDTH as u16 > area.x + area.width {
                break; // no room for this glyph (or any after it)
            }
            for (row, line) in bitmap.iter().enumerate().take(rows) {
                for (col, cell) in line.chars().enumerate() {
                    if cell == '#' {
                        buf[(x + col as u16, area.y + row as u16)]
                            .set_char('█')
                            .set_style(self.style);
                    }
                }
            }
            x += (GLYPH_WIDTH + GLYPH_GAP) as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_width_two_char_note() {
        assert_eq!(glyph_text_width("A4"), 5 * 2 + 1);
    }

    #[test]
    fn test_width_sharp_note_has_three_glyphs() {
        assert_eq!(glyph_text_width("C#4"), 5 * 3 + 2);
    }

    #[test]
    fn test_width_zero_for_all_unknown_chars() {
        assert_eq!(glyph_text_width("?!"), 0);
    }

    #[test]
    fn test_unknown_chars_do_not_widen_the_glyph() {
        // A stray character contributes no width of its own.
        assert_eq!(glyph_text_width("A4?"), glyph_text_width("A4"));
    }

    #[test]
    fn test_render_smoke_draws_something_at_normal_size() {
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::empty(area);
        NoteGlyph::new("C#4")
            .style(Style::default().fg(Color::Cyan))
            .render(area, &mut buf);

        let has_ink = (0..20).any(|x| (0..5).any(|y| buf[(x, y)].symbol() == "█"));
        assert!(has_ink, "expected the glyph to draw at least one cell");
    }

    #[test]
    fn test_render_does_not_panic_on_tiny_or_zero_areas() {
        for (w, h) in [(0, 0), (1, 1), (3, 2), (5, 1), (4, 5)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            NoteGlyph::new("C#4").render(area, &mut buf);
        }
    }

    #[test]
    fn test_render_stops_before_overflowing_area_width() {
        // "C#4" needs 17 columns; only 8 are available, so at most the
        // first glyph may be drawn, never past the area's edge.
        let area = Rect::new(2, 2, 8, 5);
        let mut buf = Buffer::empty(area);
        NoteGlyph::new("C#4").render(area, &mut buf);
        // No panic is the primary assertion (buffer indexing would panic
        // on out-of-bounds writes); this just also confirms nothing beyond
        // the first glyph column landed outside the intended sub-range.
        let ink_columns: Vec<u16> = (area.x..area.x + area.width)
            .filter(|&x| (area.y..area.y + area.height).any(|y| buf[(x, y)].symbol() == "█"))
            .collect();
        for x in ink_columns {
            assert!(x < area.x + 5, "ink must stay within the first glyph cell");
        }
    }
}
