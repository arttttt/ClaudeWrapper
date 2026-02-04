use axum::body::{Body, Bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HOST};
use axum::http::{Request, Response};
use http_body_util::BodyExt;
use reqwest::Client;
use tokio::time::sleep;
use crate::backend::BackendState;
use crate::config::{build_auth_header, DebugLogLevel};
use crate::config::ConfigStore;
use crate::metrics::{
    redact_body_preview, redact_headers, DebugLogger, ObservedStream, ObservabilityHub,
    RequestMeta, RequestParser, RequestSpan, ResponseMeta, ResponseParser, ResponsePreview,
};
use crate::proxy::error::ProxyError;
use crate::proxy::pool::PoolConfig;
use crate::proxy::thinking::{
    remove_context_management, strip_thinking_blocks, TransformContext, TransformerRegistry,
};
use crate::proxy::timeout::TimeoutConfig;
use std::sync::Arc;

pub struct UpstreamClient {
    client: Client,
    timeout_config: TimeoutConfig,
    pool_config: PoolConfig,
    config: ConfigStore,
    transformer_registry: Arc<TransformerRegistry>,
    debug_logger: Arc<DebugLogger>,
    request_parser: RequestParser,
    response_parser: ResponseParser,
}

impl UpstreamClient {
    pub fn new(
        timeout_config: TimeoutConfig,
        pool_config: PoolConfig,
        config: ConfigStore,
        transformer_registry: Arc<TransformerRegistry>,
        debug_logger: Arc<DebugLogger>,
    ) -> Self {
        let client = Client::builder()
            .connect_timeout(timeout_config.connect)
            .pool_idle_timeout(Some(pool_config.pool_idle_timeout))
            .pool_max_idle_per_host(pool_config.pool_max_idle_per_host)
            .build()
            .expect("Failed to build upstream client");

        Self {
            client,
            timeout_config,
            pool_config,
            config,
            transformer_registry,
            debug_logger,
            request_parser: RequestParser::new(true),
            response_parser: ResponseParser::new(),
        }
    }

    pub async fn forward(
        &self,
        req: Request<Body>,
        backend_state: &BackendState,
        backend_override: Option<String>,
        span: RequestSpan,
        observability: ObservabilityHub,
    ) -> Result<Response<Body>, ProxyError> {
        // Get the current active backend configuration at request time
        // This ensures the entire request uses the same backend, even if
        // a switch happens mid-request
        let backend = match backend_override.as_deref() {
            Some(backend_id) => backend_state
                .get_backend_config(backend_id)
                .map_err(|e| ProxyError::BackendNotFound {
                    backend: e.to_string(),
                }),
            None => backend_state
                .get_active_backend_config()
                .map_err(|e| ProxyError::BackendNotFound {
                    backend: e.to_string(),
                }),
        };

        let backend = match backend {
            Ok(backend) => backend,
            Err(err) => {
                observability.finish_error(span, Some(err.status_code().as_u16()));
                return Err(err);
            }
        };

        self.do_forward(req, backend, span, observability).await
    }

