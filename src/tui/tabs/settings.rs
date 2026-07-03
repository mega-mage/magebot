use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use crate::tui::app::{App, ClickAction, ClickableArea, InputId, SettingsField};
use crate::tui::{theme, widgets};

pub fn render(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(" ⚙️ MageBot 系统参数设置 ")
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));

    let inner = main_block.inner(area);
    f.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Header / Save bar
            Constraint::Length(15), // General Settings
            Constraint::Min(5),     // Cookie Info section
        ])
        .split(inner);

    // 1. Header Save Bar
    let save_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(14)])
        .split(chunks[0]);

    if app.config_dirty {
        f.render_widget(
            Paragraph::new(Span::styled("⚠️ 参数已修改未保存", Style::default().fg(Color::Yellow))),
            save_chunks[0],
        );
        widgets::render_button(
            f,
            save_chunks[1],
            " [💾 保存修改] ",
            Style::default().fg(Color::Green),
            areas,
            ClickAction::SaveConfig,
        );
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("所有配置参数已同步到磁盘 config.toml", Style::default().fg(Color::Gray))),
            save_chunks[0],
        );
    }

    // 2. Settings list
    let settings_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // auto_delete
            Constraint::Length(3), // download_dir
            Constraint::Length(3), // yt_dlp_path
            Constraint::Length(3), // yt_dlp_args
            Constraint::Length(2), // max_concurrent_uploads
        ])
        .split(chunks[1]);

    // Row 1: auto_delete
    let r1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(settings_rows[0]);
    f.render_widget(Paragraph::new("auto_delete (自动删除):"), r1[0]);
    let is_auto_del = app.pending_config.auto_delete.unwrap_or(false);
    widgets::render_toggle(f, r1[1], is_auto_del, areas, ClickAction::ToggleAutoDelete);

    // Row 2: download_dir
    let r2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(settings_rows[1]);
    f.render_widget(Paragraph::new("download_dir (下载目录):"), r2[0]);
    widgets::render_text_input(
        f,
        r2[1],
        &app.settings_download_dir,
        app.is_input_focused(InputId::SettingsDownloadDir),
        "默认: ~/.magebot/downloads",
        areas,
        ClickAction::FocusTextInput(InputId::SettingsDownloadDir),
    );

    // Row 3: yt_dlp_path
    let r3 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(settings_rows[2]);
    f.render_widget(Paragraph::new("yt_dlp_path (程序路径):"), r3[0]);
    widgets::render_text_input(
        f,
        r3[1],
        &app.settings_yt_dlp_path,
        app.is_input_focused(InputId::SettingsYtDlpPath),
        "默认: yt-dlp",
        areas,
        ClickAction::FocusTextInput(InputId::SettingsYtDlpPath),
    );

    // Row 4: yt_dlp_args
    let r4 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(settings_rows[3]);
    f.render_widget(Paragraph::new("yt_dlp_args (额外参数):"), r4[0]);
    widgets::render_text_input(
        f,
        r4[1],
        &app.settings_yt_dlp_args,
        app.is_input_focused(InputId::SettingsYtDlpArgs),
        "例如: --cookies-from-browser chrome",
        areas,
        ClickAction::FocusTextInput(InputId::SettingsYtDlpArgs),
    );

    // Row 5: max_concurrent_uploads
    let r5 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(settings_rows[4]);
    f.render_widget(Paragraph::new("max_concurrent_uploads:"), r5[0]);
    let max_upload = app.pending_config.max_concurrent_uploads.unwrap_or(3);
    widgets::render_number_stepper(f, r5[1], max_upload, areas, SettingsField::MaxConcurrentUploads);

    // 3. Cookie management info section
    let cookie_block = Block::default()
        .borders(Borders::ALL)
        .title(" 🔐 平台 Cookie 管理说明 ")
        .border_style(Style::default().fg(Color::DarkGray));

    let cookie_text = vec![
        Line::from("Cookie 支持对 YouTube / Bilibili / Twitter 等平台的视频进行解封与高速下载。"),
        Line::from(Span::styled("提示: Cookie 涉及安全加密，请在普通终端运行命令交互式配置:", Style::default().fg(Color::Yellow))),
        Line::from(Span::styled("   magebot set cookie", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("加密保存至 ~/.magebot/cookies.toml"),
    ];

    f.render_widget(Paragraph::new(cookie_text).block(cookie_block), chunks[2]);
}
