//! Stage 3: Create thinking session.
//!
//! Creates a ThinkingSession for the request AFTER backend resolution.
//! This replaces the thinking_middleware approach.

use std::sync::Arc;

use crate::config::Backend;
use crate::proxy::thinking::{ThinkingSession, TransformerRegistry};
use crate::proxy::pipeline::PipelineContext;

/// Stage 3: Create thinking session.
///
/// Creates a ThinkingSession for main agent requests (those that need
/// thinking block tracking). The session is created AFTER backend
/// resolution to ensure the correct backend is captured.
///
/// Returns `Some(ThinkingSession)` for requests that should track thinking,
/// `None` for teammate requests or when thinking is not needed.
pub fn create_thinking(
    transformer_registry: &Arc<TransformerRegistry>,
    backend: &Backend,
    ctx: &mut PipelineContext,
) -> Option<ThinkingSession> {
    // Get the request path from span metadata to detect teammate routes
    let is_teammate = ctx
        .span
        .record_mut()
        .request_meta
        .as_ref()
        .map(|meta| meta.path.starts_with("/teammate"))
        .unwrap_or(false);

    // Teammate requests don't get thinking sessions
    if is_teammate {
        ctx.debug_logger.log_auxiliary(
            "thinking",
            None,
            None,
            Some("Skipping thinking session for teammate request"),
            None,
        );
        return None;
    }

    // Create thinking session for main agent requests
    let session = transformer_registry.begin_request(
        &backend.name,
        ctx.debug_logger.clone(),
    );

    ctx.debug_logger.log_auxiliary(
        "thinking",
        None,
        None,
        Some(&format!(
            "Created thinking session for backend '{}'",
            backend.name
        )),
        None,
    );

    Some(session)
}
