# Subagent Backend Selection for the Main Client

## Context

Claude Code supports the env var `CLAUDE_CODE_SUBAGENT_MODEL` — it overrides the model for **all** subagents of the process. CC **does not propagate** this variable to teammates (the `h28` array in the binary only contains `CLAUDE_CODE_USE_BEDROCK/VERTEX/FOUNDRY`, `ANTHROPIC_BASE_URL`, `CLAUDE_CONFIG_DIR`).

AnyClaude already has `detect_marker_model()` (`src/proxy/pipeline/routing.rs:80-109`) — it detects `marker-` and `anyclaude-` prefixes in the model name and routes to the corresponding backend. `model_map` on backends also works — it rewrites the model before forwarding upstream.

**Goal:** allow the user to choose a backend for subagents of the main client (not teammates).

**Mechanism:** set `CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-{backend_name}` for the main process → subagents send `"model": "anyclaude-{backend}"` → `detect_marker_model()` routes → `transform` rewrites the model via backend's `model_map`.

### How CC Chooses the Subagent Model (reverse engineering v2.1.50+)

```js
function getSubagentModel(config, parentModel, frontmatterModel, permissionMode) {
  if (process.env.CLAUDE_CODE_SUBAGENT_MODEL)
    return parse(process.env.CLAUDE_CODE_SUBAGENT_MODEL);  // env var overrides everything
  if (frontmatterModel) return parse(frontmatterModel);     // model in agent .md
  if (config === "inherit")
    return getRuntimeMainLoopModel({ permissionMode, mainLoopModel: parentModel });
  return parse(config);
}
```

### What CC Passes to Teammates on Spawn (tmux send-keys)

**Always:**
- `CLAUDECODE=1`
- `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`

**If set (from `h28`):**
- `CLAUDE_CODE_USE_BEDROCK`
- `CLAUDE_CODE_USE_VERTEX`
- `CLAUDE_CODE_USE_FOUNDRY`
- `ANTHROPIC_BASE_URL`
- `CLAUDE_CONFIG_DIR`

