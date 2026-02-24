//! Stage 1: Extract request components.
//!
//! Extracts body bytes and parses JSON if content-type is application/json.

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::CONTENT_TYPE;
use http_body_util::BodyExt;
use serde_json::Value;

use crate::config::DebugLogLevel;
use crate::metrics::{redact_body, redact_headers, RequestMeta};
use crate::proxy::error::ProxyError;
use crate::proxy::pipeline::PipelineContext;

/// Extracted request data from Stage 1.
pub struct ExtractedRequest {
    /// HTTP method
    pub method: axum::http::Method,
    /// URI
    pub uri: axum::http::Uri,
    /// Request headers
    pub headers: axum::http::HeaderMap,
    /// Raw body bytes
    pub body_bytes: Vec<u8>,
    /// Parsed JSON body if content-type is application/json
    pub parsed_body: Option<Value>,
    /// Content type from headers
    pub content_type: String,
}

/// Stage 1: Extract request body and metadata.
///
/// Collects the body bytes and optionally parses as JSON for downstream stages.
pub async fn extract_request(
    req: Request<Body>,
    ctx: &mut PipelineContext,
) -> Result<ExtractedRequest, ProxyError> {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;

    // Collect body bytes
    let body_bytes = match body.collect().await {
        Ok(bytes) => bytes.to_bytes().to_vec(),
        Err(e) => {
            return Err(ProxyError::InvalidRequest(format!(
                "Failed to read request body: {}",
                e
            )));
        }
    };

    // Determine content type
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Parse JSON body if applicable
    let parsed_body = if content_type.contains("application/json") {
        serde_json::from_slice::<Value>(&body_bytes).ok()
    } else {
        None
    };

    // Debug logging for headers
    let debug_config = ctx.debug_logger.config();
    if debug_config.level >= DebugLogLevel::Full && debug_config.header_preview {
        let record = ctx.span.record_mut();
        let meta = record.request_meta.get_or_insert_with(|| {
            let query = uri.query().map(|v| v.to_string());
            RequestMeta {
                method: method.to_string(),
                path: uri.path().to_string(),
                query,
                headers: None,
                body_preview: None,
            }
        });
        meta.headers = Some(redact_headers(&headers));
    }

    // Debug logging for body preview
    if debug_config.level >= DebugLogLevel::Full {
        let record = ctx.span.record_mut();
        let meta = record.request_meta.get_or_insert_with(|| {
            let query = uri.query().map(|v| v.to_string());
            RequestMeta {
                method: method.to_string(),
                path: uri.path().to_string(),
                query,
                headers: None,
                body_preview: None,
            }
        });
        let limit = if debug_config.full_body {
            None
        } else {
            Some(debug_config.body_preview_bytes)
        };
        meta.body_preview = redact_body(
            &body_bytes,
            &content_type,
            limit,
            debug_config.pretty_print,
        );
    }

    // Request analysis for verbose logging
    if debug_config.level >= DebugLogLevel::Verbose {
        use crate::metrics::RequestParser;
        let parser = RequestParser::new();
        let analysis = parser.parse_request(&body_bytes);
        ctx.span.record_mut().request_analysis = Some(analysis);
    }

    Ok(ExtractedRequest {
        method,
        uri,
        headers,
        body_bytes,
        parsed_body,
        content_type,
    })
}
