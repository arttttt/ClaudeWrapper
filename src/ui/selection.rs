use crate::pty::emulator::TerminalEmulator;

/// Grid position (row, col) in emulator coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPos {
    pub row: u16,
    pub col: u16,
}

/// Active text selection state.
#[derive(Clone, Debug)]
pub struct TextSelection {
    /// Starting position (where mouse was pressed).
    pub start: GridPos,
    /// Current end position (where mouse is now).
    pub end: GridPos,
    /// True while the user is dragging.
    pub active: bool,
}

impl TextSelection {
    pub fn new(start: GridPos) -> Self {
        Self {
            start,
            end: start,
            active: true,
        }
    }

    /// Get ordered (first, last) positions for iteration.
    pub fn ordered(&self) -> (GridPos, GridPos) {
        if self.start.row < self.end.row
            || (self.start.row == self.end.row && self.start.col <= self.end.col)
        {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    /// Check if a given cell is within the selection.
    pub fn contains(&self, row: u16, col: u16) -> bool {
        let (first, last) = self.ordered();
        if row < first.row || row > last.row {
            return false;
        }
        if first.row == last.row {
            return col >= first.col && col <= last.col;
        }
        if row == first.row {
            return col >= first.col;
        }
        if row == last.row {
            return col <= last.col;
        }
        true // middle rows are fully selected
    }

    /// Create a selection spanning the word at `pos`.
    ///
    /// A "word" is a contiguous run of non-whitespace characters on the same row.
    /// Returns `None` if the cell at `pos` is whitespace or out of bounds.
    pub fn select_word(emu: &dyn TerminalEmulator, pos: GridPos) -> Option<Self> {
        let cell = emu.cell(pos.row, pos.col)?;
        if !cell.has_contents || cell.symbol.trim().is_empty() {
            return None;
        }

        // Scan left to find word start
        let mut left = pos.col;
        while left > 0 {
            let Some(c) = emu.cell(pos.row, left - 1) else {
                break;
            };
            if !c.has_contents || c.symbol.trim().is_empty() || c.is_wide_continuation {
                break;
            }
            left -= 1;
        }

        // Scan right to find word end
        let mut right = pos.col;
        loop {
            let Some(c) = emu.cell(pos.row, right + 1) else {
                break;
            };
            if !c.has_contents || c.symbol.trim().is_empty() {
                break;
            }
            if c.is_wide_continuation {
                right += 1;
                continue;
            }
            right += 1;
        }

        Some(Self {
            start: GridPos { row: pos.row, col: left },
            end: GridPos { row: pos.row, col: right },
            active: false,
        })
    }

    /// Extract selected text from the emulator grid.
    ///
    /// Iterates row-by-row, collects cell symbols, trims trailing whitespace
    /// per line, and joins with newlines.
    pub fn extract_text(&self, emu: &dyn TerminalEmulator) -> String {
        let (first, last) = self.ordered();
        let mut lines: Vec<String> = Vec::new();

        for row in first.row..=last.row {
            let col_start = if row == first.row { first.col } else { 0 };
            let col_end = if row == last.row { last.col } else { u16::MAX };

            let mut line = String::new();
            let mut col = col_start;
            loop {
                if col > col_end {
                    break;
                }
                let Some(cell) = emu.cell(row, col) else {
                    break;
                };
                if cell.is_wide_continuation {
                    col += 1;
                    continue;
                }
                if cell.has_contents {
                    line.push_str(&cell.symbol);
                } else {
                    line.push(' ');
                }
                col += 1;
            }

            lines.push(line.trim_end().to_string());
        }

        // Remove trailing empty lines
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }

        lines.join("\n")
    }
}