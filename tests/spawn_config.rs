mod common;

use anyclaude::pty::PtySpawnConfig;

fn config(base_args: Vec<&str>) -> PtySpawnConfig {
    PtySpawnConfig::new(
        "claude".to_string(),
        base_args.into_iter().map(String::from).collect(),
        "http://localhost:3000".to_string(),
    )
}

// -- initial spawn ------------------------------------------------------------

#[test]
fn initial_uses_base_args_as_is() {
    let cfg = config(vec!["--model", "opus"]);
    let p = cfg.build(vec![], vec![], false);
    assert_eq!(p.args, vec!["--model", "opus"]);
}

#[test]
fn initial_preserves_user_continue_flag() {
    let cfg = config(vec!["--continue"]);
    let p = cfg.build(vec![], vec![], false);
    assert_eq!(p.args, vec!["--continue"]);
}

#[test]
fn initial_env_contains_base_url() {
    let cfg = config(vec![]);
    let p = cfg.build(vec![], vec![], false);
    assert_eq!(p.env.len(), 1);
    assert_eq!(p.env[0].0, "ANTHROPIC_BASE_URL");
    assert_eq!(p.env[0].1, "http://localhost:3000");
}

// -- restart ------------------------------------------------------------------

#[test]
fn restart_adds_continue() {
    let cfg = config(vec!["--model", "opus"]);
    let p = cfg.build(vec![], vec![], true);
    assert_eq!(p.args, vec!["--model", "opus", "--continue"]);
}

#[test]
fn restart_deduplicates_continue() {
    let cfg = config(vec!["--continue", "--model", "opus"]);
    let p = cfg.build(vec![], vec![], true);
    assert_eq!(p.args, vec!["--model", "opus", "--continue"]);
}

#[test]
fn restart_deduplicates_multiple_continues() {
    let cfg = config(vec!["--continue", "--continue", "--model"]);
    let p = cfg.build(vec![], vec![], true);
    assert_eq!(p.args, vec!["--model", "--continue"]);
}

#[test]
fn restart_appends_extra_args() {
    let cfg = config(vec!["--model", "opus"]);
    let extra = vec!["--verbose".to_string()];
    let p = cfg.build(vec![], extra, true);
    assert_eq!(p.args, vec!["--model", "opus", "--continue", "--verbose"]);
}

#[test]
fn restart_merges_extra_env() {
    let cfg = config(vec![]);
    let extra_env = vec![("FOO".to_string(), "1".to_string())];
    let p = cfg.build(extra_env, vec![], true);
    assert_eq!(p.env.len(), 2);
    assert_eq!(p.env[0].0, "ANTHROPIC_BASE_URL");
    assert_eq!(p.env[1], ("FOO".to_string(), "1".to_string()));
}

#[test]
fn empty_base_args_restart_only_continue() {
    let cfg = config(vec![]);
    let p = cfg.build(vec![], vec![], true);
    assert_eq!(p.args, vec!["--continue"]);
}

#[test]
fn command_accessor() {
    let cfg = config(vec![]);
    assert_eq!(cfg.command(), "claude");
}
