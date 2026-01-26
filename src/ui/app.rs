use crossterm::event::KeyEvent;
use std::time::{Duration, Instant};

pub struct App {
    should_quit: bool,
    tick_rate: Duration,
    last_tick: Instant,
    show_popup: bool,
    status_message: Option<String>,
    size: Option<(u16, u16)>,
}

impl App {
    pub fn new(tick_rate: Duration) -> Self {
        Self {
            should_quit: false,
            tick_rate,
            last_tick: Instant::now(),
            show_popup: false,
            status_message: None,
            size: None,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn request_quit(&mut self) {
        self.should_quit = true;
    }

    pub fn show_popup(&self) -> bool {
        self.show_popup
    }

    pub fn on_tick(&mut self) {
        self.last_tick = Instant::now();
    }

    pub fn on_key(&mut self, _key: KeyEvent) {}

    pub fn on_resize(&mut self, cols: u16, rows: u16) {
        self.size = Some((cols, rows));
    }

    #[allow(dead_code)]
    pub fn tick_rate(&self) -> Duration {
        self.tick_rate
    }

    #[allow(dead_code)]
    pub fn last_tick(&self) -> Instant {
        self.last_tick
    }

    #[allow(dead_code)]
    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }
}
