//! Native transformer - passthrough mode relying on ThinkingRegistry.
//!
//! In Native mode:
//! - Thinking blocks are NOT stripped eagerly
//! - ThinkingRegistry handles filtering by session (in upstream.rs)
//! - NO summarization on backend switch
//!
//! This is the simplest mode - just let ThinkingRegistry do its job.

use async_trait::async_trait;
use serde_json::Value;

use super::context::{TransformContext, TransformResult};
use super::error::TransformError;
use super::traits::ThinkingTransformer;

/// Transformer that passes through requests, relying on ThinkingRegistry for filtering.
///
/// This mode:
/// - Does NOT strip thinking blocks (ThinkingRegistry filters by session)
/// - Does NOT summarize on backend switch
/// - Minimal overhead
#[derive(Debug, Default)]
pub struct NativeTransformer;

impl NativeTransformer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ThinkingTransformer for NativeTransformer {
    fn name(&self) -> &'static str {
        "native"
    }

    async fn transform_request(
        &self,
        _body: &mut Value,
        _context: &TransformContext,
    ) -> Result<TransformResult, TransformError> {
        // No transformation needed - ThinkingRegistry handles filtering in upstream.rs
        Ok(TransformResult::unchanged())
    }

    async fn on_backend_switch(
        &self,
        from_backend: &str,
        to_backend: &str,
        _body: &mut Value,
    ) -> Result<(), TransformError> {
        // No summarization - just log the switch
        tracing::info!(
            from = %from_backend,
            to = %to_backend,
            "Backend switch in native mode (no summarization)"
        );
        Ok(())
    }

    async fn on_response_complete(&self, _assistant_text: String) {
        // No-op - we don't track messages in native mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn native_does_not_modify_request() {
        let transformer = NativeTransformer::new();
        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "test thought", "signature": "sig"},
                    {"type": "text", "text": "response"}
                ]
            }]
        });

        let original = body.clone();
        let context = TransformContext::new("test".to_string(), "req-1".to_string(), "/v1/messages".to_string());
        let result = transformer.transform_request(&mut body, &context).await.unwrap();

        assert!(!result.changed);
        assert_eq!(body, original); // Body unchanged
    }

    #[tokio::test]
    async fn native_backend_switch_succeeds() {
        let transformer = NativeTransformer::new();
        let mut body = json!({});

        let result = transformer
            .on_backend_switch("anthropic", "glm", &mut body)
            .await;

        assert!(result.is_ok());
    }

    #[test]
    fn native_name() {
        let transformer = NativeTransformer::new();
        assert_eq!(transformer.name(), "native");
    }
}
