use std::cmp::max;
use std::io::Write;
use termwiz::cell::{AttributeChange, CellAttributes};
use termwiz::color::ColorAttribute;
use termwiz::escape::csi::{
    Cursor, CursorStyle, DecPrivateMode, DecPrivateModeCode, Edit, EraseInDisplay, EraseInLine,
    Mode, Sgr, TerminalMode, TerminalModeCode, CSI,
};
use termwiz::escape::{Action, ControlCode, Esc, EscCode, OperatingSystemCommand};
use termwiz::surface::{Change, CursorShape, CursorVisibility, LineAttribute, Position, Surface};

pub(crate) struct ActionTranslator {
    saved_cursor: Option<(usize, usize)>,
}

impl ActionTranslator {
    pub(crate) fn new() -> Self {
        Self { saved_cursor: None }
    }
}

pub(crate) fn apply_actions(
    screen: &mut Surface,
    translator: &mut ActionTranslator,
    actions: Vec<Action>,
) {
    for action in actions {
        let mut changes = Vec::new();
        translate_action(screen, translator, action, &mut changes);
        if !changes.is_empty() {
            screen.add_changes(changes);
        }
    }
}

fn translate_action(
    screen: &Surface,
    translator: &mut ActionTranslator,
    action: Action,
    changes: &mut Vec<Change>,
) {
    match action {
        Action::Print(ch) => changes.push(Change::Text(ch.to_string())),
        Action::PrintString(text) => changes.push(Change::Text(text)),
        Action::Control(code) => translate_control(code, changes),
        Action::CSI(csi) => translate_csi(screen, translator, csi, changes),
        Action::Esc(esc) => translate_esc(screen, translator, esc, changes),
        Action::OperatingSystemCommand(osc) => translate_osc(osc, changes),
        _ => {}
    }
}

fn translate_control(code: ControlCode, changes: &mut Vec<Change>) {
    match code {
        ControlCode::LineFeed | ControlCode::VerticalTab | ControlCode::FormFeed => {
            changes.push(Change::Text("\n".to_string()));
        }
        ControlCode::CarriageReturn => changes.push(Change::Text("\r".to_string())),
        ControlCode::Backspace => changes.push(Change::CursorPosition {
            x: Position::Relative(-1),
            y: Position::Relative(0),
        }),
        ControlCode::HorizontalTab => changes.push(Change::Text("        ".to_string())),
        _ => {}
    }
}

fn translate_csi(
    screen: &Surface,
    translator: &mut ActionTranslator,
    csi: CSI,
    changes: &mut Vec<Change>,
) {
    match csi {
        CSI::Sgr(sgr) => {
            if let Some(change) = translate_sgr(sgr) {
                changes.push(change);
            }
        }
        CSI::Cursor(cursor) => translate_cursor(screen, translator, cursor, changes),
        CSI::Edit(edit) => translate_edit(screen, edit, changes),
        CSI::Mode(mode) => translate_mode(mode, changes),
        _ => {}
    }
}

fn translate_sgr(sgr: Sgr) -> Option<Change> {
    match sgr {
        Sgr::Reset => Some(Change::AllAttributes(CellAttributes::default())),
        Sgr::Intensity(value) => Some(Change::Attribute(AttributeChange::Intensity(value))),
        Sgr::Underline(value) => Some(Change::Attribute(AttributeChange::Underline(value))),
        Sgr::Italic(value) => Some(Change::Attribute(AttributeChange::Italic(value))),
        Sgr::Blink(value) => Some(Change::Attribute(AttributeChange::Blink(value))),
        Sgr::Inverse(value) => Some(Change::Attribute(AttributeChange::Reverse(value))),
        Sgr::Invisible(value) => Some(Change::Attribute(AttributeChange::Invisible(value))),
        Sgr::StrikeThrough(value) => Some(Change::Attribute(AttributeChange::StrikeThrough(value))),
        Sgr::Foreground(color) => Some(Change::Attribute(AttributeChange::Foreground(
            ColorAttribute::from(color),
        ))),
        Sgr::Background(color) => Some(Change::Attribute(AttributeChange::Background(
            ColorAttribute::from(color),
        ))),
        Sgr::UnderlineColor(_) | Sgr::Overline(_) | Sgr::VerticalAlign(_) | Sgr::Font(_) => None,
    }
}

