Refactor plan (cl-fwd.11)

Goals
- Split UI runtime loop, terminal setup, and layout/renderer into focused modules.
- Split PTY handling into session, handle, screen, and resize responsibilities.
- Keep public interfaces narrow and stable; move helpers behind modules.

Proposed module boundaries

src/ui/
- app.rs: App state and state transitions only.
- events.rs: Event ingestion + channel API (already isolated).
- runtime.rs: run loop, event dispatch, PTY wiring.
- input.rs: key handling and routing (quit shortcuts, PTY input mapping).
- layout.rs: layout_regions, body_rect, centered_rect.
- render.rs: draw function and widget composition.
- terminal_guard.rs: setup_terminal, TerminalGuard lifecycle.

Key interfaces
- UiRuntime { run() } owns Terminal, App, EventHandler, PtySession.
- InputRouter { handle_key(app, key) } isolates exit shortcuts and forwards.
- Renderer { draw(frame, app) } uses layout + widgets only.

src/pty/
- mod.rs: pub API exports and small helpers.
- session.rs: PtySession spawn/shutdown and reader thread.
- handle.rs: PtyHandle input + resize APIs.
- screen.rs: apply_actions, translate_* helpers, ActionTranslator.
- resize.rs: ResizeWatcher.
- command.rs: parse_command, parse_command_from.
- hotkey.rs: is_wrapper_hotkey.

Traits (only if needed)
- PtyIO: send_input, resize, screen; implemented by PtyHandle.
- AppView: screen() access for renderer, if decoupling from App is needed.

Implementation order
1. Extract UI helpers (layout, terminal_guard, input) from src/ui/mod.rs.
2. Move run loop into src/ui/runtime.rs and re-export run from src/ui/mod.rs.
3. Split PTY module (session/handle/screen/resize/command/hotkey).
4. Update imports in ui/runtime.rs and other call sites.
5. Ensure tests still compile (pty parse_command tests move to command.rs).

Non-goals
- No new features, no behavior changes.
- No changes to UI visuals or key bindings.
