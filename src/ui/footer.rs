use crate::ui::theme::{GLOBAL_BORDER, HEADER_TEXT};
use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct Footer {
    hint: Line<'static>,
}

impl Footer {
    pub fn new() -> Self {
        let hint = Line::from("Ctrl+B: Switch │ Ctrl+S: Status │ Ctrl+Q: Quit");
        Self { hint }
    }

    pub fn widget(&self) -> Paragraph<'_> {
        Paragraph::new(self.hint.clone())
            .style(Style::default().fg(HEADER_TEXT).add_modifier(Modifier::DIM))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GLOBAL_BORDER)),
            )
            .alignment(Alignment::Center)
    }
}
