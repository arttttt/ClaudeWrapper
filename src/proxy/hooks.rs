//! Hook handlers for CC SubagentStart / SubagentStop events.
//!
//! CC fires these hooks when it spawns or stops in-process subagents.
//! We use SubagentStart to inject a backend marker (`⟨AC:backend⟩`) into
//! the subagent's context via `additionalContext`, pinning it to the
//! backend that was active at spawn time (session affinity).

use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::backend::SubagentBackend;

/// Input sent by CC hook (piped via curl stdin).
///
/// Fields are populated by serde deserialization only — they document
/// the CC hook contract but are not read by our handlers.
#[derive(Deserialize)]
pub struct SubagentHookInput {
    pub session_id: Option<String>,
    pub hook_event_name: Option<String>,
    pub agent_name: Option<String>,
    pub agent_type: Option<String>,
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
/// Returns `additionalContext` with backend marker so the subagent
/// stays pinned to its birth backend for its entire lifetime.
pub async fn handle_subagent_start(
    State(subagent_backend): State<SubagentBackend>,
    Json(_input): Json<SubagentHookInput>,
) -> Json<SubagentStartResponse> {
    let context = subagent_backend.get().map(|backend| {
        crate::metrics::app_log(
            "hooks",
            &format!("SubagentStart: pinning to backend '{}'", backend),
        );
        format!("\u{27E8}AC:{}\u{27E9}", backend)
    });

    Json(SubagentStartResponse {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "SubagentStart".into(),
            additional_context: context,
        },
    })
}

/// POST /api/subagent-stop
///
/// Logging only — no state to clean up (backend is encoded in the marker).
pub async fn handle_subagent_stop(
    Json(_input): Json<SubagentHookInput>,
) -> axum::http::StatusCode {
    crate::metrics::app_log("hooks", "SubagentStop received");
    axum::http::StatusCode::OK
}