    async fn do_forward(
        &self,
        req: Request<Body>,
        backend: crate::config::Backend,
        mut span: RequestSpan,
        observability: ObservabilityHub,
    ) -> Result<Response<Body>, ProxyError> {
        span.set_backend(backend.name.clone());
        let (parts, body) = req.into_parts();
        let method = parts.method;
        let uri = parts.uri;
        let headers = parts.headers;
        let path_and_query = uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let debug_config = self.debug_logger.config();
        let debug_level = debug_config.level;
        if debug_level >= DebugLogLevel::Full && debug_config.header_preview {
            let record = span.record_mut();
            let meta = record.request_meta.get_or_insert_with(|| {
                let query = uri.query().map(|value| value.to_string());
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

        // Validate backend is configured
        if !backend.is_configured() {
            let err = ProxyError::BackendNotConfigured {
                backend: backend.name.clone(),
                reason: "api_key is not set".to_string(),
            };
            observability.finish_error(span, Some(err.status_code().as_u16()));
            return Err(err);
        }

        let upstream_uri = format!("{}{}", backend.base_url, path_and_query);
        let body_bytes = match body.collect().await {
            Ok(bytes) => bytes.to_bytes(),
            Err(e) => {
                let err = ProxyError::InvalidRequest(format!("Failed to read request body: {}", e));
                observability.finish_error(span, Some(err.status_code().as_u16()));
                return Err(err);
            }
        };
        let body_bytes = body_bytes.to_vec();
        let request_content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if debug_level >= DebugLogLevel::Verbose {
            let analysis = self.request_parser.parse_request(&body_bytes);
            span.record_mut().request_analysis = Some(analysis);
        }

        if debug_level >= DebugLogLevel::Full {
            let meta = span
                .record_mut()
                .request_meta
                .get_or_insert_with(|| {
                    let query = uri.query().map(|value| value.to_string());
                    RequestMeta {
                        method: method.to_string(),
                        path: uri.path().to_string(),
                        query,
                        headers: None,
                        body_preview: None,
                    }
                });
            meta.body_preview = redact_body_preview(
                &body_bytes,
                request_content_type,
                debug_config.body_preview_bytes,
            );
        }

        // Apply transformer for summarization (prepend summary, save messages)
        // Note: This does NOT strip thinking blocks - that happens lazily on 400 retry
        let mut body_bytes = body_bytes;
        if request_content_type.contains("application/json") {
            self.transformer_registry
                .update_mode(self.config.get().thinking.mode.clone())
                .await;

            if let Ok(mut json_body) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                let context = TransformContext::new(
                    backend.name.clone(),
                    span.request_id().to_string(),
                    path_and_query,
                );

                let transformer = self.transformer_registry.get().await;

                match transformer.transform_request(&mut json_body, &context).await {
                    Ok(result) => {
                        if result.changed {
                            match serde_json::to_vec(&json_body) {
                                Ok(updated) => {
                                    body_bytes = updated;
                                    tracing::info!(
                                        backend = %backend.name,
                                        transformer = transformer.name(),
                                        summarized = result.stats.summarized_count,
                                        "Applied transformer"
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to serialize transformed request body"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "Failed to apply transformer"
                        );
                    }
                }
            }
        }

        // Save original body for potential retry with thinking block strip
        let original_body_bytes = body_bytes.clone();

        span.set_request_bytes(body_bytes.len());
        let auth_header = build_auth_header(&backend);
        let mut attempt = 0u32;

        let upstream_resp = loop {
            let mut builder = self.client.request(method.clone(), &upstream_uri);

            // Determine if we should strip incoming auth headers based on backend's auth type
            let strip_auth_headers = backend.auth_type().uses_own_credentials();

            for (name, value) in headers.iter() {
                // Always skip HOST and CONTENT_LENGTH - they will be set by the HTTP client
                // CONTENT_LENGTH must be recalculated after body transformation
                if name == HOST || name == CONTENT_LENGTH {
                    continue;
                }
                // Strip auth headers when backend uses its own credentials (bearer/api_key)
                // Passthrough mode forwards all headers unchanged
                if strip_auth_headers {
                    if name == AUTHORIZATION || name.as_str().eq_ignore_ascii_case("x-api-key") {
                        tracing::debug!(
                            header = %name,
                            backend = %backend.name,
                            auth_type = ?backend.auth_type(),
                            "Stripping incoming auth header"
                        );
                        continue;
                    }
                }
                builder = builder.header(name, value);
            }

            // Add backend's own auth header (for bearer/api_key modes)
            if let Some((name, value)) = auth_header.as_ref() {
                builder = builder.header(name, value);
            }

            let send_result = builder
                .timeout(self.timeout_config.request)
                .body(body_bytes.clone())
                .send()
                .await;

            match send_result {
                Ok(response) => break response,
                Err(err) => {
                    tracing::error!(
                        backend = %backend.name,
                        is_connect = err.is_connect(),
                        is_timeout = err.is_timeout(),
                        is_request = err.is_request(),
                        is_body = err.is_body(),
                        error = ?err,
                        "Upstream request error details"
                    );
                    let should_retry = err.is_connect() || err.is_timeout();
                    if should_retry && attempt < self.pool_config.max_retries {
                        let backoff = self
                            .pool_config
                            .retry_backoff_base
                            .saturating_mul(1u32 << attempt);
                        tracing::warn!(
                            backend = %backend.name,
                            attempt = attempt + 1,
                            max_retries = self.pool_config.max_retries,
                            backoff_ms = backoff.as_millis(),
                            error = %err,
                            "Upstream request failed, retrying"
                        );
                        sleep(backoff).await;
                        attempt += 1;
                        continue;
                    }

                    if err.is_timeout() {
                        let timeout_err = ProxyError::RequestTimeout {
                            duration: self.timeout_config.request.as_secs(),
                        };
                        span.mark_timed_out();
                        observability.finish_error(span, Some(timeout_err.status_code().as_u16()));
                        return Err(timeout_err);
                    }

                    let conn_err = ProxyError::ConnectionError {
                        backend: backend.name.clone(),
                        source: err,
                    };
                    observability.finish_error(span, Some(conn_err.status_code().as_u16()));
                    return Err(conn_err);
                }
            }
        };

        let content_type = upstream_resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|value| value.to_string());

        let is_streaming = content_type
            .as_deref()
            .map_or(false, |ct| ct.contains("text/event-stream"));

        let status = upstream_resp.status();

        // Lazy strip with retry: if we get a 400 error that looks like a thinking
        // signature error, strip thinking blocks and retry once.
        //
        // For 400 errors, we need to read the body to check if it's a thinking error.
        // We store the response headers before consuming the body, and return either:
        // - A new response from retry (if successful)
        // - The pre-read body with saved headers (if retry failed or not a thinking error)
        let (upstream_resp, status, response_headers, pre_read_body) =
            if status == reqwest::StatusCode::BAD_REQUEST && !is_streaming {
                // Save headers before consuming response body
                let saved_headers = upstream_resp.headers().clone();

                // Read the error body to check if it's a thinking signature error
                let error_body = match upstream_resp.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let err =
                            ProxyError::Internal(format!("Failed to read error body: {}", e));
                        observability.finish_error(span, Some(400));
                        return Err(err);
                    }
                };

                // Log the 400 error body for debugging
                let error_body_preview = String::from_utf8_lossy(&error_body);
                let error_preview = if error_body_preview.len() > 500 {
                    format!("{}...[truncated]", &error_body_preview[..500])
                } else {
                    error_body_preview.to_string()
                };
                tracing::debug!(
                    backend = %backend.name,
                    error_body = %error_preview,
                    "Received 400 error, checking for thinking signature error"
                );

                if is_thinking_signature_error(&error_body) {
                    tracing::info!(
                        backend = %backend.name,
                        "Detected thinking signature error, stripping thinking blocks and retrying"
                    );

                    // Try to strip thinking blocks from original body
                    let retry_body = if let Ok(mut json_body) =
                        serde_json::from_slice::<serde_json::Value>(&original_body_bytes)
                    {
                        let stripped = strip_thinking_blocks(&mut json_body);
                        if stripped > 0 {
                            remove_context_management(&mut json_body);
                            tracing::info!(
                                backend = %backend.name,
                                stripped_count = stripped,
                                "Stripped thinking blocks for retry"
                            );
                            serde_json::to_vec(&json_body).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(retry_body) = retry_body {
                        tracing::debug!(
                            backend = %backend.name,
                            retry_body_len = retry_body.len(),
                            "Prepared retry body after stripping thinking blocks"
                        );
                        // Retry with stripped body
                        let mut retry_builder = self.client.request(method.clone(), &upstream_uri);
                        let strip_auth_headers = backend.auth_type().uses_own_credentials();

                        for (name, value) in headers.iter() {
                            if name == HOST || name == CONTENT_LENGTH {
                                continue;
                            }
                            if strip_auth_headers
                                && (name == AUTHORIZATION
                                    || name.as_str().eq_ignore_ascii_case("x-api-key"))
                            {
                                continue;
                            }
                            retry_builder = retry_builder.header(name, value);
                        }

                        if let Some((name, value)) = auth_header.as_ref() {
                            retry_builder = retry_builder.header(name, value);
                        }

                        match retry_builder
                            .timeout(self.timeout_config.request)
                            .body(retry_body)
                            .send()
                            .await
                        {
                            Ok(retry_resp) => {
                                let retry_status = retry_resp.status();
                                let retry_headers = retry_resp.headers().clone();
                                tracing::info!(
                                    backend = %backend.name,
                                    status = %retry_status,
                                    "Retry after thinking strip completed"
                                );
                                (Some(retry_resp), retry_status, retry_headers, None)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    backend = %backend.name,
                                    error = %e,
                                    "Retry after thinking strip failed, returning original error"
                                );
                                // Return original 400 error with pre-read body
                                (None, status, saved_headers, Some(error_body.to_vec()))
                            }
                        }
                    } else {
                        // Couldn't strip, return original error
                        tracing::warn!(
                            backend = %backend.name,
                            "Could not strip thinking blocks (none found or JSON parse failed), returning original 400 error"
                        );
                        (None, status, saved_headers, Some(error_body.to_vec()))
                    }
                } else {
                    // Not a thinking error, pass through with pre-read body
                    tracing::debug!(
                        backend = %backend.name,
                        error_body = %error_preview,
                        "400 error is not a thinking signature error, passing through"
                    );
                    (None, status, saved_headers, Some(error_body.to_vec()))
                }
            } else {
                let resp_headers = upstream_resp.headers().clone();
                (Some(upstream_resp), status, resp_headers, None)
            };

        span.set_status(status.as_u16());

        if debug_level >= DebugLogLevel::Full && debug_config.header_preview {
            let meta = span
                .record_mut()
                .response_meta
                .get_or_insert_with(|| ResponseMeta {
                    headers: None,
                    body_preview: None,
                });
            meta.headers = Some(redact_headers(&response_headers));
        }

        // Log error responses for debugging
        if !status.is_success() {
            tracing::warn!(
                backend = %backend.name,
                status = %status,
                content_type = ?content_type.as_deref(),
                "Upstream returned error status"
            );
        }

        let mut response_builder = Response::builder().status(status);

        for (name, value) in response_headers.iter() {
            response_builder = response_builder.header(name, value);
        }

        if is_streaming {
            // For streaming, upstream_resp is always Some (400 errors are not streaming)
            let upstream_resp = upstream_resp.expect("streaming response should always be present");
            let stream = upstream_resp.bytes_stream();
            let response_preview = if debug_level >= DebugLogLevel::Full {
                ResponsePreview::new(
                    debug_config.body_preview_bytes,
                    content_type.clone().unwrap_or_default(),
                )
            } else {
                None
            };

            // Create callback to capture response for summarization
            let registry = Arc::clone(&self.transformer_registry);
            let on_complete: crate::metrics::ResponseCompleteCallback = Box::new(move |bytes| {
                let registry = Arc::clone(&registry);
                let bytes = bytes.to_vec();
                tokio::spawn(async move {
                    registry.on_response_complete(&bytes).await;
                });
            });

            let observed = ObservedStream::new(
                stream,
                span,
                observability,
                self.timeout_config.idle,
                response_preview,
            )
            .with_on_complete(on_complete);

            Ok(response_builder.body(Body::from_stream(observed))?)
        } else {
            span.mark_first_byte();
            // Use pre-read body if available (from 400 error check), otherwise read from response
            let body_bytes = if let Some(pre_read) = pre_read_body {
                Bytes::from(pre_read)
            } else if let Some(resp) = upstream_resp {
                match resp.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        let err =
                            ProxyError::Internal(format!("Failed to read response body: {}", e));
                        observability.finish_error(span, Some(err.status_code().as_u16()));
                        return Err(err);
                    }
                }
            } else {
                // This shouldn't happen - either pre_read_body or upstream_resp should be set
                let err = ProxyError::Internal("No response body available".to_string());
                observability.finish_error(span, Some(500));
                return Err(err);
            };

            if debug_level >= DebugLogLevel::Verbose {
                let mut analysis = self.response_parser.parse_response(&body_bytes);
                if analysis.cost_usd.is_none() {
                    analysis.cost_usd = compute_cost_usd(
                        &backend,
                        analysis.input_tokens.or_else(|| {
                            span.record_mut()
                                .request_analysis
                                .as_ref()
                                .and_then(|analysis| analysis.estimated_input_tokens)
                        }),
                        analysis.output_tokens,
                    );
                }
                span.record_mut().response_analysis = Some(analysis);
            }

            if debug_level >= DebugLogLevel::Full {
                let meta = span
                    .record_mut()
                    .response_meta
                    .get_or_insert_with(|| ResponseMeta {
                        headers: None,
                        body_preview: None,
                    });
                meta.body_preview = redact_body_preview(
                    &body_bytes,
                    content_type.as_deref().unwrap_or(""),
                    debug_config.body_preview_bytes,
                );
            }

            // Log error response body for debugging
            if !status.is_success() {
                let body_preview = String::from_utf8_lossy(&body_bytes);
                let truncated = if body_preview.len() > 1000 {
                    format!("{}...[truncated]", &body_preview[..1000])
                } else {
                    body_preview.to_string()
                };
                tracing::warn!(
                    backend = %backend.name,
                    status = %status,
                    body = %truncated,
                    "Upstream error response body"
                );
            }

            span.add_response_bytes(body_bytes.len());
            observability.finish_request(span);
            Ok(response_builder.body(Body::from(body_bytes))?)
        }
    }
}

fn compute_cost_usd(
    backend: &crate::config::Backend,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
) -> Option<f64> {
    let pricing = backend.pricing.as_ref()?;
    let input_tokens = input_tokens.unwrap_or(0) as f64;
    let output_tokens = output_tokens.unwrap_or(0) as f64;
    let cost = (input_tokens * pricing.input_per_million
        + output_tokens * pricing.output_per_million)
        / 1_000_000.0;
    Some(cost)
}

impl Default for UpstreamClient {
    fn default() -> Self {
        let config = ConfigStore::new(
            crate::config::Config::default(),
            crate::config::Config::config_path(),
        );
        let registry = Arc::new(TransformerRegistry::with_mode(
            crate::config::ThinkingMode::Strip,
        ));
        let debug_logger = Arc::new(DebugLogger::new(config.get().debug_logging.clone()));
        Self::new(
            TimeoutConfig::default(),
            PoolConfig::default(),
            config,
            registry,
            debug_logger,
        )
    }
}

/// Check if an error response indicates invalid thinking block signature.
///
/// Returns true if the response is a 400 error caused by thinking blocks
/// from a different backend (invalid signature).
fn is_thinking_signature_error(response_body: &[u8]) -> bool {
    // Try to parse as JSON and extract error message
    let error_message = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(response_body)
    {
        // Try common error response formats
        json.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| json.get("message").and_then(|m| m.as_str()))
            .map(|s| s.to_lowercase())
    } else {
        // Fall back to raw text
        Some(String::from_utf8_lossy(response_body).to_lowercase())
    };

    let Some(message) = error_message else {
        tracing::trace!("No error message found in response body");
        return false;
    };

    // Check for thinking-related error patterns
    // Must contain "thinking" AND one of the signature-related keywords
    let has_thinking = message.contains("thinking");
    let has_signature_keyword = message.contains("signature")
        || message.contains("invalid")
        || message.contains("verification")
        || message.contains("mismatch")
        || message.contains("unrecognized");

    let is_thinking_error = has_thinking && has_signature_keyword;

    tracing::trace!(
        has_thinking = has_thinking,
        has_signature_keyword = has_signature_keyword,
        is_thinking_error = is_thinking_error,
        message_preview = %if message.len() > 200 { &message[..200] } else { &message },
        "Checked error message for thinking signature error"
    );

    is_thinking_error
}
