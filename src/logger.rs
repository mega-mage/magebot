use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub static LOG_BROADCASTER: LazyLock<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn get_log_path() -> PathBuf {
    if let Some(mut home) = dirs::home_dir() {
        home.push(".magebot");
        home.push("magebot.log");
        home
    } else {
        PathBuf::from("magebot.log")
    }
}

pub fn log_msg(level: &str, msg: &str) {
    let path = get_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{}] [{}] {}", timestamp, level, msg);
    }
    // Also print to stdout/stderr in case we are running in foreground
    let _ = writeln!(std::io::stdout(), "[{}] {}", level, msg);

    // Broadcast log to monitor clients
    let formatted = format!("[{}] [{}] {}", timestamp, level, msg);
    if let Ok(guard) = LOG_BROADCASTER.lock() {
        if let Some(tx) = &*guard {
            let _ = tx.send(formatted);
        }
    }
}

pub fn info(msg: &str) {
    log_msg("INFO", msg);
}

pub fn error(msg: &str) {
    log_msg("ERROR", msg);
}

pub fn warn(msg: &str) {
    log_msg("WARN", msg);
}
