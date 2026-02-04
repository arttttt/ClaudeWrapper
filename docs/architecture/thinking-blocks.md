# Thinking Blocks Architecture

This document describes how AnyClaude handles Claude's thinking blocks when proxying
requests across different backends.

## Problem Statement

Claude's thinking blocks contain cryptographic signatures that are only valid for the
backend that generated them. When a user switches backends mid-conversation:

1. Old thinking blocks have signatures from the previous backend
2. The new backend rejects these invalid signatures with a 400 error
3. The conversation becomes stuck - Claude Code keeps resending the invalid blocks

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ Claude Code │────▶│  AnyClaude  │────▶│  Anthropic  │
│             │     │   (proxy)   │     │   Backend   │
└─────────────┘     └─────────────┘     └─────────────┘
       │                                       │
       │  Request with thinking blocks         │
       │  from GLM backend                     │
       │                                       │
       │                              400 Error│
       │◀──────────────────────────────────────│
       │  "Invalid thinking block signature"   │
```

## Solution: Session-Based Thinking Registry

We track thinking blocks by session. Each backend switch increments a session ID,
invalidating all previous thinking blocks.

### Core Data Structure

```rust
pub struct ThinkingRegistry {
    current_session: u64,           // Increments on each backend switch
    current_backend: String,        // Current backend name
    blocks: HashMap<u64, u64>,      // content_hash → session_id
}
```

### Content Hashing Strategy

We use a fast hash combining:
- **Prefix**: First 256 bytes of thinking content (UTF-8 safe truncation)
- **Length**: Total content length

This provides good uniqueness while being efficient for large thinking blocks.

```rust
fn fast_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    let prefix = safe_truncate(content, 256);  // UTF-8 safe
    prefix.hash(&mut hasher);
    content.len().hash(&mut hasher);
    hasher.finish()
}
```

## Request/Response Flow

### 1. Outgoing Request (to Backend)

```
┌─────────────────────────────────────────────────────────────────┐
│                     REQUEST PROCESSING                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌───────────────────┐    ┌──────────────┐  │
│  │ Incoming     │───▶│ ThinkingRegistry  │───▶│ Filtered     │  │
│  │ Request      │    │ filter_request()  │    │ Request      │  │
│  └──────────────┘    └───────────────────┘    └──────────────┘  │
│         │                     │                      │          │
│         │                     │                      │          │
│         ▼                     ▼                      ▼          │
│  ┌──────────────┐    ┌───────────────────┐    ┌──────────────┐  │
│  │ Messages:    │    │ For each thinking │    │ Messages:    │  │
│  │ - thinking A │    │ block:            │    │ - text only  │  │
│  │ - thinking B │    │ 1. Hash content   │    │              │  │
│  │ - text       │    │ 2. Check session  │    │ (thinking    │  │
│  │              │    │ 3. Keep or remove │    │  removed)    │  │
│  └──────────────┘    └───────────────────┘    └──────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 2. Backend Switch Detection

```
notify_backend_for_thinking("anthropic")
         │
         ▼
┌─────────────────────────────────┐
│  if current_backend != new:    │
│    session_id += 1             │
│    current_backend = new       │
└─────────────────────────────────┘
         │
         ▼
All previous thinking blocks now invalid
(their session_id < current_session)
```

### 3. Incoming Response (from Backend)

```
┌─────────────────────────────────────────────────────────────────┐
│                    RESPONSE PROCESSING                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SSE Stream Events:                                              │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ event: content_block_start                                │   │
│  │ data: {"type":"thinking", "thinking":"Let me analyze..."}│   │
│  └──────────────────────────────────────────────────────────┘   │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ register_thinking_from_sse(event_data)                    │   │
│  │   1. Extract thinking content                             │   │
│  │   2. Compute hash                                         │   │
│  │   3. Store: blocks[hash] = current_session                │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Session Lifecycle

```
Time ──────────────────────────────────────────────────────────────▶

Session 0          Session 1              Session 2
(initial)          (anthropic)            (glm)
    │                  │                      │
    │   switch to      │    switch to         │
    │   anthropic      │    glm               │
    ▼                  ▼                      ▼
┌───────┐         ┌───────┐              ┌───────┐
│       │         │ T1,T2 │              │ T3,T4 │
│ empty │         │ valid │              │ valid │
│       │         │       │              │       │
└───────┘         └───────┘              └───────┘
                       │                      │
                       │  T1,T2 become        │
                       │  invalid when        │
                       │  session changes     │
                       ▼                      │
                  ┌───────┐                   │
                  │ T1,T2 │◀──────────────────┘
                  │INVALID│   T3,T4 become
                  └───────┘   invalid if we
                              switch again
```

## Integration Points

### TransformerRegistry

The `ThinkingRegistry` is embedded in `TransformerRegistry`:

```rust
pub struct TransformerRegistry {
    current: RwLock<Arc<dyn ThinkingTransformer>>,
    config: std::sync::RwLock<ThinkingConfig>,
    thinking_registry: Mutex<ThinkingRegistry>,  // ◀── HERE
}
```

### Upstream Proxy (upstream.rs)

```rust
// Before sending request:
self.transformer_registry.notify_backend_for_thinking(&backend.name);
let filtered = self.transformer_registry.filter_thinking_blocks(&mut json_body);

// For SSE responses:
for event in sse_stream {
    self.transformer_registry.register_thinking_from_sse(data);
}

// For non-streaming responses:
self.transformer_registry.register_thinking_from_response(&body_bytes);
```

## Thinking Block Types

The registry handles two types of thinking blocks:

| Type | Content Field | Description |
|------|---------------|-------------|
| `thinking` | `thinking` | Normal thinking with visible content |
| `redacted_thinking` | `data` | Encrypted/redacted thinking |

```rust
fn extract_thinking_content(item: &Value) -> Option<String> {
    match item_type {
        "thinking" => item.get("thinking"),
        "redacted_thinking" => item.get("data"),
        _ => None,
    }
}
```

## Memory Management

To prevent unbounded growth, call `cleanup_old_sessions(keep_sessions)`:

```rust
// Keep only last 2 sessions worth of blocks
registry.cleanup_old_sessions(2);
```

This removes blocks from sessions older than `current_session - keep_sessions`.

## Why Not Filter by Signature?

Alternative approaches considered and rejected:

| Approach | Problem |
|----------|---------|
| Filter empty signatures | GLM might start generating signatures |
| Filter by signature pattern | Signatures are opaque, patterns may change |
| Strip all thinking blocks | Loses valuable context for ongoing work |
| Strip only on backend switch | Complex state management |

The session-based approach is:
- **Reliable**: Works regardless of signature format
- **Future-proof**: Doesn't depend on backend-specific behavior
- **Efficient**: O(1) lookup per thinking block
- **Simple**: Single session counter invalidates all old blocks

## Debug Logging

Enable detailed logging with:

```toml
[debug_log]
enabled = true
full_body = true
pretty_print = true
```

This will log:
- Thinking block registration events
- Filter decisions (kept/removed)
- Session transitions
- SSE event summaries
