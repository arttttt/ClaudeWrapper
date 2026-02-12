//! tmux PATH shim.
//!
//! Intercepts all tmux calls from Claude Code. For `send-keys` commands
//! that launch a teammate claude process, injects `ANTHROPIC_BASE_URL`
//! pointing to the `/teammate` prefix on our proxy so the routing layer
//! can direct teammate traffic to a cheaper backend.
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
LOG="$SHIM_DIR/tmux_shim.log"

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
  echo "[$(date '+%H:%M:%S.%N')] ERROR: real tmux not found" >> "$LOG"
  echo "tmux: command not found (anyclaude shim)" >&2
  exit 127
fi

# Detect send-keys with claude invocation and inject ANTHROPIC_BASE_URL.
# Claude Code passes the entire command as ONE arg to send-keys (Case B),
# but we also handle individual args (Case A) for robustness.
INJECT_URL="ANTHROPIC_BASE_URL=http://127.0.0.1:__PORT__/teammate"
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
      args+=("$arg")
      injected=true
      echo "[$(date '+%H:%M:%S.%N')] INJECT teammate URL (standalone arg)" >> "$LOG"
      continue
    fi
    # Case B: claude path embedded in a longer string (confirmed format)
    if [[ "$arg" == *"/claude "* ]] || [[ "$arg" == *"/claude" ]]; then
      # Insert env var before the absolute claude path.
      # [^ ]* matches any non-space chars including /, so multi-level paths work.
      # Handle both mid-string (/claude --flags) and end-of-string (/claude$).
      arg="$(printf '%s' "$arg" | sed -E "s| (/[^ ]*/claude)( |\$)| $INJECT_URL \1\2|")"
      args+=("$arg")
      injected=true
      echo "[$(date '+%H:%M:%S.%N')] INJECT teammate URL (embedded in string)" >> "$LOG"
      continue
    fi
  fi

  args+=("$arg")
done

if $injected; then
  echo "[$(date '+%H:%M:%S.%N')] tmux ${args[*]}" >> "$LOG"
  exec "$REAL_TMUX" "${args[@]}"
else
  echo "[$(date '+%H:%M:%S.%N')] tmux $*" >> "$LOG"
  exec "$REAL_TMUX" "$@"
fi
"#;

/// Install the tmux shim script into `dir`.
pub fn install(dir: &Path, proxy_port: u16) -> Result<()> {
    let script = TEMPLATE.replace("__PORT__", &proxy_port.to_string());
    write_executable(dir, "tmux", &script)
}
