use std::time::{Duration, Instant};
use ratatui::layout::Rect;
use tokio::io::AsyncWriteExt;
use crossterm::event::{KeyCode, KeyEvent};
use crate::ipc::{IpcMessage, TaskState, TaskType};
use crate::config::{Config, WatchRule};
use super::event::TuiEvent;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Dashboard,
    Rules,
    Settings,
    Logs,
    Download,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputId {
    AddRulePath,
    AddRuleTarget,
    DownloadUrl,
    SettingsDownloadDir,
    SettingsYtDlpPath,
    SettingsYtDlpArgs,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum SettingsField {
    DownloadDir,
    YtDlpPath,
    YtDlpArgs,
    MaxConcurrentUploads,
}

#[derive(Clone, Debug)]
pub enum ClickAction {
    SwitchTab(Tab),
    StartDaemon,
    StopDaemon,
    RestartDaemon,
    RunCheck,
    ShowAddRuleForm,
    HideAddRuleForm,
    ToggleListenMedia(usize),
    DeleteRule(usize),
    ConfirmDeleteRule(usize),
    CancelDeleteRule,
    SubmitAddRule,
    ToggleAutoDelete,
    SaveConfig,
    StepNumber(SettingsField, i32),
    ToggleAutoScroll,
    ClearLogs,
    PasteUrl,
    StartDownload,
    FocusTextInput(InputId),
    Noop,
}

pub struct ClickableArea {
    pub rect: Rect,
    pub action: ClickAction,
}

#[derive(Clone, Default, Debug)]
pub struct TextInputState {
    pub text: String,
    pub cursor: usize,
}

impl TextInputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter_char(&mut self, new_char: char) {
        self.text.insert(self.cursor, new_char);
        self.move_right();
    }

    pub fn delete_char(&mut self) {
        if self.cursor != 0 {
            let idx = self.cursor - 1;
            self.text.remove(idx);
            self.move_left();
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn set_text(&mut self, s: String) {
        self.text = s;
        self.cursor = self.text.len();
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    #[allow(dead_code)]
    pub raw: String,
}

impl LogEntry {
    pub fn parse(raw: &str) -> Self {
        // Expected format: [YYYY-MM-DD HH:MM:SS] [LEVEL] message
        if raw.starts_with('[') {
            let parts: Vec<&str> = raw.splitn(3, "] ").collect();
            if parts.len() == 3 {
                let timestamp = parts[0].trim_start_matches('[').to_string();
                let level = parts[1].trim_start_matches('[').to_string();
                let message = parts[2].to_string();
                return LogEntry {
                    timestamp,
                    level,
                    message,
                    raw: raw.to_string(),
                };
            }
        }
        LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level: "INFO".to_string(),
            message: raw.to_string(),
            raw: raw.to_string(),
        }
    }

    pub fn from_tui(msg: &str) -> Self {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        LogEntry {
            timestamp: ts.clone(),
            level: "TUI".to_string(),
            message: msg.to_string(),
            raw: format!("[{}] [TUI] {}", ts, msg),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadRecord {
    pub url: String,
    #[allow(dead_code)]
    pub filename: String,
    pub status: DownloadRecordStatus,
    pub timestamp: String,
}

#[derive(Clone, PartialEq, Debug)]
#[allow(dead_code)]
pub enum DownloadRecordStatus {
    Success,
    Failed(String),
    InProgress,
}

pub struct App {
    pub active_tab: Tab,
    pub connection_status: ConnectionStatus,
    pub daemon_pid: Option<u32>,
    pub tasks: Vec<TaskState>,
    pub logs: Vec<LogEntry>,
    pub cached_config: Config,
    pub cached_rules: Vec<WatchRule>,
    pub log_scroll_offset: usize,
    pub log_auto_scroll: bool,
    pub show_add_rule_form: bool,
    pub add_rule_path: TextInputState,
    pub add_rule_target: TextInputState,
    pub confirm_delete_rule: Option<usize>,
    pub focused_input: Option<InputId>,
    pub pending_config: Config,
    pub settings_download_dir: TextInputState,
    pub settings_yt_dlp_path: TextInputState,
    pub settings_yt_dlp_args: TextInputState,
    pub config_dirty: bool,
    pub download_url: TextInputState,
    pub download_history: Vec<DownloadRecord>,
    pub notification: Option<(String, NotifyLevel, Instant)>,
    pub clickable_areas: Vec<ClickableArea>,
}

impl App {
    pub fn new() -> Self {
        let conf = Config::load();
        let rules = conf.get_watch_rules();
        let mut download_dir_input = TextInputState::new();
        download_dir_input.set_text(conf.download_dir.clone().unwrap_or_default());
        let mut yt_dlp_path_input = TextInputState::new();
        yt_dlp_path_input.set_text(conf.yt_dlp_path.clone().unwrap_or_default());
        let mut yt_dlp_args_input = TextInputState::new();
        yt_dlp_args_input.set_text(conf.yt_dlp_args.clone().unwrap_or_default());

        App {
            active_tab: Tab::Dashboard,
            connection_status: ConnectionStatus::Disconnected,
            daemon_pid: crate::daemon::read_pid(),
            tasks: Vec::new(),
            logs: Vec::new(),
            cached_config: conf.clone(),
            cached_rules: rules,
            log_scroll_offset: 0,
            log_auto_scroll: true,
            show_add_rule_form: false,
            add_rule_path: TextInputState::new(),
            add_rule_target: TextInputState::new(),
            confirm_delete_rule: None,
            focused_input: None,
            pending_config: conf,
            settings_download_dir: download_dir_input,
            settings_yt_dlp_path: yt_dlp_path_input,
            settings_yt_dlp_args: yt_dlp_args_input,
            config_dirty: false,
            download_url: TextInputState::new(),
            download_history: Vec::new(),
            notification: None,
            clickable_areas: Vec::new(),
        }
    }

    pub fn add_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);
        if self.logs.len() > 300 {
            self.logs.remove(0);
        }
    }

    pub fn add_tui_log(&mut self, msg: &str) {
        let entry = LogEntry::from_tui(msg);
        self.add_log(entry);
    }

    pub fn notify(&mut self, msg: &str, level: NotifyLevel) {
        self.notification = Some((msg.to_string(), level, Instant::now()));
    }

    pub fn refresh_config(&mut self) {
        self.cached_config = Config::load();
        self.pending_config = self.cached_config.clone();
        self.settings_download_dir.set_text(self.pending_config.download_dir.clone().unwrap_or_default());
        self.settings_yt_dlp_path.set_text(self.pending_config.yt_dlp_path.clone().unwrap_or_default());
        self.settings_yt_dlp_args.set_text(self.pending_config.yt_dlp_args.clone().unwrap_or_default());
        self.refresh_rules();
        self.config_dirty = false;
    }

    pub fn refresh_rules(&mut self) {
        self.cached_rules = self.cached_config.get_watch_rules();
    }

    pub fn is_input_focused(&self, id: InputId) -> bool {
        self.focused_input == Some(id)
    }

    pub fn get_focused_input_mut(&mut self) -> Option<&mut TextInputState> {
        match self.focused_input {
            Some(InputId::AddRulePath) => Some(&mut self.add_rule_path),
            Some(InputId::AddRuleTarget) => Some(&mut self.add_rule_target),
            Some(InputId::DownloadUrl) => Some(&mut self.download_url),
            Some(InputId::SettingsDownloadDir) => Some(&mut self.settings_download_dir),
            Some(InputId::SettingsYtDlpPath) => Some(&mut self.settings_yt_dlp_path),
            Some(InputId::SettingsYtDlpArgs) => Some(&mut self.settings_yt_dlp_args),
            None => None,
        }
    }

    pub fn handle_key_input(&mut self, key: KeyEvent) {
        if self.focused_input.is_some() {
            match key.code {
                KeyCode::Char(c) => {
                    if let Some(input) = self.get_focused_input_mut() {
                        input.enter_char(c);
                        self.sync_settings_fields();
                    }
                }
                KeyCode::Backspace => {
                    if let Some(input) = self.get_focused_input_mut() {
                        input.delete_char();
                        self.sync_settings_fields();
                    }
                }
                KeyCode::Left => {
                    if let Some(input) = self.get_focused_input_mut() {
                        input.move_left();
                    }
                }
                KeyCode::Right => {
                    if let Some(input) = self.get_focused_input_mut() {
                        input.move_right();
                    }
                }
                KeyCode::Esc => {
                    self.focused_input = None;
                }
                KeyCode::Tab => {
                    // Cycle input focus if in add rule form
                    if self.show_add_rule_form {
                        if self.focused_input == Some(InputId::AddRulePath) {
                            self.focused_input = Some(InputId::AddRuleTarget);
                        } else {
                            self.focused_input = Some(InputId::AddRulePath);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn sync_settings_fields(&mut self) {
        if let Some(focused) = self.focused_input {
            match focused {
                InputId::SettingsDownloadDir => {
                    let val = self.settings_download_dir.text.trim().to_string();
                    self.pending_config.download_dir = if val.is_empty() { None } else { Some(val) };
                    self.config_dirty = true;
                }
                InputId::SettingsYtDlpPath => {
                    let val = self.settings_yt_dlp_path.text.trim().to_string();
                    self.pending_config.yt_dlp_path = if val.is_empty() { None } else { Some(val) };
                    self.config_dirty = true;
                }
                InputId::SettingsYtDlpArgs => {
                    let val = self.settings_yt_dlp_args.text.trim().to_string();
                    self.pending_config.yt_dlp_args = if val.is_empty() { None } else { Some(val) };
                    self.config_dirty = true;
                }
                _ => {}
            }
        }
    }

    pub fn handle_scroll_up(&mut self) {
        if self.active_tab == Tab::Logs {
            self.log_scroll_offset = self.log_scroll_offset.saturating_add(2);
            self.log_auto_scroll = false;
        }
    }

    pub fn handle_scroll_down(&mut self) {
        if self.active_tab == Tab::Logs {
            self.log_scroll_offset = self.log_scroll_offset.saturating_sub(2);
            if self.log_scroll_offset == 0 {
                self.log_auto_scroll = true;
            }
        }
    }

    pub fn handle_ipc_message(&mut self, msg: IpcMessage) {
        match msg {
            IpcMessage::StateUpdate(mut tasks) => {
                tasks.sort_by(|a, b| {
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
                self.tasks = tasks;
            }
            IpcMessage::LogReceived(log_line) => {
                let entry = LogEntry::parse(&log_line);
                self.add_log(entry);
            }
            _ => {}
        }
    }

    pub fn find_click_action(&self, x: u16, y: u16) -> Option<ClickAction> {
        for area in self.clickable_areas.iter().rev() {
            if x >= area.rect.x
                && x < area.rect.x + area.rect.width
                && y >= area.rect.y
                && y < area.rect.y + area.rect.height
            {
                return Some(area.action.clone());
            }
        }
        None
    }

    pub async fn handle_action(
        &mut self,
        action: ClickAction,
        writer: &mut Option<tokio::net::tcp::OwnedWriteHalf>,
        tx: &tokio::sync::mpsc::UnboundedSender<TuiEvent>,
    ) {
        match action {
            ClickAction::SwitchTab(tab) => {
                self.active_tab = tab;
                self.focused_input = None;
                if tab == Tab::Settings || tab == Tab::Rules {
                    self.refresh_config();
                }
            }
            ClickAction::StartDaemon => {
                self.add_tui_log("正在后台启动守护进程...");
                match crate::daemon::spawn_daemon() {
                    Ok(pid) => {
                        self.daemon_pid = Some(pid);
                        self.add_tui_log(&format!("🚀 守护进程已启动, PID: {}", pid));
                        self.notify(&format!("守护进程已启动 (PID: {})", pid), NotifyLevel::Success);
                        tokio::time::sleep(Duration::from_millis(800)).await;
                        let _ = tx.send(TuiEvent::Tick);
                    }
                    Err(e) => {
                        self.add_tui_log(&format!("❌ 启动守护进程失败: {}", e));
                        self.notify(&format!("启动失败: {}", e), NotifyLevel::Error);
                    }
                }
            }
            ClickAction::StopDaemon => {
                if let Some(w) = writer {
                    self.add_tui_log("正在发送停止指令至守护进程...");
                    let msg = IpcMessage::Command("stop".to_string());
                    if let Ok(serialized) = serde_json::to_string(&msg) {
                        let _ = w.write_all(format!("{}\n", serialized).as_bytes()).await;
                        self.notify("停止指令已发送", NotifyLevel::Info);
                    }
                } else if let Some(pid) = crate::daemon::read_pid() {
                    if crate::daemon::is_process_alive(pid) {
                        if crate::daemon::kill_process(pid) {
                            crate::daemon::delete_pid_file();
                            self.daemon_pid = None;
                            self.connection_status = ConnectionStatus::Disconnected;
                            self.add_tui_log("✅ 守护进程已强制终止");
                            self.notify("进程已强制终止", NotifyLevel::Warning);
                        }
                    } else {
                        crate::daemon::delete_pid_file();
                        self.daemon_pid = None;
                        self.add_tui_log("ℹ️ 守护进程未在运行，已清理 PID 文件");
                    }
                }
            }
            ClickAction::RestartDaemon => {
                self.add_tui_log("正在重启守护进程...");
                if let Some(pid) = crate::daemon::read_pid() {
                    if crate::daemon::is_process_alive(pid) {
                        let _ = crate::daemon::kill_process(pid);
                        crate::daemon::delete_pid_file();
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                match crate::daemon::spawn_daemon() {
                    Ok(pid) => {
                        self.daemon_pid = Some(pid);
                        self.add_tui_log(&format!("🚀 守护进程已重启, PID: {}", pid));
                        self.notify("守护进程已重启", NotifyLevel::Success);
                        tokio::time::sleep(Duration::from_millis(800)).await;
                        let _ = tx.send(TuiEvent::Tick);
                    }
                    Err(e) => {
                        self.add_tui_log(&format!("❌ 重启守护进程失败: {}", e));
                        self.notify("重启失败", NotifyLevel::Error);
                    }
                }
            }
            ClickAction::RunCheck => {
                if let Some(w) = writer {
                    let msg = IpcMessage::Command("check".to_string());
                    if let Ok(serialized) = serde_json::to_string(&msg) {
                        let _ = w.write_all(format!("{}\n", serialized).as_bytes()).await;
                        self.notify("配置检查指令已发送", NotifyLevel::Info);
                    }
                } else {
                    self.add_tui_log("正在进行本地配置检查...");
                    let conf = Config::load();
                    if conf.api_id.is_none() || conf.api_hash.is_none() {
                        self.notify("⚠️ 未配置 API ID / Hash", NotifyLevel::Warning);
                        self.add_tui_log("❌ 检查失败: 请使用 `magebot login` 配置 API 参数");
                    } else {
                        self.notify("✅ 配置完整", NotifyLevel::Success);
                        self.add_tui_log("✅ 本地配置参数检查完成");
                    }
                }
            }
            ClickAction::ShowAddRuleForm => {
                self.show_add_rule_form = true;
                self.add_rule_path.clear();
                self.add_rule_target.clear();
                self.focused_input = Some(InputId::AddRulePath);
            }
            ClickAction::HideAddRuleForm => {
                self.show_add_rule_form = false;
                self.focused_input = None;
            }
            ClickAction::ToggleListenMedia(id) => {
                let mut conf = Config::load();
                if let Some(rule) = conf.get_watch_rules().iter().find(|r| r.id == id) {
                    let new_state = !rule.listen_media;
                    if let Ok(updated) = conf.set_listen_media(id, new_state) {
                        let _ = conf.save();
                        self.add_tui_log(&format!("规则 #{} 监听设置为: {}", updated.id, updated.listen_media));
                        self.notify(
                            &format!("规则 #{} 监听: {}", updated.id, if updated.listen_media { "开启" } else { "关闭" }),
                            NotifyLevel::Info,
                        );
                        self.refresh_config();
                    }
                }
            }
            ClickAction::DeleteRule(id) => {
                self.confirm_delete_rule = Some(id);
            }
            ClickAction::ConfirmDeleteRule(id) => {
                if let Err(e) = crate::setup::run_rm(&id.to_string()) {
                    self.add_tui_log(&format!("❌ 删除规则失败: {}", e));
                    self.notify(&format!("删除失败: {}", e), NotifyLevel::Error);
                } else {
                    self.add_tui_log(&format!("✅ 成功删除规则 #{}", id));
                    self.notify(&format!("已删除规则 #{}", id), NotifyLevel::Success);
                    self.refresh_config();
                }
                self.confirm_delete_rule = None;
            }
            ClickAction::CancelDeleteRule => {
                self.confirm_delete_rule = None;
            }
            ClickAction::SubmitAddRule => {
                let path = self.add_rule_path.text.trim().to_string();
                let mut target = self.add_rule_target.text.trim().to_string();
                if target.is_empty() {
                    target = "me".to_string();
                }
                if path.is_empty() {
                    self.notify("目录路径不能为空", NotifyLevel::Warning);
                    return;
                }
                let input_str = format!("{}:{}", path, target);
                if let Err(e) = crate::setup::run_add(&input_str) {
                    self.add_tui_log(&format!("❌ 添加规则失败: {}", e));
                    self.notify(&format!("添加失败: {}", e), NotifyLevel::Error);
                } else {
                    self.add_tui_log(&format!("✅ 成功添加规则: {} -> {}", path, target));
                    self.notify("成功添加监控规则", NotifyLevel::Success);
                    self.refresh_config();
                    self.show_add_rule_form = false;
                    self.focused_input = None;
                }
            }
            ClickAction::ToggleAutoDelete => {
                let curr = self.pending_config.auto_delete.unwrap_or(false);
                self.pending_config.auto_delete = Some(!curr);
                self.config_dirty = true;
            }
            ClickAction::SaveConfig => {
                if let Err(e) = self.pending_config.save() {
                    self.add_tui_log(&format!("❌ 保存配置失败: {}", e));
                    self.notify(&format!("保存失败: {}", e), NotifyLevel::Error);
                } else {
                    self.add_tui_log("✅ 成功保存配置项");
                    self.notify("配置已保存", NotifyLevel::Success);
                    self.refresh_config();
                }
            }
            ClickAction::StepNumber(field, delta) => {
                if field == SettingsField::MaxConcurrentUploads {
                    let curr = self.pending_config.max_concurrent_uploads.unwrap_or(3);
                    let new_val = if delta < 0 {
                        curr.saturating_sub(1).max(1)
                    } else {
                        curr.saturating_add(1).min(20)
                    };
                    self.pending_config.max_concurrent_uploads = Some(new_val);
                    self.config_dirty = true;
                }
            }
            ClickAction::ToggleAutoScroll => {
                self.log_auto_scroll = !self.log_auto_scroll;
                if self.log_auto_scroll {
                    self.log_scroll_offset = 0;
                }
            }
            ClickAction::ClearLogs => {
                self.logs.clear();
                self.log_scroll_offset = 0;
            }
            ClickAction::PasteUrl => {
                match cli_clipboard::get_contents() {
                    Ok(text) => {
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            self.download_url.set_text(trimmed);
                            self.notify("已从剪贴板粘贴", NotifyLevel::Info);
                        } else {
                            self.notify("剪贴板内容为空", NotifyLevel::Warning);
                        }
                    }
                    Err(e) => {
                        self.notify(&format!("获取剪贴板失败: {}", e), NotifyLevel::Error);
                    }
                }
            }
            ClickAction::StartDownload => {
                let url = self.download_url.text.trim().to_string();
                if url.is_empty() {
                    self.notify("请输入或粘贴视频 URL", NotifyLevel::Warning);
                    return;
                }
                if let Some(w) = writer {
                    let msg = IpcMessage::Command(format!("download {}", url));
                    if let Ok(serialized) = serde_json::to_string(&msg) {
                        if w.write_all(format!("{}\n", serialized).as_bytes()).await.is_ok() {
                            self.add_tui_log(&format!("已提交下载任务: {}", url));
                            self.notify("已发起下载请求", NotifyLevel::Success);
                            self.download_history.insert(
                                0,
                                DownloadRecord {
                                    url: url.clone(),
                                    filename: url.clone(),
                                    status: DownloadRecordStatus::InProgress,
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                },
                            );
                            self.download_url.clear();
                        } else {
                            self.notify("发送下载命令失败", NotifyLevel::Error);
                        }
                    }
                } else {
                    self.notify("未连接到守护进程，无法发起下载", NotifyLevel::Error);
                }
            }
            ClickAction::FocusTextInput(id) => {
                self.focused_input = Some(id);
            }
            ClickAction::Noop => {}
        }
    }
}
