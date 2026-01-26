use crate::ui::app::{App, PopupKind};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    if is_ctrl_char(key, 'q') {
        app.request_quit();
        return;
    }

    if app.show_popup() {
        if matches!(key.code, KeyCode::Esc) {
            app.close_popup();
            return;
        }
        if is_ctrl_char(key, 'b') {
            app.toggle_popup(PopupKind::BackendSwitch);
            return;
        }
        if is_ctrl_char(key, 's') {
            app.toggle_popup(PopupKind::Status);
            return;
        }
        return;
    }

    if is_ctrl_char(key, 'b') {
        app.toggle_popup(PopupKind::BackendSwitch);
        return;
    }
    if is_ctrl_char(key, 's') {
        app.toggle_popup(PopupKind::Status);
        return;
    }

    app.on_key(key);
}

fn is_ctrl_char(key: KeyEvent, needle: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&needle))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}
