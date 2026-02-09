mod common;

use anyclaude::pty::{PtySpawnConfig, SessionMode};

fn config(base_args: Vec<&str>) -> PtySpawnConfig {
    PtySpawnConfig::new(
        "claude".to_string(),
        base_args.into_iter().map(String::from).collect(),
        "http://localhost:3000".to_string(),
    )
}

// -- initial spawn (SessionMode::Initial) -------------------------------------

#[test]
fn initial_adds_session_id() {
    let cfg = config(vec!["--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert_eq!(p.args.len(), 4);
    assert_eq!(p.args[0], "--model");
    assert_eq!(p.args[1], "opus");
    assert_eq!(p.args[2], "--session-id");
    assert_eq!(p.args[3], cfg.session_id());
}

#[test]
fn initial_strips_user_continue() {
    let cfg = config(vec!["--continue", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert!(!p.args.contains(&"--continue".to_string()));
    assert!(p.args.contains(&"--session-id".to_string()));
}

#[test]
fn initial_strips_user_resume() {
    let cfg = config(vec!["--resume", "--model"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert!(!p.args.contains(&"--resume".to_string()));
    assert!(p.args.contains(&"--session-id".to_string()));
}

#[test]
fn initial_env_contains_base_url() {
    let cfg = config(vec![]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert_eq!(p.env.len(), 1);
    assert_eq!(p.env[0].0, "ANTHROPIC_BASE_URL");
    assert_eq!(p.env[0].1, "http://localhost:3000");
}

#[test]
fn initial_session_id_is_valid_uuid() {
    let cfg = config(vec![]);
    assert!(uuid::Uuid::parse_str(cfg.session_id()).is_ok());
}

// -- restart (SessionMode::Resume) --------------------------------------------

#[test]
fn resume_uses_internal_session_id() {
    let cfg = config(vec!["--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert_eq!(
        p.args,
        vec!["--model", "opus", "--resume", cfg.session_id()]
    );
}

#[test]
fn resume_strips_continue() {
    let cfg = config(vec!["--continue", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert!(!p.args.contains(&"--continue".to_string()));
    assert_eq!(p.args[p.args.len() - 2], "--resume");
    assert_eq!(p.args[p.args.len() - 1], cfg.session_id());
}

#[test]
fn resume_strips_existing_resume() {
    let cfg = config(vec!["--resume", "--model"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    // Only one --resume (ours), not two
    assert_eq!(p.args.iter().filter(|a| *a == "--resume").count(), 1);
    assert_eq!(p.args[p.args.len() - 1], cfg.session_id());
}

#[test]
fn resume_strips_session_id_flag() {
    let cfg = config(vec!["--session-id", "--model"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert!(!p.args.contains(&"--session-id".to_string()));
    assert!(p.args.contains(&"--resume".to_string()));
}

#[test]
fn resume_appends_extra_args() {
    let cfg = config(vec!["--model", "opus"]);
    let extra = vec!["--verbose".to_string()];
    let p = cfg.build(vec![], extra, SessionMode::Resume);
    assert!(p.args.contains(&"--verbose".to_string()));
    assert!(p.args.contains(&"--resume".to_string()));
}

#[test]
fn empty_base_args_resume() {
    let cfg = config(vec![]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert_eq!(p.args, vec!["--resume", cfg.session_id()]);
}

// -- shared -------------------------------------------------------------------

#[test]
fn restart_merges_extra_env() {
    let cfg = config(vec![]);
    let extra_env = vec![("FOO".to_string(), "1".to_string())];
    let p = cfg.build(extra_env, vec![], SessionMode::Resume);
    assert_eq!(p.env.len(), 2);
    assert_eq!(p.env[0].0, "ANTHROPIC_BASE_URL");
    assert_eq!(p.env[1], ("FOO".to_string(), "1".to_string()));
}

#[test]
fn command_accessor() {
    let cfg = config(vec![]);
    assert_eq!(cfg.command(), "claude");
}

#[test]
fn session_id_stable_across_builds() {
    let cfg = config(vec![]);
    let p1 = cfg.build(vec![], vec![], SessionMode::Initial);
    let p2 = cfg.build(vec![], vec![], SessionMode::Resume);
    // Both use the same session ID
    let initial_id = &p1.args[p1.args.len() - 1];
    let resume_id = &p2.args[p2.args.len() - 1];
    assert_eq!(initial_id, resume_id);
    assert_eq!(initial_id, cfg.session_id());
}

#[test]
fn different_configs_get_different_ids() {
    let cfg1 = config(vec![]);
    let cfg2 = config(vec![]);
    assert_ne!(cfg1.session_id(), cfg2.session_id());
}

// -- flag+value stripping (#1, #8) --------------------------------------------

#[test]
fn initial_strips_resume_with_value() {
    let cfg = config(vec!["--resume", "old-id", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert!(!p.args.contains(&"--resume".to_string()));
    assert!(!p.args.contains(&"old-id".to_string()));
    assert_eq!(p.args, vec!["--model", "opus", "--session-id", cfg.session_id()]);
}

#[test]
fn initial_strips_session_id_with_value() {
    let cfg = config(vec!["--session-id", "old-id", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert!(!p.args.contains(&"old-id".to_string()));
    assert_eq!(p.args, vec!["--model", "opus", "--session-id", cfg.session_id()]);
}

#[test]
fn resume_strips_resume_with_value() {
    let cfg = config(vec!["--resume", "old-id", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert!(!p.args.contains(&"old-id".to_string()));
    assert_eq!(p.args, vec!["--model", "opus", "--resume", cfg.session_id()]);
}

#[test]
fn resume_strips_session_id_with_value() {
    let cfg = config(vec!["--session-id", "old-id", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Resume);
    assert!(!p.args.contains(&"old-id".to_string()));
    assert_eq!(p.args, vec!["--model", "opus", "--resume", cfg.session_id()]);
}

#[test]
fn continue_has_no_value_to_strip() {
    // --continue is a bare flag (no value), next arg should be preserved
    let cfg = config(vec!["--continue", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], SessionMode::Initial);
    assert!(!p.args.contains(&"--continue".to_string()));
    assert!(p.args.contains(&"--model".to_string()));
    assert!(p.args.contains(&"opus".to_string()));
}
