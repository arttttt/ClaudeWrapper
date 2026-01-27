use std::fs;
use std::path::PathBuf;
use thiserror::Error;

use crate::config::types::Config;

/// Errors that can occur when loading configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    ReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse config file '{path}': {source}")]
    ParseError {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("Config validation failed: {message}")]
    ValidationError { message: String },
}

impl Config {
    /// Returns the path to the configuration file.
    ///
    /// Uses `~/.config/claude-wrapper/config.toml` on Unix/macOS,
    /// or equivalent on other platforms via `dirs::config_dir()`.
    /// Falls back to current directory if config_dir is unavailable.
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        config_dir.join("claude-wrapper").join("config.toml")
    }

    /// Loads configuration from the default config file.
    ///
    /// - If the file doesn't exist, returns `Config::default()`.
    /// - If the file exists, parses it as TOML and validates.
    /// - Returns an error if reading, parsing, or validation fails.
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path();

        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path).map_err(|e| ConfigError::ReadError {
            path: path.clone(),
            source: e,
        })?;

        let config: Config = toml::from_str(&content).map_err(|e| ConfigError::ParseError {
            path: path.clone(),
            source: e,
        })?;

        config.validate()?;
        Ok(config)
    }

    /// Validates the configuration.
    ///
    /// Checks:
    /// - At least one backend is configured
    /// - The active backend exists in the backends list
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.backends.is_empty() {
            return Err(ConfigError::ValidationError {
                message: "At least one backend must be configured".to_string(),
            });
        }

        let active = &self.defaults.active;
        let backend_exists = self.backends.iter().any(|b| &b.name == active);

        if !backend_exists {
            return Err(ConfigError::ValidationError {
                message: format!(
                    "Active backend '{}' not found in configured backends",
                    active
                ),
            });
        }

        Ok(())
    }
}
