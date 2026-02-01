use crate::ui::app::{App, PopupKind};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Action to take after processing a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    /// No further action needed (handled internally).
    None,
    /// Request image paste from clipboard.
    ImagePaste,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> InputAction {
    if key.kind != KeyEventKind::Press {
        return InputAction::None;
    }

    if is_ctrl_char(key, 'q') {
        app.request_quit();
        return InputAction::None;
    }

    // Ctrl+V or Ctrl+Shift+V: paste with image support
    // We intercept Ctrl+V because terminal can't represent image content as text.
    // When clipboard has an image, terminal either sends empty paste or key event.
    // By handling both Ctrl+V and Ctrl+Shift+V here, we check clipboard directly.
    if is_ctrl_char(key, 'v') || is_ctrl_shift_char(key, 'v') {
        return InputAction::ImagePaste;
    }

    if app.show_popup() {
        if matches!(key.code, KeyCode::Esc) {
            app.close_popup();
            return InputAction::None;
        }
        if is_ctrl_char(key, 'b') {
            let opened = app.toggle_popup(PopupKind::BackendSwitch);
            if opened {
                app.request_backends_refresh();
            }
            return InputAction::None;
        }
        if is_ctrl_char(key, 's') {
            let opened = app.toggle_popup(PopupKind::Status);
            if opened {
                app.request_status_refresh();
                app.request_metrics_refresh(None);
            }
            return InputAction::None;
        }
        if matches!(app.popup_kind(), Some(PopupKind::BackendSwitch)) {
            match key.code {
                KeyCode::Up => {
                    app.move_backend_selection(-1);
                    return InputAction::None;
                }
                KeyCode::Down => {
                    app.move_backend_selection(1);
                    return InputAction::None;
                }
                KeyCode::Enter => {
                    let index = app.backend_selection();
                    if app.request_switch_backend_by_index(index + 1) {
                        app.close_popup();
                    }
                    return InputAction::None;
                }
                _ => {}
            }
            if let KeyCode::Char(ch) = key.code {
                if ch.is_ascii_digit() {
                    let index = ch.to_digit(10).unwrap_or(0) as usize;
                    if index > 0 && app.request_switch_backend_by_index(index) {
                        app.close_popup();
                    }
                    return InputAction::None;
                }
            }
        }
        return InputAction::None;
    }

    if is_ctrl_char(key, 'b') {
        let opened = app.toggle_popup(PopupKind::BackendSwitch);
        if opened {
            app.request_backends_refresh();
        }
        return InputAction::None;
    }
    if is_ctrl_char(key, 's') {
        let opened = app.toggle_popup(PopupKind::Status);
        if opened {
            app.request_status_refresh();
            app.request_metrics_refresh(None);
        }
        return InputAction::None;
    }

    app.on_key(key);
    InputAction::None
}

fn is_ctrl_char(key: KeyEvent, needle: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&needle))
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::SHIFT)
}

fn is_ctrl_shift_char(key: KeyEvent, needle: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&needle))
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && key.modifiers.contains(KeyModifiers::SHIFT)
}
