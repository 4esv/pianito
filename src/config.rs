//! CLI arguments and configuration.

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// CLI Piano Tuner with guided coaching.
#[derive(Parser, Debug)]
#[command(name = "pianito")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Resume interrupted session.
    #[arg(long)]
    pub resume: bool,

    /// Start with Quick Tune preselected (tune relative to current pitch center).
    #[arg(long)]
    pub quick: bool,

    /// Custom A4 reference frequency in Hz.
    #[arg(long)]
    pub a4: Option<f32>,

    /// Beep once when a note locks into the in-tune zone.
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
        /// Duration in seconds (0-60).
        #[arg(long, default_value = "2.0", value_parser = parse_duration)]
        duration: f32,
    },
    /// Show tuning history.
    History,
    /// Clear saved sessions.
    Reset,
}

/// Parse and validate the reference tone duration.
///
/// NOTE: `Duration::from_secs_f32` panics on negative/NaN values and huge
/// values overflow the sample buffer allocation, so reject them at the CLI
/// edge. The range check also rejects NaN and infinity.
fn parse_duration(s: &str) -> Result<f32, String> {
    let duration: f32 = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid number", s))?;
    if !(0.0..=60.0).contains(&duration) {
        return Err("duration must be between 0 and 60 seconds".to_string());
    }
    Ok(duration)
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
        ProjectDirs::from("", "", "pianito").map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load configuration from ~/.config/pianito/config.toml.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config_values() {
        let config = Config::default();
        assert_eq!(config.a4, 440.0);
        assert_eq!(config.tolerance, 5.0);
        assert!(!config.beep);
        assert_eq!(config.default_mode, "concert");
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_a4(), 440.0);
        assert_eq!(default_tolerance(), 5.0);
        assert_eq!(default_mode(), "concert");
    }

    #[test]
    fn test_merge_with_args_defaults() {
        let config = Config::default();
        let args = Args {
            command: None,
            resume: false,
            quick: false,
            a4: None,
            beep: false,
        };
        let effective = config.merge_with_args(&args);

        assert_eq!(effective.a4, 440.0);
        assert_eq!(effective.tolerance, 5.0);
        assert!(!effective.beep);
        assert!(!effective.quick_mode);
        assert!(!effective.resume);
    }

    #[test]
    fn test_merge_with_args_overrides_a4() {
        let config = Config::default();
        let args = Args {
            command: None,
            resume: false,
            quick: false,
            a4: Some(442.0),
            beep: false,
        };
        let effective = config.merge_with_args(&args);
        assert_eq!(effective.a4, 442.0);
    }

    #[test]
    fn test_merge_with_args_enables_beep() {
        let config = Config::default();
        let args = Args {
            command: None,
            resume: false,
            quick: false,
            a4: None,
            beep: true,
        };
        let effective = config.merge_with_args(&args);
        assert!(effective.beep);
    }

    #[test]
    fn test_merge_with_args_quick_mode_from_arg() {
        let config = Config::default();
        let args = Args {
            command: None,
            resume: false,
            quick: true,
            a4: None,
            beep: false,
        };
        let effective = config.merge_with_args(&args);
        assert!(effective.quick_mode);
    }

    #[test]
    fn test_merge_with_args_quick_mode_from_config() {
        let config = Config {
            default_mode: "quick".to_string(),
            ..Config::default()
        };
        let args = Args {
            command: None,
            resume: false,
            quick: false,
            a4: None,
            beep: false,
        };
        let effective = config.merge_with_args(&args);
        assert!(effective.quick_mode);
    }

    #[test]
    fn test_merge_with_args_resume_flag() {
        let config = Config::default();
        let args = Args {
            command: None,
            resume: true,
            quick: false,
            a4: None,
            beep: false,
        };
        let effective = config.merge_with_args(&args);
        assert!(effective.resume);
    }

    #[test]
    fn test_merge_beep_from_config() {
        let config = Config {
            beep: true,
            ..Config::default()
        };
        let args = Args {
            command: None,
            resume: false,
            quick: false,
            a4: None,
            beep: false,
        };
        let effective = config.merge_with_args(&args);
        assert!(effective.beep); // Config beep is true
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            a4: 442.0,
            tolerance: 10.0,
            beep: true,
            default_mode: "quick".to_string(),
        };

        let toml = toml::to_string(&config).expect("Should serialize");
        assert!(toml.contains("a4 = 442"));
        assert!(toml.contains("tolerance = 10"));
        assert!(toml.contains("beep = true"));
        assert!(toml.contains("default_mode = \"quick\""));
    }

    #[test]
    fn test_config_deserialization() {
        let toml = r#"
            a4 = 442.0
            tolerance = 10.0
            beep = true
            default_mode = "quick"
        "#;

        let config: Config = toml::from_str(toml).expect("Should deserialize");
        assert_eq!(config.a4, 442.0);
        assert_eq!(config.tolerance, 10.0);
        assert!(config.beep);
        assert_eq!(config.default_mode, "quick");
    }

    #[test]
    fn test_config_partial_deserialization_uses_defaults() {
        let toml = r#"
            a4 = 442.0
        "#;

        let config: Config = toml::from_str(toml).expect("Should deserialize");
        assert_eq!(config.a4, 442.0);
        assert_eq!(config.tolerance, 5.0); // default
        assert!(!config.beep); // default
        assert_eq!(config.default_mode, "concert"); // default
    }

    #[test]
    fn test_config_load_missing_file_returns_default() {
        // This will try to load from the real config path
        // which likely doesn't exist in test environment
        let config = Config::load();
        assert_eq!(config.a4, 440.0);
        assert_eq!(config.tolerance, 5.0);
    }

    #[test]
    fn test_config_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().expect("Should create temp dir");
        let config_path = temp_dir.path().join("config.toml");

        let original = Config {
            a4: 442.0,
            tolerance: 10.0,
            beep: true,
            default_mode: "quick".to_string(),
        };

        // Save to temp file
        let content = toml::to_string_pretty(&original).expect("Should serialize");
        fs::write(&config_path, content).expect("Should write");

        // Load from temp file
        let content = fs::read_to_string(&config_path).expect("Should read");
        let loaded: Config = toml::from_str(&content).expect("Should deserialize");

        assert_eq!(loaded.a4, 442.0);
        assert_eq!(loaded.tolerance, 10.0);
        assert!(loaded.beep);
        assert_eq!(loaded.default_mode, "quick");
    }

    #[test]
    fn test_invalid_toml_falls_back_to_default() {
        let invalid_toml = "this is not valid toml {{{}";
        let config: Config = toml::from_str(invalid_toml).unwrap_or_default();
        assert_eq!(config.a4, 440.0); // Should fall back to default
    }

    fn args(a4: Option<f32>, quick: bool, beep: bool) -> Args {
        Args {
            command: None,
            resume: false,
            quick,
            a4,
            beep,
        }
    }

    #[test]
    fn empty_toml_is_all_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.a4, 440.0);
        assert_eq!(config.tolerance, 5.0);
        assert!(!config.beep);
        assert_eq!(config.default_mode, "concert");
    }

    #[test]
    fn malformed_field_fails_the_whole_parse() {
        // WARNING: pins the behavior behind load()'s unwrap_or_default():
        // one bad field (e.g. a quoted number) discards the ENTIRE config,
        // silently reverting a4 to 440.
        assert!(toml::from_str::<Config>("a4 = \"442\"").is_err());
        assert!(toml::from_str::<Config>("a4 = 442.0\ntolerance = true").is_err());
    }

    #[test]
    fn cli_a4_overrides_config_a4() {
        let config = Config {
            a4: 442.0,
            ..Config::default()
        };
        let effective = config.merge_with_args(&args(Some(443.0), false, false));
        assert_eq!(effective.a4, 443.0);
    }

    #[test]
    fn config_a4_used_without_cli_override() {
        let config = Config {
            a4: 442.0,
            ..Config::default()
        };
        let effective = config.merge_with_args(&args(None, false, false));
        assert_eq!(effective.a4, 442.0);
    }

    #[test]
    fn duration_accepts_valid_values() {
        assert_eq!(parse_duration("2.0").unwrap(), 2.0);
        assert_eq!(parse_duration("60").unwrap(), 60.0);
        assert_eq!(parse_duration("0").unwrap(), 0.0);
    }

    #[test]
    fn duration_rejects_out_of_range_and_non_finite() {
        assert!(parse_duration("-1").is_err());
        assert!(parse_duration("60.1").is_err());
        assert!(parse_duration("1e30").is_err());
        assert!(parse_duration("NaN").is_err());
        assert!(parse_duration("inf").is_err());
        assert!(parse_duration("abc").is_err());
    }
}
