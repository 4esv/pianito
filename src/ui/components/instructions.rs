//! Coaching instructions component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Widget},
};

use crate::ui::theme::Theme;

/// Step in the tuning process for trichord notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningStep {
    /// Mute outer strings.
    MuteOuter,
    /// Tune center string.
    TuneCenter,
    /// Tune left string to unison.
    TuneLeft,
    /// Tune right string to unison.
    TuneRight,
}

impl TuningStep {
    /// Get step number (1-4).
    pub fn number(&self) -> u8 {
        match self {
            Self::MuteOuter => 1,
            Self::TuneCenter => 2,
            Self::TuneLeft => 3,
            Self::TuneRight => 4,
        }
    }

    /// Get the step title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::MuteOuter => "Mute outer strings",
            Self::TuneCenter => "Tune center string",
            Self::TuneLeft => "Tune left string",
            Self::TuneRight => "Tune right string",
        }
    }

    /// Get instruction text.
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::MuteOuter => "Use felt strip or rubber mutes to mute the outer strings. Only the center string should sound.",
            Self::TuneCenter => "Tune the center string to the target pitch using the meter above.",
            Self::TuneLeft => "Unmute the left string. Tune it to match the center string until you hear no beats.",
            Self::TuneRight => "Unmute the right string. Tune it to match the center string until you hear no beats.",
        }
    }

    /// Get the next step.
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::MuteOuter => Some(Self::TuneCenter),
            Self::TuneCenter => Some(Self::TuneLeft),
            Self::TuneLeft => Some(Self::TuneRight),
            Self::TuneRight => None,
        }
    }
}

/// Instructions panel for coaching the user.
pub struct Instructions {
    step: Option<TuningStep>,
    total_steps: u8,
    direction_hint: Option<String>,
    is_trichord: bool,
}

impl Instructions {
    /// Create instructions for a trichord note.
    pub fn trichord(step: TuningStep) -> Self {
        Self {
            step: Some(step),
            total_steps: 4,
            direction_hint: None,
            is_trichord: true,
        }
    }

    /// Create instructions for a non-trichord note (1-2 strings).
    pub fn simple() -> Self {
        Self {
            step: None,
            total_steps: 1,
            direction_hint: None,
            is_trichord: false,
        }
    }

    /// Set a direction hint based on cents deviation.
    pub fn with_direction_hint(mut self, cents: f32) -> Self {
        if cents.abs() > 5.0 {
            let hint = if cents < 0.0 {
                "Turn tuning pin CLOCKWISE (tighten) slightly"
            } else {
                "Turn tuning pin COUNTER-CLOCKWISE (loosen) slightly"
            };
            self.direction_hint = Some(hint.to_string());
        }
        self
    }
}

impl Widget for Instructions {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 10 {
            return;
        }

        let mut y = inner.y;

        if self.is_trichord {
            if let Some(step) = &self.step {
                // Step indicator
                let step_text = format!(
                    "Step {} of {}: {}",
                    step.number(),
                    self.total_steps,
                    step.title()
                );
                let step_style = Theme::accent();
                buf.set_string(inner.x + 1, y, &step_text, step_style);
                y += 2;

                // Instruction text
                if y < inner.y + inner.height {
                    let instruction = step.instruction();
                    let available_width = inner.width.saturating_sub(2) as usize;

                    // Word wrap
                    for line in textwrap(instruction, available_width) {
                        if y >= inner.y + inner.height {
                            break;
                        }
                        buf.set_string(inner.x + 1, y, &line, Style::default());
                        y += 1;
                    }
                }
            }
        } else {
            // Simple note instruction
            let text = "Tune this string to the target pitch using the meter above.";
            buf.set_string(inner.x + 1, y, text, Style::default());
            y += 2;
        }

        // Direction hint
        if let Some(hint) = &self.direction_hint {
            if y < inner.y + inner.height {
                y += 1;
                buf.set_string(inner.x + 1, y, hint, Theme::warning());
            }
        }
    }
}

/// Simple text wrapping helper.
fn textwrap(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}
