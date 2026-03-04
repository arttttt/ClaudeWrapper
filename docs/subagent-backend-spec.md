# Subagent Backend Selection for the Main Client

## Context

Claude Code supports the env var `CLAUDE_CODE_SUBAGENT_MODEL` — it overrides the model for **all** subagents of the process. CC **does not propagate** this variable to teammates (the `h28` array in the binary only contains `CLAUDE_CODE_USE_BEDROCK/VERTEX/FOUNDRY`, `ANTHROPIC_BASE_URL`, `CLAUDE_CONFIG_DIR`).

AnyClaude already has `detect_marker_model()` (`src/proxy/pipeline/routing.rs:80-109`) — it detects `marker-` and `anyclaude-` prefixes in the model name and routes to the corresponding backend. `model_map` on backends also works — it rewrites the model before forwarding upstream.

**Goal:** allow the user to choose a backend for subagents of the main client (not teammates).

**Mechanism:** set `CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-subagent` once at process start (fixed marker) → subagents send `"model": "anyclaude-subagent"` → `detect_marker_model()` sees the special marker → looks up current subagent backend from shared runtime state (`SubagentBackend`) → routes to that backend → `transform` rewrites the model via backend's `model_map`. Changing subagent backend = updating shared state, **no PTY restart**.

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
┌──────────────────────────────────────────────────────────────────┐
│                     AnyClaude (main)                             │
│                                                                  │
│  config.toml:                                                    │
│    [agent_teams]                                                 │
│    teammate_backend = "cheap-api"                                │
│    subagent_backend = "cheap-api"  ← initial value               │
│                                                                  │
│  env_builder → CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-subagent     │
│                (fixed marker, set once at PTY start)             │
│                                                                  │
│  SubagentBackend (Arc<RwLock<Option<String>>>)                   │
│    └─ shared runtime state, updated via UI without PTY restart   │
│    └─ initialized from config.subagent_backend on start          │
│                                                                  │
│  ┌──────────────────────────┐                                    │
│  │  Claude Code (PTY)       │                                    │
│  │  env: CLAUDE_CODE_       │                                    │
│  │    SUBAGENT_MODEL=       │                                    │
│  │    anyclaude-subagent    │  (always the same fixed marker)    │
│  │                          │                                    │
│  │  Main agent → proxy      │──→ active backend (routing.rs)     │
│  │  Subagent  → proxy       │──→ detect "anyclaude-subagent" →   │
│  │                          │    read SubagentBackend state →    │
│  │                          │    route to cheap-api              │
│  │  Teammate  → /teammate   │──→ cheap-api (BackendOverride)     │
│  │    └─ Subagent → proxy   │──→ active backend (no marker)      │
│  └──────────────────────────┘                                    │
└──────────────────────────────────────────────────────────────────┘
```

**Subagent request flow:**
1. CC spawns subagent with `model: "anyclaude-subagent"` (from env var, fixed)
2. Subagent makes API request: `POST /v1/messages {"model": "anyclaude-subagent", ...}`
3. `routing.rs::detect_marker_model()` → sees `"anyclaude-subagent"` → special case
4. Reads `SubagentBackend` shared state → current value: `"cheap-api"`
5. Routes to `cheap-api` backend
6. `transform.rs` → `backend.resolve_model(...)` → `model_map` → real model
7. `headers.rs` → auth headers for `cheap-api` backend
8. `forward.rs` → request goes to upstream URL of `cheap-api` backend

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
    /// Used as initial value for SubagentBackend runtime state.
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

### 3. Shared state: `SubagentBackend`

**File:** `src/backend/state.rs` (or new file `src/proxy/subagent_state.rs`)

```rust
use std::sync::{Arc, RwLock};

/// Runtime state for subagent backend routing.
/// Initialized from config on startup, updated via UI (Ctrl+B popup).
/// Read by detect_marker_model() on every subagent request.
#[derive(Clone)]
pub struct SubagentBackend {
    inner: Arc<RwLock<Option<String>>>,
}

