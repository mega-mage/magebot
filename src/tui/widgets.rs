use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use super::app::{ClickAction, ClickableArea, SettingsField, TextInputState};
use super::theme;

pub fn render_button(
    f: &mut Frame,
    area: Rect,
    label: &str,
    style: Style,
    areas: &mut Vec<ClickableArea>,
    action: ClickAction,
) {
    if area.height >= 3 {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(style.fg.unwrap_or(theme::BUTTON_FG)));

        let para = Paragraph::new(Line::from(Span::styled(
            label,
            style.add_modifier(Modifier::BOLD),
        )))
        .alignment(ratatui::layout::Alignment::Center)
        .block(block);

        f.render_widget(para, area);
    } else {
        let text = label.trim();
        let display_text = if text.starts_with('[') && text.ends_with(']') {
            text.to_string()
        } else {
            format!("[ {} ]", text)
        };
        let para = Paragraph::new(Span::styled(
            display_text,
            style.add_modifier(Modifier::BOLD),
        ))
        .alignment(ratatui::layout::Alignment::Center);

        f.render_widget(para, area);
    }
    areas.push(ClickableArea { rect: area, action });
}

pub fn render_toggle(
    f: &mut Frame,
    area: Rect,
    enabled: bool,
    areas: &mut Vec<ClickableArea>,
    action: ClickAction,
) {
    let (text, color) = if enabled {
        (" [🔵 ON] ", theme::TOGGLE_ON)
    } else {
        (" [⚫ OFF] ", theme::TOGGLE_OFF)
    };

    let para = Paragraph::new(Span::styled(text, Style::default().fg(color).add_modifier(Modifier::BOLD)));
    f.render_widget(para, area);
    areas.push(ClickableArea { rect: area, action });
}

pub fn render_text_input(
    f: &mut Frame,
    area: Rect,
    state: &TextInputState,
    focused: bool,
    placeholder: &str,
    areas: &mut Vec<ClickableArea>,
    focus_action: ClickAction,
) {
    let border_color = if focused {
        theme::BORDER_FOCUSED
    } else {
        theme::BORDER_NORMAL
    };

    if area.height >= 3 {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if state.text.is_empty() && !focused {
            let para = Paragraph::new(Span::styled(placeholder, Style::default().fg(Color::DarkGray)));
            f.render_widget(para, inner);
        } else {
            let para = Paragraph::new(Span::styled(&state.text, Style::default().fg(Color::White)));
            f.render_widget(para, inner);

            if focused {
                let cursor_x = inner.x + state.cursor as u16;
                let cursor_y = inner.y;
                if cursor_x < inner.x + inner.width {
                    f.set_cursor(cursor_x, cursor_y);
                }
            }
        }
    } else {
        if state.text.is_empty() && !focused {
            let para = Paragraph::new(Span::styled(placeholder, Style::default().fg(Color::DarkGray)));
            f.render_widget(para, area);
        } else {
            let para = Paragraph::new(Span::styled(&state.text, Style::default().fg(Color::White)));
            f.render_widget(para, area);

            if focused {
                let cursor_x = area.x + state.cursor as u16;
                let cursor_y = area.y;
                if cursor_x < area.x + area.width {
                    f.set_cursor(cursor_x, cursor_y);
                }
            }
        }
    }

    areas.push(ClickableArea {
        rect: area,
        action: focus_action,
    });
}

pub fn render_number_stepper(
    f: &mut Frame,
    area: Rect,
    value: usize,
    areas: &mut Vec<ClickableArea>,
    field: SettingsField,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(area);

    // Left Arrow
    let left_para = Paragraph::new(Span::styled(" [◀]", Style::default().fg(Color::Cyan)));
    f.render_widget(left_para, chunks[0]);
    areas.push(ClickableArea {
        rect: chunks[0],
        action: ClickAction::StepNumber(field, -1),
    });

    // Value
    let val_para = Paragraph::new(Span::styled(
        format!(" {} ", value),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ));
    f.render_widget(val_para, chunks[1]);

    // Right Arrow
    let right_para = Paragraph::new(Span::styled("[▶] ", Style::default().fg(Color::Cyan)));
    f.render_widget(right_para, chunks[2]);
    areas.push(ClickableArea {
        rect: chunks[2],
        action: ClickAction::StepNumber(field, 1),
    });
}
