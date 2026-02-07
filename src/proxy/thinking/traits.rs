//! Core trait for thinking block transformers.

use async_trait::async_trait;
use serde_json::Value;

use super::context::{TransformContext, TransformResult};
use super::error::TransformError;

/// Trait for transforming thinking blocks in API requests.
///
/// Transformers are called on every request to potentially modify
/// the message history before sending to the upstream API.
#[async_trait]
pub trait ThinkingTransformer: Send + Sync {
    /// Returns the name of this transformer for logging.
    fn name(&self) -> &'static str;

    /// Transform the request body before sending upstream.
    ///
    /// This is called on every API request. The transformer can modify
    /// the `body` in place to transform thinking blocks.
    async fn transform_request(
        &self,
        body: &mut Value,
        context: &TransformContext,
    ) -> Result<TransformResult, TransformError>;

    /// Called when a response is complete with the assistant's text content.
    ///
    /// Default implementation does nothing.
    async fn on_response_complete(&self, _response_text: String) {
        // Default: do nothing
    }
}