impl SubagentBackend {
    pub fn new(initial: Option<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(initial)),
        }
    }

    /// Get current subagent backend name.
    pub fn get(&self) -> Option<String> {
        self.inner.read().unwrap().clone()
    }

    /// Set subagent backend. None = disable (inherit parent model).
    pub fn set(&self, backend: Option<String>) {
        *self.inner.write().unwrap() = backend;
    }
}
```

### 4. Routing: special case for `anyclaude-subagent`

**File:** `src/proxy/pipeline/routing.rs`

In `detect_marker_model()`, add special case **before** the generic `anyclaude-` prefix handling:

```rust
// Special marker: subagent routing via runtime state
if model == "anyclaude-subagent" {
    if let Some(backend_name) = subagent_state.get() {
        return Some(RoutingDecision::backend(
            &backend_name,
            "subagent marker model",
        ));
    }
    // No subagent backend configured — fall through to default routing
    return None;
}

// Generic anyclaude-{backend} handling (existing code)
if let Some(backend) = model.strip_prefix("anyclaude-") {
    // ...
}
```

`detect_marker_model()` needs access to `SubagentBackend` — add it as a parameter or access via shared state passed to the pipeline context.

### 5. EnvSet: `with_subagent_routing` method

**File:** `src/args/env_builder.rs`

```rust
/// Set CLAUDE_CODE_SUBAGENT_MODEL to the fixed "anyclaude-subagent" marker.
///
/// When enabled, Claude Code will use "anyclaude-subagent" as the model name
/// for all subagents. The proxy's detect_marker_model() will treat this
/// as a special case and look up the current subagent backend from
/// shared runtime state (SubagentBackend).
pub fn with_subagent_routing(mut self, enabled: bool) -> Self {
    if enabled {
        self.vars.push((
            "CLAUDE_CODE_SUBAGENT_MODEL".into(),
            "anyclaude-subagent".into(),
        ));
    }
    self
}
```

### 6. Pipeline: pass `subagent_routing` flag

**File:** `src/args/pipeline.rs`

Update signatures of `build_spawn_params` and `build_restart_params`:

```rust
pub fn build_spawn_params(
    raw_args: &[String],
    proxy_url: &str,
    session_token: &str,
    settings: &ClaudeSettingsManager,
    shim: Option<&TeammateShim>,
    subagent_routing: bool,  // NEW — just a flag
) -> SpawnParams
```

In the `EnvSet::new()` chain:

```rust
let env = EnvSet::new()
    .with_proxy_url(proxy_url)
    .with_session_token(session_token)
    .with_settings(settings)
    .with_shim(shim)
    .with_subagent_routing(subagent_routing)  // NEW
    .build();
```

Same for `build_restart_params`:

```rust
let env = EnvSet::new()
    .with_proxy_url(proxy_url)
    .with_session_token(session_token)
    .with_settings(settings)
    .with_shim(shim)
    .with_subagent_routing(subagent_routing)  // NEW
    .with_extra(extra_env)
    .build();
```

### 7. Runtime: initialize shared state, pass flag on spawn

**File:** `src/ui/runtime.rs`

On startup (in `run()`):

```rust
// Initialize subagent backend shared state from config
let subagent_initial = config_store.get().agent_teams
    .as_ref()
    .and_then(|at| at.subagent_backend.clone());
let subagent_state = SubagentBackend::new(subagent_initial.clone());

// Pass to proxy pipeline context (so detect_marker_model can read it)
// ... (depends on how pipeline context is structured)

