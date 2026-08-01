//! Versioned configuration for GostUI.
//!
//! Two properties are load-bearing:
//!
//! - **Every write is atomic** (see [`atomic`]). A crash mid-save must never leave
//!   a config the shell cannot parse.
//! - **Every file carries a schema version.** Reading a config from a future
//!   version is an error the caller can report, not a silent misparse.
//!
//! A broken or missing config is never fatal: [`Config::load_or_default`] falls
//! back to defaults and tells the caller what went wrong, because a shell that
//! refuses to start over a stray character is worse than one with wrong colours.
//!
//! Appearance lives in its own file, [`theme`] — `theme.toml` beside
//! `config.toml`. Separate because a theme is something users swap, copy and
//! share whole, and mixing it into the settings file would make that impossible
//! (D-032).

#![forbid(unsafe_code)]

pub mod atomic;
pub mod theme;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Schema version written into every config file. Bump when a change cannot be
/// read by an older build.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
    /// The file was written by a newer version of GostUI.
    UnsupportedVersion {
        found: u32,
        supported: u32,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "config I/O error: {e}"),
            Self::Parse(e) => write!(f, "config parse error: {e}"),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "config schema version {found} is newer than supported version {supported}"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Schema version of this file.
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub layout: LayoutConfig,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Appearance {
    /// Font size in logical units. Configurable because it is the cheap half of
    /// accessibility we keep in scope (D-018).
    pub font_size: u32,
    pub icon_theme: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_size: 14,
            icon_theme: "hicolor".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LayoutConfig {
    /// Gap between tiles, in logical units.
    pub inner_gap: i32,
    /// Gap between the tiled area and the screen edge, in logical units.
    pub outer_gap: i32,
    /// Divider position between two tiles, in permille. Persisted because the
    /// divider is draggable (D-025).
    pub split_permille: i32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            inner_gap: 4,
            outer_gap: 0,
            split_permille: 500,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            appearance: Appearance::default(),
            layout: LayoutConfig::default(),
        }
    }
}

impl Config {
    /// Parse a config from TOML text, rejecting versions we cannot read.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let cfg: Config = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if cfg.version > SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedVersion {
                found: cfg.version,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(cfg)
    }

    pub fn to_toml(&self) -> String {
        // Serialising our own type cannot fail; a panic here would mean a bug in
        // this crate, not bad user input.
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_toml(&text)
    }

    /// Load, or fall back to defaults on any problem.
    ///
    /// Returns the error alongside the defaults so the caller can log it. A
    /// missing file is not an error — it is a first run.
    pub fn load_or_default(path: &Path) -> (Self, Option<ConfigError>) {
        match Self::load(path) {
            Ok(cfg) => (cfg, None),
            Err(ConfigError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), None)
            }
            Err(e) => (Self::default(), Some(e)),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        atomic::write(path, &self.to_toml())?;
        Ok(())
    }
}

/// `$XDG_CONFIG_HOME/gostui/config.toml`, falling back to `~/.config/...`.
pub fn default_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("gostui").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let cfg = Config::default();
        let parsed = Config::from_toml(&cfg.to_toml()).unwrap();
        assert_eq!(cfg, parsed);
    }

    #[test]
    fn a_future_schema_version_is_reported_not_guessed_at() {
        let text = format!("version = {}\n", SCHEMA_VERSION + 1);
        match Config::from_toml(&text) {
            Err(ConfigError::UnsupportedVersion { found, .. }) => {
                assert_eq!(found, SCHEMA_VERSION + 1)
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn a_partial_file_fills_in_defaults() {
        let cfg = Config::from_toml("[appearance]\nfont_size = 20\n").unwrap();
        assert_eq!(cfg.appearance.font_size, 20);
        assert_eq!(cfg.appearance.icon_theme, Appearance::default().icon_theme);
        assert_eq!(cfg.layout, LayoutConfig::default());
    }

    #[test]
    fn a_missing_file_is_a_first_run_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (cfg, err) = Config::load_or_default(&dir.path().join("nope.toml"));
        assert_eq!(cfg, Config::default());
        assert!(err.is_none());
    }

    #[test]
    fn a_corrupt_file_yields_defaults_and_an_error_rather_than_a_dead_shell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not toml {{{").unwrap();
        let (cfg, err) = Config::load_or_default(&path);
        assert_eq!(cfg, Config::default());
        assert!(matches!(err, Some(ConfigError::Parse(_))));
    }

    #[test]
    fn saving_then_loading_preserves_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.layout.split_permille = 618;
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap().layout.split_permille, 618);
    }
}
