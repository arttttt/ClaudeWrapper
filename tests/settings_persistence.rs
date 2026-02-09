mod common;

use std::collections::HashMap;

use anyclaude::config::{save_claude_settings, Config};
use tempfile::TempDir;

#[test]
fn save_and_reload_settings() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Write a minimal valid config first
    let initial = r#"[defaults]
active = "claude"
timeout_seconds = 30

[[backends]]
name = "claude"
display_name = "Claude"
base_url = "https://api.anthropic.com"
auth_type = "passthrough"
"#;
    std::fs::write(&config_path, initial).unwrap();

    // Save settings
    let mut settings = HashMap::new();
    settings.insert("agent_teams".to_string(), true);
    save_claude_settings(&config_path, &settings).unwrap();

    // Reload and verify
    let config = Config::load_from(&config_path).unwrap();
    assert_eq!(config.claude_settings.get("agent_teams"), Some(&true));
}

#[test]
fn save_preserves_other_sections() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let initial = r#"[defaults]
active = "claude"
timeout_seconds = 30

[[backends]]
name = "claude"
display_name = "Claude"
base_url = "https://api.anthropic.com"
auth_type = "passthrough"
"#;
    std::fs::write(&config_path, initial).unwrap();

    // Save settings
    let mut settings = HashMap::new();
    settings.insert("agent_teams".to_string(), false);
    save_claude_settings(&config_path, &settings).unwrap();

    // Verify other config sections are preserved
    let config = Config::load_from(&config_path).unwrap();
    assert_eq!(config.defaults.active, "claude");
    assert_eq!(config.defaults.timeout_seconds, 30);
    assert_eq!(config.backends.len(), 1);
    assert_eq!(config.backends[0].name, "claude");
}

#[test]
fn save_creates_config_if_not_exists() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("subdir").join("config.toml");

    let mut settings = HashMap::new();
    settings.insert("agent_teams".to_string(), true);
    save_claude_settings(&config_path, &settings).unwrap();

    assert!(config_path.exists());

    // Read back raw TOML to verify structure
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[claude_settings]"));
    assert!(content.contains("agent_teams = true"));
}

#[test]
fn save_overwrites_existing_settings() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let initial = r#"[defaults]
active = "claude"
timeout_seconds = 30

[claude_settings]
agent_teams = false

[[backends]]
name = "claude"
display_name = "Claude"
base_url = "https://api.anthropic.com"
auth_type = "passthrough"
"#;
    std::fs::write(&config_path, initial).unwrap();

    // Save with different value
    let mut settings = HashMap::new();
    settings.insert("agent_teams".to_string(), true);
    save_claude_settings(&config_path, &settings).unwrap();

    let config = Config::load_from(&config_path).unwrap();
    assert_eq!(config.claude_settings.get("agent_teams"), Some(&true));
}
