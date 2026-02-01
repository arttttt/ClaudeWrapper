//! PTY lifecycle state machine.
//!
//! Manages input buffering during Claude Code startup to prevent message loss
//! when Gas Town sends messages before the internal process is ready.

use crate::pty::PtyHandle;
use std::collections::VecDeque;

/// PTY lifecycle state machine.
/// Ensures input is buffered until Claude Code is ready to receive it.
pub enum PtyState {
    /// PTY not yet attached, buffering input
    Pending {
        buffer: VecDeque<Vec<u8>>,
    },
    /// PTY attached but Claude Code not ready (no output yet)
    Attached {
        pty: PtyHandle,
        buffer: VecDeque<Vec<u8>>,
    },
    /// Claude Code ready (produced output), input goes directly to PTY
    Ready {
        pty: PtyHandle,
    },
}

impl Default for PtyState {
    fn default() -> Self {
        PtyState::Pending {
            buffer: VecDeque::new(),
        }
    }
}

impl PtyState {
    /// Send input - buffers if not ready, sends directly if ready.
    pub fn send_input(&mut self, bytes: &[u8]) {
        match self {
            PtyState::Pending { buffer } | PtyState::Attached { buffer, .. } => {
                buffer.push_back(bytes.to_vec());
            }
            PtyState::Ready { pty } => {
                let _ = pty.send_input(bytes);
            }
        }
    }

    /// Get PTY handle if attached (in Attached or Ready state).
    pub fn pty_handle(&self) -> Option<&PtyHandle> {
        match self {
            PtyState::Pending { .. } => None,
            PtyState::Attached { pty, .. } | PtyState::Ready { pty } => Some(pty),
        }
    }

    /// Attach PTY. Transitions from Pending to Attached.
    pub fn attach(&mut self, pty: PtyHandle) {
        *self = match std::mem::take(self) {
            PtyState::Pending { buffer } => PtyState::Attached { pty, buffer },
            PtyState::Attached { buffer, .. } => PtyState::Attached { pty, buffer },
            PtyState::Ready { .. } => PtyState::Ready { pty },
        };
    }

    /// Called when PTY produces output. Transitions to Ready and flushes buffer.
    pub fn on_output(&mut self) {
        *self = match std::mem::take(self) {
            PtyState::Attached { pty, mut buffer } => {
                // Flush buffered input - Claude Code is ready
                while let Some(bytes) = buffer.pop_front() {
                    let _ = pty.send_input(&bytes);
                }
                PtyState::Ready { pty }
            }
            other => other,
        };
    }
}