// Determine if subagent routing is enabled (for env var)
let subagent_routing = subagent_initial.is_some();
```

Pass `subagent_routing` to `build_spawn_params(...)` and `build_restart_params(...)`.

### 8. UI: subagent backend selection in Backend Switch popup (Ctrl+B)

#### 8a. App state

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

#### 8b. UiCommand

**File:** `src/ui/app.rs`

```rust
pub enum UiCommand {
    SwitchBackend { backend_id: String },
    SetSubagentBackend { backend_id: Option<String> },  // NEW
    RestartClaude,
    // ...
}
```

#### 8c. Input handling

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

#### 8d. Rendering

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

#### 8e. Runtime: handling SetSubagentBackend

**File:** `src/ui/runtime.rs`

```rust
UiCommand::SetSubagentBackend { backend_id } => {
    // 1. Update app UI state
    app.set_subagent_backend(backend_id.clone());

    // 2. Update shared proxy state — no PTY restart needed!
    subagent_state.set(backend_id);
}
```

Next subagent request from CC will automatically route to the new backend via `detect_marker_model()`.

---

## Files That Do NOT Need Changes

| File | Reason |
|------|--------|
| `src/proxy/pipeline/transform.rs` | `model_map` on backends already rewrites the model |
| `src/proxy/router.rs` | Routing via main pipeline, no new route needed |
| `src/shim/tmux.rs` | No shim injection needed (main client only) |
| `src/ipc/` | No new IPC commands needed (subagent_backend is local state) |

---

## Commit Order

### Commit 1: `feat(config): add subagent_backend to AgentTeamsConfig`
- `src/config/types.rs` — new field `subagent_backend: Option<String>`
- `src/config/loader.rs` — validation

### Commit 2: `feat(proxy): subagent backend runtime state and routing`
- `SubagentBackend` shared state (new or in `src/backend/state.rs`)
- `src/proxy/pipeline/routing.rs` — special case `"anyclaude-subagent"` in `detect_marker_model()`
- `src/args/env_builder.rs` — `with_subagent_routing()` method
- `src/args/pipeline.rs` — new `subagent_routing: bool` parameter
- `src/ui/runtime.rs` — initialize `SubagentBackend` from config, pass to proxy, pass flag to spawn

### Commit 3: `feat(ui): subagent backend selection in backend popup`
- `src/ui/app.rs` — state (`BackendPopupSection`, `subagent_selection`, `subagent_backend`), methods, `UiCommand::SetSubagentBackend`
- `src/ui/input.rs` — Tab sections, Up/Down/Enter/Delete per section
- `src/ui/render.rs` — two-section popup
- `src/ui/runtime.rs` — handle `SetSubagentBackend` → update `SubagentBackend` shared state (no restart)

---

## Verification

1. **Config:** add `subagent_backend = "openrouter"` to `~/.config/anyclaude/config.toml` → AnyClaude starts without errors
2. **Env:** check in debug log that `CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-subagent` is present in PTY process env (fixed marker, always the same)
3. **Routing:** launch Claude Code, create subagent (Task tool) → proxy debug log shows: `routing_decision: { backend: "openrouter", reason: "subagent marker model" }`
4. **Model rewrite:** in forwarded request to upstream — model is rewritten via `model_map` (not `anyclaude-subagent`)
5. **UI:** Ctrl+B → two sections, Tab switches, Enter selects → **no PTY restart**, next subagent request routes to new backend
6. **Runtime switch:** change subagent backend via Ctrl+B → immediately verify in proxy debug log that next subagent request routes to the new backend
7. **Teammates:** check in shim log (`~/.config/anyclaude/tmux_shim.log`) that `CLAUDE_CODE_SUBAGENT_MODEL` is NOT present in send-keys command
8. **Tests:** `cargo test` — all existing tests pass
9. **Validation:** `subagent_backend = "nonexistent"` → error on startup

---

## Limitations and Edge Cases

1. **Main client only** — teammates do not receive the env var (by design)
2. **`model_map` is required on the target backend** — if `model_map` is not set, upstream will receive the model name as-is and may return an error. Either:
   - Validate that `subagent_backend` has a `model_map` entry
   - Or document the requirement
3. **frontmatter `model:` in agents** — env var takes priority over frontmatter, see `getSubagentModel()` above
4. **`parse()` in CC** — `CLAUDE_CODE_SUBAGENT_MODEL=anyclaude-subagent` goes through `parse()` in CC. Need to verify that `parse()` passes arbitrary strings (not just `opus`/`sonnet`/`haiku`)
5. **Thread safety** — `SubagentBackend` uses `Arc<RwLock<...>>`, safe for concurrent reads from proxy threads and writes from UI thread
