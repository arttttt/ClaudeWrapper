//! Thinking block registry - tracks which thinking blocks belong to which session.
//!
//! When switching backends, old thinking blocks become invalid (signatures don't match).
//! This registry tracks thinking blocks by hashing their content and associating them
//! with an internal session ID that increments on each backend switch.

use serde_json::Value;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Registry for tracking thinking blocks across backend switches.
///
/// Each thinking block is identified by a hash of its content (prefix + length).
/// When a backend switch occurs, the session ID increments, invalidating
/// all previous thinking blocks.
#[derive(Debug)]
pub struct ThinkingRegistry {
    /// Current session ID (increments on each backend switch)
    current_session: u64,

    /// Current backend name
    current_backend: String,

    /// Map of content_hash → session_id
    blocks: HashMap<u64, u64>,
}

impl Default for ThinkingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkingRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            current_session: 0,
            current_backend: String::new(),
            blocks: HashMap::new(),
        }
    }

    /// Called when the backend changes. Increments the session ID.
    ///
    /// This invalidates all thinking blocks from previous sessions.
    pub fn on_backend_switch(&mut self, new_backend: &str) {
        if self.current_backend != new_backend {
            self.current_session += 1;
            self.current_backend = new_backend.to_string();
            tracing::debug!(
                new_backend = %new_backend,
                new_session = self.current_session,
                "Backend switch detected, incremented thinking session"
            );
        }
    }

    /// Register thinking blocks from a response.
    ///
    /// Extracts thinking blocks from the response and records their hashes
    /// with the current session ID.
    pub fn register_from_response(&mut self, response_body: &[u8]) {
        // Try to parse as JSON
        let Ok(json) = serde_json::from_slice::<Value>(response_body) else {
            return;
        };

        // Look for thinking blocks in the response content
        if let Some(content) = json.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if let Some(thinking) = extract_thinking_content(item) {
                    let hash = fast_hash(&thinking);
                    self.blocks.insert(hash, self.current_session);
                    tracing::trace!(
                        hash = hash,
                        session = self.current_session,
                        content_preview = %truncate(&thinking, 50),
                        "Registered thinking block"
                    );
                }
            }
        }
    }

    /// Register thinking blocks from SSE stream events.
    ///
    /// Call this for each SSE event that might contain thinking content.
    pub fn register_from_sse_event(&mut self, event_data: &str) {
        // Parse SSE data as JSON
        let Ok(json) = serde_json::from_str::<Value>(event_data) else {
            return;
        };

        // Check for content_block_start with thinking type
        if json.get("type").and_then(|t| t.as_str()) == Some("content_block_start") {
            if let Some(block) = json.get("content_block") {
                if let Some(thinking) = extract_thinking_content(block) {
                    let hash = fast_hash(&thinking);
                    self.blocks.insert(hash, self.current_session);
                    tracing::trace!(
                        hash = hash,
                        session = self.current_session,
                        "Registered thinking block from SSE"
                    );
                }
            }
        }
    }

    /// Filter thinking blocks in a request body.
    ///
    /// Removes thinking blocks that don't belong to the current session.
    /// Returns the number of blocks removed.
    pub fn filter_request(&self, body: &mut Value) -> u32 {
        let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) else {
            return 0;
        };

        let mut removed_count = 0u32;

        for message in messages.iter_mut() {
            let Some(content) = message.get_mut("content").and_then(|v| v.as_array_mut()) else {
                continue;
            };

            let before_len = content.len();

            content.retain(|item| {
                // Check if this is a thinking block
                let item_type = item.get("type").and_then(|t| t.as_str());
                if !matches!(item_type, Some("thinking") | Some("redacted_thinking")) {
                    return true; // Keep non-thinking blocks
                }

                // Extract thinking content and compute hash
                let Some(thinking) = extract_thinking_content(item) else {
                    return false; // Remove if we can't extract content
                };

                let hash = fast_hash(&thinking);

                // Check if this block belongs to the current session
                match self.blocks.get(&hash) {
                    Some(&session) if session == self.current_session => {
                        tracing::trace!(
                            hash = hash,
                            session = session,
                            "Keeping thinking block from current session"
                        );
                        true
                    }
                    Some(&session) => {
                        tracing::debug!(
                            hash = hash,
                            block_session = session,
                            current_session = self.current_session,
                            "Removing thinking block from old session"
                        );
                        false
                    }
                    None => {
                        tracing::debug!(
                            hash = hash,
                            content_preview = %truncate(&thinking, 50),
                            "Removing unregistered thinking block"
                        );
                        false
                    }
                }
            });

            removed_count += (before_len - content.len()) as u32;
        }

        if removed_count > 0 {
            tracing::info!(
                removed = removed_count,
                current_session = self.current_session,
                "Filtered thinking blocks from request"
            );
        }

        removed_count
    }

    /// Get the current session ID.
    pub fn current_session(&self) -> u64 {
        self.current_session
    }

    /// Get the current backend name.
    pub fn current_backend(&self) -> &str {
        &self.current_backend
    }

    /// Get the number of registered blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Clear old blocks that are no longer needed.
    ///
    /// Removes blocks from sessions older than `keep_sessions` ago.
    pub fn cleanup_old_sessions(&mut self, keep_sessions: u64) {
        if self.current_session <= keep_sessions {
            return;
        }

        let min_session = self.current_session - keep_sessions;
        let before = self.blocks.len();
        self.blocks.retain(|_, &mut session| session >= min_session);
        let removed = before - self.blocks.len();

        if removed > 0 {
            tracing::debug!(
                removed = removed,
                remaining = self.blocks.len(),
                min_session = min_session,
                "Cleaned up old thinking blocks"
            );
        }
    }
}

