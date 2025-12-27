//! CLI arguments and configuration.

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    #[arg(long)]
    pub a4: Option<f32>,

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default A4 reference.
    #[serde(default = "default_a4")]
    pub a4: f32,
    /// Default tolerance in cents.
    #[serde(default = "default_tolerance")]
    pub tolerance: f32,
    /// Enable beep on lock.
    #[serde(default)]
    pub beep: bool,
    /// Default tuning mode ("concert" or "quick").
    #[serde(default = "default_mode")]
    pub default_mode: String,
}

fn default_a4() -> f32 {
    440.0
}

fn default_tolerance() -> f32 {
    5.0
}

fn default_mode() -> String {
    "concert".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            a4: default_a4(),
            tolerance: default_tolerance(),
            beep: false,
            default_mode: default_mode(),
        }
    }
}

impl Config {
    /// Get the config file path.
    pub fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "onkey").map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load configuration from ~/.config/onkey/config.toml.
    pub fn load() -> Self {
        let path = match Self::config_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save configuration to file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;

        Ok(())
    }

    /// Merge CLI args with config, returning effective settings.
    pub fn merge_with_args(&self, args: &Args) -> EffectiveConfig {
        EffectiveConfig {
            a4: args.a4.unwrap_or(self.a4),
            tolerance: self.tolerance,
            beep: args.beep || self.beep,
            quick_mode: args.quick || self.default_mode == "quick",
            resume: args.resume,
        }
    }
}

/// Effective configuration after merging config file and CLI args.
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    /// A4 reference frequency.
    pub a4: f32,
    /// Tolerance in cents.
    pub tolerance: f32,
    /// Enable beep on lock.
    pub beep: bool,
    /// Use quick tune mode.
    pub quick_mode: bool,
    /// Resume previous session.
    pub resume: bool,
}
