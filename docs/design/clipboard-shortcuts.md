# Design: Clipboard Shortcuts (Ctrl+C/Ctrl+V) Passthrough

## Problem Statement

Clipboard keyboard shortcuts (Ctrl+C/Ctrl+V) are not working in the Claude Code subprocess. Users cannot copy or paste text when using the wrapper.

## Root Cause Analysis

Three issues were identified:

### 1. Paste Events Silently Discarded

**Location:** `src/ui/events.rs:44`

```rust
match event::read() {
    Ok(Event::Key(key)) => { ... }
    Ok(Event::Resize(cols, rows)) => { ... }
    Ok(_) => {}  // <-- Event::Paste is caught here and ignored
    Err(_) => break,
}
```

Crossterm's `Event::Paste(String)` contains pasted text when bracketed paste mode is enabled. The current code silently discards these events.

### 2. Bracketed Paste Mode Not Enabled

**Location:** `src/ui/terminal_guard.rs`

The terminal setup does not enable bracketed paste mode:
```rust
pub fn setup_terminal() -> io::Result<...> {
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    // Missing: stdout.execute(EnableBracketedPaste)?;
    ...
}
```

Without bracketed paste, the terminal cannot send paste content as events.

### 3. OSC 52 Clipboard Sequences Not Forwarded

**Location:** `src/pty/screen.rs:338-349`

The `translate_osc()` function only handles window title OSC sequences:
```rust
fn translate_osc(osc: Box<OperatingSystemCommand>, changes: &mut Vec<Change>) {
    match *osc {
        OperatingSystemCommand::SetWindowTitle(title) => { ... }
        // OSC 52 (SystemClipboard) is silently ignored
        _ => {}
    }
}
```

When Claude Code sends OSC 52 to write to clipboard, the sequence is parsed but never forwarded to the parent terminal.

## Solution Design

### Part 1: Handle Paste Events

**File: `src/ui/events.rs`**

Add a new event variant:
```rust
pub enum AppEvent {
    Input(KeyEvent),
    Paste(String),  // NEW
    Tick,
    ...
}
```

Handle `Event::Paste` in the event loop:
```rust
match event::read() {
    Ok(Event::Key(key)) => {
        let _ = event_tx.send(AppEvent::Input(key));
    }
    Ok(Event::Paste(text)) => {
        let _ = event_tx.send(AppEvent::Paste(text));
    }
    Ok(Event::Resize(cols, rows)) => {
        let _ = event_tx.send(AppEvent::Resize(cols, rows));
    }
    Ok(_) => {}
    Err(_) => break,
}
```

**File: `src/ui/runtime.rs`**

Handle paste events by sending the text to the PTY:
```rust
Ok(AppEvent::Paste(text)) => {
    if let Some(pty) = app.pty() {
        // Send paste content as bracketed paste to subprocess
        let bracketed = format!("\x1b[200~{}\x1b[201~", text);
        let _ = pty.send_input(bracketed.as_bytes());
    }
}
```

### Part 2: Enable Bracketed Paste Mode

**File: `src/ui/terminal_guard.rs`**

Enable bracketed paste during setup:
```rust
use crossterm::event::{EnableBracketedPaste, DisableBracketedPaste};

pub fn setup_terminal() -> io::Result<...> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableBracketedPaste)?;  // NEW
    ...

    guard.set_cleanup(|| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableBracketedPaste);  // NEW
        let _ = stdout.execute(LeaveAlternateScreen);
        let _ = stdout.execute(Show);
    });
    ...
}
```

### Part 3: Forward OSC 52 Clipboard Sequences

**File: `src/pty/screen.rs`**

Modify `translate_osc()` to forward OSC 52 sequences to parent terminal:
```rust
use termwiz::escape::osc::Selection;

fn translate_osc(osc: Box<OperatingSystemCommand>, changes: &mut Vec<Change>) {
    match *osc {
        OperatingSystemCommand::SetWindowTitle(title) => { ... }
        OperatingSystemCommand::SystemClipboard(selection, data) => {
            // Forward OSC 52 to parent terminal
            forward_osc52_to_parent(selection, data);
        }
        _ => {}
    }
}

fn forward_osc52_to_parent(selection: Selection, data: String) {
    // Write OSC 52 sequence directly to stdout (parent terminal)
    let selection_char = match selection {
        Selection::Clipboard => 'c',
        Selection::Primary => 'p',
        Selection::Select => 's',
        Selection::Cut0 => '0',
        // ... etc
    };
    let seq = format!("\x1b]52;{};{}\x07", selection_char, data);
    let _ = io::stdout().write_all(seq.as_bytes());
    let _ = io::stdout().flush();
}
```

**Alternative approach:** Instead of intercepting in screen.rs, we could:
- Pass raw OSC 52 sequences through without parsing
- Or add a separate output filter that forwards clipboard sequences

### Part 4: Ctrl+C Behavior (Interrupt Signal)

Ctrl+C (0x03) is already correctly passed through to the subprocess:
- Not filtered in `is_wrapper_hotkey()` (only 0x02, 0x11, 0x13)
- Converted correctly in `key_event_to_bytes()` to 0x03
- Sent to PTY subprocess

Claude Code will receive the interrupt signal as expected. If "copy" functionality requires OSC 52 (Claude Code uses OSC 52 to write to clipboard), then Part 3 addresses this.

## File Changes Summary

| File | Change |
|------|--------|
| `src/ui/events.rs` | Add `Paste(String)` event variant, handle `Event::Paste` |
| `src/ui/runtime.rs` | Handle `AppEvent::Paste`, send to PTY as bracketed paste |
| `src/ui/terminal_guard.rs` | Enable/disable bracketed paste mode |
| `src/pty/screen.rs` | Forward OSC 52 sequences to parent terminal |

## Testing Plan

1. **Paste test**: Copy text in another app, Ctrl+V in Claude Code → text should appear
2. **Copy test**: Select text in Claude Code, trigger copy → text should be in system clipboard
3. **Interrupt test**: Ctrl+C should still send interrupt signal to subprocess
4. **Cleanup test**: Exit wrapper → bracketed paste should be disabled (test by pasting in shell)

## Dependencies

- crossterm already supports `Event::Paste` and `EnableBracketedPaste`
- termwiz already parses `OperatingSystemCommand::SystemClipboard`
- No new crate dependencies required