fn translate_cursor(
    screen: &Surface,
    translator: &mut ActionTranslator,
    cursor: Cursor,
    changes: &mut Vec<Change>,
) {
    match cursor {
        Cursor::Up(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Relative(-(count as isize)),
        }),
        Cursor::Down(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Relative(count as isize),
        }),
        Cursor::Left(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(-(count as isize)),
            y: Position::Relative(0),
        }),
        Cursor::Right(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(count as isize),
            y: Position::Relative(0),
        }),
        Cursor::Position { line, col } | Cursor::CharacterAndLinePosition { line, col } => {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(col.as_zero_based() as usize),
                y: Position::Absolute(line.as_zero_based() as usize),
            });
        }
        Cursor::CharacterAbsolute(col) | Cursor::CharacterPositionAbsolute(col) => {
            changes.push(Change::CursorPosition {
                x: Position::Absolute(col.as_zero_based() as usize),
                y: Position::Relative(0),
            });
        }
        Cursor::CharacterPositionBackward(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(-(count as isize)),
            y: Position::Relative(0),
        }),
        Cursor::CharacterPositionForward(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(count as isize),
            y: Position::Relative(0),
        }),
        Cursor::LinePositionAbsolute(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Absolute(max(1, count) as usize - 1),
        }),
        Cursor::LinePositionBackward(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Relative(-(count as isize)),
        }),
        Cursor::LinePositionForward(count) => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Relative(count as isize),
        }),
        Cursor::NextLine(count) => changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Relative(count as isize),
        }),
        Cursor::PrecedingLine(count) => changes.push(Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Relative(-(count as isize)),
        }),
        Cursor::SaveCursor => translator.saved_cursor = Some(screen.cursor_position()),
        Cursor::RestoreCursor => {
            if let Some((x, y)) = translator.saved_cursor {
                changes.push(Change::CursorPosition {
                    x: Position::Absolute(x),
                    y: Position::Absolute(y),
                });
            }
        }
        Cursor::CursorStyle(style) => {
            if let Some(shape) = cursor_shape_from_style(style) {
                changes.push(Change::CursorShape(shape));
            }
        }
        _ => {}
    }
}

fn cursor_shape_from_style(style: CursorStyle) -> Option<CursorShape> {
    match style {
        CursorStyle::Default => Some(CursorShape::Default),
        CursorStyle::BlinkingBlock => Some(CursorShape::BlinkingBlock),
        CursorStyle::SteadyBlock => Some(CursorShape::SteadyBlock),
        CursorStyle::BlinkingUnderline => Some(CursorShape::BlinkingUnderline),
        CursorStyle::SteadyUnderline => Some(CursorShape::SteadyUnderline),
        CursorStyle::BlinkingBar => Some(CursorShape::BlinkingBar),
        CursorStyle::SteadyBar => Some(CursorShape::SteadyBar),
    }
}

fn translate_edit(screen: &Surface, edit: Edit, changes: &mut Vec<Change>) {
    let (_width, height) = screen.dimensions();
    match edit {
        Edit::EraseInLine(EraseInLine::EraseToEndOfLine) => {
            changes.push(Change::ClearToEndOfLine(Default::default()));
        }
        Edit::EraseInLine(EraseInLine::EraseToStartOfLine) => {
            erase_to_start_of_line(screen, changes);
        }
        Edit::EraseInLine(EraseInLine::EraseLine) => {
            erase_entire_line(screen, changes);
        }
        Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay) => {
            changes.push(Change::ClearToEndOfScreen(Default::default()));
        }
        Edit::EraseInDisplay(EraseInDisplay::EraseDisplay) => {
            changes.push(Change::ClearScreen(Default::default()));
        }
        Edit::ScrollUp(count) => changes.push(Change::ScrollRegionUp {
            first_row: 0,
            region_size: height,
            scroll_count: count as usize,
        }),
        Edit::ScrollDown(count) => changes.push(Change::ScrollRegionDown {
            first_row: 0,
            region_size: height,
            scroll_count: count as usize,
        }),
        _ => {}
    }
}

