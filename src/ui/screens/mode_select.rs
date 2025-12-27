//! Mode selection screen.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::ui::theme::{Shortcuts, Theme};

/// Selected tuning mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectedMode {
    #[default]
    QuickTune,
    ConcertPitch,
}

impl SelectedMode {
    /// Get the mode title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::QuickTune => "Quick Tune",
            Self::ConcertPitch => "Concert Pitch (A4 = 440 Hz)",
        }
    }

    /// Get the mode description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::QuickTune => "Calibrate to the piano's current pitch center, then tune all strings relative to that. Best for regular maintenance.",
            Self::ConcertPitch => "Tune all strings to standard concert pitch (A4 = 440 Hz). Use for pianos that are already close to pitch.",
        }
    }
}

/// Mode selection screen.
pub struct ModeSelectScreen {
    selected: SelectedMode,
}

impl ModeSelectScreen {
    /// Create a new mode select screen.
    pub fn new() -> Self {
        Self {
            selected: SelectedMode::default(),
        }
    }

    /// Get the currently selected mode.
    pub fn selected(&self) -> SelectedMode {
        self.selected
    }

    /// Select the next mode.
    pub fn next(&mut self) {
        self.selected = match self.selected {
            SelectedMode::QuickTune => SelectedMode::ConcertPitch,
            SelectedMode::ConcertPitch => SelectedMode::QuickTune,
        };
    }

    /// Select the previous mode (same as next for 2 options).
    pub fn prev(&mut self) {
        self.next();
    }
}

impl Default for ModeSelectScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &ModeSelectScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Main container
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(" onkey - Piano Tuner ")
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 10 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Layout: title area, mode options, help text
        let chunks = Layout::vertical([
            Constraint::Length(3), // Title
            Constraint::Length(1), // Spacer
            Constraint::Min(8),    // Mode options
            Constraint::Length(3), // Help text
        ])
        .split(inner);

        // Title
        let title = Paragraph::new("Select Tuning Mode")
            .style(Theme::title())
            .alignment(Alignment::Center);
        title.render(chunks[0], buf);

        // Mode options
        let modes = [SelectedMode::QuickTune, SelectedMode::ConcertPitch];
        let option_height = 4;
        let options_area = chunks[2];

        for (i, mode) in modes.iter().enumerate() {
            let is_selected = *mode == self.selected;
            let y_offset = i as u16 * (option_height + 1);

            if y_offset + option_height > options_area.height {
                break;
            }

            let option_area = Rect {
                x: options_area.x + 2,
                y: options_area.y + y_offset,
                width: options_area.width.saturating_sub(4),
                height: option_height,
            };

            render_mode_option(*mode, is_selected, option_area, buf);
        }

        // Help text at bottom
        let help_text = format!(
            "{} Navigate  {} Select  {} Quit",
            Shortcuts::ARROWS,
            Shortcuts::ENTER,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(chunks[3], buf);
    }
}

fn render_mode_option(mode: SelectedMode, is_selected: bool, area: Rect, buf: &mut Buffer) {
    let (border_style, title_style) = if is_selected {
        (Theme::selected(), Theme::selected())
    } else {
        (Theme::muted(), Style::default())
    };

    let prefix = if is_selected { "▶ " } else { "  " };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.height < 2 {
        return;
    }

    // Title line
    let title_line = format!("{}{}", prefix, mode.title());
    buf.set_string(inner.x, inner.y, &title_line, title_style.add_modifier(Modifier::BOLD));

    // Description (wrapped if needed)
    if inner.height >= 2 && inner.width > 4 {
        let desc = mode.description();
        let max_width = inner.width.saturating_sub(2) as usize;
        let truncated = if desc.len() > max_width {
            format!("{}...", &desc[..max_width.saturating_sub(3)])
        } else {
            desc.to_string()
        };
        buf.set_string(inner.x + 2, inner.y + 1, &truncated, Theme::muted());
    }
}
