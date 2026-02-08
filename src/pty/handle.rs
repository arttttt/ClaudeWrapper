use crate::pty::emulator::TerminalEmulator;
use crate::pty::hotkey::is_wrapper_hotkey;
use parking_lot::Mutex;
use portable_pty::MasterPty;
use std::io::{self, Write};
use std::sync::Arc;

#[derive(Clone)]
pub struct PtyHandle {
    emulator: Arc<Mutex<Box<dyn TerminalEmulator>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
}

impl PtyHandle {
    pub fn new(
        emulator: Arc<Mutex<Box<dyn TerminalEmulator>>>,
        writer: Box<dyn Write + Send>,
        master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    ) -> Self {
        Self {
            emulator,
            writer: Arc::new(Mutex::new(Some(writer))),
            master,
        }
    }

    pub fn emulator(&self) -> Arc<Mutex<Box<dyn TerminalEmulator>>> {
        Arc::clone(&self.emulator)
    }

    pub fn send_input(&self, bytes: &[u8]) -> io::Result<()> {
        let mut writer_guard = self.writer.lock();
        let Some(writer) = writer_guard.as_mut() else {
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

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), Box<dyn std::error::Error>> {
        let size = portable_pty::PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master.lock().resize(size)?;
        self.emulator.lock().set_size(rows, cols);
        Ok(())
    }

    /// Get the current scrollback offset.
    pub fn scrollback(&self) -> usize {
        self.emulator.lock().scrollback()
    }

    /// Set the scrollback offset.
    pub fn set_scrollback(&self, offset: usize) {
        self.emulator.lock().set_scrollback(offset);
    }

    /// Scroll up by the given number of lines.
    pub fn scroll_up(&self, lines: usize) {
        let mut emu = self.emulator.lock();
        let current = emu.scrollback();
        emu.set_scrollback(current.saturating_add(lines));
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&self, lines: usize) {
        let mut emu = self.emulator.lock();
        let current = emu.scrollback();
        emu.set_scrollback(current.saturating_sub(lines));
    }

    /// Reset scrollback to show current (live) content.
    pub fn reset_scrollback(&self) {
        self.set_scrollback(0);
    }

    pub fn close_writer(&self) {
        *self.writer.lock() = None;
    }
}
