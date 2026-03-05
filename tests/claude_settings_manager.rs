mod common;

use std::collections::HashMap;

use anyclaude::config::{ClaudeSettingsManager, SettingId};

#[test]
fn new_manager_returns_defaults() {
    let mgr = ClaudeSettingsManager::new();
    // Agents defaults to false
    assert!(!mgr.get(SettingId::Agents));
}

#[test]
fn set_and_get() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, true);
    assert!(mgr.get(SettingId::Agents));
}

#[test]
fn toggle_inverts_value() {
    let mut mgr = ClaudeSettingsManager::new();
    assert!(!mgr.get(SettingId::Agents));
    mgr.toggle(SettingId::Agents);
    assert!(mgr.get(SettingId::Agents));
    mgr.toggle(SettingId::Agents);
    assert!(!mgr.get(SettingId::Agents));
}

#[test]
fn to_env_vars_when_disabled() {
    let mgr = ClaudeSettingsManager::new();
    let vars = mgr.to_env_vars();
    assert!(vars.is_empty());
}

#[test]
fn to_env_vars_when_enabled() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, true);
    let vars = mgr.to_env_vars();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].0, "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS");
    assert_eq!(vars[0].1, "1");
}

#[test]
fn to_cli_args_empty_when_no_cli_flags() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, true);
    // Agents has no cli_flag, so args should be empty
    let args = mgr.to_cli_args();
    assert!(args.is_empty());
}

#[test]
fn toml_roundtrip() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, true);

    let map = mgr.to_toml_map();
    assert_eq!(map.get("agents"), Some(&true));

    // Load into a fresh manager
    let mut mgr2 = ClaudeSettingsManager::new();
    mgr2.load_from_toml(&map);
    assert!(mgr2.get(SettingId::Agents));
}

#[test]
fn load_from_toml_ignores_unknown_keys() {
    let mut map = HashMap::new();
    map.insert("agents".to_string(), true);
    map.insert("unknown_setting".to_string(), false);

    let mut mgr = ClaudeSettingsManager::new();
    mgr.load_from_toml(&map);
    assert!(mgr.get(SettingId::Agents));
}

#[test]
fn is_dirty_detects_changes() {
    let mut mgr = ClaudeSettingsManager::new();
    let snapshot = mgr.snapshot_values();
    assert!(!mgr.is_dirty(&snapshot));

    mgr.set(SettingId::Agents, true);
    assert!(mgr.is_dirty(&snapshot));
}

#[test]
fn is_dirty_no_change_after_set_same_value() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, false); // same as default
    let snapshot = mgr.snapshot_values();
    assert!(!mgr.is_dirty(&snapshot));
}

#[test]
fn snapshots_roundtrip() {
    let mut mgr = ClaudeSettingsManager::new();
    mgr.set(SettingId::Agents, true);

    let snapshots = mgr.to_snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, SettingId::Agents);
    assert!(snapshots[0].value);

    // Mutate snapshot and apply back
    let mut modified = snapshots;
    modified[0].value = false;

    mgr.apply_snapshots(&modified);
    assert!(!mgr.get(SettingId::Agents));
}

#[test]
fn setting_id_as_str_roundtrip() {
    for &id in SettingId::all() {
        let s = id.as_str();
        let parsed = SettingId::parse(s);
        assert_eq!(parsed, Some(id), "roundtrip failed for {:?}", id);
    }
}

#[test]
fn setting_id_parse_unknown_returns_none() {
    assert_eq!(SettingId::parse("nonexistent_setting"), None);
}

#[test]
fn registry_has_entries() {
    let mgr = ClaudeSettingsManager::new();
    assert!(!mgr.registry().is_empty());
}
