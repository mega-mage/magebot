use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};
use crate::tui::app::{App, ClickableArea, ConnectionStatus};
use crate::tui::theme;
use crate::ipc::{TaskStatus, TaskType};

pub fn render(f: &mut Frame, area: Rect, app: &App, _areas: &mut Vec<ClickableArea>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(5)])
        .split(area);

    // System overview card
    let status_str = match app.connection_status {
        ConnectionStatus::Connected => "✅ 运行中 (已连接)",
        ConnectionStatus::Connecting => "🟡 正在建立连接...",
        ConnectionStatus::Disconnected => "❌ 未连接 (守护进程可启动)",
    };

    let pid_str = if let Some(pid) = app.daemon_pid {
        format!("PID: {}", pid)
    } else {
        "PID: 未运行".to_string()
    };

    let summary_text = vec![
        Line::from(vec![
            Span::styled(" 守护进程状态: ", Style::default().fg(Color::Gray)),
            Span::styled(status_str, Style::default().fg(if app.connection_status == ConnectionStatus::Connected { Color::Green } else { Color::Yellow })),
            Span::raw("   "),
            Span::styled(pid_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled(" 监控规则总数: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{} 条", app.cached_rules.len()), Style::default().fg(Color::White)),
            Span::styled(format!(" (其中 {} 条开启媒体监听)", app.cached_rules.iter().filter(|r| r.listen_media).count()), Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(" 当前活动任务: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{} 个", app.tasks.len()), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let summary_block = Block::default()
        .borders(Borders::ALL)
        .title(" 📊 系统状态概览 ")
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));

    f.render_widget(Paragraph::new(summary_text).block(summary_block), chunks[0]);

    // Active tasks panel
    let tasks_block = Block::default()
        .borders(Borders::ALL)
        .title(" 📥↓📤↑ 活动任务列表 ")
        .border_style(Style::default().fg(theme::BORDER_NORMAL));

    let inner_tasks = tasks_block.inner(chunks[1]);
    f.render_widget(tasks_block, chunks[1]);

    if app.tasks.is_empty() {
        let no_tasks = Paragraph::new("\n 暂无活动任务\n 可在 [🔽 下载] Tab 或后台文件变动时触发任务")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(no_tasks, inner_tasks);
        return;
    }

    let task_count = app.tasks.len();
    let mut constraints = Vec::new();
    for _ in 0..task_count {
        constraints.push(Constraint::Length(4));
    }
    constraints.push(Constraint::Min(0));

    let task_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_tasks);

    for (i, task) in app.tasks.iter().enumerate() {
        if i >= task_chunks.len() - 1 {
            break;
        }
        let chunk = task_chunks[i];

        let row_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(chunk);

        let icon = match task.task_type {
            TaskType::Download => "📥↓",
            TaskType::Upload => "📤↑",
        };
        let icon_color = match task.task_type {
            TaskType::Download => Color::LightBlue,
            TaskType::Upload => Color::LightYellow,
        };

        let name_line = Line::from(vec![
            Span::styled(format!("{} ", icon), Style::default().fg(icon_color).add_modifier(Modifier::BOLD)),
            Span::styled(truncate_str(&task.filename, 45), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]);
        f.render_widget(Paragraph::new(name_line), row_layout[0]);

        let status_text = match &task.status {
            TaskStatus::Pending => " 等待中...".to_string(),
            TaskStatus::Downloading { speed, eta, .. } => format!(" 下载中 | 速度: {} | 剩余时间: {}", speed, eta),
            TaskStatus::Uploading { speed, eta, .. } => format!(" 上传中 | 速度: {} | 剩余时间: {}", speed, eta),
            TaskStatus::Completed => " 已完成".to_string(),
            TaskStatus::Failed(e) => format!(" 失败: {}", truncate_str(e, 30)),
        };
        f.render_widget(Paragraph::new(Span::styled(status_text, Style::default().fg(Color::Gray))), row_layout[1]);

        let percent = match &task.status {
            TaskStatus::Downloading { progress, .. } => *progress,
            TaskStatus::Uploading { progress, .. } => *progress,
            TaskStatus::Completed => 100.0,
            _ => 0.0,
        };

        let gauge_color = if percent >= 100.0 {
            theme::PROGRESS_COMPLETE
        } else {
            theme::PROGRESS_BAR_FG
        };

        let gauge = Gauge::default()
            .gauge_style(Style::default().fg(gauge_color).bg(theme::PROGRESS_BAR_BG))
            .percent(percent as u16)
            .label(format!("{:.1}%", percent));

        f.render_widget(gauge, row_layout[2]);
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
