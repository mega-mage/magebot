use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};
use crate::tui::app::{App, ClickAction, ClickableArea};
use crate::tui::{theme, widgets};

pub fn render(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(" 📜 守护进程实时日志 (Daemon Logs) ")
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));

    let inner = main_block.inner(area);
    f.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Top control bar
            Constraint::Min(5),    // Log content
        ])
        .split(inner);

    // Top control bar
    let ctrl_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(18),
            Constraint::Length(12),
        ])
        .split(chunks[0]);

    let scroll_status = if app.log_auto_scroll {
        "● 自动滚动 (最新日志)"
    } else {
        "▲ 已暂停滚动 (使用滚轮上下浏览)"
    };
    f.render_widget(
        ratatui::widgets::Paragraph::new(Span::styled(
            scroll_status,
            Style::default().fg(if app.log_auto_scroll { Color::Green } else { Color::Yellow }),
        )),
        ctrl_chunks[0],
    );

    let auto_scroll_label = if app.log_auto_scroll {
        " [🔽 自动滚动: ON] "
    } else {
        " [🔽 自动滚动: OFF] "
    };
    widgets::render_button(
        f,
        ctrl_chunks[1],
        auto_scroll_label,
        Style::default().fg(if app.log_auto_scroll { Color::Cyan } else { Color::Gray }),
        areas,
        ClickAction::ToggleAutoScroll,
    );

    widgets::render_button(
        f,
        ctrl_chunks[2],
        " [🗑 清空] ",
        Style::default().fg(Color::Red),
        areas,
        ClickAction::ClearLogs,
    );

    // Log lines rendering
    let visible_lines = chunks[1].height as usize;

    let items_to_show = if app.log_auto_scroll || app.logs.len() <= visible_lines {
        let start = app.logs.len().saturating_sub(visible_lines);
        &app.logs[start..]
    } else {
        let end = app.logs.len().saturating_sub(app.log_scroll_offset);
        let start = end.saturating_sub(visible_lines);
        let end_clamped = end.min(app.logs.len());
        let start_clamped = start.min(end_clamped);
        &app.logs[start_clamped..end_clamped]
    };

    let log_items: Vec<ListItem> = items_to_show
        .iter()
        .map(|entry| {
            let level_color = match entry.level.as_str() {
                "ERROR" => theme::LOG_ERROR,
                "WARN" => theme::LOG_WARN,
                "INFO" => theme::LOG_INFO,
                "TUI" => Color::Cyan,
                _ => Color::Gray,
            };

            let line = Line::from(vec![
                Span::styled(format!("[{}] ", entry.timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{}] ", entry.level), Style::default().fg(level_color)),
                Span::styled(&entry.message, Style::default().fg(Color::White)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list_widget = List::new(log_items);
    f.render_widget(list_widget, chunks[1]);
}
