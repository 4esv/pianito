//! Coaching instructions component.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Widget},
};

use crate::ui::theme::Theme;

/// Step in the tuning process for multi-string notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuningStep {
    // Bichord (2 strings) - 2 steps
    /// Tune left string to pitch (mute right first).
    TuneBichordLeft,
    /// Tune right string to unison with left.
    TuneBichordRight,

    // Trichord (3 strings) - 4 steps
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
    /// Create first step for given string count.
    pub fn first_for_strings(strings: u8) -> Option<Self> {
        match strings {
            2 => Some(Self::TuneBichordLeft),
            3 => Some(Self::MuteOuter),
            _ => None, // Monochord has no steps
        }
    }

    /// Check if this is a muting step (no tuning hints).
    pub fn is_muting(&self) -> bool {
        matches!(self, Self::MuteOuter)
    }

    /// Get total steps for this string type.
    pub fn total_steps(&self) -> u8 {
        match self {
            Self::TuneBichordLeft | Self::TuneBichordRight => 2,
            Self::MuteOuter | Self::TuneCenter | Self::TuneLeft | Self::TuneRight => 4,
        }
    }

    /// Get step number (1-based).
    pub fn number(&self) -> u8 {
        match self {
            Self::TuneBichordLeft => 1,
            Self::TuneBichordRight => 2,
            Self::MuteOuter => 1,
            Self::TuneCenter => 2,
            Self::TuneLeft => 3,
            Self::TuneRight => 4,
        }
    }

    /// Get the step title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::TuneBichordLeft => "Tune left string",
            Self::TuneBichordRight => "Tune right string",
            Self::MuteOuter => "Mute outer strings",
            Self::TuneCenter => "Tune center string",
            Self::TuneLeft => "Tune left string",
            Self::TuneRight => "Tune right string",
        }
    }

    /// Get instruction text.
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::TuneBichordLeft => "Mute the right string. Tune the left string to the target pitch using the meter.",
            Self::TuneBichordRight => "Unmute the right string. Tune it to match the left until you hear no beats.",
            Self::MuteOuter => "Use felt strip or rubber mutes to mute the outer strings. Only the center string should sound.",
            Self::TuneCenter => "Tune the center string to the target pitch using the meter.",
            Self::TuneLeft => "Unmute the left string. Tune it to match the center string until you hear no beats.",
            Self::TuneRight => "Unmute the right string. Tune it to match the center string until you hear no beats.",
        }
    }

    /// Get the next step.
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::TuneBichordLeft => Some(Self::TuneBichordRight),
            Self::TuneBichordRight => None,
            Self::MuteOuter => Some(Self::TuneCenter),
            Self::TuneCenter => Some(Self::TuneLeft),
            Self::TuneLeft => Some(Self::TuneRight),
            Self::TuneRight => None,
        }
    }

    /// Get the previous step.
    pub fn prev(&self) -> Option<Self> {
        match self {
            Self::TuneBichordLeft => None,
            Self::TuneBichordRight => Some(Self::TuneBichordLeft),
            Self::MuteOuter => None,
            Self::TuneCenter => Some(Self::MuteOuter),
            Self::TuneLeft => Some(Self::TuneCenter),
            Self::TuneRight => Some(Self::TuneLeft),
        }
    }
}

/// Instructions panel for coaching the user.
pub struct Instructions {
    step: Option<TuningStep>,
    direction_hint: Option<String>,
}

impl Instructions {
    /// Create instructions for a note with given step.
    pub fn for_step(step: TuningStep, _string_count: u8) -> Self {
        Self {
            step: Some(step),
            direction_hint: None,
        }
    }

    /// Create instructions for a monochord note (1 string, no steps).
    pub fn simple() -> Self {
        Self {
            step: None,
            direction_hint: None,
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

        if let Some(step) = &self.step {
            // Multi-string note with steps (bichord or trichord)
            // Step indicator
            let step_text = format!(
                "Step {} of {}: {}",
                step.number(),
                step.total_steps(),
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
        } else {
            // Monochord note - simple instruction
            let text = "Tune this string to the target pitch using the meter.";
            buf.set_string(inner.x + 1, y, text, Style::default());
            y += 2;
        }

        // Direction hint (not shown during muting steps)
        if let Some(hint) = &self.direction_hint {
            if y < inner.y + inner.height {
                y += 1;
                buf.set_string(inner.x + 1, y, hint, Theme::warning());
            }
        }

        // Press SPACE prompt
        if y + 1 < inner.y + inner.height {
            let prompt = "Press SPACE to continue";
            buf.set_string(inner.x + 1, inner.y + inner.height - 1, prompt, Theme::muted());
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
