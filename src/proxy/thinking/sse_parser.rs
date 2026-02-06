//! SSE response parser for extracting assistant text from Claude API streaming responses.
//!
//! Uses the shared SSE parser from `crate::sse`.

/// Parse SSE response bytes and extract the assistant's text content.
///
/// Accumulates all `text_delta` content and returns the full response text.
pub fn extract_assistant_text(sse_bytes: &[u8]) -> Option<String> {
    let events = crate::sse::parse_sse_events(sse_bytes);
    let mut result = String::new();

    for event in &events {
        if event.event_type == "content_block_delta" {
            if let Some(delta) = event.data.get("delta") {
                if delta.get("type").and_then(|t| t.as_str()) == Some("text_delta") {
                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                        result.push_str(text);
                    }
                }
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_sse_events() {
        let sse = b"data: {\"type\": \"content_block_delta\", \"delta\": {\"type\": \"text_delta\", \"text\": \"Hello \"}}\n\
                    data: {\"type\": \"content_block_delta\", \"delta\": {\"type\": \"text_delta\", \"text\": \"world!\"}}\n\
                    data: {\"type\": \"message_stop\"}\n";

        let result = extract_assistant_text(sse);
        assert_eq!(result, Some("Hello world!".to_string()));
    }

    #[test]
    fn handles_empty_response() {
        let sse = b"data: {\"type\": \"message_start\"}\n\
                    data: {\"type\": \"message_stop\"}\n";

        let result = extract_assistant_text(sse);
        assert_eq!(result, None);
    }

    #[test]
    fn handles_thinking_blocks() {
        let sse = b"data: {\"type\": \"content_block_delta\", \"delta\": {\"type\": \"thinking_delta\", \"thinking\": \"Let me think...\"}}\n\
                    data: {\"type\": \"content_block_delta\", \"delta\": {\"type\": \"text_delta\", \"text\": \"Here's the answer\"}}\n\
                    data: {\"type\": \"message_stop\"}\n";

        let result = extract_assistant_text(sse);
        assert_eq!(result, Some("Here's the answer".to_string()));
    }

    #[test]
    fn handles_done_marker() {
        let sse = b"data: {\"type\": \"content_block_delta\", \"delta\": {\"type\": \"text_delta\", \"text\": \"Test\"}}\n\
                    data: [DONE]\n";

        let result = extract_assistant_text(sse);
        assert_eq!(result, Some("Test".to_string()));
    }

    #[test]
    fn handles_compact_format() {
        let sse = b"data:{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n";

        let result = extract_assistant_text(sse);
        assert_eq!(result, Some("Hi".to_string()));
    }
}
