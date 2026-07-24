pub mod app;
pub mod connection;
pub mod event;
pub mod tabs;
pub mod theme;
pub mod ui;
pub mod widgets;

use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub async fn run_monitor() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Create Event Channels
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<event::TuiEvent>();
    event::spawn_event_reader(tx.clone());

    // 3. Initialize App State
    let mut app = app::App::new();
    app.add_tui_log("系统 GUI/TUI 控制台初始化成功。");
    app.add_tui_log("正在连接守护进程 (127.0.0.1:42424)...");

    // 4. Try initial connection
    let mut writer = connection::try_connect(tx.clone()).await;
    if writer.is_some() {
        app.connection_status = app::ConnectionStatus::Connected;
        app.daemon_pid = crate::daemon::read_pid();
        app.add_tui_log("✅ 已成功建立与守护进程的 IPC 连接。");
    } else {
        app.add_tui_log("⚠️ 无法连接到守护进程。点击底部 [▶ 启动服务] 启动后台守护进程。");
    }

    let mut last_reconnect = Instant::now();
    let reconnect_interval = Duration::from_secs(5);

    // 5. Main Loop
    loop {
        terminal.draw(|f| {
            ui::draw(f, &mut app);
        })?;

        tokio::select! {
            Some(tui_event) = rx.recv() => {
                match tui_event {
                    event::TuiEvent::Key(key) => {
                        if key.code == KeyCode::Esc && app.focused_input.is_none() && !app.show_add_rule_form && app.confirm_delete_rule.is_none() {
                            break;
                        }
                        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            break;
                        }
                        app.handle_key_input(key);
                    }
                    event::TuiEvent::Mouse(mouse) => {
                        match mouse.kind {
                            MouseEventKind::Down(MouseButton::Left) => {
                                if let Some(action) = app.find_click_action(mouse.column, mouse.row) {
                                    app.handle_action(action, &mut writer, &tx).await;
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                app.handle_scroll_up();
                            }
                            MouseEventKind::ScrollDown => {
                                app.handle_scroll_down();
                            }
                            _ => {}
                        }
                    }
                    event::TuiEvent::Ipc(msg) => {
                        app.handle_ipc_message(msg);
                    }
                    event::TuiEvent::Disconnected => {
                        writer = None;
                        app.connection_status = app::ConnectionStatus::Disconnected;
                        app.tasks.clear();
                        app.add_tui_log("⚠️ 守护进程已断开连接。");
                    }
                    event::TuiEvent::Tick => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Auto-reconnect logic
                if app.connection_status == app::ConnectionStatus::Disconnected
                    && last_reconnect.elapsed() >= reconnect_interval
                {
                    last_reconnect = Instant::now();
                    if let Some(pid) = crate::daemon::read_pid() {
                        if crate::daemon::is_process_alive(pid) {
                            app.connection_status = app::ConnectionStatus::Connecting;
                            writer = connection::try_connect(tx.clone()).await;
                            if writer.is_some() {
                                app.connection_status = app::ConnectionStatus::Connected;
                                app.daemon_pid = Some(pid);
                                app.add_tui_log("✅ 已重新建立与守护进程的 IPC 连接。");
                            } else {
                                app.connection_status = app::ConnectionStatus::Disconnected;
                            }
                        }
                    }
                }
            }
        }

        // Clear expired notification
        if let Some((_, _, created_at)) = &app.notification {
            if created_at.elapsed() > Duration::from_secs(3) {
                app.notification = None;
            }
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

    std::process::exit(0);
}
