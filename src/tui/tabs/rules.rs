use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use crate::tui::app::{App, ClickAction, ClickableArea, InputId};
use crate::tui::{theme, widgets};

pub fn render(f: &mut Frame, area: Rect, app: &App, areas: &mut Vec<ClickableArea>) {
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(" 📋 监控规则管理 ")
        .border_style(Style::default().fg(theme::BORDER_FOCUSED));

    let inner = main_block.inner(area);
    f.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Action bar
            Constraint::Min(5),    // Table list
            Constraint::Length(if app.show_add_rule_form { 10 } else { 0 }), // Add rule form
            Constraint::Length(1), // Footer info
        ])
        .split(inner);

    // 1. Top Action bar
    let action_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(14)])
        .split(chunks[0]);

    let title_para = Paragraph::new(Span::styled("现有监控规则列表 (点击开关/按钮直接操作):", Style::default().fg(Color::Gray)));
    f.render_widget(title_para, action_chunks[0]);

    if app.show_add_rule_form {
        widgets::render_button(
            f,
            action_chunks[1],
            " [✖ 关闭] ",
            Style::default().fg(Color::Yellow),
            areas,
            ClickAction::HideAddRuleForm,
        );
    } else {
        widgets::render_button(
            f,
            action_chunks[1],
            " [➕ 添加规则] ",
            Style::default().fg(Color::Green),
            areas,
            ClickAction::ShowAddRuleForm,
        );
    }

    // 2. Rules List / Table
    if app.cached_rules.is_empty() {
        let empty_para = Paragraph::new("\n 当前暂未配置任何监控规则。点击右上角 [➕ 添加规则] 创建。")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty_para, chunks[1]);
    } else {
        let mut row_constraints = vec![Constraint::Length(1)]; // Header
        for r in &app.cached_rules {
            if app.confirm_delete_rule == Some(r.id) {
                row_constraints.push(Constraint::Length(2));
            } else {
                row_constraints.push(Constraint::Length(1));
            }
        }
        row_constraints.push(Constraint::Min(0));

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(chunks[1]);

        // Header Row
        let header_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(5),  // ID
                Constraint::Min(25),   // Path
                Constraint::Length(20), // Target
                Constraint::Length(10), // Listen Toggle
                Constraint::Length(8),  // Delete
            ])
            .split(rows[0]);

        f.render_widget(Paragraph::new(Span::styled("ID", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))), header_cols[0]);
        f.render_widget(Paragraph::new(Span::styled("监控目录", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))), header_cols[1]);
        f.render_widget(Paragraph::new(Span::styled("目标频道/群组", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))), header_cols[2]);
        f.render_widget(Paragraph::new(Span::styled("媒体监听", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))), header_cols[3]);
        f.render_widget(Paragraph::new(Span::styled("删除", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))), header_cols[4]);

        // Content Rows
        for (idx, r) in app.cached_rules.iter().enumerate() {
            if idx + 1 >= rows.len() - 1 {
                break;
            }
            let row_area = rows[idx + 1];

            if app.confirm_delete_rule == Some(r.id) {
                let sub_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Length(1)])
                    .split(row_area);

                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(5),
                        Constraint::Min(25),
                        Constraint::Length(20),
                        Constraint::Length(10),
                        Constraint::Length(8),
                    ])
                    .split(sub_chunks[0]);

                f.render_widget(Paragraph::new(format!("{}", r.id)), cols[0]);
                f.render_widget(Paragraph::new(r.path.as_str()), cols[1]);
                f.render_widget(Paragraph::new(r.target.as_str()), cols[2]);
                widgets::render_toggle(f, cols[3], r.listen_media, areas, ClickAction::ToggleListenMedia(r.id));

                // Confirm prompt line
                let confirm_cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Min(20),
                        Constraint::Length(10),
                        Constraint::Length(10),
                    ])
                    .split(sub_chunks[1]);

                f.render_widget(
                    Paragraph::new(Span::styled("  ⚠️ 确定要删除此规则吗？", Style::default().fg(Color::Red))),
                    confirm_cols[0],
                );
                widgets::render_button(
                    f,
                    confirm_cols[1],
                    " [确认] ",
                    Style::default().fg(Color::Red),
                    areas,
                    ClickAction::ConfirmDeleteRule(r.id),
                );
                widgets::render_button(
                    f,
                    confirm_cols[2],
                    " [取消] ",
                    Style::default().fg(Color::Gray),
                    areas,
                    ClickAction::CancelDeleteRule,
                );
            } else {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(5),
                        Constraint::Min(25),
                        Constraint::Length(20),
                        Constraint::Length(10),
                        Constraint::Length(8),
                    ])
                    .split(row_area);

                f.render_widget(Paragraph::new(format!("{}", r.id)), cols[0]);
                f.render_widget(Paragraph::new(r.path.as_str()), cols[1]);
                f.render_widget(Paragraph::new(r.target.as_str()), cols[2]);

                widgets::render_toggle(f, cols[3], r.listen_media, areas, ClickAction::ToggleListenMedia(r.id));
                widgets::render_button(
                    f,
                    cols[4],
                    " [🗑] ",
                    Style::default().fg(Color::Red),
                    areas,
                    ClickAction::DeleteRule(r.id),
                );
            }
        }
    }

    // 3. Add Rule Form (If visible)
    if app.show_add_rule_form {
        let form_block = Block::default()
            .borders(Borders::ALL)
            .title(" ➕ 添加新监控规则 ")
            .border_style(Style::default().fg(Color::Green));

        let form_inner = form_block.inner(chunks[2]);
        f.render_widget(form_block, chunks[2]);

        let form_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Length(2)])
            .split(form_inner);

        // Path row
        let path_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(12), Constraint::Min(20)])
            .split(form_rows[0]);

        f.render_widget(Paragraph::new("监控目录路径:"), path_cols[0]);
        widgets::render_text_input(
            f,
            path_cols[1],
            &app.add_rule_path,
            app.is_input_focused(InputId::AddRulePath),
            "例如: D:\\Videos 或 /home/user/videos",
            areas,
            ClickAction::FocusTextInput(InputId::AddRulePath),
        );

        // Target row
        let target_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(12), Constraint::Min(20)])
            .split(form_rows[1]);

        f.render_widget(Paragraph::new("投递目标频道:"), target_cols[0]);
        widgets::render_text_input(
            f,
            target_cols[1],
            &app.add_rule_target,
            app.is_input_focused(InputId::AddRuleTarget),
            "例如: me (默认收藏夹) 或 @mychannel 或 -100123456",
            areas,
            ClickAction::FocusTextInput(InputId::AddRuleTarget),
        );

        // Buttons row
        let btn_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(10), Constraint::Length(12), Constraint::Length(12)])
            .split(form_rows[2]);

        widgets::render_button(
            f,
            btn_cols[1],
            " [✔ 提交] ",
            Style::default().fg(Color::Green),
            areas,
            ClickAction::SubmitAddRule,
        );
        widgets::render_button(
            f,
            btn_cols[2],
            " [✖ 取消] ",
            Style::default().fg(Color::Gray),
            areas,
            ClickAction::HideAddRuleForm,
        );
    }

    // 4. Footer info
    let global_del = app.cached_config.auto_delete.unwrap_or(false);
    let footer_line = Line::from(vec![
        Span::raw("⚙️ 全局默认 auto_delete: "),
        Span::styled(
            if global_del { "✅ 自动删除本地文件" } else { "❌ 不删除 (重命名为 .uploaded)" },
            Style::default().fg(if global_del { Color::Yellow } else { Color::Gray }),
        ),
    ]);
    f.render_widget(Paragraph::new(footer_line), chunks[3]);
}
