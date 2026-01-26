use crate::pty::hotkey::is_wrapper_hotkey;
use portable_pty::{MasterPty, PtySize};
use std::error::Error;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use termwiz::surface::Surface;

#[derive(Clone)]
pub struct PtyHandle {
    screen: Arc<Mutex<Surface>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
}

impl PtyHandle {
    pub fn new(
        screen: Arc<Mutex<Surface>>,
        writer: Box<dyn Write + Send>,
        master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    ) -> Self {
        Self {
            screen,
            writer: Arc::new(Mutex::new(Some(writer))),
            master,
        }
    }

    pub fn screen(&self) -> Arc<Mutex<Surface>> {
        Arc::clone(&self.screen)
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
        if let Ok(mut screen) = self.screen.lock() {
            screen.resize(usize::from(cols), usize::from(rows));
        }
        Ok(())
    }

    pub fn close_writer(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            *writer = None;
        }
    }
}
