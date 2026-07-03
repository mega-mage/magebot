use std::io;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};
use crate::ipc::{IpcMessage, TaskState, TaskStatus, TaskType};

struct App {
    tasks: Vec<TaskState>,
    logs: Vec<String>,
    input: String,
    cursor_position: usize,
    error_msg: Option<String>,
}

impl App {
    fn new() -> App {
        App {
            tasks: Vec::new(),
            logs: Vec::new(),
            input: String::new(),
            cursor_position: 0,
            error_msg: None,
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        self.input.insert(self.cursor_position, new_char);
        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_first_char = self.cursor_position != 0;
        if is_not_first_char {
            let idx = self.cursor_position - 1;
            self.input.remove(idx);
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_pos: usize) -> usize {
        new_pos.clamp(0, self.input.len())
    }

    fn reset_input(&mut self) {
        self.input.clear();
        self.cursor_position = 0;
    }
}

async fn try_connect(tx_tui: tokio::sync::mpsc::UnboundedSender<TuiEvent>) -> Option<tokio::net::tcp::OwnedWriteHalf> {
    if let Ok(stream) = TcpStream::connect("127.0.0.1:42424").await {
        let (reader, writer) = stream.into_split();
        let tx = tx_tui.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(reader).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(msg) = serde_json::from_str::<IpcMessage>(&line) {
                    let _ = tx.send(TuiEvent::Ipc(msg));
                }
            }
            let _ = tx.send(TuiEvent::Disconnected);
        });
        Some(writer)
    } else {
        None
    }
}

