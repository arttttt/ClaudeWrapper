use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear as TermClear, ClearType, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout, Write};
use std::sync::{Arc, Mutex};

pub struct TerminalGuard {
    cleanup: Arc<Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>>,
}

impl TerminalGuard {
    fn new() -> Self {
        Self {
            cleanup: Arc::new(Mutex::new(None)),
        }
    }

    fn set_cleanup<F: FnOnce() + Send + 'static>(&self, cleanup: F) {
        if let Ok(mut slot) = self.cleanup.lock() {
            *slot = Some(Box::new(cleanup));
        }
    }

    fn install_panic_hook(&self) {
        let cleanup = Arc::clone(&self.cleanup);
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Ok(mut slot) = cleanup.lock() {
                if let Some(cleanup) = slot.take() {
                    cleanup();
                }
            }
            default_hook(info);
        }));
    }

    fn restore(&self) {
        if let Ok(mut slot) = self.cleanup.lock() {
            if let Some(cleanup) = slot.take() {
                cleanup();
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

pub fn setup_terminal() -> io::Result<(Terminal<CrosstermBackend<Stdout>>, TerminalGuard)> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableBracketedPaste)?;
    stdout.execute(TermClear(ClearType::All))?;
    stdout.write_all(b"\x1b[3J")?;
    stdout.flush()?;
    stdout.execute(Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    let guard = TerminalGuard::new();
    guard.set_cleanup(|| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableBracketedPaste);
        let _ = stdout.execute(LeaveAlternateScreen);
        let _ = stdout.execute(Show);
    });
    guard.install_panic_hook();

    Ok((terminal, guard))
}
