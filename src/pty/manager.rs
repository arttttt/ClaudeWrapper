use crate::pty::emulator;
use crate::pty::emulator::TerminalEmulator;
use crate::pty::hotkey::is_wrapper_hotkey;
use crate::pty::resize::ResizeWatcher;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::error::Error;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::thread;

pub struct PtyManager {
    emulator: Arc<Mutex<Box<dyn TerminalEmulator>>>,
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyManager {
    pub fn new() -> Self {
        let (cols, rows) = terminal_size().unwrap_or((80, 24));
        Self {
            emulator: Arc::new(Mutex::new(emulator::create(rows, cols, 0))),
        }
    }

    pub fn run_command(
        &mut self,
        command: String,
        args: Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let pty_system = native_pty_system();
        let (cols, rows) = terminal_size().unwrap_or((80, 24));
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        self.resize_emulator(cols, rows);

        let mut cmd = CommandBuilder::new(command);
        cmd.args(args);
        cmd.cwd(std::env::current_dir()?);

        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let raw_mode_guard = RawModeGuard::new()?;

        let master = pair.master;
        let reader = master.try_clone_reader()?;
        let writer = master.take_writer()?;
        let resize_master = Arc::new(Mutex::new(master));
        let resize_watcher =
            ResizeWatcher::start(Arc::clone(&resize_master), Arc::clone(&self.emulator))?;

        let reader_handle = thread::spawn(move || {
            let mut reader = reader;
            let mut stdout = io::stdout();
            let _ = io::copy(&mut reader, &mut stdout);
            let _ = stdout.flush();
        });

        let _writer_handle = thread::spawn(move || {
            let mut stdin = io::stdin();
            let mut writer = writer;
            let mut buffer = [0u8; 1024];

            loop {
                let read_bytes = match stdin.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => count,
                    Err(_) => break,
                };

                let mut filtered = Vec::with_capacity(read_bytes);
                for &byte in &buffer[..read_bytes] {
                    if is_wrapper_hotkey(byte) {
                        continue;
                    }
                    filtered.push(byte);
                }

                if filtered.is_empty() {
                    continue;
                }

                if writer.write_all(&filtered).is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
        });

        let status = child.wait()?;
        drop(raw_mode_guard);
        if let Some(watcher) = resize_watcher {
            watcher.stop();
        }
        let _ = reader_handle.join();

        if status.success() {
            return Ok(());
        }

        std::process::exit(status.exit_code() as i32);
    }

    fn resize_emulator(&self, cols: u16, rows: u16) {
        self.emulator.lock().set_size(rows, cols);
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self, Box<dyn Error>> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
