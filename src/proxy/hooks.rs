//! Hook handlers for agent registration events.
//!
//! Subagents: CC fires SubagentStart/SubagentStop hooks when it spawns
//! or stops in-process subagents. We register the agent_id → backend
//! mapping and inject `⟨AC:{id}⟩` into additionalContext.
//!
//! Teammates: the tmux shim calls /api/teammate-start when it detects
//! a teammate spawn (--agent-id flag). We register the agent_id → backend
//! mapping. The shim injects x-agent-id header for routing lookup.

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::backend::{BackendState, AgentBackendState, AgentRegistry};

/// Axum state for hook endpoints — bundles the two pieces of state
/// that hook handlers need.
#[derive(Clone)]
pub struct HookState {
    pub backend_state: BackendState,
    pub subagent_backend: AgentBackendState,
    pub teammate_backend: AgentBackendState,
    pub registry: AgentRegistry,
}

/// Input sent by CC hook (piped via curl stdin).
///
/// CC sends various fields — we only deserialize what we use.
/// Unknown fields are silently ignored by serde.
#[derive(Deserialize)]
pub struct SubagentHookInput {
    /// Unique identifier for this subagent instance (e.g. "a1b2c3d4e5f6a7b8").
    /// Used as the registry key for session affinity.
    pub agent_id: Option<String>,
    /// Parent session ID. Not used for routing but kept for diagnostics.
    pub session_id: Option<String>,
}

#[derive(Serialize)]
pub struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    #[serde(rename = "additionalContext", skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

#[derive(Serialize)]
pub struct SubagentStartResponse {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

/// POST /api/subagent-start
///
/// Registers the subagent's identifier in the registry, mapping it to
/// the currently active subagent backend. Injects `⟨AC:{id}⟩` into
/// `additionalContext` so routing can look it up later.
pub async fn handle_subagent_start(
    State(state): State<HookState>,
    Json(input): Json<SubagentHookInput>,
) -> Json<SubagentStartResponse> {
    let context = match (input.agent_id.as_deref(), state.subagent_backend.get()) {
        (Some(id), Some(backend)) => {
            state.registry.register(id, &backend);
            crate::metrics::app_log(
                "hooks",
                &format!("SubagentStart: registered '{}' → backend '{}'", id, backend),
            );
            Some(AgentRegistry::format_marker(id))
        }
        _ => {
            crate::metrics::app_log(
                "hooks",
                "SubagentStart: no agent_id or no subagent backend configured",
            );
            None
        }
    };

    Json(SubagentStartResponse {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "SubagentStart".into(),
            additional_context: context,
        },
    })
}

/// POST /api/subagent-stop
///
/// Removes the subagent's identifier from the registry.
pub async fn handle_subagent_stop(
    State(state): State<HookState>,
    Json(input): Json<SubagentHookInput>,
) -> axum::http::StatusCode {
    if let Some(id) = input.agent_id.as_deref() {
        state.registry.remove(id);
        crate::metrics::app_log("hooks", &format!("SubagentStop: removed '{}'", id));
    }
    axum::http::StatusCode::OK
}

/// Input from tmux shim teammate registration.
#[derive(Deserialize)]
pub struct TeammateStartInput {
    pub agent_id: String,
}

/// Response to teammate-start — confirms registration.
#[derive(Serialize)]
pub struct TeammateStartResponse {
    /// The backend name assigned to this teammate (for logging/diagnostics).
    pub backend: String,
}

/// POST /api/teammate-start
///
/// Called by the tmux shim when it detects a teammate spawn.
/// Registers the teammate's agent_id in the shared registry,
/// mapping it to the current teammate backend.
pub async fn handle_teammate_start(
    State(state): State<HookState>,
    Json(input): Json<TeammateStartInput>,
) -> Json<TeammateStartResponse> {
    let backend = state.teammate_backend.get()
        .unwrap_or_else(|| state.backend_state.get_active_backend());

    state.registry.register(&input.agent_id, &backend);
    crate::metrics::app_log(
        "hooks",
        &format!("TeammateStart: registered '{}' → backend '{}'", input.agent_id, backend),
    );

    Json(TeammateStartResponse { backend })
}
