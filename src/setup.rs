use std::io::{self, Write};
use crate::config::Config;
use crate::crypto;

pub fn read_line(prompt: &str) -> Result<String, String> {
    print!("{}", prompt);
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
    Ok(input.trim().to_string())
}

pub fn read_multiline(prompt: &str) -> Result<String, String> {
    println!("{}", prompt);
    println!("(请输入或粘贴内容，在独立新行中输入 'EOF' 并回车结束)：");
    io::stdout().flush().map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
        let trimmed = input.trim();
        if trimmed == "EOF" {
            break;
        }
        lines.push(input);
    }
    Ok(lines.join("").trim().to_string())
}

pub fn parse_key_value(args: &[String]) -> Result<(String, String), String> {
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

pub fn print_set_help() {
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
  watch_rule     自定义监控目录和目标群组 (格式: "magebot set watch_rule <目录路径>:<目标群组ID或用户名>")
  del_watch_rule 删除指定监控目录规则 (格式: "magebot set del_watch_rule <目录路径>")
  max_concurrent_uploads 限制最大同时上传的文件数量 (例如: "magebot set max_concurrent_uploads 2")
"#);
}

pub fn run_interactive_set_cookie() -> Result<(), String> {
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

    let cookie_content = read_multiline(&format!("\n请准备输入或粘贴平台 '{}' 的 Cookie 串 / Netscape 格式 / JSON 数组格式：", platform))?;
    if cookie_content.is_empty() {
        return Err("Cookie 内容不能为空".to_string());
    }

    // Encrypt
    let key = crypto::get_encryption_key()?;
    let encrypted = crypto::encrypt_cookie(&cookie_content, &key)?;

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

pub fn run_set(args: &[String]) -> Result<(), String> {
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
        "watch_rule" => {
            if let Some((path, target)) = clean_val.rsplit_once(':') {
                let path = path.trim().to_string();
                let target = target.trim().to_string();
                if path.is_empty() || target.is_empty() {
                    return Err("Format must be path:target".to_string());
                }
                let mut rules = config.watch_rules.unwrap_or_default();
                rules.insert(path.clone(), target.clone());
                config.watch_rules = Some(rules);
                
                config.save().map_err(|e| format!("Failed to save config: {}", e))?;
                println!("Successfully added watch rule: '{}' -> '{}'.", path, target);
                return Ok(());
            } else {
                return Err("Format must be watch_rule <path>:<target> (e.g., watch_rule \"C:\\path:-1001234567890\")".to_string());
            }
        }
        "del_watch_rule" => {
            let path = clean_val.trim().to_string();
            let mut removed = false;
            if let Some(ref mut rules) = config.watch_rules {
                if rules.remove(&path).is_some() {
                    removed = true;
                }
            }
            if removed {
                config.save().map_err(|e| format!("Failed to save config: {}", e))?;
                println!("Successfully removed watch rule for path: '{}'.", path);
                return Ok(());
            } else {
                return Err(format!("No watch rule found for path: '{}'.", path));
            }
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
        "max_concurrent_uploads" => {
            let limit = clean_val.parse::<usize>().map_err(|_| "max_concurrent_uploads must be a positive integer")?;
            config.max_concurrent_uploads = Some(limit);
        }
        _ => {
            return Err(format!(
                "Unknown configuration key: '{}'. Valid keys: api_id, api_hash, phone_number, watch_dir, auto_delete, download_dir, yt_dlp_path, yt_dlp_args, watch_rule, del_watch_rule, max_concurrent_uploads",
                key
            ));
        }
    }

    config.save().map_err(|e| format!("Failed to save config: {}", e))?;
    println!("Successfully set '{}' to '{}'.", key, clean_val);
    Ok(())
}
