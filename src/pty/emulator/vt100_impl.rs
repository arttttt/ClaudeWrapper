use super::{CursorState, TermCell, TermColor, TerminalEmulator};

pub(super) struct Vt100Emulator {
    parser: vt100::Parser,
}

impl Vt100Emulator {
    pub(super) fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, scrollback_len),
        }
    }
}

impl TerminalEmulator for Vt100Emulator {
    fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    fn set_size(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
    }

    fn cell(&self, row: u16, col: u16) -> Option<TermCell> {
        let cell = self.parser.screen().cell(row, col)?;
        Some(TermCell {
            symbol: cell.contents().to_string(),
            fg: convert_color(cell.fgcolor()),
            bg: convert_color(cell.bgcolor()),
            bold: cell.bold(),
            italic: cell.italic(),
            underline: cell.underline(),
            inverse: cell.inverse(),
            has_contents: cell.has_contents(),
            is_wide_continuation: cell.is_wide_continuation(),
        })
    }

    fn scrollback(&self) -> usize {
        self.parser.screen().scrollback()
    }

    fn set_scrollback(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    fn cursor(&self) -> CursorState {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        CursorState {
            row,
            col,
            visible: !screen.hide_cursor(),
        }
    }
}

fn convert_color(color: vt100::Color) -> TermColor {
    match color {
        vt100::Color::Default => TermColor::Default,
        vt100::Color::Idx(idx) => TermColor::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => TermColor::Rgb(r, g, b),
    }
}
