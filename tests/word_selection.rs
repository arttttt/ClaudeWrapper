use anyclaude::pty::emulator::{CursorState, TermCell, TermColor, TerminalEmulator};
use anyclaude::ui::selection::{GridPos, TextSelection};

// ── Mock emulator ───────────────────────────────────────────────

struct MockEmu {
    rows: Vec<Vec<&'static str>>,
}

impl MockEmu {
    fn from_lines(lines: &[&'static str]) -> Self {
        let rows = lines
            .iter()
            .map(|line| line.chars().map(|c| leak(c)).collect())
            .collect();
        Self { rows }
    }
}

fn leak(c: char) -> &'static str {
    Box::leak(c.to_string().into_boxed_str())
}

fn cell(symbol: &str) -> TermCell {
    let is_space = symbol.trim().is_empty();
    TermCell {
        symbol: symbol.to_string(),
        fg: TermColor::Default,
        bg: TermColor::Default,
        bold: false,
        italic: false,
        underline: false,
        inverse: false,
        has_contents: !is_space,
        is_wide_continuation: false,
    }
}

impl TerminalEmulator for MockEmu {
    fn process(&mut self, _bytes: &[u8]) {}
    fn set_size(&mut self, _rows: u16, _cols: u16) {}
    fn cell(&self, row: u16, col: u16) -> Option<TermCell> {
        let r = self.rows.get(row as usize)?;
        let s = r.get(col as usize)?;
        Some(cell(s))
    }
    fn scrollback(&self) -> usize { 0 }
    fn set_scrollback(&mut self, _offset: usize) {}
    fn cursor(&self) -> CursorState {
        CursorState { row: 0, col: 0, visible: true }
    }
    fn mouse_tracking(&self) -> bool { false }
}

fn pos(row: u16, col: u16) -> GridPos {
    GridPos { row, col }
}

// ── select_word: boundary detection ─────────────────────────────

#[test]
fn word_in_middle_of_line() {
    let emu = MockEmu::from_lines(&["hello world foo"]);
    let sel = TextSelection::select_word(&emu, pos(0, 7)).unwrap();
    assert_eq!(sel.start, pos(0, 6));
    assert_eq!(sel.end, pos(0, 10));
    assert!(!sel.active);
    assert_eq!(sel.extract_text(&emu), "world");
}

#[test]
fn word_at_start_of_line() {
    let emu = MockEmu::from_lines(&["hello world"]);
    let sel = TextSelection::select_word(&emu, pos(0, 2)).unwrap();
    assert_eq!(sel.start, pos(0, 0));
    assert_eq!(sel.end, pos(0, 4));
    assert_eq!(sel.extract_text(&emu), "hello");
}

#[test]
fn word_at_end_of_line() {
    let emu = MockEmu::from_lines(&["hello world"]);
    let sel = TextSelection::select_word(&emu, pos(0, 8)).unwrap();
    assert_eq!(sel.start, pos(0, 6));
    assert_eq!(sel.end, pos(0, 10));
    assert_eq!(sel.extract_text(&emu), "world");
}

#[test]
fn click_on_first_char_of_word() {
    let emu = MockEmu::from_lines(&["  hello  "]);
    let sel = TextSelection::select_word(&emu, pos(0, 2)).unwrap();
    assert_eq!(sel.start, pos(0, 2));
    assert_eq!(sel.end, pos(0, 6));
    assert_eq!(sel.extract_text(&emu), "hello");
}

#[test]
fn click_on_last_char_of_word() {
    let emu = MockEmu::from_lines(&["  hello  "]);
    let sel = TextSelection::select_word(&emu, pos(0, 6)).unwrap();
    assert_eq!(sel.start, pos(0, 2));
    assert_eq!(sel.end, pos(0, 6));
    assert_eq!(sel.extract_text(&emu), "hello");
}

#[test]
fn entire_line_is_one_word() {
    let emu = MockEmu::from_lines(&["abcdef"]);
    let sel = TextSelection::select_word(&emu, pos(0, 3)).unwrap();
    assert_eq!(sel.start, pos(0, 0));
    assert_eq!(sel.end, pos(0, 5));
    assert_eq!(sel.extract_text(&emu), "abcdef");
}

#[test]
fn single_character_word() {
    let emu = MockEmu::from_lines(&["a b c"]);
    let sel = TextSelection::select_word(&emu, pos(0, 2)).unwrap();
    assert_eq!(sel.start, pos(0, 2));
    assert_eq!(sel.end, pos(0, 2));
    assert_eq!(sel.extract_text(&emu), "b");
}

#[test]
fn word_on_second_row() {
    let emu = MockEmu::from_lines(&["first", "second line"]);
    let sel = TextSelection::select_word(&emu, pos(1, 8)).unwrap();
    assert_eq!(sel.start, pos(1, 7));
    assert_eq!(sel.end, pos(1, 10));
    assert_eq!(sel.extract_text(&emu), "line");
}

// ── select_word: returns None ───────────────────────────────────

#[test]
fn click_on_whitespace_returns_none() {
    let emu = MockEmu::from_lines(&["hello world"]);
    assert!(TextSelection::select_word(&emu, pos(0, 5)).is_none());
}

#[test]
fn click_out_of_bounds_returns_none() {
    let emu = MockEmu::from_lines(&["hello"]);
    assert!(TextSelection::select_word(&emu, pos(0, 99)).is_none());
    assert!(TextSelection::select_word(&emu, pos(99, 0)).is_none());
}

#[test]
fn empty_line_returns_none() {
    let emu = MockEmu::from_lines(&["   "]);
    assert!(TextSelection::select_word(&emu, pos(0, 1)).is_none());
}

// ── select_word: special characters ─────────────────────────────

#[test]
fn punctuation_is_part_of_word() {
    let emu = MockEmu::from_lines(&["hello, world!"]);
    let sel = TextSelection::select_word(&emu, pos(0, 3)).unwrap();
    assert_eq!(sel.extract_text(&emu), "hello,");
    let sel = TextSelection::select_word(&emu, pos(0, 9)).unwrap();
    assert_eq!(sel.extract_text(&emu), "world!");
}

#[test]
fn multiple_spaces_between_words() {
    let emu = MockEmu::from_lines(&["foo   bar"]);
    let sel = TextSelection::select_word(&emu, pos(0, 1)).unwrap();
    assert_eq!(sel.extract_text(&emu), "foo");
    let sel = TextSelection::select_word(&emu, pos(0, 7)).unwrap();
    assert_eq!(sel.extract_text(&emu), "bar");
    assert!(TextSelection::select_word(&emu, pos(0, 4)).is_none());
}

// ── select_word: selection properties ───────────────────────────

#[test]
fn selection_is_not_active() {
    let emu = MockEmu::from_lines(&["word"]);
    let sel = TextSelection::select_word(&emu, pos(0, 0)).unwrap();
    assert!(!sel.active, "word selection should not be marked active");
}

#[test]
fn selection_stays_on_same_row() {
    let emu = MockEmu::from_lines(&["aaa", "bbb"]);
    let sel = TextSelection::select_word(&emu, pos(0, 1)).unwrap();
    assert_eq!(sel.start.row, 0);
    assert_eq!(sel.end.row, 0);
    assert_eq!(sel.extract_text(&emu), "aaa");
}
