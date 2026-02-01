use crate::pty::hotkey::is_wrapper_hotkey;
use portable_pty::{MasterPty, PtySize};
use std::error::Error;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PtyHandle {
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
}

impl PtyHandle {
    pub fn new(
        parser: Arc<Mutex<vt100::Parser>>,
        writer: Box<dyn Write + Send>,
        master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    ) -> Self {
        Self {
            parser,
            writer: Arc::new(Mutex::new(Some(writer))),
            master,
        }
    }

    pub fn parser(&self) -> Arc<Mutex<vt100::Parser>> {
        Arc::clone(&self.parser)
    }

    pub fn send_input(&self, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "input writer lock poisoned"))?;
        let Some(writer) = writer.as_mut() else {
            return Ok(());
        };
        let mut filtered = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            if is_wrapper_hotkey(byte) {
                continue;
            }
            filtered.push(byte);
        }
        if filtered.is_empty() {
            return Ok(());
        }
        writer.write_all(&filtered)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), Box<dyn Error>> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Ok(master) = self.master.lock() {
            master.resize(size)?;
        }
        if let Ok(mut parser) = self.parser.lock() {
            parser.screen_mut().set_size(rows, cols);
        }
        Ok(())
    }

    /// Get the current scrollback offset.
    pub fn scrollback(&self) -> usize {
        self.parser
            .lock()
            .map(|p| p.screen().scrollback())
            .unwrap_or(0)
    }

    /// Set the scrollback offset.
    pub fn set_scrollback(&self, offset: usize) {
        if let Ok(mut parser) = self.parser.lock() {
            parser.screen_mut().set_scrollback(offset);
        }
    }

    /// Scroll up by the given number of lines.
    pub fn scroll_up(&self, lines: usize) {
        if let Ok(mut parser) = self.parser.lock() {
            let current = parser.screen().scrollback();
            parser.screen_mut().set_scrollback(current.saturating_add(lines));
        }
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&self, lines: usize) {
        if let Ok(mut parser) = self.parser.lock() {
            let current = parser.screen().scrollback();
            parser.screen_mut().set_scrollback(current.saturating_sub(lines));
        }
    }

    /// Reset scrollback to show current (live) content.
    pub fn reset_scrollback(&self) {
        self.set_scrollback(0);
    }

    pub fn close_writer(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            *writer = None;
        }
    }
}
