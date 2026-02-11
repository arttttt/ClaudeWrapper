# Per-Agent Backend Routing for Agent Teams

**Date**: 2026-02-11
**Status**: Draft / RFC
**Parent**: [Agent Teams Integration (Ctrl+T)](agent-teams-integration.md)

## Problem

When Claude Code spawns an Agent Team (1 lead + N teammates), every agent
makes API requests through the same backend with the same API key. A 4-agent
team on Opus costs ~$10-20 per session. Most teammate work (code review,
testing, simple refactors) doesn't need a frontier model.

## Goal

Route lead requests through an expensive backend (Anthropic/Opus) and
teammate requests through a cheap backend (OpenRouter/Sonnet), transparently.
No changes to Claude Code internals.

## Background

### How AnyClaude Already Works

AnyClaude spawns Claude Code as a child process with:

```
ANTHROPIC_BASE_URL=http://127.0.0.1:{PORT}
```

All API requests go through our local proxy, which forwards them to the
configured backend. This is how we already support multi-backend switching
(Ctrl+B), metrics, and thinking block management.

### How Agent Teams Spawn

Claude Code's lead process spawns teammates as child processes. Each teammate
inherits the parent's environment, including `ANTHROPIC_BASE_URL`. This means
**all teammate traffic already flows through our proxy** — we just can't
distinguish it from the lead's traffic.

### Teammate Environment Variables

Confirmed from Claude Code binary analysis and official docs. Each teammate
process has these env vars set before exec:

| Variable | Example | Purpose |
|----------|---------|---------|
| `CLAUDE_CODE_TEAM_NAME` | `"debug-session"` | Team namespace |
| `CLAUDE_CODE_AGENT_ID` | `"abc-123"` | Unique agent identifier |
| `CLAUDE_CODE_AGENT_NAME` | `"investigator-a"` | Display name |
| `CLAUDE_CODE_AGENT_TYPE` | `"teammate"` | Role (lead has no type or different value) |

### Prior Art: HydraTeams

HydraTeams is a standalone proxy that solves the same problem. It detects
teammates via "hidden marker in CLAUDE.md" — a fragile hack. Our approach
is cleaner because we control the process spawn chain.

---

## Design: PATH Shim

### Core Idea

Intercept teammate process spawning via a `claude` shim script placed first
in PATH. The shim detects teammates by their env vars and modifies
`ANTHROPIC_BASE_URL` to route them through a different proxy path.

### Architecture

```
AnyClaude
  |
  +-- Proxy :PORT
  |     |-- /v1/messages          --> lead backend (Anthropic/Opus)
  |     '-- /teammate/v1/messages --> teammate backend (OpenRouter/Sonnet)
  |
  '-- PTY: PATH={shim_dir}:$PATH claude ...
             |
             '-- lead process (ANTHROPIC_BASE_URL=http://127.0.0.1:PORT)
                   |
                   '-- spawns teammate
                        |
                        '-- {shim_dir}/claude (our shim)
                             sees CLAUDE_CODE_AGENT_TYPE != ""
                             sets ANTHROPIC_BASE_URL=http://127.0.0.1:PORT/teammate
                             exec {real_claude} "$@"
```

### Shim Script

Generated at runtime by AnyClaude into a temp directory:

```bash
#!/bin/bash
# AnyClaude teammate routing shim.
# Intercepts Claude Code teammate spawns to route API requests
# through a separate (cheaper) backend.

if [ -n "$CLAUDE_CODE_AGENT_TYPE" ]; then
  export ANTHROPIC_BASE_URL="http://127.0.0.1:__PORT__/teammate"
fi

exec "__REAL_CLAUDE__" "$@"
```

AnyClaude replaces `__PORT__` and `__REAL_CLAUDE__` before writing the file.

`__REAL_CLAUDE__` is resolved at startup by scanning PATH (excluding the
shim directory) for the real `claude` binary.

### Proxy Changes

The proxy router gains a path-prefix check:

```rust
fn resolve_backend(request: &Request) -> &BackendConfig {
    let path = request.uri().path();
    if path.starts_with("/teammate/") {
        // Strip prefix, forward to teammate backend
        return &config.teammate_backend;
    }
    // Default: lead backend (existing behavior)
    &config.active_backend
}
```

Teammate requests arrive as `/teammate/v1/messages`. The proxy strips the
`/teammate` prefix and forwards `/v1/messages` to the teammate backend.

### Configuration

```toml
# ~/.config/anyclaude/config.toml

# Existing backend config (used for lead)
[[backends]]
name = "anthropic"
display_name = "Anthropic (Lead)"
base_url = "https://api.anthropic.com"
auth_type = "passthrough"

# Teammate backend
[[backends]]
name = "openrouter-sonnet"
display_name = "OpenRouter (Teammates)"
base_url = "https://openrouter.ai/api/v1"
auth_type = "bearer"
api_key = "${OPENROUTER_API_KEY}"

# Routing config
[agent_teams]
teammate_backend = "openrouter-sonnet"  # backend name from [[backends]]
```

