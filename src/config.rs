//! CLI arguments and configuration.

use clap::{Parser, Subcommand};

/// CLI Piano Tuner with guided coaching.
#[derive(Parser, Debug)]
#[command(name = "onkey")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Resume interrupted session.
    #[arg(long)]
    pub resume: bool,

    /// Quick tune mode (tune relative to current pitch center).
    #[arg(long)]
    pub quick: bool,

    /// Custom A4 reference frequency in Hz.
    #[arg(long, default_value = "440.0")]
    pub a4: f32,

    /// Enable audio confirmation beep.
    #[arg(long)]
    pub beep: bool,
}

/// Subcommands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Analyze a recording for pitch.
    Analyze {
        /// Path to WAV file.
        file: String,
    },
    /// Generate a reference tone.
    Reference {
        /// Note name (e.g., "A4", "C5").
        note: String,
        /// Duration in seconds.
        #[arg(long, default_value = "2.0")]
        duration: f32,
    },
    /// Show tuning history.
    History,
    /// Clear saved sessions.
    Reset,
}

/// Application configuration loaded from file.
#[derive(Debug, Default)]
pub struct Config {
    /// Default A4 reference.
    pub a4: f32,
    /// Default tolerance in cents.
    pub tolerance: f32,
    /// Enable beep on lock.
    pub beep: bool,
    /// Default tuning mode.
    pub default_mode: String,
}

impl Config {
    /// Load configuration from ~/.config/onkey/config.toml.
    pub fn load() -> Self {
        // TODO: Implement config loading
        Self {
            a4: 440.0,
            tolerance: 5.0,
            beep: false,
            default_mode: "concert".to_string(),
        }
    }
}
