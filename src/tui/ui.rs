use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use super::app::*;
use super::tabs;
use super::theme;
use super::widgets;

pub fn draw(f: &mut Frame, app: &mut App) {
    let mut areas = Vec::new();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: Top Status Bar
            Constraint::Length(1), // 1: Tab Navigation Bar
            Constraint::Min(5),    // 2: Main Content Area
            Constraint::Length(1), // 3: Bottom Action Toolbar
        ])
        .split(f.size());

    draw_status_bar(f, chunks[0], app);
    draw_tab_bar(f, chunks[1], app, &mut areas);

    // Draw active tab content
    match app.active_tab {
        Tab::Dashboard => tabs::dashboard::render(f, chunks[2], app, &mut areas),
        Tab::Rules => tabs::rules::render(f, chunks[2], app, &mut areas),
        Tab::Settings => tabs::settings::render(f, chunks[2], app, &mut areas),
        Tab::Logs => tabs::logs::render(f, chunks[2], app, &mut areas),
        Tab::Download => tabs::download::render(f, chunks[2], app, &mut areas),
    }

    draw_bottom_bar(f, chunks[3], app, &mut areas);
    draw_notification(f, f.size(), app);

    app.clickable_areas = areas;
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let (status_text, status_color) = match app.connection_status {
        ConnectionStatus::Connected => {
            let pid_info = if let Some(pid) = app.daemon_pid {
                format!("Connected (PID: {})", pid)
            } else {
                "Connected".to_string()
            };
            (format!("● {}", pid_info), theme::CONNECTED)
        }
        ConnectionStatus::Connecting => ("● Connecting...".to_string(), theme::CONNECTING),
        ConnectionStatus::Disconnected => ("● Disconnected".to_string(), theme::DISCONNECTED),
    };

    let status_bar_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(30)])
        .split(area);

    let title_line = Line::from(vec![
        Span::styled(" 🤖 MageBot ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("v0.1.0", Style::default().fg(Color::DarkGray)),
        Span::styled(" | 纯鼠标交互图形控制台", Style::default().fg(Color::Gray)),
    ]);

    let title_para = Paragraph::new(title_line).style(Style::default().bg(theme::STATUS_BAR_BG));
    f.render_widget(title_para, status_bar_layout[0]);

    let status_para = Paragraph::new(Span::styled(
        status_text,
        Style::default().fg(status_color).add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Right)
    .style(Style::default().bg(theme::STATUS_BAR_BG));

    f.render_widget(status_para, status_bar_layout[1]);
}

fn draw_tab_bar(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let tab_items = [
        (Tab::Dashboard, " 📊 概览 "),
        (Tab::Rules, " 📋 监控规则 "),
        (Tab::Settings, " ⚙️ 参数设置 "),
        (Tab::Logs, " 📜 实时日志 "),
        (Tab::Download, " 🔽 视频下载 "),
    ];

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    for (idx, (tab, label)) in tab_items.iter().enumerate() {
        let is_active = app.active_tab == *tab;
        let style = if is_active {
            Style::default()
                .fg(theme::TAB_ACTIVE_FG)
                .bg(theme::TAB_ACTIVE_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TAB_INACTIVE_FG)
        };

        let para = Paragraph::new(Span::styled(*label, style)).alignment(Alignment::Center);
        f.render_widget(para, chunks[idx]);

        areas.push(ClickableArea {
            rect: chunks[idx],
            action: ClickAction::SwitchTab(*tab),
        });
    }
}

fn draw_bottom_bar(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Start
            Constraint::Length(12), // Stop
            Constraint::Length(12), // Restart
            Constraint::Length(12), // Check
            Constraint::Min(10),    // Filler / Esc prompt
        ])
        .split(area);

    // Start button
    let is_connected = app.connection_status == ConnectionStatus::Connected;
    let start_style = if is_connected {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Green)
    };
    let start_action = if is_connected {
        ClickAction::Noop
    } else {
        ClickAction::StartDaemon
    };
    widgets::render_button(f, chunks[0], " [▶ 启动服务] ", start_style, areas, start_action);

    // Stop button
    widgets::render_button(
        f,
        chunks[1],
        " [■ 停止服务] ",
        Style::default().fg(Color::Red),
        areas,
        ClickAction::StopDaemon,
    );

    // Restart button
    widgets::render_button(
        f,
        chunks[2],
        " [↻ 重启服务] ",
        Style::default().fg(Color::Yellow),
        areas,
        ClickAction::RestartDaemon,
    );

    // Check button
    widgets::render_button(
        f,
        chunks[3],
        " [🔍 诊断检查] ",
        Style::default().fg(Color::Cyan),
        areas,
        ClickAction::RunCheck,
    );

    // Right prompt
    let right_para = Paragraph::new(Span::styled(" 按 Esc 键退出 TUI 面板 ", Style::default().fg(Color::DarkGray)))
        .alignment(Alignment::Right);
    f.render_widget(right_para, chunks[4]);
}

fn draw_notification(f: &mut Frame, area: Rect, app: &App) {
    if let Some((msg, level, _)) = &app.notification {
        let pop_width = 44u16.min(area.width);
        let pop_height = 3u16;

        let pop_x = area.width.saturating_sub(pop_width + 2);
        let pop_y = area.height.saturating_sub(pop_height + 2);

        let pop_area = Rect::new(pop_x, pop_y, pop_width, pop_height);

        let border_color = match level {
            NotifyLevel::Success => theme::NOTIFY_SUCCESS,
            NotifyLevel::Warning => theme::NOTIFY_WARNING,
            NotifyLevel::Error => theme::NOTIFY_ERROR,
            NotifyLevel::Info => theme::NOTIFY_INFO,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" 通知提示 ")
            .border_style(Style::default().fg(border_color));

        let para = Paragraph::new(Span::styled(msg, Style::default().fg(Color::White))).block(block);

        f.render_widget(Clear, pop_area);
        f.render_widget(para, pop_area);
    }
}