`CLAUDE_CODE_SUBAGENT_MODEL` **is not in the list** → teammates will not receive this variable → their subagents will use default routing (active backend).

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     AnyClaude (main)                         │
│                                                              │
│  config.toml:                                                │
│    [agent_teams]                                             │
│    teammate_backend = "cheap-api"                            │
│    subagent_backend = "cheap-api"  ← NEW                     │
│                                                              │
│  env_builder → CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-cheap-api│
│                                                              │
│  ┌──────────────────────────┐                                │
│  │  Claude Code (PTY)       │                                │
│  │  env: CLAUDE_CODE_       │                                │
│  │    SUBAGENT_MODEL=       │                                │
│  │    anyclaude-cheap-api   │                                │
│  │                          │                                │
│  │  Main agent → proxy      │──→ active backend (routing.rs) │
│  │  Subagent  → proxy       │──→ cheap-api (marker model)    │
│  │  Teammate  → /teammate   │──→ cheap-api (BackendOverride) │
│  │    └─ Subagent → proxy   │──→ active backend (no marker)  │
│  └──────────────────────────┘                                │
└─────────────────────────────────────────────────────────────┘
```

**Subagent request flow:**
1. CC spawns subagent with `model: "anyclaude-cheap-api"` (from env var)
2. Subagent makes API request: `POST /v1/messages {"model": "anyclaude-cheap-api", ...}`
3. `routing.rs::detect_marker_model()` → sees `anyclaude-` prefix → routing decision: `cheap-api`
4. `transform.rs` → `backend.resolve_model("anyclaude-cheap-api")` → `model_map` → real model
5. `headers.rs` → auth headers for `cheap-api` backend
6. `forward.rs` → request goes to upstream URL of `cheap-api` backend

---

## Changes by File

### 1. Config: add `subagent_backend` field

**File:** `src/config/types.rs` (line ~249)

```rust
/// Agent Teams routing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeamsConfig {
    /// Backend name for teammate requests (must exist in [[backends]]).
    pub teammate_backend: String,
    /// Backend for subagents of the main client (optional).
    /// Sets CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-{backend} for the main process.
    /// Does NOT affect teammates — CC does not propagate this env var.
    #[serde(default)]
    pub subagent_backend: Option<String>,
}
```

**Example config.toml:**
```toml
[agent_teams]
teammate_backend = "openrouter"
subagent_backend = "openrouter"   # optional, if omitted — inherits parent model
```

### 2. Config validation

**File:** `src/config/loader.rs` (after line ~141, inside `if let Some(ref at) = self.agent_teams`)

```rust
if let Some(ref sb) = at.subagent_backend {
    if !self.backends.iter().any(|b| b.name == *sb) {
        return Err(ConfigError::ValidationError {
            message: format!(
                "agent_teams.subagent_backend '{}' not found in configured backends",
                sb
            ),
        });
    }
}
```

### 3. EnvSet: `with_subagent_backend` method

**File:** `src/args/env_builder.rs`

```rust
/// Set CLAUDE_CODE_SUBAGENT_MODEL for subagent routing via marker model.
///
/// When set, Claude Code will use "anyclaude-{backend}" as the model name
/// for all subagents. The proxy's detect_marker_model() will route these
/// requests to the specified backend, and model_map will rewrite the model
/// to the real model name before forwarding upstream.
pub fn with_subagent_backend(mut self, backend: Option<&str>) -> Self {
    if let Some(name) = backend {
        self.vars.push((
            "CLAUDE_CODE_SUBAGENT_MODEL".into(),
            format!("anyclaude-{}", name),
        ));
    }
    self
}
```

### 4. Pipeline: pass `subagent_backend` through

**File:** `src/args/pipeline.rs`

Update signatures of `build_spawn_params` and `build_restart_params`:

```rust
pub fn build_spawn_params(
    raw_args: &[String],
    proxy_url: &str,
    session_token: &str,
    settings: &ClaudeSettingsManager,
    shim: Option<&TeammateShim>,
    subagent_backend: Option<&str>,  // NEW
) -> SpawnParams
```

In the `EnvSet::new()` chain:

```rust
let env = EnvSet::new()
    .with_proxy_url(proxy_url)
    .with_session_token(session_token)
    .with_settings(settings)
    .with_shim(shim)
    .with_subagent_backend(subagent_backend)  // NEW — before extra
    .build();
```

Same for `build_restart_params`:

```rust
let env = EnvSet::new()
    .with_proxy_url(proxy_url)
    .with_session_token(session_token)
    .with_settings(settings)
    .with_shim(shim)
    .with_subagent_backend(subagent_backend)  // NEW — before extra
    .with_extra(extra_env)
    .build();
```

### 5. Runtime: pass `subagent_backend` on spawn

**File:** `src/ui/runtime.rs`

When calling `build_spawn_params` (at the start of `run()`) and `build_restart_params` (in `AppEvent::PtyRestart` handler):

```rust
let subagent_backend = config_store.get().agent_teams
    .as_ref()
    .and_then(|at| at.subagent_backend.as_deref());
```

Pass as parameter to `build_spawn_params(..., subagent_backend)` and `build_restart_params(..., subagent_backend)`.

### 6. UI: subagent backend selection in Backend Switch popup (Ctrl+B)

#### 6a. App state

**File:** `src/ui/app.rs`

New enum:
```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendPopupSection {
    ActiveBackend,
    SubagentBackend,
}
```

New fields in `App`:
```rust
/// Which section of backend popup is focused
backend_popup_section: BackendPopupSection,
/// Selection index for subagent backend list
subagent_selection: usize,
/// Current subagent backend (runtime state, from config on start)
subagent_backend: Option<String>,
```

New methods:
- `toggle_backend_popup_section()` — Tab switches between sections
- `move_subagent_selection(delta: i32)` — navigation in SubagentBackend section
- `request_set_subagent_backend(index)` → `UiCommand::SetSubagentBackend`
- `clear_subagent_backend()` → `UiCommand::SetSubagentBackend { backend_id: None }`
- `subagent_backend() -> Option<&str>` — getter
- `backend_popup_section() -> BackendPopupSection` — getter

On `reset_backend_selection()`:
```rust
self.backend_popup_section = BackendPopupSection::ActiveBackend;
self.subagent_selection = self.backends.iter()
    .position(|b| Some(b.id.as_str()) == self.subagent_backend.as_deref())
    .unwrap_or(0);
