//! Terminal UI screens and components.

use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

pub mod app;
pub mod components;
pub mod screens;
pub mod theme;

pub use app::App;

/// Type alias for our terminal.
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize the terminal for TUI mode.
pub fn init() -> io::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

/// Restore the terminal to normal mode.
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Event handler result.
pub enum EventResult {
    /// Continue running.
    Continue,
    /// Quit the application.
    Quit,
}

/// Poll for events with a timeout.
pub fn poll_event(timeout: std::time::Duration) -> io::Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Check if a key event is a press (not release).
pub fn is_key_press(event: &Event) -> Option<KeyCode> {
    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press {
            return Some(key.code);
        }
    }
    None
}