fn erase_to_start_of_line(screen: &Surface, changes: &mut Vec<Change>) {
    let (cursor_x, cursor_y) = screen.cursor_position();
    let count = cursor_x.saturating_add(1);
    if count == 0 {
        return;
    }
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(cursor_y),
    });
    changes.push(Change::Text(" ".repeat(count)));
    changes.push(Change::CursorPosition {
        x: Position::Absolute(cursor_x),
        y: Position::Absolute(cursor_y),
    });
}

fn erase_entire_line(screen: &Surface, changes: &mut Vec<Change>) {
    let (width, _height) = screen.dimensions();
    let (cursor_x, cursor_y) = screen.cursor_position();
    if width == 0 {
        return;
    }
    changes.push(Change::CursorPosition {
        x: Position::Absolute(0),
        y: Position::Absolute(cursor_y),
    });
    changes.push(Change::Text(" ".repeat(width)));
    changes.push(Change::CursorPosition {
        x: Position::Absolute(cursor_x),
        y: Position::Absolute(cursor_y),
    });
}

fn translate_mode(mode: Mode, changes: &mut Vec<Change>) {
    match &mode {
        Mode::SetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::ShowCursor))
        | Mode::SetMode(TerminalMode::Code(TerminalModeCode::ShowCursor)) => {
            changes.push(Change::CursorVisibility(CursorVisibility::Visible));
        }
        Mode::ResetDecPrivateMode(DecPrivateMode::Code(DecPrivateModeCode::ShowCursor))
        | Mode::ResetMode(TerminalMode::Code(TerminalModeCode::ShowCursor)) => {
            changes.push(Change::CursorVisibility(CursorVisibility::Hidden));
        }
        // Forward mouse mode sequences to real terminal
        Mode::SetDecPrivateMode(dec_mode) | Mode::ResetDecPrivateMode(dec_mode) => {
            if is_mouse_mode(dec_mode) {
                forward_mode_to_terminal(&mode);
            }
        }
        _ => {}
    }
}

/// Check if a DecPrivateMode is related to mouse tracking.
fn is_mouse_mode(mode: &DecPrivateMode) -> bool {
    match mode {
        DecPrivateMode::Code(code) => matches!(
            code,
            DecPrivateModeCode::MouseTracking           // 1000 - X10 mouse
                | DecPrivateModeCode::HighlightMouseTracking // 1001
                | DecPrivateModeCode::ButtonEventMouse  // 1002
                | DecPrivateModeCode::AnyEventMouse     // 1003
                | DecPrivateModeCode::SGRMouse          // 1006 - SGR mouse
        ),
        DecPrivateMode::Unspecified(n) => {
            // Mouse-related modes: 1000-1006
            (1000..=1006).contains(n)
        }
    }
}

