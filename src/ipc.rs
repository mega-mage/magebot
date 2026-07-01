use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TaskType {
    Download,
    Upload,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TaskStatus {
    Pending,
    Downloading {
        progress: f64,
        speed: String,
        eta: String,
    },
    Uploading {
        progress: f64,
        speed: String,
        eta: String,
    },
    Completed,
    Failed(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TaskState {
    pub id: String,
    pub task_type: TaskType,
    pub filename: String,
    pub status: TaskStatus,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum IpcMessage {
    // Sent from Daemon -> Monitor
    StateUpdate(Vec<TaskState>),
    LogReceived(String),

    // Sent from Monitor -> Daemon
    Command(String),
}

type ClientSender = tokio::sync::mpsc::UnboundedSender<String>;

static CLIENTS: LazyLock<Mutex<Vec<ClientSender>>> = LazyLock::new(|| Mutex::new(Vec::new()));
pub static ACTIVE_TASKS: LazyLock<Mutex<HashMap<String, TaskState>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
pub static RECENT_LOGS: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn update_task(id: &str, task_type: TaskType, filename: &str, status: TaskStatus) {
    let now = chrono::Local::now().format("%H:%M:%S").to_string();
    let state = TaskState {
        id: id.to_string(),
        task_type,
        filename: filename.to_string(),
        status,
        updated_at: now,
    };

    {
        let mut tasks = ACTIVE_TASKS.lock().unwrap();
        tasks.insert(id.to_string(), state);
    }

    broadcast_state();
}

pub fn remove_task(id: &str) {
    {
        let mut tasks = ACTIVE_TASKS.lock().unwrap();
        tasks.remove(id);
    }
    broadcast_state();
}

fn broadcast_state() {
    let tasks_vec: Vec<TaskState> = {
        let tasks = ACTIVE_TASKS.lock().unwrap();
        tasks.values().cloned().collect()
    };

    let msg = IpcMessage::StateUpdate(tasks_vec);
    if let Ok(serialized) = serde_json::to_string(&msg) {
        broadcast_string(serialized);
    }
}

pub fn broadcast_log(log_line: String) {
    // Keep last 100 logs in buffer
    {
        let mut logs = RECENT_LOGS.lock().unwrap();
        logs.push(log_line.clone());
        if logs.len() > 100 {
            logs.remove(0);
        }
    }

    let msg = IpcMessage::LogReceived(log_line);
    if let Ok(serialized) = serde_json::to_string(&msg) {
        broadcast_string(serialized);
    }
}

fn broadcast_string(data: String) {
    let mut clients = CLIENTS.lock().unwrap();
    clients.retain(|client| {
        client.send(format!("{}\n", data)).is_ok()
    });
}

pub async fn start_ipc_server(cmd_tx: tokio::sync::mpsc::UnboundedSender<String>) {
    // Register the log broadcaster
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let mut guard = crate::logger::LOG_BROADCASTER.lock().unwrap();
        *guard = Some(log_tx);
    }

    // Spawn task to handle incoming log broadcasts from logger::log_msg
    tokio::spawn(async move {
        while let Some(log_line) = log_rx.recv().await {
            broadcast_log(log_line);
        }
    });

    let listener = match TcpListener::bind("127.0.0.1:42424").await {
        Ok(l) => l,
        Err(e) => {
            crate::logger::error(&format!("IPC Server: Failed to bind port 42424: {}", e));
            return;
        }
    };

    crate::logger::info("IPC Server started on 127.0.0.1:42424");

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                let cmd_tx_clone = cmd_tx.clone();
                tokio::spawn(async move {
                    handle_client(socket, cmd_tx_clone).await;
                });
            }
            Err(e) => {
                crate::logger::warn(&format!("IPC Server: accept error: {}", e));
            }
        }
    }
}

async fn handle_client(mut socket: TcpStream, cmd_tx: tokio::sync::mpsc::UnboundedSender<String>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    
    // Add client sender to global list
    {
        let mut clients = CLIENTS.lock().unwrap();
        clients.push(tx);
    }

    // Send initial state & recent logs to the newly connected client
    {
        let tasks_vec = {
            let tasks = ACTIVE_TASKS.lock().unwrap();
            tasks.values().cloned().collect::<Vec<TaskState>>()
        };
        let msg = IpcMessage::StateUpdate(tasks_vec);
        if let Ok(serialized) = serde_json::to_string(&msg) {
            let _ = socket.write_all(format!("{}\n", serialized).as_bytes()).await;
        }

        let logs_clone = {
            let logs = RECENT_LOGS.lock().unwrap();
            logs.clone()
        };
        for log_line in logs_clone {
            let msg = IpcMessage::LogReceived(log_line);
            if let Ok(serialized) = serde_json::to_string(&msg) {
                let _ = socket.write_all(format!("{}\n", serialized).as_bytes()).await;
            }
        }
    }

    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    // Spawn a task to send updates to this client
    let rx_task = tokio::spawn(async move {
        while let Some(msg_str) = rx.recv().await {
            if writer.write_all(msg_str.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // Handle incoming commands from this client
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                if let Ok(IpcMessage::Command(cmd_str)) = serde_json::from_str::<IpcMessage>(&line.trim()) {
                    let _ = cmd_tx.send(cmd_str);
                }
            }
            Err(_) => break,
        }
    }

    rx_task.abort();
}
