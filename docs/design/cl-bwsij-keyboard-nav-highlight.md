# Design: Backend Selector Keyboard Navigation Highlight

**Bead:** cl-bwsij
**Problem:** No visual feedback when navigating with keyboard in backend selector popup

## Analysis

### Current Implementation

The code has three relevant parts:

1. **Input handling** (`src/ui/input.rs:36-43`):
   - Up/Down arrows call `app.move_backend_selection(-1/1)`
   - This works correctly

2. **State management** (`src/ui/app.rs:237-258`):
   - `move_backend_selection()` updates `backend_selection` index with wraparound
   - This works correctly

3. **Rendering** (`src/ui/render.rs:88-129`):
   ```rust
   let selected_index = app.backend_selection();
   for (idx, backend) in app.backends().iter().enumerate() {
       // Build spans with foreground colors...
       let mut line = Line::from(spans);
       if is_selected {
           line = line.style(Style::default().bg(ACTIVE_HIGHLIGHT));
       }
       lines.push(line);
   }
   ```

### Root Cause

The issue is how ratatui handles `Line::style()` with pre-styled spans:

```rust
// Spans have foreground colors set:
Span::styled("text", Style::default().fg(HEADER_TEXT))

// Line style only sets background:
line.style(Style::default().bg(ACTIVE_HIGHLIGHT))
```

When spans already have `Style::default().fg(...)`, their style is `Style { fg: Some(color), bg: None, ... }`. The line's style `Style { fg: None, bg: Some(highlight), ... }` should merge, but in practice the span's `bg: None` overrides the line's background.

This is a known ratatui behavior - to get background colors on styled spans, you must explicitly set the background on each span.

## Solution

Modify the span styling to include the highlight background when selected:

```rust
for (idx, backend) in app.backends().iter().enumerate() {
    let is_selected = idx == selected_index;

    // Base style includes highlight background when selected
    let base_style = if is_selected {
        Style::default().bg(ACTIVE_HIGHLIGHT)
    } else {
        Style::default()
    };

    let mut spans = Vec::new();
    spans.push(Span::styled(
        format!("    {}. ", idx + 1),
        base_style.fg(HEADER_TEXT),
    ));
    spans.push(Span::styled(
        format!("{:<width$}", backend.display_name, width = max_name_width),
        base_style.fg(HEADER_TEXT),
    ));
    spans.push(Span::styled("  [", base_style));
    spans.push(Span::styled(status_text, base_style.fg(status_color)));
    spans.push(Span::styled("]", base_style));

    lines.push(Line::from(spans));
}
```

Key changes:
- Create `base_style` with highlight background when selected
- Apply `base_style` to all spans (with additional `.fg()` where needed)
- Remove the `line.style()` call since it's not needed

## Alternative: More Visible Highlight

Additionally, consider making `ACTIVE_HIGHLIGHT` more visible. Current value is `RGB(58, 58, 58)` which is very subtle. Options:
- Brighter gray: `RGB(80, 80, 80)`
- Use accent color with transparency: `RGB(100, 60, 50)`
- Add bold or underline to selected text

## Files to Modify

1. `src/ui/render.rs` - Fix span styling for highlight
2. (Optional) `src/ui/theme.rs` - Adjust `ACTIVE_HIGHLIGHT` brightness

## Testing

1. Open backend selector with Ctrl+B
2. Press Up/Down arrows
3. Verify highlight moves between backends
4. Verify Enter selects the highlighted backend
