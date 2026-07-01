mod config;
mod daemon;
mod downloader;
mod logger;
mod watcher;
mod crypto;
mod ipc;
mod monitor;

use clap::{Parser, Subcommand};
use config::Config;
use grammers_client::{Client, SignInError};
use grammers_session::storages::SqliteSession;
use grammers_session::defs::{PeerId, PeerRef};
use grammers_mtsender::SenderPool;
use std::sync::Arc;
use std::io::{self, Write};

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
            if let Err(e) = run_checks().await {
                eprintln!("❌ Check failed: {}", e);
                std::process::exit(1);
            } else {
                println!("✅ All checks passed successfully!");
            }
        }
        Commands::Set { args } => {
            if let Err(e) = run_set(&args) {
                eprintln!("❌ Failed to set configuration: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Login => {
            if let Err(e) = run_login().await {
                eprintln!("❌ Login failed: {}", e);
                std::process::exit(1);
            }
        }
        Commands::Start => {
            // 1. Run checks first
            println!("Running configuration checks...");
            if let Err(e) = run_checks().await {
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
            if let Err(e) = run_checks().await {
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
            let client = match get_client(&config).await {
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

async fn get_client(config: &Config) -> Result<Client, String> {
    let (client, mut updates) = get_client_with_updates(config).await?;
    tokio::spawn(async move {
        while let Some(_) = updates.recv().await {}
    });
    Ok(client)
}

async fn get_client_with_updates(config: &Config) -> Result<(Client, tokio::sync::mpsc::UnboundedReceiver<grammers_session::updates::UpdatesLike>), String> {
    let api_id = config.api_id.ok_or_else(|| "api_id is not set. Use `magebot set api_id <id>`".to_string())?;
    let _api_hash = config.api_hash.as_ref().ok_or_else(|| "api_hash is not set. Use `magebot set api_hash <hash>`".to_string())?;
    let session_path = Config::get_session_path();

    let mut session = None;
    let mut attempts = 0;
    while attempts < 10 {
        match SqliteSession::open(&session_path) {
            Ok(s) => {
                session = Some(s);
                break;
            }
            Err(e) => {
                attempts += 1;
                if attempts >= 10 {
                    return Err(format!("Failed to load or create session file after 10 attempts: {}", e));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    let session = session.unwrap();

    let pool = SenderPool::new(Arc::new(session), api_id);
    let client = Client::new(&pool);
    let runner = pool.runner;
    tokio::spawn(async move {
        runner.run().await;
    });

    Ok((client, pool.updates))
}

async fn run_checks() -> Result<(), String> {
    let config = Config::load();
    let mut missing = Vec::new();

    if config.api_id.is_none() {
        missing.push("api_id");
    }
    if config.api_hash.is_none() {
        missing.push("api_hash");
    }
    if config.phone_number.is_none() {
        missing.push("phone_number");
    }
    if config.watch_dir.is_none() {
        missing.push("watch_dir");
    }

    if !missing.is_empty() {
        return Err(format!("Missing configuration fields: {}", missing.join(", ")));
    }

    // Check watch_dir exists or create warning
    let watch_dir = config.watch_dir.as_ref().unwrap();
    let watch_path = std::path::Path::new(watch_dir);
    if !watch_path.exists() {
        println!("Warning: watch_dir ({}) does not exist yet. It will be created when the bot starts.", watch_dir);
    } else if !watch_path.is_dir() {
        return Err(format!("watch_dir ({}) exists but is not a directory", watch_dir));
    }

    // Check yt-dlp
    let yt_dlp = config.get_yt_dlp_path();
    let yt_dlp_check = tokio::process::Command::new(&yt_dlp)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    if yt_dlp_check.is_err() || !yt_dlp_check.unwrap().success() {
        return Err(format!(
            "yt-dlp not found or not executable. Path: {}. Please make sure it is installed and in your PATH or dependency folder.",
            yt_dlp
        ));
    }

    // Check ffmpeg (warn only)
    let ffmpeg_check = tokio::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    if ffmpeg_check.is_err() || !ffmpeg_check.unwrap().success() {
        println!("Warning: ffmpeg not found in PATH or dependency folder. yt-dlp might fail to merge high-quality video/audio streams.");
    }

    // Check if session file exists
    let session_path = Config::get_session_path();
    if !session_path.exists() {
        return Err("Session file not found. Please run `magebot login` first to sign in.".to_string());
    }

    // Test connection & authorization status
    println!("Connecting to Telegram and checking session authorization...");
    let client = get_client(&config).await?;
    let authorized = client.is_authorized().await
        .map_err(|e| format!("Failed to verify session authorization: {}", e))?;

    if !authorized {
        return Err("Session is invalid or not authorized. Please run `magebot login` to sign in again.".to_string());
    }

    let me = client.get_me().await
        .map_err(|e| format!("Failed to fetch user info: {}", e))?;
    println!("✅ Authorization checked successfully! Logged in as: {} {}", me.first_name().unwrap_or(""), me.last_name().unwrap_or(""));

    Ok(())
}

async fn run_login() -> Result<(), String> {
    let mut config = Config::load();
    let mut config_changed = false;

    // 1. Interactive prompt for api_id if not set
    let _api_id = match config.api_id {
        Some(id) => id,
        None => {
            println!("api_id is not set. Please enter your Telegram API ID (obtain from my.telegram.org):");
            let id_str = read_line("> ")?;
            let id = id_str.parse::<i32>().map_err(|_| "api_id must be a valid integer")?;
            config.api_id = Some(id);
            config_changed = true;
            id
        }
    };

    // 2. Interactive prompt for api_hash if not set
    let api_hash = match &config.api_hash {
        Some(hash) => hash.clone(),
        None => {
            println!("api_hash is not set. Please enter your Telegram API Hash:");
            let hash = read_line("> ")?;
            if hash.is_empty() {
                return Err("api_hash cannot be empty".to_string());
            }
            config.api_hash = Some(hash.clone());
            config_changed = true;
            hash
        }
    };

    // 3. Interactive prompt for phone_number
    let phone_number = match &config.phone_number {
        Some(phone) => {
            println!("Enter your phone number (default: {}):", phone);
            let input = read_line("> ")?;
            if input.is_empty() {
                phone.clone()
            } else {
                config.phone_number = Some(input.clone());
                config_changed = true;
                input
            }
        }
        None => {
            println!("phone_number is not set. Please enter your phone number (e.g. +8613800000000):");
            let input = read_line("> ")?;
            if input.is_empty() {
                return Err("phone_number cannot be empty".to_string());
            }
            config.phone_number = Some(input.clone());
            config_changed = true;
            input
        }
    };

    // Save configuration changes if any parameter was updated interactively
    if config_changed {
        config.save().map_err(|e| format!("Failed to save config: {}", e))?;
        println!("✅ Configuration updated successfully.");
    }

    // 4. Initialize client
    let client = get_client(&config).await?;

    if client.is_authorized().await.map_err(|e| format!("Authorization check failed: {}", e))? {
        println!("✅ You are already authorized!");
        return Ok(());
    }

    println!("Requesting login code for {}...", phone_number);

    let login_token = client.request_login_code(&phone_number, &api_hash).await
        .map_err(|e| format!("Failed to request login code: {}", e))?;

    let code = read_line("Enter the login code you received: ")?;

    let sign_in_res = client.sign_in(&login_token, &code).await;

    match sign_in_res {
        Ok(user) => {
            println!("✅ Login successful! Welcome, {}", user.first_name().unwrap_or(""));
        }
        Err(SignInError::PasswordRequired(password_token)) => {
            println!("Two-factor authentication (2FA) is enabled.");
            let password = read_line("Enter your password: ")?;

            let user = client.check_password(password_token, &password).await
                .map_err(|e| format!("Failed to check 2FA password: {}", e))?;

            println!("✅ Login successful! Welcome, {}", user.first_name().unwrap_or(""));
        }
        Err(e) => {
            return Err(format!("Sign in failed: {}", e));
        }
    }

    Ok(())
}



fn read_line(prompt: &str) -> Result<String, String> {
    print!("{}", prompt);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
    Ok(input.trim().to_string())
}

fn parse_key_value(args: &[String]) -> Result<(String, String), String> {
    if args.is_empty() {
        return Err("Usage: magebot set <key>:<value> OR magebot set <key> <value>".to_string());
    }

    let (key, value) = if args.len() == 1 {
        let arg = &args[0];
        let parts: Vec<&str> = arg.splitn(2, ':').collect();
        if parts.len() < 2 {
            return Err("Invalid format. Use key:value or key value".to_string());
        }
        (parts[0].trim().to_string(), parts[1].trim().to_string())
    } else {
        (args[0].trim().to_string(), args[1].trim().to_string())
    };

    let mut clean_val = value;
    if clean_val.starts_with('[') && clean_val.ends_with(']') {
        clean_val = clean_val[1..clean_val.len() - 1].to_string();
    }

    Ok((key, clean_val))
}

fn print_set_help() {
    println!(r#"Magebot 参数设置命令 (set)

使用方法:
  magebot set <参数名>:<值>      (例如: magebot set api_id:123456)
  magebot set <参数名>:[值]      (例如: magebot set api_hash:[abcdef])
  magebot set <参数名> <值>      (例如: magebot set phone_number "+86138xxxx")
  magebot set cookie             (交互式设置平台的 Cookie，进行加密保存)

必填参数:
  api_id         Telegram API ID (在 my.telegram.org 申请)。
  api_hash       Telegram API Hash (在 my.telegram.org 申请)。
  phone_number   您的登录手机号 (需要包含国际区号，例如 +8613800000000)。
  watch_dir      监控的文件夹路径，新写入完成的文件会自动上传到您的“收藏夹 (Saved Messages)”。

可选参数:
  auto_delete    上传成功后是否自动删除本地文件。
                 可选值: true/false, 1/0, yes/no (默认: false，不删除则重命名为 .uploaded)
  download_dir   视频临时下载目录 (默认: ~/.magebot/downloads)
  yt_dlp_path    yt-dlp 可执行程序路径 (默认: 在系统环境变量 PATH 中寻找 "yt-dlp")
  yt_dlp_args    yt-dlp 额外自定义参数 (例如: "--cookies-from-browser chrome" 用于规避机器人验证)
"#);
}

fn run_interactive_set_cookie() -> Result<(), String> {
    println!("--- Magebot 交互式 Cookie 设置 ---");
    println!("请选择平台:");
    println!("1) youtube");
    println!("2) bilibili");
    println!("3) twitter");
    println!("4) 自定义平台名称");

    let choice = read_line("请输入选项 (1-4): ")?;
    let platform = match choice.as_str() {
        "1" => "youtube".to_string(),
        "2" => "bilibili".to_string(),
        "3" => "twitter".to_string(),
        "4" => {
            let custom = read_line("请输入自定义平台名称 (小写，例如 tiktok): ")?;
            if custom.is_empty() {
                return Err("平台名称不能为空".to_string());
            }
            custom.to_lowercase()
        }
        _ => return Err("无效选项".to_string()),
    };

    println!("\n请输入或粘贴平台 '{}' 的 Cookie 串 / Netscape 格式文本：", platform);
    // Simplified input: press Enter to finish
    let cookie_content = read_line("Cookie: ")?;
    let cookie_content = cookie_content.trim();
    if cookie_content.is_empty() {
        return Err("Cookie 内容不能为空".to_string());
    }

    // Encrypt
    let key = crypto::get_encryption_key()?;
    let encrypted = crypto::encrypt_cookie(cookie_content, &key)?;

    // Load toml, insert, save
    let mut cookies_map = crypto::load_cookies_toml()?;
    cookies_map.insert(platform.clone(), encrypted);
    crypto::save_cookies_toml(&cookies_map)?;

    println!(
        "\n✅ 成功加密并保存平台 '{}' 的 Cookie 至 ~/.magebot/cookies.toml",
        platform
    );
    Ok(())
}

fn run_set(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        print_set_help();
        return Ok(());
    }

    if args.len() == 1 && args[0].to_lowercase() == "cookie" {
        return run_interactive_set_cookie();
    }
    let mut config = Config::load();
    let (key, clean_val) = parse_key_value(args)?;

    match key.as_str() {
        "api_id" => {
            let id = clean_val.parse::<i32>().map_err(|_| "api_id must be an integer")?;
            config.api_id = Some(id);
        }
        "api_hash" => {
            config.api_hash = Some(clean_val.clone());
        }
        "phone_number" => {
            config.phone_number = Some(clean_val.clone());
        }
        "watch_dir" => {
            config.watch_dir = Some(clean_val.clone());
        }
        "auto_delete" => {
            let val = match clean_val.to_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => true,
                "false" | "0" | "no" | "off" => false,
                _ => return Err("auto_delete must be a boolean (true/false)".to_string()),
            };
            config.auto_delete = Some(val);
        }
        "download_dir" => {
            config.download_dir = Some(clean_val.clone());
        }
        "yt_dlp_path" => {
            config.yt_dlp_path = Some(clean_val.clone());
        }
        "yt_dlp_args" => {
            config.yt_dlp_args = Some(clean_val.clone());
        }
        _ => {
            return Err(format!(
                "Unknown configuration key: '{}'. Valid keys: api_id, api_hash, phone_number, watch_dir, auto_delete, download_dir, yt_dlp_path, yt_dlp_args",
                key
            ));
        }
    }

    config.save().map_err(|e| format!("Failed to save config: {}", e))?;
    println!("Successfully set '{}' to '{}'.", key, clean_val);
    Ok(())
}

async fn run_daemon() {
    logger::info("magebot daemon process started.");
    let config = Config::load();

    // Keep a memory-only set of message IDs currently being processed to avoid duplicate handling at runtime.
    let mut processing_ids: std::collections::HashSet<i32> = std::collections::HashSet::new();

    // 1. Get client
    let (client, updates_rx) = match get_client_with_updates(&config).await {
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
        let args = vec!["api_id:123456".to_string()];
        let res = parse_key_value(&args).unwrap();
        assert_eq!(res.0, "api_id");
        assert_eq!(res.1, "123456");
    }

    #[test]
    fn test_parse_key_value_brackets() {
        let args = vec!["api_hash:[abcdef]".to_string()];
        let res = parse_key_value(&args).unwrap();
        assert_eq!(res.0, "api_hash");
        assert_eq!(res.1, "abcdef");
    }

    #[test]
    fn test_parse_key_value_space() {
        let args = vec!["watch_dir".to_string(), "j:\\my_folder".to_string()];
        let res = parse_key_value(&args).unwrap();
        assert_eq!(res.0, "watch_dir");
        assert_eq!(res.1, "j:\\my_folder");
    }

    #[test]
    fn test_parse_key_value_empty() {
        let args: Vec<String> = vec![];
        let res = parse_key_value(&args);
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
