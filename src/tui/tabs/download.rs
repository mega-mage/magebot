use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use crate::tui::app::{App, ClickAction, ClickableArea, DownloadRecordStatus, InputId};
use crate::tui::{theme, widgets};

pub fn render(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(" 🔽 视频在线下载 ")
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));

    let inner = main_block.inner(area);
    f.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Form section
            Constraint::Length(4), // Platform support info
            Constraint::Min(5),    // Download History list
        ])
        .split(inner);

    // 1. Form Section
    let form_block = Block::default()
        .borders(Borders::ALL)
        .title(" 🔗 提交视频下载地址 ")
        .border_style(Style::default().fg(theme::BORDER_NORMAL));

    let form_inner = form_block.inner(chunks[0]);
    f.render_widget(form_block, chunks[0]);

    let form_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(form_inner);

    // Row 1: URL input + Paste button
    let url_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(20),
            Constraint::Length(12),
        ])
        .split(form_rows[0]);

    f.render_widget(Paragraph::new("URL:"), url_cols[0]);

    widgets::render_text_input(
        f,
        url_cols[1],
        &app.download_url,
        app.is_input_focused(InputId::DownloadUrl),
        "在此粘贴 YouTube / Bilibili / Twitter / Twitch 视频链接...",
        areas,
        ClickAction::FocusTextInput(InputId::DownloadUrl),
    );

    widgets::render_button(
        f,
        url_cols[2],
        " [📋 粘贴] ",
        Style::default().fg(Color::Cyan),
        areas,
        ClickAction::PasteUrl,
    );

    // Row 2: Submit download button
    let btn_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(20),
            Constraint::Min(10),
        ])
        .split(form_rows[1]);

    widgets::render_button(
        f,
        btn_cols[1],
        " [⬇ 开始异步下载] ",
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        areas,
        ClickAction::StartDownload,
    );

    // 2. Supported platform info
    let info_block = Block::default()
        .borders(Borders::ALL)
        .title(" 💡 支持平台 ")
        .border_style(Style::default().fg(Color::DarkGray));

    let info_text = vec![
        Line::from(vec![
            Span::styled("支持平台: ", Style::default().fg(Color::Gray)),
            Span::styled("YouTube · Bilibili · Twitter/X · Twitch · TikTok · 包含所有 yt-dlp 支持的网站", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("下载文件将自动保存至 ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.cached_config.get_download_dir().to_string_lossy().to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("，并根据规则自动同步上传至 Telegram", Style::default().fg(Color::Gray)),
        ]),
    ];

    f.render_widget(Paragraph::new(info_text).block(info_block), chunks[1]);

    // 3. History list
    let history_block = Block::default()
        .borders(Borders::ALL)
        .title(" 📜 本次会话下载发起记录 ")
        .border_style(Style::default().fg(Color::DarkGray));

    let history_inner = history_block.inner(chunks[2]);
    f.render_widget(history_block, chunks[2]);

    if app.download_history.is_empty() {
        let empty_history = Paragraph::new("\n 暂无下载发起记录。在上方粘贴链接并点击 [⬇ 开始异步下载] 试试吧！")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty_history, history_inner);
    } else {
        let mut history_lines = Vec::new();
        for item in &app.download_history {
            let (icon, status_str, color) = match &item.status {
                DownloadRecordStatus::Success => ("✅", "已完成", Color::Green),
                DownloadRecordStatus::InProgress => ("⏳", "处理中...", Color::Yellow),
                DownloadRecordStatus::Failed(e) => ("❌", e.as_str(), Color::Red),
            };

            let line = Line::from(vec![
                Span::styled(format!("[{}] ", item.timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(format!("{:<10} ", status_str), Style::default().fg(color)),
                Span::styled(&item.url, Style::default().fg(Color::White)),
            ]);
            history_lines.push(line);
        }

        f.render_widget(Paragraph::new(history_lines), history_inner);
    }
}
