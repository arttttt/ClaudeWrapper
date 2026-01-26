use crate::ui::app::App;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if matches!(key.code, KeyCode::Esc)
        || (matches!(key.code, KeyCode::Char('q')) && key.modifiers.contains(KeyModifiers::CONTROL))
    {
        app.request_quit();
    } else {
        app.on_key(key);
    }
}
