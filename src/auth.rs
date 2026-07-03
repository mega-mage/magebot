use std::sync::Arc;
use std::path::PathBuf;
use grammers_client::{Client, SignInError};
use grammers_session::storages::SqliteSession;
use grammers_mtsender::SenderPool;
use crate::config::Config;
use crate::setup::read_line;

pub async fn get_client(config: &Config) -> Result<Client, String> {
    let (client, mut updates) = get_client_with_updates(config).await?;
    tokio::spawn(async move {
        while let Some(_) = updates.recv().await {}
    });
    Ok(client)
}

pub async fn get_client_with_updates(config: &Config) -> Result<(Client, tokio::sync::mpsc::UnboundedReceiver<grammers_session::updates::UpdatesLike>), String> {
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

fn expand_tilde(path_str: &str) -> PathBuf {
    if path_str.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let mut s = &path_str[1..];
            if s.starts_with('/') || s.starts_with('\\') {
                s = &s[1..];
            }
            home.join(s)
        } else {
            PathBuf::from(path_str)
        }
    } else {
        PathBuf::from(path_str)
    }
}

pub async fn run_checks() -> Result<(), String> {
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
    
    let has_watch = config.watch_dir.is_some() 
        || config.watch_rules.as_ref().map(|r| !r.is_empty()).unwrap_or(false);
    if !has_watch {
        missing.push("watch_dir (或 watch_rules)");
    }

    if !missing.is_empty() {
        return Err(format!("Missing configuration fields: {}", missing.join(", ")));
    }

    // Check watch_dir exists or create warning
    if let Some(ref watch_dir) = config.watch_dir {
        let watch_path = expand_tilde(watch_dir);
        if !watch_path.exists() {
            println!("Warning: watch_dir ({}) does not exist yet. It will be created when the bot starts.", watch_dir);
        } else if !watch_path.is_dir() {
            return Err(format!("watch_dir ({}) exists but is not a directory", watch_dir));
        }
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

pub async fn run_login() -> Result<(), String> {
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
    let mut phone_changed = false;
    let phone_number = match &config.phone_number {
        Some(phone) => {
            println!("Enter your phone number (default: {}):", phone);
            let input = read_line("> ")?;
            if input.is_empty() {
                phone.clone()
            } else {
                if &input != phone {
                    phone_changed = true;
                }
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

    if phone_changed {
        let session_path = Config::get_session_path();
        if session_path.exists() {
            let _ = std::fs::remove_file(&session_path);
            println!("ℹ️ 手机号码与原先登入的不一致，已清理旧的登入会话文件：{:?}", session_path);
        }
    }

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
