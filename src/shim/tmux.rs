//! tmux PATH shim.
//!
//! Intercepts all tmux calls from Claude Code. For `send-keys` commands
//! that launch a teammate claude process, injects `ANTHROPIC_BASE_URL`
//! pointing to the `/teammate` prefix on our proxy and `ANTHROPIC_CUSTOM_HEADERS`
//! with the session token so the routing layer can authenticate and direct
//! teammate traffic to a cheaper backend.
//!
//! All other tmux commands are forwarded unchanged to the real binary.

use std::path::Path;

use anyhow::Result;

use super::write_executable;

/// Log file name inside the shim directory.
pub const LOG_FILENAME: &str = "tmux_shim.log";

const TEMPLATE: &str = r#"#!/bin/bash
# AnyClaude tmux shim — intercepts send-keys to inject teammate routing.
#
# Claude Code spawns teammates via:
#   tmux -L claude-swarm-PID send-keys -t %N \
#     cd /path && ENV=val /abs/path/claude --flags Enter
#
# We inject ANTHROPIC_BASE_URL=http://127.0.0.1:PORT/teammate before
# the absolute claude path so teammate requests hit the /teammate route.

SHIM_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_ENABLED=__LOG_ENABLED__
LOG="$SHIM_DIR/tmux_shim.log"
# Persistent log survives TempDir cleanup
PLOG="$HOME/.config/anyclaude/tmux_shim.log"

slog() {
  $LOG_ENABLED || return
  echo "[$(date '+%H:%M:%S.%N')] $1" | tee -a "$LOG" >> "$PLOG"
}

# Find real tmux, skipping our shim directory.
find_real_tmux() {
  local IFS=':'
  for d in $PATH; do
    [ "$d" = "$SHIM_DIR" ] && continue
    [ -x "$d/tmux" ] && echo "$d/tmux" && return
  done
}

REAL_TMUX="$(find_real_tmux)"
if [ -z "$REAL_TMUX" ]; then
  slog "ERROR: real tmux not found"
  echo "tmux: command not found (anyclaude shim)" >&2
  exit 127
fi

# Detect send-keys with claude invocation and inject ANTHROPIC_BASE_URL.
# Claude Code passes the entire command as ONE arg to send-keys (Case B),
# but we also handle individual args (Case A) for robustness.
INJECT_URL="ANTHROPIC_BASE_URL=http://127.0.0.1:__PORT__/teammate"
INJECT_HEADERS="ANTHROPIC_CUSTOM_HEADERS=x-session-token:__SESSION_TOKEN__"
args=()
has_send_keys=false
injected=false
for arg in "$@"; do
  if [ "$arg" = "send-keys" ]; then
    has_send_keys=true
    args+=("$arg")
    continue
  fi

  if $has_send_keys && ! $injected; then
    # Case A: claude path as standalone arg (/abs/path/claude)
    if [[ "$arg" == /* ]] && [[ "$arg" == *"/claude" ]]; then
      args+=("$INJECT_URL")
      args+=("$INJECT_HEADERS")
      args+=("$arg")
      injected=true
      slog "INJECT teammate URL + headers (standalone arg)"
      continue
    fi
    # Case B: claude path embedded in a longer string (confirmed format)
    if [[ "$arg" == *"/claude "* ]] || [[ "$arg" == *"/claude" ]]; then
      # Claude Code may pass its own ANTHROPIC_BASE_URL in the command.
      # We must REPLACE it (not add a second one) to avoid the original
      # overwriting ours depending on variable order.
      # Same for ANTHROPIC_CUSTOM_HEADERS — replace if present.
      # Uses bash regex (=~) — no sed subprocess or escaping issues.
      slog "BEFORE inject: $(printf '%q' "$arg")"

      # Replace or inject ANTHROPIC_BASE_URL
      if [[ "$arg" =~ ^(.*)ANTHROPIC_BASE_URL=[^[:space:]]+(.*) ]]; then
        arg="${BASH_REMATCH[1]}$INJECT_URL${BASH_REMATCH[2]}"
      elif [[ "$arg" =~ ^(.*[[:space:]])(/[^[:space:]]*/claude)([[:space:]].*|$) ]]; then
        arg="${BASH_REMATCH[1]}$INJECT_URL ${BASH_REMATCH[2]}${BASH_REMATCH[3]}"
      elif [[ "$arg" =~ ^(/[^[:space:]]*/claude)([[:space:]].*|$) ]]; then
        arg="$INJECT_URL ${BASH_REMATCH[1]}${BASH_REMATCH[2]}"
      fi

      # Replace or inject ANTHROPIC_CUSTOM_HEADERS
      if [[ "$arg" =~ ^(.*)ANTHROPIC_CUSTOM_HEADERS=[^[:space:]]+(.*) ]]; then
        arg="${BASH_REMATCH[1]}$INJECT_HEADERS${BASH_REMATCH[2]}"
      elif [[ "$arg" =~ ^(.*[[:space:]])(/[^[:space:]]*/claude)([[:space:]].*|$) ]]; then
        arg="${BASH_REMATCH[1]}$INJECT_HEADERS ${BASH_REMATCH[2]}${BASH_REMATCH[3]}"
      elif [[ "$arg" =~ ^(/[^[:space:]]*/claude)([[:space:]].*|$) ]]; then
        arg="$INJECT_HEADERS ${BASH_REMATCH[1]}${BASH_REMATCH[2]}"
      fi

      slog "AFTER  inject: $(printf '%q' "$arg")"
      args+=("$arg")
      injected=true
      slog "INJECT teammate URL + headers (embedded in string)"
      continue
    fi
  fi

  args+=("$arg")
done

if $injected; then
  slog "EXEC: $(printf '%q ' "${args[@]}")"
  exec "$REAL_TMUX" "${args[@]}"
else
  slog "tmux $*"
  exec "$REAL_TMUX" "$@"
fi
"#;

/// Install the tmux shim script into `dir`.
pub fn install(dir: &Path, proxy_port: u16, session_token: &str, log_enabled: bool) -> Result<()> {
    let script = TEMPLATE
        .replace("__PORT__", &proxy_port.to_string())
        .replace("__SESSION_TOKEN__", session_token)
        .replace("__LOG_ENABLED__", if log_enabled { "true" } else { "false" });
    write_executable(dir, "tmux", &script)
}
