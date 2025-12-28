//! ASCII piano keyboard visualization.

use std::collections::HashSet;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::Widget,
};

use crate::ui::theme::Theme;

/// Piano keyboard display showing keys centered on current note.
pub struct Piano {
    /// Currently active note index (0-87, where 0 = A0).
    current_note: usize,
    /// Set of completed note indices (for progress mode).
    completed: HashSet<usize>,
    /// Whether to show progress (completed keys lit).
    show_progress: bool,
}

impl Piano {
    /// Create a new piano centered on the given note.
    pub fn new(current_note: usize) -> Self {
        Self {
            current_note,
            completed: HashSet::new(),
            show_progress: false,
        }
    }

    /// Enable progress display with the given completed notes.
    pub fn with_progress(mut self, completed: HashSet<usize>) -> Self {
        self.completed = completed;
        self.show_progress = true;
        self
    }

    /// Check if a note index is a black key.
    fn is_black_key(note_idx: usize) -> bool {
        // Note indices: 0=A, 1=A#, 2=B, 3=C, 4=C#, 5=D, 6=D#, 7=E, 8=F, 9=F#, 10=G, 11=G#
        // Black keys are: A#, C#, D#, F#, G# (indices 1, 4, 6, 9, 11 in each octave)
        let note_in_octave = note_idx % 12;
        matches!(note_in_octave, 1 | 4 | 6 | 9 | 11)
    }

    /// Get the white key index for a given note (only valid for white keys).
    fn white_key_position(note_idx: usize) -> usize {
        // Count white keys from start
        let octave = note_idx / 12;
        let note_in_octave = note_idx % 12;

        // White keys per note in octave: A=0, B=1, C=2, D=3, E=4, F=5, G=6
        let white_offset = match note_in_octave {
            0 => 0,  // A
            2 => 1,  // B
            3 => 2,  // C
            5 => 3,  // D
            7 => 4,  // E
            8 => 5,  // F
            10 => 6, // G
            _ => 0,  // Black key, shouldn't be called
        };

        octave * 7 + white_offset
    }

    /// Get which white key a black key sits between/above.
    fn black_key_white_position(note_idx: usize) -> usize {
        // Black keys sit above the gap between white keys
        let note_in_octave = note_idx % 12;
        let octave = note_idx / 12;

        let white_offset = match note_in_octave {
            1 => 0,  // A# is between A and B
            4 => 2,  // C# is between C and D
            6 => 3,  // D# is between D and E
            9 => 5,  // F# is between F and G
            11 => 6, // G# is between G and A
            _ => 0,
        };

        octave * 7 + white_offset
    }
}

impl Widget for Piano {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 4 || area.width < 20 {
            return;
        }

        // Each white key is 2 chars wide, black keys overlay between
        // 88 keys: A0 to C8, 52 white keys total
        let chars_per_white = 2;
        let total_white_keys = 52;
        let full_width = total_white_keys * chars_per_white;

        // Calculate visible window centered on current note
        let available_width = area.width as usize;

        // Find the white key position for centering
        let center_white_pos = if Self::is_black_key(self.current_note) {
            Self::black_key_white_position(self.current_note)
        } else {
            Self::white_key_position(self.current_note)
        };

        // Calculate the start position (in chars) for centering
        let center_char = center_white_pos * chars_per_white + 1;
        let half_width = available_width / 2;

        let start_char = if center_char > half_width {
            (center_char - half_width).min(full_width.saturating_sub(available_width))
        } else {
            0
        };

        let start_white = start_char / chars_per_white;
        let visible_whites = (available_width / chars_per_white).min(total_white_keys - start_white);

        // Row 1 & 2: Black keys (2 rows for height)
        // Row 3: White key upper portion
        // Row 4: Bottom border

        let style_off = Theme::muted();
        let style_current = Theme::selected();
        let style_completed = Theme::in_tune();

        // Build the display
        for row in 0..4 {
            let y = area.y + row;
            if y >= area.y + area.height {
                break;
            }

            let mut x = area.x;

            for white_idx in start_white..(start_white + visible_whites) {
                if x + 2 > area.x + area.width {
                    break;
                }

                // Convert white key index back to note index
                let octave = white_idx / 7;
                let key_in_octave = white_idx % 7;
                let white_note_idx = octave * 12 + match key_in_octave {
                    0 => 0,  // A
                    1 => 2,  // B
                    2 => 3,  // C
                    3 => 5,  // D
                    4 => 7,  // E
                    5 => 8,  // F
                    6 => 10, // G
                    _ => 0,
                };

                // Check if there's a black key to the right of this white key
                let has_black_right = match key_in_octave {
                    0 | 2 | 3 | 5 | 6 => true, // A, C, D, F, G have sharps
                    _ => false,
                };

                let black_note_idx = if has_black_right {
                    Some(white_note_idx + 1)
                } else {
                    None
                };

                // Determine styles
                let white_style = if white_note_idx == self.current_note {
                    style_current
                } else if self.show_progress && self.completed.contains(&white_note_idx) {
                    style_completed
                } else {
                    style_off
                };

                let black_style = black_note_idx.map(|idx| {
                    if idx == self.current_note {
                        style_current
                    } else if self.show_progress && self.completed.contains(&idx) {
                        style_completed
                    } else {
                        style_off
                    }
                });

                match row {
                    0 | 1 => {
                        // Black key row
                        // First char is part of white key (or gap), second might be black
                        buf.set_string(x, y, "║", style_off);

                        if let Some(b_style) = black_style {
                            // Black key character
                            let black_char = if b_style == style_current {
                                "█"
                            } else if b_style == style_completed {
                                "▓"
                            } else {
                                "░"
                            };
                            buf.set_string(x + 1, y, black_char, b_style);
                        } else {
                            buf.set_string(x + 1, y, " ", Style::default());
                        }
                    }
                    2 => {
                        // White key upper row
                        let white_char = if white_style == style_current {
                            "█"
                        } else if white_style == style_completed {
                            "▓"
                        } else {
                            " "
                        };
                        buf.set_string(x, y, "║", style_off);
                        buf.set_string(x + 1, y, white_char, white_style);
                    }
                    3 => {
                        // Bottom border
                        buf.set_string(x, y, "╩", style_off);
                        buf.set_string(x + 1, y, "═", style_off);
                    }
                    _ => {}
                }

                x += chars_per_white as u16;
            }

            // Close the right edge
            if x <= area.x + area.width {
                match row {
                    0..=2 => buf.set_string(x, y, "║", style_off),
                    3 => buf.set_string(x, y, "╝", style_off),
                    _ => {}
                }
            }
        }
    }
}