```

#### 6b. UiCommand

**File:** `src/ui/app.rs`

```rust
pub enum UiCommand {
    SwitchBackend { backend_id: String },
    SetSubagentBackend { backend_id: Option<String> },  // NEW
    RestartClaude,
    // ...
}
```

#### 6c. Input handling

**File:** `src/ui/input.rs`

In `handle_backend_switch_key()`:

```rust
KeyKind::Tab => {
    app.toggle_backend_popup_section();
}
KeyKind::Arrow(Direction::Up) => {
    match app.backend_popup_section() {
        BackendPopupSection::ActiveBackend => app.move_backend_selection(-1),
        BackendPopupSection::SubagentBackend => app.move_subagent_selection(-1),
    }
}
KeyKind::Arrow(Direction::Down) => {
    match app.backend_popup_section() {
        BackendPopupSection::ActiveBackend => app.move_backend_selection(1),
        BackendPopupSection::SubagentBackend => app.move_subagent_selection(1),
    }
}
KeyKind::Enter => {
    match app.backend_popup_section() {
        BackendPopupSection::ActiveBackend => return handle_backend_switch_enter(app),
        BackendPopupSection::SubagentBackend => return handle_subagent_backend_enter(app),
    }
}
KeyKind::Delete | KeyKind::Backspace => {
    if app.backend_popup_section() == BackendPopupSection::SubagentBackend {
        app.clear_subagent_backend();
        app.close_popup();
    }
}
```

#### 6d. Rendering

**File:** `src/ui/render.rs`

In `PopupKind::BackendSwitch` branch:

```
┌─ Select Backend ──────────────────────────┐
│                                            │
│  ▸ Active Backend                          │  ← Tab switches section
│  ─────────────────                         │
│  -> 1. Claude Direct    [Active]           │
│     2. OpenRouter       [Ready]            │
│     3. Local LLM        [Missing]          │
│                                            │
│    Subagent Backend                        │
│  ─────────────────                         │
│     Disabled (inherit parent model)        │
│  OR:                                       │
│     1. Claude Direct                       │
│  -> 2. OpenRouter       [Selected]         │
│     3. Local LLM                           │
│                                            │
│  Tab: Section  ↑↓: Move  Enter: Select     │
│  Del: Disable subagent  Esc: Close         │
└────────────────────────────────────────────┘
```

- Active section is highlighted (bold header or `▸` marker)
- SubagentBackend section — additional "Disabled" item or `[Selected]` marker on current
- Updated footer: `"Tab: Section  Up/Down: Move  Enter: Select  Del: Disable  Esc: Close"`

#### 6e. Runtime: handling SetSubagentBackend

**File:** `src/ui/runtime.rs`

```rust
UiCommand::SetSubagentBackend { backend_id } => {
    // 1. Update runtime state
    app.set_subagent_backend(backend_id.clone());

    // 2. Restart PTY with new CLAUDE_CODE_SUBAGENT_MODEL env var
    let subagent_backend = backend_id.as_deref();
    let spawn = build_restart_params(
        &raw_args,
        &actual_base_url,
        &session_token,
        &settings,
        _teammate_shim.as_ref(),
        subagent_backend,
        vec![],  // no extra env
        vec![],  // no extra args
    );
    respawn_pty(&mut app, spawn, &async_runtime);
}
```

**Important:** changing subagent_backend requires a PTY restart because `CLAUDE_CODE_SUBAGENT_MODEL` is an env var read by the Claude Code process at startup. The user will see a brief restart (similar to changing settings).

---

## Files That Do NOT Need Changes

| File | Reason |
|------|--------|
| `src/proxy/pipeline/routing.rs` | `detect_marker_model()` already handles `anyclaude-{backend}` |
| `src/proxy/pipeline/transform.rs` | `model_map` on backends already rewrites the model |
| `src/proxy/router.rs` | Routing via main pipeline, no new route needed |
| `src/shim/tmux.rs` | No shim injection needed (main client only) |
| `src/ipc/` | No new IPC commands needed (subagent_backend is local state) |
| `src/backend/state.rs` | No separate locks needed (subagent is not per-pane) |

---

## Commit Order

### Commit 1: `feat(config): add subagent_backend to AgentTeamsConfig`
- `src/config/types.rs` — new field `subagent_backend: Option<String>`
- `src/config/loader.rs` — validation

### Commit 2: `feat(args): inject CLAUDE_CODE_SUBAGENT_MODEL env var`
- `src/args/env_builder.rs` — `with_subagent_backend()` method
- `src/args/pipeline.rs` — new parameter in `build_spawn_params` and `build_restart_params`
- `src/ui/runtime.rs` — pass `subagent_backend` from config on spawn and restart

### Commit 3: `feat(ui): subagent backend selection in backend popup`
- `src/ui/app.rs` — state (`BackendPopupSection`, `subagent_selection`, `subagent_backend`), methods, `UiCommand::SetSubagentBackend`
- `src/ui/input.rs` — Tab sections, Up/Down/Enter/Delete per section
- `src/ui/render.rs` — two-section popup
- `src/ui/runtime.rs` — handle `SetSubagentBackend` → PTY restart

---

## Verification

1. **Config:** add `subagent_backend = "openrouter"` to `~/.config/anyclaude/config.toml` → AnyClaude starts without errors
2. **Env:** check in debug log that `CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-openrouter` is present in PTY process env
3. **Routing:** launch Claude Code, create subagent (Task tool) → proxy debug log shows: `routing_decision: { backend: "openrouter", reason: "subagent marker model" }`
4. **Model rewrite:** in forwarded request to upstream — model is rewritten via `model_map` (not `anyclaude-openrouter`)
5. **UI:** Ctrl+B → two sections, Tab switches, Enter selects → PTY restarts with new env
6. **Teammates:** check in shim log (`~/.config/anyclaude/tmux_shim.log`) that `CLAUDE_CODE_SUBAGENT_MODEL` is NOT present in send-keys command
7. **Tests:** `cargo test` — all existing tests pass
8. **Validation:** `subagent_backend = "nonexistent"` → error on startup

---

## Limitations and Edge Cases

1. **Changing subagent backend = PTY restart** — unavoidable since the env var is read by the CC process
2. **Main client only** — teammates do not receive the env var (by design)
3. **`model_map` is required on the target backend** — if `model_map` is not set, upstream will receive `"model": "anyclaude-openrouter"` and return an error. Either:
   - Validate that `subagent_backend` has a `model_map` covering `anyclaude-*`
   - Or document the requirement
4. **frontmatter `model:` in agents** — if an agent in `.claude/agents/` sets `model: haiku`, CC uses it instead of `CLAUDE_CODE_SUBAGENT_MODEL`. Actually no — env var takes priority, see `getSubagentModel()` above
5. **`parse()` in CC** — `CLAUDE_CODE_SUBAGENT_MODEL` goes through `parse()` in CC. If CC doesn't recognize `anyclaude-openrouter` as a valid model ID, there could be an issue. Need to verify that `parse()` passes arbitrary strings (not just `opus`/`sonnet`/`haiku`)
