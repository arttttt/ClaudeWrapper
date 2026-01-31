use crate::ui::app::{App, PopupKind};
use crate::ui::footer::Footer;
use crate::ui::header::Header;
use crate::ui::layout::{centered_rect_by_size, layout_regions};
use crate::ui::terminal::TerminalBody;
use crate::ui::theme::{
    ACTIVE_HIGHLIGHT, CLAUDE_ORANGE, HEADER_TEXT, POPUP_BORDER, STATUS_ERROR, STATUS_OK,
};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::sync::Arc;
use termwiz::surface::CursorVisibility;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.size();
    let (header, body, footer) = layout_regions(area);

    let header_widget = Header::new();
    frame.render_widget(header_widget.widget(app.proxy_status()), header);
    frame.render_widget(Clear, body);
    if let Some(screen) = app.screen() {
        frame.render_widget(TerminalBody::new(Arc::clone(&screen)), body);
        if app.focus_is_terminal() && body.width > 0 && body.height > 0 {
            if let Ok(screen) = screen.lock() {
                if screen.cursor_visibility() == CursorVisibility::Visible {
                    let (x, y) = screen.cursor_position();
                    let x = body.x + x.min(body.width.saturating_sub(1) as usize) as u16;
                    let y = body.y + y.min(body.height.saturating_sub(1) as usize) as u16;
                    frame.set_cursor(x, y);
                }
            }
        }
    }
    let footer_widget = Footer::new();
    frame.render_widget(footer_widget.widget(), footer);

    if let Some(kind) = app.popup_kind() {
        let (title, lines) = match kind {
            PopupKind::Status => {
                let mut lines = Vec::new();
                if let Some(status) = app.proxy_status() {
                    lines.push(Line::from(format!(
                        "Active backend: {}",
                        status.active_backend
                    )));
                    lines.push(Line::from(format!("Uptime: {}s", status.uptime_seconds)));
                    lines.push(Line::from(format!(
                        "Total requests: {}",
                        status.total_requests
                    )));
                    lines.push(Line::from(format!("Healthy: {}", status.healthy)));
                } else {
                    lines.push(Line::from("Status pending..."));
                }

                if let Some(metrics) = app.metrics() {
                    if let Some(active) = app.proxy_status().map(|s| s.active_backend.as_str()) {
                        if let Some(backend) = metrics.per_backend.get(active) {
                            lines.push(Line::from(""));
                            lines.push(Line::from(format!(
                                "Latency p50/p95/p99: {:?}/{:?}/{:?} ms",
                                backend.p50_latency_ms,
                                backend.p95_latency_ms,
                                backend.p99_latency_ms
                            )));
                            lines.push(Line::from(format!(
                                "Avg latency: {:.1} ms",
                                backend.avg_latency_ms
                            )));
                            lines.push(Line::from(format!(
                                "Avg TTFB: {:.1} ms",
                                backend.avg_ttfb_ms
                            )));
                            lines.push(Line::from(format!("Timeouts: {}", backend.timeouts)));
                        }
                    }
                }

                if let Some(error) = app.last_ipc_error() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("IPC error: {error}")));
                }

                ("Status", lines)
            }
            PopupKind::BackendSwitch => {
                let mut lines = Vec::new();
                if app.backends().is_empty() {
                    lines.push(Line::from("No backends available."));
                } else {
                    let selected_index = app.backend_selection();
                    let max_name_width = app
                        .backends()
                        .iter()
                        .map(|backend| backend.display_name.chars().count())
                        .max()
                        .unwrap_or(0);
                    let mut entry_lines: Vec<Vec<Line>> = Vec::new();

                    for (idx, backend) in app.backends().iter().enumerate() {
                        let (status_text, status_color) = if backend.is_active {
                            ("🟢 Active", STATUS_OK)
                        } else if backend.is_configured {
                            ("🟡 Ready", STATUS_OK)
                        } else {
                            ("🔴 Missing", STATUS_ERROR)
                        };
                        let model_hint = backend.model_hint.as_deref().unwrap_or("unknown");
                        let highlight = backend.is_active || idx == selected_index;

                        let mut name_spans = Vec::new();
                        name_spans.push(Span::styled(
                            format!("{:>2}. ", idx + 1),
                            Style::default().fg(HEADER_TEXT),
                        ));
                        name_spans.push(Span::styled(
                            format!("{:<width$}", backend.display_name, width = max_name_width),
                            Style::default().fg(HEADER_TEXT),
                        ));
                        name_spans.push(Span::raw("  ["));
                        name_spans
                            .push(Span::styled(status_text, Style::default().fg(status_color)));
                        name_spans.push(Span::raw("]"));

                        let mut name_line = Line::from(name_spans);
                        let mut model_line = Line::from(vec![
                            Span::raw("    Model: "),
                            Span::styled(model_hint, Style::default().fg(HEADER_TEXT)),
                        ]);

                        if highlight {
                            let highlight_style = Style::default().bg(ACTIVE_HIGHLIGHT);
                            name_line = name_line.style(highlight_style);
                            model_line = model_line.style(highlight_style);
                        }

                        entry_lines.push(vec![name_line, model_line]);
                    }

                    let mut max_line_width = entry_lines
                        .iter()
                        .flat_map(|entry| entry.iter())
                        .map(Line::width)
                        .max()
                        .unwrap_or(0);

                    if max_line_width == 0 {
                        max_line_width = 1;
                    }
                    let separator = "-".repeat(max_line_width);

                    for (idx, entry) in entry_lines.into_iter().enumerate() {
                        lines.extend(entry);
                        if idx + 1 < app.backends().len() {
                            lines.push(Line::from(separator.clone()));
                            lines.push(Line::from(""));
                        }
                    }

                    lines.push(Line::from(""));
                    lines.push(Line::from(
                        "Up/Down: Move  Enter: Select  Esc/Ctrl+B: Close",
                    ));
                }

                if let Some(error) = app.last_ipc_error() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("IPC error: {error}")));
                }

                ("Select Backend", lines)
            }
        };

        let content_width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
        let popup_min_width = match kind {
            PopupKind::BackendSwitch => 56,
            _ => 0,
        };
        let popup_width = content_width.saturating_add(4).max(popup_min_width);
        let popup_height = lines.len().saturating_add(2) as u16;
        let area = centered_rect_by_size(body, popup_width, popup_height);

        frame.render_widget(Clear, area);
        let popup = Block::default()
            .title(Span::styled(title, Style::default().fg(CLAUDE_ORANGE)))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(POPUP_BORDER));
        let widget = Paragraph::new(lines).block(popup);
        frame.render_widget(widget, area);
    }
}
