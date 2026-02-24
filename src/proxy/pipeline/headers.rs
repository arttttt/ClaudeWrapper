//! Stage 5: Build upstream headers.
//!
//! Builds the headers for the upstream request:
//! - Filters out HOST and CONTENT_LENGTH (set by HTTP client)
//! - Strips auth headers when backend uses own credentials
//! - Patches anthropic-beta header for non-Anthropic backends
//! - Adds backend's own auth header if configured

use axum::http::header::{AUTHORIZATION, CONTENT_LENGTH, HOST};
use axum::http::HeaderMap;

use crate::config::Backend;
use crate::config::build_auth_header;
use crate::proxy::error::ProxyError;
use crate::proxy::pipeline::PipelineContext;

/// Stage 5: Build headers for upstream request.
///
/// Returns a Vec of (name, value) pairs to preserve multiple values for the same header name.
pub fn build_headers(
    incoming_headers: &HeaderMap,
    backend: &Backend,
    ctx: &mut PipelineContext,
) -> Result<Vec<(String, String)>, ProxyError> {
    let mut headers: Vec<(String, String)> = Vec::new();

    // Determine if we should strip incoming auth headers based on backend's auth type
    let strip_auth_headers = backend.auth_type().uses_own_credentials();
    let needs_thinking_compat = backend.needs_thinking_compat();

    for (name, value) in incoming_headers.iter() {
        let name_str = name.as_str();

        // Always skip HOST and CONTENT_LENGTH - they will be set by the HTTP client
        // CONTENT_LENGTH must be recalculated after body transformation
        if name == HOST || name == CONTENT_LENGTH {
            continue;
        }

        // Strip auth headers when backend uses its own credentials (bearer/api_key)
        // Passthrough mode forwards all headers unchanged
        if strip_auth_headers
            && (name == AUTHORIZATION || name_str.eq_ignore_ascii_case("x-api-key"))
        {
            crate::metrics::app_log(
                "upstream",
                &format!(
                    "Stripping incoming auth header '{}' for backend '{}' (auth_type={:?})",
                    name, backend.name, backend.auth_type()
                ),
            );
            continue;
        }

        // Rewrite anthropic-beta header for non-Anthropic backends
        if needs_thinking_compat && name_str.eq_ignore_ascii_case("anthropic-beta") {
            if let Ok(val) = value.to_str() {
                let patched = patch_anthropic_beta_header(val);
                if patched != val {
                    ctx.debug_logger.log_auxiliary(
                        "thinking_compat",
                        None,
                        None,
                        Some(&format!("Patched anthropic-beta: '{}' -> '{}'", val, patched)),
                        None,
                    );
                }
                headers.push((name_str.to_string(), patched));
                continue;
            }
        }

        // Add header to result
        if let Ok(val) = value.to_str() {
            headers.push((name_str.to_string(), val.to_string()));
        }
    }

    // Add backend's own auth header (for bearer/api_key modes)
    if let Some((name, value)) = build_auth_header(backend) {
        headers.push((name, value));
    }

    Ok(headers)
}

/// Rewrite anthropic-beta header for non-Anthropic backends:
/// strip `adaptive-thinking-*` and ensure `interleaved-thinking-2025-05-14` is present.
fn patch_anthropic_beta_header(value: &str) -> String {
    let mut parts: Vec<&str> = value
        .split(',')
        .map(|p| p.trim())
        .filter(|part| !part.starts_with("adaptive-thinking-"))
        .collect();

    let has_interleaved = parts
        .iter()
        .any(|p| p.starts_with("interleaved-thinking-"));
    if !has_interleaved {
        parts.push("interleaved-thinking-2025-05-14");
    }

    parts.join(",")
}
