use crate::pty::{parse_command, PtySession};
use crate::ui::app::App;
use crate::ui::events::{AppEvent, EventHandler};
use crate::ui::input::handle_key;
use crate::ui::layout::body_rect;
use crate::ui::render::draw;
use crate::ui::terminal_guard::setup_terminal;
use ratatui::layout::Rect;
use std::io;
use std::time::Duration;

pub fn run() -> io::Result<()> {
    let (mut terminal, guard) = setup_terminal()?;
    let tick_rate = Duration::from_millis(250);
    let mut app = App::new(tick_rate);
    let events = EventHandler::new(tick_rate);
    let (command, args) = parse_command();
    let mut pty_session = PtySession::spawn(command, args, events.sender())
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
    app.attach_pty(pty_session.handle());
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let body = body_rect(Rect {
            x: 0,
            y: 0,
            width: cols,
            height: rows,
        });
        app.on_resize(body.width.max(1), body.height.max(1));
    }

    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if app.should_quit() {
            break;
        }

        match events.next(tick_rate) {
            Ok(AppEvent::Input(key)) => handle_key(&mut app, key),
            Ok(AppEvent::Tick) => app.on_tick(),
            Ok(AppEvent::Resize(cols, rows)) => {
                let body = body_rect(Rect {
                    x: 0,
                    y: 0,
                    width: cols,
                    height: rows,
                });
                app.on_resize(body.width.max(1), body.height.max(1));
            }
            Ok(AppEvent::PtyOutput) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = pty_session.shutdown();
    drop(guard);
    Ok(())
}