pub async fn run_monitor() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Create channels for TUI thread communication
    let (tx_tui, mut rx_tui) = tokio::sync::mpsc::unbounded_channel::<TuiEvent>();

    // Spawn crossterm event reader task on a blocking thread pool to avoid starving the single-core executor
    let tx_tui_input = tx_tui.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            match event::poll(Duration::from_millis(50)) {
                Ok(true) => {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind == event::KeyEventKind::Press {
                            if tx_tui_input.send(TuiEvent::Input(key)).is_err() {
                                break;
                            }
                        }
                    }
                }
                Ok(false) => {}
                Err(_) => {
                    break;
                }
            }
        }
    });

    let mut app = App::new();
    app.logs.push("[TUI] 系统初始化成功。".to_string());
    app.logs.push("[TUI] 可用本地指令: start, stop, exit, help".to_string());
    app.logs.push("[TUI] 正在连接守护进程 (127.0.0.1:42424)...".to_string());

    // Try initial connection
    let mut writer = try_connect(tx_tui.clone()).await;
    if writer.is_some() {
        app.logs.push("[TUI] 已成功连接至守护进程。".to_string());
    } else {
        app.logs.push("[TUI] ⚠️ 无法连接到守护进程。请输入 'start' 启动守护进程。".to_string());
    }

    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        tokio::select! {
            Some(tui_event) = rx_tui.recv() => {
                match tui_event {
                    TuiEvent::Input(key) => {
                        // Exit on Esc or Ctrl+C
                        if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                            break;
                        }
                        match key.code {
                            KeyCode::Enter => {
                                let cmd = app.input.trim().to_string();
                                if !cmd.is_empty() {
                                    let cmd_lower = cmd.to_lowercase();
                                    if cmd_lower == "exit" || cmd_lower == "/exit" || cmd_lower == "quit" || cmd_lower == "/quit" {
                                        break;
                                    } else if cmd_lower == "help" || cmd_lower == "/help" {
                                        app.logs.push("[TUI] 可用命令:".to_string());
                                        app.logs.push("  exit           - 退出监控界面".to_string());
                                        app.logs.push("  start          - 启动守护进程".to_string());
                                        app.logs.push("  stop           - 停止守护进程".to_string());
                                        app.logs.push("  help           - 显示帮助菜单".to_string());
                                        app.logs.push("  download <URL> - 下载视频链接 (需要已连接)".to_string());
                                    } else if cmd_lower == "start" || cmd_lower == "/start" {
                                        app.logs.push("[TUI] 正在尝试在后台启动守护进程...".to_string());
                                        match crate::daemon::spawn_daemon() {
                                            Ok(pid) => {
                                                app.logs.push(format!("[TUI] 🚀 守护进程已启动，PID: {}", pid));
                                                app.logs.push("[TUI] 正在尝试建立连接...".to_string());
                                                tokio::time::sleep(Duration::from_millis(1000)).await;
                                                writer = try_connect(tx_tui.clone()).await;
                                                if writer.is_some() {
                                                    app.logs.push("[TUI] ✅ 已成功连接到守护进程。".to_string());
                                                    app.error_msg = None;
                                                } else {
                                                    app.logs.push("[TUI] ❌ 连接建立失败，请尝试手动运行 'magebot start'".to_string());
                                                }
                                            }
                                            Err(e) => {
                                                app.logs.push(format!("[TUI] ❌ 启动进程失败: {}", e));
                                            }
                                        }
                                    } else if cmd_lower == "stop" || cmd_lower == "/stop" {
                                        if let Some(ref mut w) = writer {
                                            app.logs.push("[TUI] 正在发送停止指令至守护进程...".to_string());
                                            let msg = IpcMessage::Command("stop".to_string());
                                            if let Ok(serialized) = serde_json::to_string(&msg) {
                                                let _ = w.write_all(format!("{}\n", serialized).as_bytes()).await;
                                            }
                                        } else {
                                            app.logs.push("[TUI] 未建立连接，尝试通过 PID 强制停止本地进程...".to_string());
                                            if let Some(pid) = crate::daemon::read_pid() {
                                                if crate::daemon::is_process_alive(pid) {
                                                    if crate::daemon::kill_process(pid) {
                                                        crate::daemon::delete_pid_file();
                                                        app.logs.push("[TUI] ✅ 守护进程已强制终止。".to_string());
                                                    } else {
                                                        app.logs.push("[TUI] ❌ 进程强制终止失败。".to_string());
                                                    }
                                                } else {
                                                    app.logs.push("[TUI] ℹ️ 守护进程并未在后台运行。清理无效的 PID 文件。".to_string());
                                                    crate::daemon::delete_pid_file();
                                                }
                                            } else {
                                                app.logs.push("[TUI] ℹ️ 未找到有效的 PID 文件，判定守护进程未在运行。".to_string());
                                            }
                                        }
                                    } else {
                                        // Forward normal commands (e.g. download or /download) to server
                                        if let Some(ref mut w) = writer {
                                            let msg = IpcMessage::Command(cmd.clone());
                                            if let Ok(serialized) = serde_json::to_string(&msg) {
                                                if w.write_all(format!("{}\n", serialized).as_bytes()).await.is_err() {
                                                    app.logs.push("[TUI] ❌ 发送指令失败，连接已断开".to_string());
                                                    writer = None;
                                                }
                                            }
                                        } else {
                                            app.logs.push(format!("[TUI] ❌ 未连接到守护进程，无法发送指令: '{}'", cmd));
                                        }
                                    }
                                    app.reset_input();
                                }
                            }
                            KeyCode::Char(c) => {
                                app.enter_char(c);
                            }
                            KeyCode::Backspace => {
                                app.delete_char();
                            }
                            KeyCode::Left => {
                                app.move_cursor_left();
                            }
                            KeyCode::Right => {
                                app.move_cursor_right();
                            }
                            _ => {}
                        }
                    }
                    TuiEvent::Ipc(msg) => {
                        match msg {
                            IpcMessage::StateUpdate(tasks) => {
                                app.tasks = tasks;
                                app.tasks.sort_by(|a, b| {
                                    let type_a = match a.task_type {
                                        TaskType::Download => 0,
                                        TaskType::Upload => 1,
                                    };
                                    let type_b = match b.task_type {
                                        TaskType::Download => 0,
                                        TaskType::Upload => 1,
                                    };
                                    type_a.cmp(&type_b).then(a.id.cmp(&b.id))
                                });
                            }
                            IpcMessage::LogReceived(log) => {
                                app.logs.push(log);
                                if app.logs.len() > 100 {
                                    app.logs.remove(0);
                                }
                            }
                            _ => {}
                        }
                    }
                    TuiEvent::Disconnected => {
                        writer = None;
                        app.tasks.clear(); // Clear active tasks when disconnected
                        app.error_msg = Some("守护进程已断开连接。请检查服务是否已关闭。".to_string());
                        app.logs.push("[TUI] ⚠️ 守护进程已断开连接。".to_string());
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {}
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Restore Terminal
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();

    if let Some(err) = &app.error_msg {
        eprintln!("\n❌ {}", err);
        std::process::exit(1);
    }

    std::process::exit(0);
}

enum TuiEvent {
    Input(event::KeyEvent),
    Ipc(IpcMessage),
    Disconnected,
}

fn ui(f: &mut Frame, app: &App) {
    // Outer layout: Top Panel (Columns) + Bottom Panel (Input Console)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(f.size());

    // Top Panel Layout: Left Column (Tasks) + Right Column (Logs)
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(35), Constraint::Min(10)])
        .split(chunks[0]);

    draw_left_panel(f, main_chunks[0], app);
    draw_right_panel(f, main_chunks[1], app);
    draw_bottom_panel(f, chunks[1], app);
}

