use crate::pty::hotkey::is_wrapper_hotkey;
use crate::pty::resize::ResizeWatcher;
use crate::pty::vt::VtParser;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::error::Error;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use termwiz::surface::Surface;

pub struct PtyManager {
    vt_parser: VtParser,
    screen: Arc<Mutex<Surface>>,
}

impl PtyManager {
    pub fn new() -> Self {
        let (cols, rows) = terminal_size().unwrap_or((80, 24));
        let screen = Surface::new(usize::from(cols), usize::from(rows));
        Self {
            vt_parser: VtParser::new(),
            screen: Arc::new(Mutex::new(screen)),
        }
    }

    pub fn parse_output(&mut self, bytes: &[u8]) -> Vec<termwiz::escape::Action> {
        self.vt_parser.parse(bytes)
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
        self.resize_screen(cols, rows);

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
            ResizeWatcher::start(Arc::clone(&resize_master), Arc::clone(&self.screen))?;

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

    fn resize_screen(&self, cols: u16, rows: u16) {
        if let Ok(mut screen) = self.screen.lock() {
            screen.resize(usize::from(cols), usize::from(rows));
        }
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
