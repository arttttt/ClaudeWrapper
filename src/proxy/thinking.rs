use serde_json::Value;

use crate::config::ThinkingMode;

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
}

impl ThinkingTracker {
    pub fn new(mode: ThinkingMode) -> Self {
        Self { mode }
    }

    pub fn set_mode(&mut self, mode: ThinkingMode) {
        self.mode = mode;
    }

    pub fn transform_request(&mut self, _backend: &str, body: &[u8]) -> ThinkingTransformOutput {
        let mut result = ThinkingTransformResult::default();
        let mut json: Value = match serde_json::from_slice(body) {
            Ok(value) => value,
            Err(_) => {
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

                        let text = obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .or_else(|| obj.get("thinking").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        match self.mode {
                            ThinkingMode::DropSignature => {
                                // Only drop signature, keep thinking block structure
                                if obj.remove("signature").is_some() {
                                    result.drop_count = result.drop_count.saturating_add(1);
                                    changed = true;
                                }
                            }
                            ThinkingMode::ConvertToText => {
                                // Always convert thinking to plain text
                                *item = serde_json::json!({
                                    "type": "text",
                                    "text": text,
                                });
                                result.convert_count = result.convert_count.saturating_add(1);
                                changed = true;
                            }
                            ThinkingMode::ConvertToTags => {
                                // Always convert thinking to <think> tags
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

        if changed {
            // Remove context_management when thinking blocks are transformed
            // This field is used by Claude Code to manage thinking blocks,
            // but becomes invalid after we transform them to text
            if let Some(obj) = json.as_object_mut() {
                if obj.remove("context_management").is_some() {
                    tracing::debug!("Removed context_management field after thinking transform");
                }
            }

            match serde_json::to_vec(&json) {
                Ok(body) => {
                    return ThinkingTransformOutput {
                        body: Some(body),
                        result,
                    };
                }
                Err(e) => {
                    // Serialization failed - log error and return unchanged
                    // This prevents sending partially modified data
                    tracing::error!(
                        error = %e,
                        "Failed to serialize transformed request body, using original"
                    );
                    result.changed = false;
                    return ThinkingTransformOutput { body: None, result };
                }
            }
        }

        ThinkingTransformOutput { body: None, result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_with_thinking(text: &str) -> Vec<u8> {
        serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": text, "signature": "some-sig"},
                        {"type": "text", "text": "hello"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    fn body_without_thinking() -> Vec<u8> {
        serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "hello"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn drop_signature_removes_signature_keeps_thinking() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::DropSignature);
        let body = body_with_thinking("my thoughts");
        let output = tracker.transform_request("any", &body);

        assert!(output.result.changed);
        assert_eq!(output.result.drop_count, 1);

        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("\"type\":\"thinking\""));
        assert!(!transformed.contains("\"signature\""));
    }

    #[test]
    fn convert_to_text_always_converts() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToText);
        let body = body_with_thinking("my thoughts");
        let output = tracker.transform_request("any", &body);

        assert!(output.result.changed);
        assert_eq!(output.result.convert_count, 1);

        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("\"type\":\"text\""));
        assert!(transformed.contains("my thoughts"));
        assert!(!transformed.contains("\"type\":\"thinking\""));
    }

    #[test]
    fn convert_to_tags_wraps_in_think_tags() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToTags);
        let body = body_with_thinking("my thoughts");
        let output = tracker.transform_request("any", &body);

        assert!(output.result.changed);
        assert_eq!(output.result.tag_count, 1);

        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("<think>my thoughts</think>"));
        assert!(!transformed.contains("\"type\":\"thinking\""));
    }

    #[test]
    fn no_change_when_no_thinking_blocks() {
        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToTags);
        let body = body_without_thinking();
        let output = tracker.transform_request("any", &body);

        assert!(!output.result.changed);
        assert!(output.body.is_none());
    }

    #[test]
    fn handles_multiple_thinking_blocks() {
        let body = serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "thought 1", "signature": "sig1"},
                        {"type": "text", "text": "response 1"}
                    ]
                },
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "thought 2", "signature": "sig2"},
                        {"type": "text", "text": "response 2"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes();

        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToTags);
        let output = tracker.transform_request("any", &body);

        assert!(output.result.changed);
        assert_eq!(output.result.tag_count, 2);

        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("<think>thought 1</think>"));
        assert!(transformed.contains("<think>thought 2</think>"));
    }

    #[test]
    fn handles_thinking_field_instead_of_text() {
        // Some providers use "thinking" field instead of "text"
        let body = serde_json::json!({
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "thinking", "thinking": "deep thoughts", "signature": "sig"},
                        {"type": "text", "text": "response"}
                    ]
                }
            ]
        })
        .to_string()
        .into_bytes();

        let mut tracker = ThinkingTracker::new(ThinkingMode::ConvertToTags);
        let output = tracker.transform_request("any", &body);

        assert!(output.result.changed);
        let transformed = String::from_utf8(output.body.unwrap()).unwrap();
        assert!(transformed.contains("<think>deep thoughts</think>"));
    }
}
