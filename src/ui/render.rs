use crate::ui::app::App;
use crate::ui::footer::Footer;
use crate::ui::header::Header;
use crate::ui::layout::{centered_rect, layout_regions};
use crate::ui::terminal::TerminalBody;
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;
use std::sync::Arc;
use termwiz::surface::CursorVisibility;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.size();
    let (header, body, footer) = layout_regions(area);

    let header_widget = Header::new();
    frame.render_widget(header_widget.widget(), header);
    frame.render_widget(Clear, body);
    if let Some(screen) = app.screen() {
        frame.render_widget(TerminalBody::new(Arc::clone(&screen)), body);
        if body.width > 0 && body.height > 0 {
            if let Ok(screen) = screen.lock() {
                if screen.cursor_visibility() == CursorVisibility::Visible {
                    let (x, y) = screen.cursor_position();
                    let x = body.x + x.min(body.width.saturating_sub(1) as usize) as u16;
                    let y = body.y + y.min(body.height.saturating_sub(1) as usize) as u16;
                    frame.set_cursor(x, y);
                }
            }
        }
    }
    let footer_widget = Footer::new();
    frame.render_widget(footer_widget.widget(), footer);

    if app.show_popup() {
        let popup = Block::default().title("Popup").borders(Borders::ALL);
        frame.render_widget(popup, centered_rect(60, 30, frame.size()));
    }
}
