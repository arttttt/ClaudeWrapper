# Ratatui Scaffold Plan

Goal: establish a minimal TUI runtime with ratatui + crossterm that can host
header/footer/body widgets later, with safe terminal cleanup on exit or panic.

## Entry point

- Keep `src/main.rs` as the program entry and call into `ui::run()`.
- `ui::run()` owns terminal setup/teardown and the event loop.

## Terminal setup/teardown

- Use `crossterm::terminal::enable_raw_mode()` and `EnterAlternateScreen`.
- Build `Terminal<CrosstermBackend<Stdout>>`.
- On exit or panic, restore terminal state:
  - `disable_raw_mode()`
  - `LeaveAlternateScreen`
  - `Show` cursor

Implementation detail: store a `TerminalGuard` that implements `Drop` for cleanup.
Register a panic hook that calls the guard cleanup before forwarding to the
default hook.

## App struct

Create an `App` struct in `src/ui/app.rs` to hold UI state:

- `should_quit: bool`
- `tick_rate: Duration`
- `last_tick: Instant`
- `show_popup: bool` (placeholder for modal)
- `status_message: Option<String>` (placeholder for header/footer status)

Provide `App::new()` and simple handlers:

- `on_tick()` for periodic updates
- `on_key(KeyEvent)` to mutate state (quit on `q`/`Esc`)
- `on_resize(u16, u16)` placeholder

## Event loop

Add `ui::events` helper with an enum:

- `AppEvent::Input(KeyEvent)`
- `AppEvent::Tick`
- `AppEvent::Resize(u16, u16)`

Spawn a dedicated thread that polls crossterm events and sends `AppEvent`s over
`std::sync::mpsc::channel`. The main loop:

1. Draw UI every iteration (using `Terminal::draw`).
2. Block on next `AppEvent` with timeout derived from `tick_rate`.
3. Dispatch to `App` handlers and exit when `should_quit` is true.

## Rendering (placeholder)

Keep the initial draw minimal: split into header/body/footer layout and render
`Block` titles without real widgets. This enables later beads to plug in header,
footer, and body components without restructuring.
