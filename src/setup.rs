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

pub fn parse_watch_dir_input(input: &str) -> (String, String) {
    let trimmed = input.trim();
    if let Some((left, right)) = trimmed.rsplit_once(':') {
        let right_clean = right.trim();
        let left_clean = left.trim();
        let is_path_component = right_clean.contains('/') || right_clean.contains('\\');
        if !is_path_component && !left_clean.is_empty() {
            let is_known_target = right_clean.eq_ignore_ascii_case("me")
                || right_clean.eq_ignore_ascii_case("saved")
                || right_clean.eq_ignore_ascii_case("saved_messages")
                || right_clean.starts_with('@')
                || right_clean.parse::<i64>().is_ok();
            if is_known_target || left_clean.contains('/') || left_clean.contains('\\') {
                return (left_clean.to_string(), right_clean.to_string());
            }
        }
    }
    (trimmed.to_string(), "me".to_string())
}

pub fn print_set_help() {
    println!(r#"Magebot 参数设置命令 (set)

使用方法:
  magebot set <参数名>:<值>      (例如: magebot set auto_delete:true)
  magebot set <参数名> <值>      (例如: magebot set download_dir "/path/to/dir")
  magebot set cookie             (交互式设置平台的 Cookie，进行加密保存)

提示: 账号登录请使用 `magebot login`，监控规则请使用 `magebot add/rm/ls/listen` 命令。

可配置参数:
  auto_delete    上传成功后是否自动删除本地文件。
                 可选值: true/false, 1/0, yes/no (默认: false，不删除则重命名为 .uploaded)
  download_dir   视频临时下载目录 (默认: ~/.magebot/downloads)
  yt_dlp_path    yt-dlp 可执行程序路径 (默认: 在系统环境变量 PATH 中寻找 "yt-dlp")
  yt_dlp_args    yt-dlp 额外自定义参数 (例如: "--cookies-from-browser chrome" 用于规避机器人验证)
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
        "api_id" | "api_hash" | "phone_number" => {
            return Err(format!(
                "'{}' 不能通过 'set' 命令设置。请使用 `magebot login` 交互式配置。",
                key
            ));
        }
        "watch_dir" | "watch_rule" | "del_watch_dir" | "del_watch_rule" => {
            return Err(format!(
                "'{}' 已迁移。请使用 `magebot add <路径>[:<目标>]` 或 `magebot rm <ID或路径>` 命令。",
                key
            ));
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
                "未知参数: '{}'\n可用参数: auto_delete, download_dir, yt_dlp_path, yt_dlp_args, max_concurrent_uploads\n设置 Cookie: magebot set cookie\n监控规则: magebot add/rm/ls/listen",
                key
            ));
        }
    }

    config.save().map_err(|e| format!("Failed to save config: {}", e))?;
    println!("✅ 成功设置 '{}' = '{}'", key, clean_val);
    Ok(())
}

pub fn run_add(rule_input: &str) -> Result<(), String> {
    let (path, target) = parse_watch_dir_input(rule_input);
    if path.is_empty() {
        return Err("监控目录路径不能为空".to_string());
    }
    let mut config = Config::load();
    let rule = config.add_or_update_watch_rule(path, target);
    config.save().map_err(|e| format!("Failed to save config: {}", e))?;
    println!(
        "✅ 成功添加/更新监控规则 [ID: {}]: '{}' → '{}' (listen_media: {})",
        rule.id, rule.path, rule.target, rule.listen_media
    );
    Ok(())
}

pub fn run_rm(target: &str) -> Result<(), String> {
    let mut config = Config::load();

    // Try parsing as numeric ID first
    if let Ok(id) = target.parse::<usize>() {
        let rules = config.get_watch_rules();
        if let Some(rule) = rules.iter().find(|r| r.id == id) {
            let path = rule.path.clone();
            let removed = config.remove_watch_rule(&path);
            if removed {
                config.save().map_err(|e| format!("Failed to save config: {}", e))?;
                println!("✅ 成功删除监控规则 [ID: {}]: '{}'", id, path);
                return Ok(());
            }
        }
        return Err(format!("未找到 ID 为 {} 的监控规则", id));
    }

    // Otherwise try as path
    let removed = config.remove_watch_rule(target);
    if removed {
        config.save().map_err(|e| format!("Failed to save config: {}", e))?;
        println!("✅ 成功删除监控规则: '{}'", target);
        Ok(())
    } else {
        Err(format!("未找到路径为 '{}' 的监控规则", target))
    }
}

pub fn run_ls() {
    let config = Config::load();
    let rules = config.get_watch_rules();
    let global_auto_delete = config.auto_delete.unwrap_or(false);

    if rules.is_empty() {
        println!("ℹ️ 当前未配置任何监控规则。你可以通过 `magebot add <目录路径>` 添加。");
        println!("⚙️ 全局默认 auto_delete (自动删除本地文件): {}\n", global_auto_delete);
        return;
    }

    println!("\n======= Magebot 监控规则列表 =======");
    println!("⚙️ 全局默认 auto_delete: {}\n", global_auto_delete);
    println!(
        "{:<5} {:<35} {:<25} {:<10} {:<10}",
        "ID", "监控目录", "目标", "监听", "自动删除"
    );
    println!("{}", "─".repeat(85));
    for r in &rules {
        let auto_del = r.get_auto_delete(config.auto_delete);
        println!(
            "{:<5} {:<35} {:<25} {:<10} {:<10}",
            r.id, r.path, r.target,
            if r.listen_media { "✅" } else { "❌" },
            if auto_del { "✅" } else { "❌" }
        );
    }
    println!("{}\n", "─".repeat(85));
}

pub fn run_listen(id: usize, enabled_str: &str) -> Result<(), String> {
    let enabled = match enabled_str.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => return Err("状态值必须为布尔值 (true/false)".to_string()),
    };

    let mut config = Config::load();
    let updated = config.set_listen_media(id, enabled)?;
    config.save().map_err(|e| format!("Failed to save config: {}", e))?;

    println!(
        "✅ 规则 [ID: {}] ('{}') 媒体链接监听已设置为: {}",
        updated.id, updated.path, if updated.listen_media { "开启 ✅" } else { "关闭 ❌" }
    );
    Ok(())
}