When `agent_teams.teammate_backend` is not set, the shim is not generated
and all agents use the active backend (current behavior).

---

## Implementation Plan

### Phase 1: Shim + Path Routing (MVP)

**Goal**: Lead on backend A, teammates on backend B.

#### New Files

| File | Purpose |
|------|---------|
| `src/shim.rs` | Generate shim script, resolve real claude path, manage temp dir |
| `src/proxy/teammate_route.rs` | `/teammate/` prefix detection and stripping |

#### Modified Files

| File | Change |
|------|--------|
| `src/config/types.rs` | Add `AgentTeamsConfig { teammate_backend: Option<String> }` |
| `src/proxy/router.rs` | Integrate teammate routing |
| `src/pty/spawn_config.rs` | Prepend shim dir to PATH |
| `src/ui/runtime.rs` | Initialize shim on startup |

#### Flow

1. AnyClaude starts, reads config
2. If `agent_teams.teammate_backend` is set:
   a. Resolve real `claude` binary path
   b. Generate shim script in temp dir
   c. Prepend temp dir to PATH in PTY spawn env
3. Lead process starts, makes requests to `/v1/messages` → lead backend
4. Lead spawns teammate → teammate runs through shim → requests go to
   `/teammate/v1/messages` → teammate backend
5. Proxy strips prefix, forwards to teammate backend

### Phase 2: Per-Agent Metrics

**Goal**: Track cost/tokens separately for lead vs teammates.

The proxy already has metrics collection. Extend it to tag requests with
agent role based on the path prefix:

```rust
struct RequestMetrics {
    // ... existing fields ...
    agent_role: AgentRole,  // Lead | Teammate
}

enum AgentRole {
    Lead,
    Teammate,
}
```

Status popup (Ctrl+S) shows breakdown:

```
Lead (Anthropic):     $1.80  (32 req)
Teammates (OR):       $0.54  (15 req)
Total:                $2.34  (47 req)
Saved vs all-Opus:    ~$4.70
```

### Phase 3: Per-Teammate Routing (Optional)

If we want different backends per teammate (not just lead vs all teammates),
the shim can use `CLAUDE_CODE_AGENT_NAME` to look up a mapping:

```toml
[agent_teams.routing]
default = "openrouter-sonnet"
"architect" = "anthropic"        # architect gets Opus
"test-runner" = "openrouter-haiku"  # test runner gets cheapest
```

The shim would encode the agent name in the URL path:
`/teammate/{agent-name}/v1/messages`

This is optional and can be added later without breaking Phase 1.

---

## Edge Cases

| Case | Handling |
|------|----------|
| Agent Teams disabled | No shim generated, all traffic goes through lead backend |
| Teammate backend not configured | Same as above |
| Teammate backend is same as lead | Shim still works, just no cost difference |
| Real `claude` not found in PATH | Error at startup, refuse to generate shim |
| Shim dir cleanup on crash | Use temp dir that OS cleans up; also clean in TerminalGuard |
| Claude Code updates change spawn mechanism | Shim is a no-op if env vars aren't set; graceful degradation |
| `--teammate-mode in-process` | Teammates are still separate processes with env vars; shim works |
| Lead process itself has CLAUDE_CODE_AGENT_TYPE | Unlikely; verify empirically. If so, shim checks for specific value |

## Open Questions

1. **Does the lead process have `CLAUDE_CODE_AGENT_TYPE` set?**
   If yes, we need to distinguish by value (e.g., `"lead"` vs `"teammate"`).
   If no, the simple `[ -n "$CLAUDE_CODE_AGENT_TYPE" ]` check works.
   **Action**: Test empirically with `env | grep CLAUDE_CODE` in both contexts.

2. **Does `ANTHROPIC_BASE_URL` with a path prefix work with Claude Code?**
   Claude Code likely appends `/v1/messages` to the base URL. If the base URL
   is `http://127.0.0.1:PORT/teammate`, requests go to
   `http://127.0.0.1:PORT/teammate/v1/messages`. Need to verify.
   **Action**: Test with a simple proxy that logs request paths.

3. **Thinking block compatibility for teammate backend.**
   If teammates use a non-Anthropic backend, thinking blocks need translation.
   AnyClaude already handles this via `thinking_compat` config.
   Should work out of the box if teammate backend has `thinking_compat = true`.

4. **Model override in teammate prompts.**
   Users can ask the lead to "use Sonnet for teammates" — Claude Code may
   set a model field in the API request. If the teammate backend doesn't
   support that model name, the request fails.
   **Mitigation**: Proxy can strip/remap the `model` field for teammate requests.

---

## Cost Analysis

Example: 4-agent team, 1-hour session.

| Scenario | Lead | 3 Teammates | Total | Savings |
|----------|------|-------------|-------|---------|
| All Opus (Anthropic) | $5 | $15 | $20 | — |
| Lead Opus + Teammates Sonnet (OpenRouter) | $5 | $2.25 | $7.25 | 64% |
| Lead Opus + Teammates Haiku (OpenRouter) | $5 | $0.45 | $5.45 | 73% |

Assumes ~100k input + 30k output tokens per agent per hour.