/// Extract thinking content from a JSON value.
fn extract_thinking_content(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(|t| t.as_str())?;

    match item_type {
        "thinking" => item.get("thinking").and_then(|t| t.as_str()).map(|s| s.to_string()),
        "redacted_thinking" => {
            // For redacted thinking, use the data field
            item.get("data").and_then(|d| d.as_str()).map(|s| s.to_string())
        }
        _ => None,
    }
}

/// Fast hash using prefix + length for efficiency.
///
/// Hashes the first ~256 bytes of content plus the total length.
/// This provides good uniqueness while being fast for large content.
fn fast_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Hash prefix (first ~256 bytes, adjusted to char boundary)
    let prefix = safe_truncate(content, 256);
    prefix.hash(&mut hasher);

    // Hash the total length to distinguish blocks with same prefix
    content.len().hash(&mut hasher);

    hasher.finish()
}

/// Safely truncate a string at a char boundary.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Truncate a string for logging.
fn truncate(s: &str, max_len: usize) -> String {
    let truncated = safe_truncate(s, max_len);
    if truncated.len() < s.len() {
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new_registry() {
        let registry = ThinkingRegistry::new();
        assert_eq!(registry.current_session(), 0);
        assert_eq!(registry.current_backend(), "");
        assert_eq!(registry.block_count(), 0);
    }

    #[test]
    fn test_backend_switch_increments_session() {
        let mut registry = ThinkingRegistry::new();

        registry.on_backend_switch("anthropic");
        assert_eq!(registry.current_session(), 1);
        assert_eq!(registry.current_backend(), "anthropic");

        registry.on_backend_switch("glm");
        assert_eq!(registry.current_session(), 2);
        assert_eq!(registry.current_backend(), "glm");

        // Same backend doesn't increment
        registry.on_backend_switch("glm");
        assert_eq!(registry.current_session(), 2);

        registry.on_backend_switch("anthropic");
        assert_eq!(registry.current_session(), 3);
    }

    #[test]
    fn test_filter_removes_unregistered_blocks() {
        let mut registry = ThinkingRegistry::new();
        registry.on_backend_switch("anthropic");

        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Some thought", "signature": "abc"},
                    {"type": "text", "text": "Hello"}
                ]
            }]
        });

        let removed = registry.filter_request(&mut body);
        assert_eq!(removed, 1);

        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
    }

    #[test]
    fn test_filter_keeps_registered_blocks() {
        let mut registry = ThinkingRegistry::new();
        registry.on_backend_switch("anthropic");

        // Register a thinking block
        let hash = fast_hash("Some thought");
        registry.blocks.insert(hash, registry.current_session());

        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Some thought", "signature": "abc"},
                    {"type": "text", "text": "Hello"}
                ]
            }]
        });

        let removed = registry.filter_request(&mut body);
        assert_eq!(removed, 0);

        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
    }

    #[test]
    fn test_filter_removes_old_session_blocks() {
        let mut registry = ThinkingRegistry::new();
        registry.on_backend_switch("anthropic");

        // Register a block in session 1
        let hash = fast_hash("Old thought");
        registry.blocks.insert(hash, 1);

        // Switch to new session
        registry.on_backend_switch("glm");
        registry.on_backend_switch("anthropic"); // Now session 3

        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Old thought", "signature": "abc"},
                    {"type": "text", "text": "Hello"}
                ]
            }]
        });

        let removed = registry.filter_request(&mut body);
        assert_eq!(removed, 1); // Old session block removed
    }

    #[test]
    fn test_fast_hash_uniqueness() {
        // Different content should produce different hashes
        let hash1 = fast_hash("Hello world");
        let hash2 = fast_hash("Hello world!");
        let hash3 = fast_hash("Hello world");

        assert_ne!(hash1, hash2);
        assert_eq!(hash1, hash3);
    }

    #[test]
    fn test_fast_hash_long_content() {
        // Long content with same prefix but different length
        let short = "a".repeat(100);
        let long = "a".repeat(1000);

        let hash1 = fast_hash(&short);
        let hash2 = fast_hash(&long);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_cleanup_old_sessions() {
        let mut registry = ThinkingRegistry::new();

        // Create blocks in multiple sessions
        for i in 1..=5 {
            registry.on_backend_switch(&format!("backend{}", i));
            let hash = fast_hash(&format!("thought {}", i));
            registry.blocks.insert(hash, registry.current_session());
        }

        assert_eq!(registry.block_count(), 5);
        assert_eq!(registry.current_session(), 5);

        // Cleanup keeping only last 2 sessions (current=5, keep sessions >= 5-1=4)
        registry.cleanup_old_sessions(1);

        // Should keep sessions 4 and 5
        assert_eq!(registry.block_count(), 2);
    }
}
