use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub fn get_pid_path() -> PathBuf {
    if let Some(mut home) = dirs::home_dir() {
        home.push(".magebot");
        home.push("magebot.pid");
        home
    } else {
        PathBuf::from("magebot.pid")
    }
}

pub fn read_pid() -> Option<u32> {
    let path = get_pid_path();
    if !path.exists() {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    contents.trim().parse::<u32>().ok()
}

pub fn write_pid(pid: u32) -> Result<(), std::io::Error> {
    let path = get_pid_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    write!(file, "{}", pid)?;
    Ok(())
}

pub fn delete_pid_file() {
    let path = get_pid_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains(&pid.to_string())
        } else {
            false
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status();
        if let Ok(stat) = status {
            stat.success()
        } else {
            false
        }
    }
}

pub fn kill_process(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status();
        if let Ok(stat) = status {
            stat.success()
        } else {
            false
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        if let Ok(stat) = status {
            stat.success()
        } else {
            false
        }
    }
}

pub fn spawn_daemon() -> Result<u32, std::io::Error> {
    let current_exe = std::env::current_exe()?;
    let mut cmd = Command::new(current_exe);
    cmd.arg("daemon");

    // Redirect standard I/O streams
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        // CREATE_NO_WINDOW is 0x08000000
        cmd.creation_flags(0x08000000);
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    write_pid(pid)?;
    Ok(pid)
}
