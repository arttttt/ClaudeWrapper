//! Tests for Ctrl+R Claude Code restart feature.

mod common;

use anyclaude::ui::app::UiCommand;
use anyclaude::ui::pty::PtyLifecycleState;
use common::*;
use tokio::sync::mpsc;

#[test]
fn request_restart_transitions_to_restarting() {
    let mut app = make_app();
    let (tx, _rx) = mpsc::channel(8);
    app.set_ipc_sender(tx);

    // Get to Ready state
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::Attach);
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::GotOutput);
    assert!(app.is_pty_ready());

    app.request_restart_claude();
    assert!(app.pty_lifecycle.is_restarting());
}

#[test]
fn request_restart_sends_command() {
    let mut app = make_app();
    let (tx, mut rx) = mpsc::channel(8);
    app.set_ipc_sender(tx);

    app.request_restart_claude();

    let cmd = rx.try_recv().expect("should have received a command");
    assert!(
        matches!(cmd, UiCommand::RestartClaude),
        "expected RestartClaude, got {:?}",
        cmd
    );
}

#[test]
fn request_restart_without_sender_reverts_to_spawn_failed() {
    let mut app = make_app();
    // No IPC sender set — send_command will fail

    // Get to Ready
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::Attach);
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::GotOutput);
    assert!(app.is_pty_ready());

    app.request_restart_claude();

    // Should revert to Pending (SpawnFailed from Ready→Restarting→SpawnFailed)
    assert!(
        matches!(app.pty_lifecycle, PtyLifecycleState::Pending { .. }),
        "expected Pending after failed restart, got {:?}",
        std::mem::discriminant(&app.pty_lifecycle)
    );
}

#[test]
fn restart_clears_input_buffer() {
    let mut app = make_app();
    let (tx, _rx) = mpsc::channel(8);
    app.set_ipc_sender(tx);

    // Buffer some input in Attached state
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::Attach);
    app.send_input(b"hello");

    match &app.pty_lifecycle {
        PtyLifecycleState::Attached { buffer } => assert_eq!(buffer.len(), 1),
        other => panic!("expected Attached, got {:?}", std::mem::discriminant(other)),
    }

    // Restart
    app.request_restart_claude();
    assert!(app.pty_lifecycle.is_restarting());

    // Re-attach — buffer should be empty
    app.dispatch_pty(anyclaude::ui::pty::PtyIntent::Attach);
    match &app.pty_lifecycle {
        PtyLifecycleState::Attached { buffer } => {
            assert!(buffer.is_empty(), "buffer should be cleared after restart");
        }
        other => panic!("expected Attached, got {:?}", std::mem::discriminant(other)),
    }
}