fn draw_left_panel(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 📥↓📤↑ 任务列表 ")
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if app.tasks.is_empty() {
        let no_tasks = Paragraph::new("\n 暂无活动任务\n 输入指令开始")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(no_tasks, inner_area);
        return;
    }

    // Split inner area for each task: 4 rows per task (Name, Status, Gauge, Spacer)
    let task_count = app.tasks.len();
    let mut constraints = Vec::new();
    for _ in 0..task_count {
        constraints.push(Constraint::Length(4));
    }
    constraints.push(Constraint::Min(0));

    let task_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (i, task) in app.tasks.iter().enumerate() {
        if i >= task_chunks.len() - 1 {
            break;
        }
        let chunk = task_chunks[i];

        // Split task row into: name (1), status (1), bar (1), spacer (1)
        let row_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(chunk);

        // Render task name line
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
            Span::styled(truncate_str(&task.filename, 22), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]);
        f.render_widget(Paragraph::new(name_line), row_layout[0]);

        // Render status line
        let status_text = match &task.status {
            TaskStatus::Pending => " 等待中...".to_string(),
            TaskStatus::Downloading { speed, eta, .. } => {
                format!(" 速度: {} | 剩余: {}", speed, eta)
            }
            TaskStatus::Uploading { speed, eta, .. } => {
                format!(" 速度: {} | 剩余: {}", speed, eta)
            }
            TaskStatus::Completed => " 已完成".to_string(),
            TaskStatus::Failed(e) => format!(" 失败: {}", truncate_str(e, 20)),
        };
        let status_para = Paragraph::new(Span::styled(status_text, Style::default().fg(Color::Gray)));
        f.render_widget(status_para, row_layout[1]);

        // Render progress bar (Gauge)
        let percent = match &task.status {
            TaskStatus::Downloading { progress, .. } => *progress,
            TaskStatus::Uploading { progress, .. } => *progress,
            TaskStatus::Completed => 100.0,
            _ => 0.0,
        };

        let gauge = if let TaskStatus::Uploading { .. } = task.status {
            Gauge::default()
                .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
                .percent(percent as u16)
                .label(format!("{:.1}% (上传中)", percent))
        } else {
            let bar_color = if percent >= 100.0 { Color::Green } else { Color::Blue };
            Gauge::default()
                .gauge_style(Style::default().fg(bar_color).bg(Color::DarkGray))
                .percent(percent as u16)
                .label(format!("{:.1}%", percent))
        };

        f.render_widget(gauge, row_layout[2]);
    }
}

fn draw_right_panel(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 📋 实时日志输出 (Daemon Logs) ")
        .border_style(Style::default().fg(Color::Magenta));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // List logs
    let visible_lines = inner_area.height as usize;
    let logs_to_show = if app.logs.len() > visible_lines {
        &app.logs[app.logs.len() - visible_lines..]
    } else {
        &app.logs
    };

    let log_items: Vec<ListItem> = logs_to_show
        .iter()
        .map(|log| {
            let style = if log.contains("[ERROR]") {
                Style::default().fg(Color::Red)
            } else if log.contains("[WARN]") {
                Style::default().fg(Color::LightYellow)
            } else if log.contains("[INFO]") {
                Style::default().fg(Color::LightGreen)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Span::styled(log.clone(), style))
        })
        .collect();

    let logs_list = List::new(log_items);
    f.render_widget(logs_list, inner_area);
}

fn draw_bottom_panel(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" 💻 控制台命令输入 (Console) ")
        .border_style(Style::default().fg(Color::Green));

    let input_area = block.inner(area);
    f.render_widget(block, area);

    // Render input string
    let prompt = "magebot > ";
    let input_line = Line::from(vec![
        Span::styled(prompt, Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
        Span::styled(&app.input, Style::default().fg(Color::White)),
    ]);

    let input_para = Paragraph::new(input_line);
    f.render_widget(input_para, input_area);

    // Put cursor at input position
    f.set_cursor(
        input_area.x + (prompt.len() + app.cursor_position) as u16,
        input_area.y,
    );
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() > max_len {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
