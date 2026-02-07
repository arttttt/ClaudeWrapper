//! Thinking block transformation system.
//!
//! This module provides the thinking block handling infrastructure for proxying
//! requests to different backends.
//!
//! # Architecture
//!
//! - **NativeTransformer**: Passthrough transformer (the only mode)
//! - **ThinkingRegistry**: Cross-provider block tracking, filters invalid blocks per session
//! - **TransformerRegistry**: Manages active transformer + ThinkingRegistry

mod context;
mod error;
mod native;
mod registry;
#[cfg(test)]
mod request_structure_tests;
mod sse_parser;
mod traits;

pub use context::{TransformContext, TransformResult, TransformStats};
pub use error::TransformError;
pub use native::NativeTransformer;
pub use registry::{CacheStats, ThinkingRegistry};
pub use sse_parser::extract_assistant_text;
pub use traits::ThinkingTransformer;

use crate::metrics::DebugLogger;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Registry that manages the thinking transformer and ThinkingRegistry.
///
/// Also manages the ThinkingRegistry for tracking thinking blocks across sessions.
pub struct TransformerRegistry {
    current: RwLock<Arc<dyn ThinkingTransformer>>,
    debug_logger: Option<Arc<DebugLogger>>,
    /// Registry for tracking thinking blocks by session
    thinking_registry: Mutex<ThinkingRegistry>,
}

impl TransformerRegistry {
    /// Create a new registry.
    pub fn new(debug_logger: Option<Arc<DebugLogger>>) -> Self {
        tracing::info!("Creating NativeTransformer (passthrough)");
        let transformer: Arc<dyn ThinkingTransformer> = Arc::new(NativeTransformer::new());
        Self {
            current: RwLock::new(transformer),
            debug_logger,
            thinking_registry: Mutex::new(ThinkingRegistry::new()),
        }
    }

    /// Get the current transformer.
    ///
    /// Returns a clone of the Arc'd transformer, so the caller owns a reference
    /// and no lock is held after this returns.
    pub async fn get(&self) -> Arc<dyn ThinkingTransformer> {
        self.current.read().await.clone()
    }

    /// Notify the transformer that a response is complete.
    ///
    /// This parses the SSE response bytes and extracts the assistant's text,
    /// then forwards it to the transformer for potential storage.
    pub async fn on_response_complete(&self, sse_bytes: &[u8]) {
        if let Some(text) = extract_assistant_text(sse_bytes) {
            let transformer = self.get().await;
            transformer.on_response_complete(text).await;
        }
    }

    // ========================================================================
    // Thinking Registry methods
    // ========================================================================

    /// Notify the thinking registry about a backend switch.
    ///
    /// This increments the internal session ID, invalidating thinking blocks
    /// from previous sessions.
    pub fn notify_backend_for_thinking(&self, backend_name: &str) {
        let mut registry = self.thinking_registry.lock();
        registry.on_backend_switch(backend_name);
    }

    /// Register thinking blocks from a response body.
    ///
    /// Extracts thinking blocks and records them with the given session ID.
    /// The session_id should be captured at request time to avoid races
    /// with concurrent backend switches.
    pub fn register_thinking_from_response(&self, response_body: &[u8], session_id: u64) {
        let mut registry = self.thinking_registry.lock();
        registry.register_from_response(response_body, session_id);
    }

    /// Register thinking blocks from pre-parsed SSE events.
    ///
    /// Accumulates thinking deltas and registers complete thinking blocks.
    /// The session_id should be captured at request time to avoid races
    /// with concurrent backend switches.
    pub fn register_thinking_from_sse_stream(&self, events: &[crate::sse::SseEvent], session_id: u64) {
        let mut registry = self.thinking_registry.lock();
        registry.register_from_sse_stream(events, session_id);
    }

    /// Filter thinking blocks in a request body.
    ///
    /// This is the main entry point for request processing. It performs:
    /// 1. Confirm: Mark blocks present in request as confirmed
    /// 2. Cleanup: Remove old/orphaned blocks from cache
    /// 3. Filter: Remove invalid blocks from request body
    ///
    /// Returns the number of blocks removed from the request.
    pub fn filter_thinking_blocks(&self, body: &mut serde_json::Value) -> u32 {
        let mut registry = self.thinking_registry.lock();
        registry.filter_request(body)
    }

    /// Get the current thinking session ID.
    pub fn current_thinking_session(&self) -> u64 {
        let registry = self.thinking_registry.lock();
        registry.current_session()
    }

    /// Get cache statistics for monitoring.
    pub fn thinking_cache_stats(&self) -> CacheStats {
        let registry = self.thinking_registry.lock();
        registry.cache_stats()
    }

    /// Log current thinking cache state (for debugging).
    pub fn log_thinking_cache_state(&self) {
        let registry = self.thinking_registry.lock();
        registry.log_cache_state();
    }

    /// Get the debug logger (if configured).
    #[allow(dead_code)]
    pub fn debug_logger(&self) -> Option<&Arc<DebugLogger>> {
        self.debug_logger.as_ref()
    }
}

impl std::fmt::Debug for TransformerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformerRegistry")
            .field("mode", &"native")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_creates_native_transformer() {
        let registry = TransformerRegistry::new(None);
        let transformer = registry.get().await;
        assert_eq!(transformer.name(), "native");
    }
}
