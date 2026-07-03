mod config;
mod daemon;
mod downloader;
mod logger;
mod watcher;
mod crypto;
mod ipc;
mod tui;
mod auth;
mod setup;

use clap::{Parser, Subcommand};
use config::Config;
use grammers_session::defs::{PeerId, PeerRef};

#[derive(Parser)]
#[command(name = "magebot")]
#[command(about = "A selfhosted Telegram client bot for file upload synchronization and video downloading (MTProto)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // ── 账号管理 ──────────────────────────────────────
    /// 交互式登录 Telegram 账号
    Login,

    /// 退出登录并清除会话
    Logout,

    // ── 配置管理 ──────────────────────────────────────
    /// 设置配置参数 (例如: magebot set auto_delete true)
    Set {
        /// Configuration arguments in key:value or key value format
        #[arg(required = false, num_args = 0..=2)]
        args: Vec<String>,
    },

    // ── 监控规则 ──────────────────────────────────────
    /// 添加监控规则 (例如: magebot add ~/videos 或 magebot add ~/videos:-5589877937)
    Add {
        /// 监控目录路径，可附加 ":目标群组" (默认投递至收藏夹)
        rule: String,
    },

    /// 删除监控规则 (按规则 ID 或目录路径)
    Rm {
        /// 规则 ID 或目录路径
        target: String,
    },

    /// 列出所有监控规则
    Ls,

    /// 开/关指定规则的媒体链接监听 (例如: magebot listen 1 true)
    Listen {
        /// 监控规则 ID
        id: usize,
        /// 开启/关闭 (true/false)
        enabled: String,
    },

    // ── 服务控制 ──────────────────────────────────────
    /// 启动后台守护进程
    Start,

    /// 停止后台守护进程
    Stop,

    /// 重启后台守护进程
    Restart,

    /// 查看守护进程运行状态
    Status,

    /// 打开 TUI 实时监控面板
    Monitor,

    // ── 诊断 ──────────────────────────────────────────
    /// 检查配置与授权状态
    Check,

    // ── 内部命令 (hidden) ─────────────────────────────
    /// Run the bot in daemon mode (internal command)
    #[command(hide = true)]
    Daemon,

    /// Send a test message to Saved Messages
    #[command(hide = true)]
    TestMsg {
        url: String,
    },
}

