use std::collections::{HashMap, VecDeque};

use serde_json::Value;

use crate::config::ThinkingMode;

const DEFAULT_SIGNATURE_CACHE_SIZE: usize = 2048;

#[derive(Debug, Default, Clone)]
pub struct ThinkingTransformResult {
    pub changed: bool,
    pub drop_count: u32,
    pub convert_count: u32,
    pub tag_count: u32,
}

#[derive(Debug)]
pub struct ThinkingTransformOutput {
    pub body: Option<Vec<u8>>,
    pub result: ThinkingTransformResult,
}

pub struct ThinkingTracker {
    mode: ThinkingMode,
    last_backend: Option<String>,
    signatures: SignatureCache,
}

impl ThinkingTracker {
    pub fn new(mode: ThinkingMode) -> Self {
        Self {
            mode,
            last_backend: None,
            signatures: SignatureCache::new(DEFAULT_SIGNATURE_CACHE_SIZE),
        }
    }

    pub fn set_mode(&mut self, mode: ThinkingMode) {
        self.mode = mode;
    }

    pub fn transform_request(
        &mut self,
        target_backend: &str,
        body: &[u8],
    ) -> ThinkingTransformOutput {
        let mut result = ThinkingTransformResult::default();
        let mut json: Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
                self.last_backend = Some(target_backend.to_string());
                return ThinkingTransformOutput { body: None, result };
            }
        };

        let mut changed = false;
        if let Some(messages) = json
            .get_mut("messages")
            .and_then(|value| value.as_array_mut())
        {
            for message in messages {
                let Some(content) = message.get_mut("content") else {
                    continue;
                };

                if let Value::Array(items) = content {
                    for item in items.iter_mut() {
                        let Some(obj) = item.as_object_mut() else {
                            continue;
                        };

                        let Some(item_type) = obj.get("type").and_then(|v| v.as_str()) else {
                            continue;
                        };

                        if item_type != "thinking" {
                            continue;
                        }

                        let signature = obj.get("signature").and_then(|v| v.as_str());
                        let text = obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .or_else(|| obj.get("thinking").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        let source_backend = self.resolve_source_backend(signature, target_backend);

                        if source_backend == target_backend {
                            if let Some(sig) = signature {
                                self.signatures
                                    .insert(sig.to_string(), target_backend.to_string());
                            }
                            continue;
                        }

                        match self.mode {
                            ThinkingMode::DropSignature => {
                                if obj.remove("signature").is_some() {
                                    result.drop_count = result.drop_count.saturating_add(1);
                                    changed = true;
                                }
                            }
                            ThinkingMode::ConvertToText => {
                                *item = serde_json::json!({
                                    "type": "text",
                                    "text": text,
                                });
                                result.convert_count = result.convert_count.saturating_add(1);
                                changed = true;
                            }
                            ThinkingMode::ConvertToTags => {
                                *item = serde_json::json!({
                                    "type": "text",
                                    "text": format!("<think>{}</think>", text),
                                });
                                result.tag_count = result.tag_count.saturating_add(1);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }

        result.changed = changed;
        self.last_backend = Some(target_backend.to_string());

        if changed {
            let body = serde_json::to_vec(&json).ok();
            return ThinkingTransformOutput { body, result };
        }

        ThinkingTransformOutput { body: None, result }
    }

    fn resolve_source_backend(&mut self, signature: Option<&str>, target_backend: &str) -> String {
        let Some(signature) = signature else {
            return target_backend.to_string();
        };

        if let Some(mapped) = self.signatures.get(signature) {
            return mapped;
        }

        let switched = self
            .last_backend
            .as_deref()
            .map(|backend| backend != target_backend)
            .unwrap_or(false);

        if switched {
            return self
                .last_backend
                .clone()
                .unwrap_or_else(|| target_backend.to_string());
        }

        target_backend.to_string()
    }
}

struct SignatureCache {
    max_entries: usize,
    entries: HashMap<String, (String, u64)>,
    order: VecDeque<(String, u64)>,
    counter: u64,
}

impl SignatureCache {
    fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: HashMap::new(),
            order: VecDeque::new(),
            counter: 0,
        }
    }

    fn get(&mut self, signature: &str) -> Option<String> {
        let (backend, _) = self.entries.get(signature)?.clone();
        self.touch(signature.to_string(), backend.clone());
        Some(backend)
    }

    fn insert(&mut self, signature: String, backend: String) {
        self.touch(signature, backend);
    }

    fn touch(&mut self, signature: String, backend: String) {
        self.counter = self.counter.saturating_add(1);
        let seq = self.counter;
        self.entries.insert(signature.clone(), (backend, seq));
        self.order.push_back((signature, seq));
        self.evict();
    }

    fn evict(&mut self) {
        while self.entries.len() > self.max_entries {
            let Some((signature, seq)) = self.order.pop_front() else {
                break;
            };

            let should_remove = self
                .entries
                .get(&signature)
                .map(|(_, current_seq)| *current_seq == seq)
                .unwrap_or(false);

            if should_remove {
                self.entries.remove(&signature);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_with_thinking(signature: &str) -> Vec<u8> {
        serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "text": "ponder", "signature": signature},
                        {"type": "text", "text": "hello"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn keeps_signature_for_same_backend() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::DropSignature);
        let body = body_with_thinking("sig-1");
        let output = tracker.transform_request("anthropic", &body);
        assert!(!output.result.changed);
        assert!(output.body.is_none());
        assert!(tracker.signatures.get("sig-1").is_some());
    }

    #[test]
    fn drops_signature_on_switch() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::DropSignature);
        let body = body_with_thinking("sig-1");
        let _ = tracker.transform_request("anthropic", &body);
        let output = tracker.transform_request("glm", &body);
        assert!(output.result.changed);
        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(!transformed.contains("\"signature\""));
        assert_eq!(output.result.drop_count, 1);
    }

    #[test]
    fn converts_thinking_to_text() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);
        let body = body_with_thinking("sig-1");
        let _ = tracker.transform_request("anthropic", &body);
        let output = tracker.transform_request("glm", &body);
        assert!(output.result.changed);
        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("\"type\":\"text\""));
        assert_eq!(output.result.convert_count, 1);
    }

    #[test]
    fn converts_thinking_to_tags() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToTags);
        let body = body_with_thinking("sig-1");
        let _ = tracker.transform_request("anthropic", &body);
        let output = tracker.transform_request("glm", &body);
        assert!(output.result.changed);
        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("<think>ponder</think>"));
        assert_eq!(output.result.tag_count, 1);
    }

    // =========================================================================
    // Tests for backend switch scenarios (documenting desired behavior)
    // =========================================================================

    fn body_with_multiple_thinking() -> Vec<u8> {
        serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "thought 1", "signature": "sig-anthropic-1"},
                        {"type": "text", "text": "response 1"}
                    ]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "follow up"}]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "thought 2", "signature": "sig-anthropic-2"},
                        {"type": "text", "text": "response 2"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    /// Fresh tracker (simulating proxy restart) should transform thinking blocks
    /// when sending to a backend, because we don't know where the signatures came from.
    ///
    /// Current behavior: FAILS - fresh tracker assumes signatures belong to target backend.
    /// Desired behavior: Transform all thinking blocks on first request after restart.
    #[test]
    fn fresh_tracker_first_request_should_transform() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);
        // Simulate: proxy just restarted, first request has thinking from previous session
        let body = body_with_thinking("sig-from-previous-session");

        // First request to GLM - tracker has no history
        let output = tracker.transform_request("glm", &body);

        // We want this to transform because we can't verify the signature origin
        assert!(
            output.result.changed,
            "Fresh tracker should transform thinking blocks on first request"
        );
        assert_eq!(output.result.convert_count, 1);
    }

    /// After switching backends, ALL thinking blocks in history should be transformed,
    /// not just ones with known signatures.
    #[test]
    fn switch_transforms_all_thinking_blocks() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);
        let body = body_with_multiple_thinking();

        // First: establish we're on anthropic
        let _ = tracker.transform_request("anthropic", &body);

        // Switch to GLM - should transform ALL thinking blocks
        let output = tracker.transform_request("glm", &body);

        assert!(output.result.changed);
        assert_eq!(
            output.result.convert_count, 2,
            "All thinking blocks should be transformed on backend switch"
        );
    }

    /// Switching back to original backend should still transform thinking blocks
    /// that were created by the intermediate backend.
    ///
    /// Scenario: anthropic -> glm -> anthropic
    /// The thinking blocks from GLM session should be transformed when going back to anthropic.
    #[test]
    fn switch_back_transforms_intermediate_thinking() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);

        // Start with anthropic
        let body_anthropic = body_with_thinking("sig-anthropic");
        let _ = tracker.transform_request("anthropic", &body_anthropic);

        // Switch to GLM, get response with GLM signature
        let body_glm = body_with_thinking("sig-glm");
        let _ = tracker.transform_request("glm", &body_glm);

        // Now switch back to anthropic with history containing GLM thinking
        let output = tracker.transform_request("anthropic", &body_glm);

        assert!(
            output.result.changed,
            "Thinking from GLM should be transformed when switching back to anthropic"
        );
        assert_eq!(output.result.convert_count, 1);
    }

    /// Mixed history: some thinking from anthropic, some from GLM.
    /// When sending to anthropic, only GLM thinking should transform.
    /// When sending to GLM, only anthropic thinking should transform.
    #[test]
    fn mixed_history_transforms_foreign_thinking_only() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);

        // Build history: anthropic thinking, then GLM thinking
        let body_anthropic = body_with_thinking("sig-anthropic");
        let _ = tracker.transform_request("anthropic", &body_anthropic);

        let body_glm = body_with_thinking("sig-glm");
        let _ = tracker.transform_request("glm", &body_glm);

        // Now create a mixed body with both
        let mixed_body = serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "anthropic thought", "signature": "sig-anthropic"},
                        {"type": "text", "text": "response"}
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "glm thought", "signature": "sig-glm"},
                        {"type": "text", "text": "response"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes();

        // Send to anthropic - should transform GLM thinking only
        let output = tracker.transform_request("anthropic", &mixed_body);
        assert!(output.result.changed);
        assert_eq!(
            output.result.convert_count, 1,
            "Only foreign (GLM) thinking should be transformed"
        );

        // Verify the anthropic thinking is preserved and GLM thinking is converted
        let transformed: serde_json::Value =
            serde_json::from_slice(&output.body.unwrap()).unwrap();
        let messages = transformed["messages"].as_array().unwrap();

        // First message (anthropic) should still have thinking block
        let first_content = messages[0]["content"].as_array().unwrap();
        assert_eq!(first_content[0]["type"], "thinking");

        // Second message (was GLM) should be converted to text
        let second_content = messages[1]["content"].as_array().unwrap();
        assert_eq!(second_content[0]["type"], "text");
    }

    /// Signature cache overflow: old signatures get evicted.
    /// After eviction, those thinking blocks should still be handled correctly.
    #[test]
    fn evicted_signature_still_transforms_on_switch() {
        // Use tiny cache to force eviction
        let mut tracker = ThinkingTracker {
            mode: ThinkingMode::ConvertToText,
            last_backend: None,
            signatures: SignatureCache::new(2), // Only 2 entries
        };

        // Fill cache with anthropic signatures
        for i in 0..5 {
            let body = body_with_thinking(&format!("sig-{}", i));
            let _ = tracker.transform_request("anthropic", &body);
        }

        // sig-0, sig-1, sig-2 should be evicted, only sig-3, sig-4 remain

        // Now switch to GLM with evicted signature
        let body_evicted = body_with_thinking("sig-0");
        let output = tracker.transform_request("glm", &body_evicted);

        // Even though sig-0 was evicted, we switched backends so it should transform
        assert!(
            output.result.changed,
            "Evicted signature should still transform on backend switch"
        );
    }
}
