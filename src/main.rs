mod config;
mod daemon;
mod downloader;
mod logger;
mod watcher;
mod crypto;
mod ipc;
mod monitor;
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
    /// Check if all necessary configuration and session authorization are correct
    Check,

    /// Set a configuration parameter
    Set {
        /// Configuration arguments in key:value or key value format
        #[arg(required = false, num_args = 0..=2)]
        args: Vec<String>,
    },

    /// Interactively login to your Telegram user account
    Login,

    /// Start the bot in the background
    Start,

    /// Stop the background bot process
    Stop,

    /// Restart the background bot process to reload configuration
    Restart,

    /// Run the bot in daemon mode (internal command)
    #[command(hide = true)]
    Daemon,

    /// Monitor the background daemon in real-time
    Monitor,

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
        Commands::Check => {
            if let Err(e) = auth::run_checks().await {
                eprintln!("❌ Check failed: {}", e);
                std::process::exit(1);
            } else {
                println!("✅ All checks passed successfully!");
            }
        }
        Commands::Set { args } => {
            if let Err(e) = setup::run_set(&args) {
                eprintln!("❌ Failed to set configuration: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Login => {
            if let Err(e) = auth::run_login().await {
                eprintln!("❌ Login failed: {}", e);
                std::process::exit(1);
            }
        }
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
        Commands::Daemon => {
            run_daemon().await;
        }
        Commands::Monitor => {
            if let Err(e) = monitor::run_monitor().await {
                eprintln!("❌ Monitor error: {}", e);
                std::process::exit(1);
            }
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

    // Keep a memory-only set of message IDs currently being processed to avoid duplicate handling at runtime.
    let mut processing_ids: std::collections::HashSet<i32> = std::collections::HashSet::new();

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
            } else if cmd_trimmed == "/help" || cmd_trimmed == "help" {
                logger::info("可用指令列表:\n  download <URL> - 下载视频链接\n  stop           - 停止守护进程\n  help           - 显示帮助菜单");
            } else if cmd_trimmed == "/stop" || cmd_trimmed == "stop" {
                logger::info("收到停止指令，正在关闭守护进程...");
                daemon::delete_pid_file();
                std::process::exit(0);
            } else {
                logger::warn(&format!("IPC: Unknown command: {}", cmd_trimmed));
            }
        }
    });

    logger::info("MTProto Update Listener starting...");

    let mut updates_stream = client.stream_updates(
        updates_rx,
        grammers_client::UpdatesConfiguration {
            catch_up: true,
            update_queue_limit: None,
        },
    );

    loop {
        let update = match updates_stream.next().await {
            Ok(up) => up,
            Err(e) => {
                logger::error(&format!("Update error: {}", e));
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        match update {
            grammers_client::Update::NewMessage(message) => {
                // In Saved Messages, chat ID is our own user ID
                if message.peer_id() == my_id {
                    let text_str = message.text().to_string();
                    let text_lower = text_str.to_lowercase();
                    if downloader::is_video_url(&text_str)
                        && !text_lower.contains("[uploaded]")
                        && !text_lower.contains("[已上传]")
                        && !text_lower.contains("[failed]")
                        && !text_lower.contains("[已失败]")
                    {
                        let msg_id = message.id();
                        logger::info(&format!("识别到视频链接 (消息 ID: {})", msg_id));
                        if processing_ids.contains(&msg_id) {
                            logger::info(&format!("链接已在处理中，跳过 (ID: {})", msg_id));
                        } else {
                            processing_ids.insert(msg_id);
                            
                            let urls: Vec<String> = text_str
                                .split_whitespace()
                                .filter(|word| downloader::is_video_url(word))
                                .map(|w| w.to_string())
                                .collect();

                            for url in urls {
                                let client_clone = client.clone();
                                let config_clone = config.clone();
                                let target_chat = match message.peer() {
                                    Ok(peer) => PeerRef::from(peer),
                                    Err(peer_ref) => peer_ref,
                                };
                                tokio::spawn(async move {
                                    downloader::handle_video_download(client_clone, config_clone, target_chat, url, msg_id).await;
                                });
                            }
                        }
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
        let args = vec!["api_hash:[abcdef]".to_string()];
        let res = setup::parse_key_value(&args).unwrap();
        assert_eq!(res.0, "api_hash");
        assert_eq!(res.1, "abcdef");
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
        assert!(!is_video_url("https://google.com"));
    }
}