#[tokio::main]
async fn main() {
    // Prepend dependency folders to PATH for local fallback of yt-dlp and ffmpeg
    let mut fallback_paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        fallback_paths.push(cwd.join("dependency"));
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            fallback_paths.push(exe_dir.join("dependency"));
            if exe_dir.ends_with("target\\debug") || exe_dir.ends_with("target/debug") {
                if let Some(project_root) = exe_dir.parent().and_then(|p| p.parent()) {
                    fallback_paths.push(project_root.join("dependency"));
                }
            }
        }
    }

    if let Some(path_val) = std::env::var_os("PATH") {
        let mut paths = std::env::split_paths(&path_val).collect::<Vec<_>>();
        let mut inserted = 0;
        for path in fallback_paths {
            if path.exists() && !paths.contains(&path) {
                paths.insert(inserted, path);
                inserted += 1;
            }
        }
        if let Ok(new_path) = std::env::join_paths(paths) {
            unsafe {
                std::env::set_var("PATH", new_path);
            }
        }
    }

    let cli = Cli::parse();

    match cli.command {
        // ── 账号管理 ──────────────────────────────────
        Commands::Login => {
            if let Err(e) = auth::run_login().await {
                eprintln!("❌ Login failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Logout => {
            if let Err(e) = auth::run_logout().await {
                eprintln!("❌ Logout failed: {}", e);
                std::process::exit(1);
            }
        }

        // ── 配置管理 ──────────────────────────────────
        Commands::Set { args } => {
            if let Err(e) = setup::run_set(&args) {
                eprintln!("❌ Failed to set configuration: {}", e);
                std::process::exit(1);
            }
        }

        // ── 监控规则 ──────────────────────────────────
        Commands::Add { rule } => {
            if let Err(e) = setup::run_add(&rule) {
                eprintln!("❌ Failed to add watch rule: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Rm { target } => {
            if let Err(e) = setup::run_rm(&target) {
                eprintln!("❌ Failed to remove watch rule: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Ls => {
            setup::run_ls();
        }
        Commands::Listen { id, enabled } => {
            if let Err(e) = setup::run_listen(id, &enabled) {
                eprintln!("❌ Failed to update listen status: {}", e);
                std::process::exit(1);
            }
        }

        // ── 服务控制 ──────────────────────────────────
        Commands::Start => {
            // 1. Run checks first
            println!("Running configuration checks...");
            if let Err(e) = auth::run_checks().await {
                eprintln!("❌ Pre-start check failed: {}. Startup aborted.", e);
                std::process::exit(1);
            }

            // 2. Check if already running
            if let Some(pid) = daemon::read_pid() {
                if daemon::is_process_alive(pid) {
                    println!("ℹ️ magebot is already running (PID: {}).", pid);
                    return;
                } else {
                    println!("Cleaning up stale PID file...");
                    daemon::delete_pid_file();
                }
            }

            // 3. Spawn daemon
            match daemon::spawn_daemon() {
                Ok(pid) => {
                    println!("🚀 magebot started successfully in the background (PID: {}).", pid);
                }
                Err(e) => {
                    eprintln!("❌ Failed to start background process: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Stop => {
            let pid = match daemon::read_pid() {
                Some(pid) => pid,
                None => {
                    println!("ℹ️ magebot is not running (no PID file found).");
                    return;
                }
            };

            if !daemon::is_process_alive(pid) {
                println!("ℹ️ magebot is not running (PID {} is stale). Cleaning up PID file.", pid);
                daemon::delete_pid_file();
                return;
            }

            println!("Stopping magebot (PID: {})...", pid);
            if daemon::kill_process(pid) {
                daemon::delete_pid_file();
                println!("✅ magebot stopped successfully.");
            } else {
                eprintln!("❌ Failed to stop process {}.", pid);
                std::process::exit(1);
            }
        }
        Commands::Restart => {
            // Stop if running
            if let Some(pid) = daemon::read_pid() {
                if daemon::is_process_alive(pid) {
                    println!("Stopping running instance (PID: {})...", pid);
                    if daemon::kill_process(pid) {
                        daemon::delete_pid_file();
                    } else {
                        eprintln!("❌ Failed to stop running process {}.", pid);
                        std::process::exit(1);
                    }
                }
            }

            // Run pre-start checks
            println!("Running configuration checks...");
            if let Err(e) = auth::run_checks().await {
                eprintln!("❌ Pre-start check failed: {}. Startup aborted.", e);
                std::process::exit(1);
            }

            // Start
            match daemon::spawn_daemon() {
                Ok(pid) => {
                    println!("🚀 magebot restarted successfully in the background (PID: {}).", pid);
                }
                Err(e) => {
                    eprintln!("❌ Failed to start background process: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Status => {
            match daemon::read_pid() {
                Some(pid) => {
                    if daemon::is_process_alive(pid) {
                        println!("✅ magebot is running (PID: {})", pid);
                    } else {
                        println!("⚠️ magebot is not running (PID {} is stale)", pid);
                        daemon::delete_pid_file();
                    }
                }
                None => {
                    println!("⏹️ magebot is not running");
                }
            }
        }
        Commands::Monitor => {
            if let Err(e) = tui::run_monitor().await {
                eprintln!("❌ Monitor error: {}", e);
                std::process::exit(1);
            }
        }

        // ── 诊断 ──────────────────────────────────────
        Commands::Check => {
            if let Err(e) = auth::run_checks().await {
                eprintln!("❌ Check failed: {}", e);
                std::process::exit(1);
            } else {
                println!("✅ All checks passed successfully!");
            }
        }

        // ── 内部命令 ──────────────────────────────────
        Commands::Daemon => {
            run_daemon().await;
        }
        Commands::TestMsg { url } => {
            let config = Config::load();
            let client = match auth::get_client(&config).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("❌ Failed to get client: {}", e);
                    std::process::exit(1);
                }
            };
            let me = client.get_me().await.unwrap();
            let target_chat = PeerRef::from(grammers_client::types::Peer::User(me));
            if let Err(e) = client.send_message(target_chat, url).await {
                eprintln!("❌ Failed to send message: {}", e);
                std::process::exit(1);
            }
            println!("✅ Sent test message to Saved Messages successfully.");
        }
    }
}

async fn run_daemon() {
    logger::info("magebot daemon process started.");
    let config = Config::load();

    // Keep a shared memory-only set of message IDs currently being processed to avoid duplicate handling at runtime.
    let processing_ids: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<i32>>> =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

    // 1. Get client
    let (client, updates_rx) = match auth::get_client_with_updates(&config).await {
        Ok(c) => c,
        Err(e) => {
            logger::error(&format!("Failed to initialize client: {}", e));
            return;
        }
    };

    let is_auth = match client.is_authorized().await {
        Ok(auth) => auth,
        Err(e) => {
            logger::error(&format!("Failed to check authorization: {}", e));
            return;
        }
    };

    if !is_auth {
        logger::error("Client is not authorized. Exiting daemon.");
        return;
    }

    logger::info("MTProto Update Listener starting...");
    let mut updates_stream = client.stream_updates(
        updates_rx,
        grammers_client::UpdatesConfiguration {
            catch_up: true,
            update_queue_limit: None,
        },
    );

    // 2. Start directory watcher
    let client_watcher = client.clone();
    let config_watcher = config.clone();
    tokio::spawn(async move {
        watcher::run_watcher(client_watcher, config_watcher).await;
    });

    // 3. Listen for updates in Saved Messages (Outgoing messages to ourselves)
    let me = match client.get_me().await {
        Ok(u) => u,
        Err(e) => {
            logger::error(&format!("Failed to get self user: {}", e));
            return;
        }
    };
    let my_id = PeerId::user(me.bare_id());

    // Start heartbeat ping loop to keep MTProto TCP connection alive and prevent idle socket drops
    let client_heartbeat = client.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            if let Err(e) = client_heartbeat.get_me().await {
                logger::warn(&format!("Heartbeat ping failed (reconnecting): {}", e));
            }
        }
    });

    // Start Active Media Channel Poller (Scans recent messages in listened groups & Saved Messages every 5s)
    let client_poller = client.clone();
    let me_poller = me.clone();
    let processing_ids_poller = processing_ids.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;

            let conf = Config::load();
            let mut target_peers: Vec<PeerRef> = Vec::new();

            // Always check Saved Messages (me)
            target_peers.push(PeerRef::from(grammers_client::types::Peer::User(me_poller.clone())));

            // Add all rules with listen_media = true
            for rule in conf.get_watch_rules() {
                if rule.listen_media {
                    if let Ok(peer_ref) = watcher::resolve_peer(&client_poller, &rule.target).await {
                        target_peers.push(peer_ref);
                    }
                }
            }

            for target_chat in target_peers {
                let mut messages_stream = client_poller.iter_messages(target_chat.clone()).limit(5);
                while let Ok(Some(message)) = messages_stream.next().await {
                    let text_str = message.text().to_string();
                    let text_lower = text_str.to_lowercase();

                    if downloader::is_video_url(&text_str)
                        && !text_lower.contains("[uploaded]")
                        && !text_lower.contains("[已上传]")
                        && !text_lower.contains("[failed]")
                        && !text_lower.contains("[已失败]")
                    {
                        let msg_id = message.id();
                        let is_already_processing = {
                            let mut lock = processing_ids_poller.lock().unwrap();
                            if lock.contains(&msg_id) {
                                true
                            } else {
                                lock.insert(msg_id);
                                false
                            }
                        };

                        if !is_already_processing {
                            logger::info(&format!(
                                "轮询捕获到视频链接 (消息 ID: {}, 来自: {:?})",
                                msg_id,
                                message.peer_id()
                            ));

                            let urls: Vec<String> = text_str
                                .split_whitespace()
                                .filter(|word| downloader::is_video_url(word))
                                .map(|w| w.to_string())
                                .collect();

                            for url in urls {
                                let client_clone = client_poller.clone();
                                let config_clone = conf.clone();
                                let target_chat_clone = target_chat.clone();
                                tokio::spawn(async move {
                                    downloader::handle_video_download(
                                        client_clone,
                                        config_clone,
                                        target_chat_clone,
                                        url,
                                        msg_id,
                                    )
                                    .await;
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    // Pre-resolve peers for rules with listen_media = true
    let mut listened_peers: std::collections::HashMap<i64, PeerRef> = std::collections::HashMap::new();
    for rule in config.get_watch_rules() {
        if rule.listen_media {
            match watcher::resolve_peer(&client, &rule.target).await {
                Ok(peer_ref) => {
                    let dialog_id = peer_ref.id.bot_api_dialog_id();
                    logger::info(&format!(
                        "Daemon: Listening for media links in target '{}' (Rule #{}, DialogID: {})",
                        rule.target, rule.id, dialog_id
                    ));
                    listened_peers.insert(dialog_id, peer_ref);
                }
                Err(e) => {
                    logger::error(&format!(
                        "Daemon: Failed to resolve target '{}' for listen_media (Rule #{}): {}",
                        rule.target, rule.id, e
                    ));
                }
            }
        }
    }

    // Start IPC Server
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        ipc::start_ipc_server(cmd_tx).await;
    });

    // Spawn task to process commands received from IPC monitor clients
    let client_cmd = client.clone();
    let config_cmd = config.clone();
    let me_clone = me.clone();
    tokio::spawn(async move {
        while let Some(cmd_str) = cmd_rx.recv().await {
            let cmd_trimmed = cmd_str.trim();
            let is_download = cmd_trimmed.starts_with("/download ") || cmd_trimmed.starts_with("download ");
            if is_download {
                let url = if cmd_trimmed.starts_with("/download ") {
                    cmd_trimmed["/download ".len()..].trim().to_string()
                } else {
                    cmd_trimmed["download ".len()..].trim().to_string()
                };
                if downloader::is_video_url(&url) {
                    logger::info(&format!("IPC: Received command to download: {}", url));
                    let target_chat = PeerRef::from(grammers_client::types::Peer::User(me_clone.clone()));
                    let client_clone = client_cmd.clone();
                    let config_clone = config_cmd.clone();
                    tokio::spawn(async move {
                        downloader::handle_video_download(client_clone, config_clone, target_chat, url, 0).await;
                    });
                } else {
                    logger::warn(&format!("IPC: Invalid download URL: {}", url));
                }
            } else if cmd_trimmed == "/ls" || cmd_trimmed == "ls" {
                let conf = Config::load();
                let rules = conf.get_watch_rules();
                let global_auto_del = conf.auto_delete.unwrap_or(false);
                if rules.is_empty() {
                    logger::info(&format!("IPC: 当前未配置任何监控组。(全局 auto_delete: {})", global_auto_del));
                } else {
                    let mut output = format!("Magebot 监控组列表 (全局 auto_delete: {}):\n", global_auto_del);
                    for r in &rules {
                        let auto_del = r.get_auto_delete(conf.auto_delete);
                        output.push_str(&format!(
                            "  [ID: {}] {} -> {} (listen_media: {}, auto_delete: {})\n",
                            r.id, r.path, r.target, r.listen_media, auto_del
                        ));
                    }
                    logger::info(&output);
                }
            } else if cmd_trimmed.starts_with("/listen ") || cmd_trimmed.starts_with("listen ") {
                let args_str = if cmd_trimmed.starts_with("/listen ") {
                    cmd_trimmed["/listen ".len()..].trim()
                } else {
                    cmd_trimmed["listen ".len()..].trim()
                };
                let parts: Vec<&str> = args_str.split_whitespace().collect();
                if parts.len() == 2 {
                    if let Ok(id) = parts[0].parse::<usize>() {
                        if let Err(e) = setup::run_listen(id, parts[1]) {
                            logger::error(&format!("IPC: listen 失败: {}", e));
                        } else {
                            logger::info(&format!("IPC: 成功更新规则 #{} listen 为 {}", id, parts[1]));
                        }
                    } else {
                        logger::warn("IPC: 规则 ID 必须为有效数字");
                    }
                } else {
                    logger::warn("IPC: 用法 listen <id> <true|false>");
                }
            } else if cmd_trimmed == "/help" || cmd_trimmed == "help" {
                logger::info("可用指令列表:\n  download <URL>      - 下载视频链接\n  ls                  - 列出监控规则\n  listen <id> <t/f>   - 开/关媒体链接监听\n  stop                - 停止守护进程\n  help                - 显示帮助菜单");
            } else if cmd_trimmed == "/stop" || cmd_trimmed == "stop" {
                logger::info("收到停止指令，正在关闭守护进程...");
                daemon::delete_pid_file();
                std::process::exit(0);
            } else {
                logger::warn(&format!("IPC: Unknown command: {}", cmd_trimmed));
            }
        }
    });

    loop {
        let update = match updates_stream.next().await {
            Ok(up) => up,
            Err(e) => {
                logger::error(&format!("Update stream error: {}", e));
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        match update {
            grammers_client::Update::NewMessage(message) => {
                let text_str = message.text().to_string();
                let text_lower = text_str.to_lowercase();
                if downloader::is_video_url(&text_str)
                    && !text_lower.contains("[uploaded]")
                    && !text_lower.contains("[已上传]")
                    && !text_lower.contains("[failed]")
                    && !text_lower.contains("[已失败]")
                {
                    let msg_peer_id = message.peer_id();
                    let msg_dialog_id = msg_peer_id.bot_api_dialog_id();
                    let is_saved_messages = msg_peer_id == my_id
                        || msg_dialog_id == me.bare_id()
                        || matches!(message.peer(), Ok(grammers_client::types::Peer::User(u)) if u.bare_id() == me.bare_id());

                    // If not saved messages and not currently cached in listened_peers, refresh config dynamically
                    if !is_saved_messages && !listened_peers.contains_key(&msg_dialog_id) {
                        let latest_config = Config::load();
                        for rule in latest_config.get_watch_rules() {
                            if rule.listen_media {
                                if let Ok(peer_ref) = watcher::resolve_peer(&client, &rule.target).await {
                                    listened_peers.insert(peer_ref.id.bot_api_dialog_id(), peer_ref);
                                }
                            }
                        }
                    }

                    let listened_peer_opt = listened_peers.get(&msg_dialog_id).cloned();

                    if is_saved_messages || listened_peer_opt.is_some() {
                        let msg_id = message.id();
                        logger::info(&format!("识别到视频链接 (消息 ID: {}, 来自 DialogID: {})", msg_id, msg_dialog_id));
                        let is_already_processing = {
                            let mut lock = processing_ids.lock().unwrap();
                            if lock.contains(&msg_id) {
                                true
                            } else {
                                lock.insert(msg_id);
                                false
                            }
                        };
                        if is_already_processing {
                            logger::info(&format!("链接已在处理中，跳过 (ID: {})", msg_id));
                        } else {
                            
                            let urls: Vec<String> = text_str
                                .split_whitespace()
                                .filter(|word| downloader::is_video_url(word))
                                .map(|w| w.to_string())
                                .collect();

                            let target_chat = if let Some(peer_ref) = listened_peer_opt {
                                peer_ref
                            } else {
                                match message.peer() {
                                    Ok(peer) => PeerRef::from(peer),
                                    Err(peer_ref) => peer_ref,
                                }
                            };

                            for url in urls {
                                let client_clone = client.clone();
                                let config_clone = config.clone();
                                let target_chat_clone = target_chat.clone();
                                tokio::spawn(async move {
                                    downloader::handle_video_download(client_clone, config_clone, target_chat_clone, url, msg_id).await;
                                });
                            }
                        }
                    } else {
                        logger::info(&format!("识别到视频链接 (ID: {})，但当前聊天 (DialogID: {}) 未在监控组中开启 listen_media。", message.id(), msg_dialog_id));
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_value_colon() {
        let args = vec!["yt_dlp_args:[--cookies-from-browser chrome]".to_string()];
        let res = setup::parse_key_value(&args).unwrap();
        assert_eq!(res.0, "yt_dlp_args");
        assert_eq!(res.1, "--cookies-from-browser chrome");
    }

    #[test]
    fn test_parse_key_value_space() {
        let args = vec!["watch_dir".to_string(), "j:\\my_folder".to_string()];
        let res = setup::parse_key_value(&args).unwrap();
        assert_eq!(res.0, "watch_dir");
        assert_eq!(res.1, "j:\\my_folder");
    }

    #[test]
    fn test_parse_key_value_empty() {
        let args: Vec<String> = vec![];
        let res = setup::parse_key_value(&args);
        assert!(res.is_err());
    }

    #[test]
    fn test_is_video_url() {
        use crate::downloader::is_video_url;
        assert!(is_video_url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(is_video_url("https://youtu.be/dQw4w9WgXcQ"));
        assert!(is_video_url("https://x.com/username/status/123456"));
        assert!(is_video_url("https://twitter.com/username/status/123456"));
        assert!(is_video_url("https://www.twitch.tv/videos/123456789"));
        assert!(is_video_url("https://www.bilibili.com/video/BV1xx"));
        assert!(is_video_url("https://b23.tv/BV1xx"));
        assert!(is_video_url("https://bili.live/123456"));
        assert!(is_video_url("https://www.bilibili.tv/en/video/123"));
        assert!(!is_video_url("https://google.com"));
    }

    #[test]
    fn test_parse_watch_dir_input() {
        use crate::setup::parse_watch_dir_input;

        // Windows path without target
        assert_eq!(
            parse_watch_dir_input("C:\\Users\\bofan\\Desktop\\watch"),
            ("C:\\Users\\bofan\\Desktop\\watch".to_string(), "me".to_string())
        );

        // Windows path with numeric group ID target
        assert_eq!(
            parse_watch_dir_input("C:\\Users\\bofan\\Desktop\\watch:-5589877937"),
            ("C:\\Users\\bofan\\Desktop\\watch".to_string(), "-5589877937".to_string())
        );

        // Windows path with @username target
        assert_eq!(
            parse_watch_dir_input("D:\\Uploads:@my_channel"),
            ("D:\\Uploads".to_string(), "@my_channel".to_string())
        );

        // Linux path without target
        assert_eq!(
            parse_watch_dir_input("/home/user/watch"),
            ("/home/user/watch".to_string(), "me".to_string())
        );

        // Tilde path with target
        assert_eq!(
            parse_watch_dir_input("~/.magebot/savings:-1001234567890"),
            ("~/.magebot/savings".to_string(), "-1001234567890".to_string())
        );
    }
}