/// Forward a mode escape sequence to the real terminal.
fn forward_mode_to_terminal(mode: &Mode) {
    let seq = match mode {
        Mode::SetDecPrivateMode(dec) => format_dec_private_mode(dec, 'h'),
        Mode::ResetDecPrivateMode(dec) => format_dec_private_mode(dec, 'l'),
        _ => return,
    };
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Format a DEC private mode as an escape sequence.
fn format_dec_private_mode(mode: &DecPrivateMode, suffix: char) -> String {
    use num_traits::ToPrimitive;
    let code = match mode {
        DecPrivateMode::Code(c) => c.to_u16().unwrap_or(0),
        DecPrivateMode::Unspecified(n) => *n,
    };
    format!("\x1b[?{}{}", code, suffix)
}

fn translate_esc(
    screen: &Surface,
    translator: &mut ActionTranslator,
    esc: Esc,
    changes: &mut Vec<Change>,
) {
    let Esc::Code(code) = esc else {
        return;
    };

    match code {
        EscCode::Index => changes.push(Change::Text("\n".to_string())),
        EscCode::NextLine => changes.push(Change::Text("\r\n".to_string())),
        EscCode::ReverseIndex => changes.push(Change::CursorPosition {
            x: Position::Relative(0),
            y: Position::Relative(-1),
        }),
        EscCode::CursorPositionLowerLeft => {
            let (_width, height) = screen.dimensions();
            if height > 0 {
                changes.push(Change::CursorPosition {
                    x: Position::Absolute(0),
                    y: Position::Absolute(height.saturating_sub(1)),
                });
            }
        }
        EscCode::DecSaveCursorPosition => translator.saved_cursor = Some(screen.cursor_position()),
        EscCode::DecRestoreCursorPosition => {
            if let Some((x, y)) = translator.saved_cursor {
                changes.push(Change::CursorPosition {
                    x: Position::Absolute(x),
                    y: Position::Absolute(y),
                });
            }
        }
        EscCode::DecDoubleHeightTopHalfLine => {
            changes.push(Change::LineAttribute(
                LineAttribute::DoubleHeightTopHalfLine,
            ));
        }
        EscCode::DecDoubleHeightBottomHalfLine => {
            changes.push(Change::LineAttribute(
                LineAttribute::DoubleHeightBottomHalfLine,
            ));
        }
        EscCode::DecSingleWidthLine => {
            changes.push(Change::LineAttribute(LineAttribute::SingleWidthLine));
        }
        EscCode::DecDoubleWidthLine => {
            changes.push(Change::LineAttribute(LineAttribute::DoubleWidthLine));
        }
        EscCode::FullReset => {
            changes.push(Change::AllAttributes(CellAttributes::default()));
            changes.push(Change::ClearScreen(Default::default()));
        }
        _ => {}
    }
}

fn translate_osc(osc: Box<OperatingSystemCommand>, changes: &mut Vec<Change>) {
    match *osc {
        OperatingSystemCommand::SetWindowTitle(title)
        | OperatingSystemCommand::SetIconNameAndWindowTitle(title)
        | OperatingSystemCommand::SetWindowTitleSun(title)
        | OperatingSystemCommand::SetIconName(title)
        | OperatingSystemCommand::SetIconNameSun(title) => {
            changes.push(Change::Title(title));
        }
        OperatingSystemCommand::SetSelection(selection, data) => {
            forward_osc52_set_selection(selection, &data);
        }
        OperatingSystemCommand::ClearSelection(selection) => {
            forward_osc52_clear_selection(selection);
        }
        OperatingSystemCommand::QuerySelection(selection) => {
            forward_osc52_query_selection(selection);
        }
        _ => {}
    }
}

/// Forward OSC 52 SetSelection to parent terminal for clipboard write.
fn forward_osc52_set_selection(selection: termwiz::escape::osc::Selection, data: &str) {
    // Data is already base64-encoded by the subprocess
    let seq = format!("\x1b]52;{};{}\x07", selection, data);
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Forward OSC 52 ClearSelection to parent terminal.
fn forward_osc52_clear_selection(selection: termwiz::escape::osc::Selection) {
    let seq = format!("\x1b]52;{};\x07", selection);
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}

/// Forward OSC 52 QuerySelection to parent terminal for clipboard read.
fn forward_osc52_query_selection(selection: termwiz::escape::osc::Selection) {
    let seq = format!("\x1b]52;{};?\x07", selection);
    let _ = std::io::stdout().write_all(seq.as_bytes());
    let _ = std::io::stdout().flush();
}
