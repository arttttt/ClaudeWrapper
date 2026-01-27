use claudewrapper::config::{Backend, Config, ConfigError, Defaults};

/// Test that Config::default() produces the expected values per spec.
#[test]
fn test_config_default_values() {
    let config = Config::default();

    // Defaults
    assert_eq!(config.defaults.active, "claude");
    assert_eq!(config.defaults.timeout_seconds, 30);

    // Should have exactly one backend
    assert_eq!(config.backends.len(), 1);

    let backend = &config.backends[0];
    assert_eq!(backend.name, "claude");
    assert_eq!(backend.display_name, "Claude");
    assert_eq!(backend.base_url, "https://api.anthropic.com");
    assert_eq!(backend.auth_type, "api_key");
    assert_eq!(backend.auth_env_var, "ANTHROPIC_API_KEY");
    assert_eq!(backend.models, vec!["claude-sonnet-4-20250514"]);
}

/// Test that Config::config_path() returns a path ending with the expected filename.
#[test]
fn test_config_path_ends_with_expected() {
    let path = Config::config_path();
    assert!(path.ends_with("claude-wrapper/config.toml"));
}

/// Test validation passes for default config.
#[test]
fn test_validation_passes_for_default() {
    let config = Config::default();
    assert!(config.validate().is_ok());
}

/// Test validation fails when no backends are configured.
#[test]
fn test_validation_fails_empty_backends() {
    let config = Config {
        defaults: Defaults::default(),
        backends: vec![],
    };

    let result = config.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::ValidationError { message } => {
            assert!(message.contains("At least one backend"));
        }
        _ => panic!("Expected ValidationError"),
    }
}

/// Test validation fails when active backend doesn't exist.
#[test]
fn test_validation_fails_missing_active_backend() {
    let config = Config {
        defaults: Defaults {
            active: "nonexistent".to_string(),
            timeout_seconds: 30,
        },
        backends: vec![Backend::default()],
    };

    let result = config.validate();
    assert!(result.is_err());

    match result.unwrap_err() {
        ConfigError::ValidationError { message } => {
            assert!(message.contains("nonexistent"));
            assert!(message.contains("not found"));
        }
        _ => panic!("Expected ValidationError"),
    }
}

/// Test that valid TOML parses correctly.
#[test]
fn test_parse_valid_toml() {
    let toml_content = r#"
[defaults]
active = "claude"
timeout_seconds = 60

[[backends]]
name = "claude"
display_name = "Claude"
base_url = "https://api.anthropic.com"
auth_type = "api_key"
auth_env_var = "ANTHROPIC_API_KEY"
models = ["claude-sonnet-4-20250514", "claude-3-opus-20240229"]
"#;

    let config: Config = toml::from_str(toml_content).expect("Should parse valid TOML");

    assert_eq!(config.defaults.active, "claude");
    assert_eq!(config.defaults.timeout_seconds, 60);
    assert_eq!(config.backends.len(), 1);
    assert_eq!(config.backends[0].models.len(), 2);
}

/// Test that invalid TOML produces a parse error.
#[test]
fn test_parse_invalid_toml() {
    let invalid_toml = "this is not valid toml [[[";

    let result: Result<Config, _> = toml::from_str(invalid_toml);
    assert!(result.is_err());
}

/// Test round-trip serialization/deserialization.
#[test]
fn test_config_roundtrip() {
    let original = Config::default();
    let serialized = toml::to_string(&original).expect("Should serialize");
    let deserialized: Config = toml::from_str(&serialized).expect("Should deserialize");

    assert_eq!(original.defaults.active, deserialized.defaults.active);
    assert_eq!(
        original.defaults.timeout_seconds,
        deserialized.defaults.timeout_seconds
    );
    assert_eq!(original.backends.len(), deserialized.backends.len());
    assert_eq!(original.backends[0].name, deserialized.backends[0].name);
}
