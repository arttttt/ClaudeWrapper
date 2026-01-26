use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn layout_regions(area: Rect) -> (Rect, Rect, Rect) {
    let header_height = area.height.min(3);
    let footer_height = 3.min(area.height.saturating_sub(header_height));
    let header = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: header_height,
    };
    let footer = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(footer_height),
        width: area.width,
        height: footer_height,
    };
    let body = Rect {
        x: area.x,
        y: area.y + header_height,
        width: area.width,
        height: area.height.saturating_sub(header_height + footer_height),
    };
    (header, body, footer)
}

pub fn body_rect(area: Rect) -> Rect {
    layout_regions(area).1
}

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
