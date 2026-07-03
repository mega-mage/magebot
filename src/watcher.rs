use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Semaphore;
use tokio::fs;
use grammers_client::Client;
use grammers_session::defs::{PeerId, PeerRef};
use grammers_client::types::InputMessage;

use crate::config::Config;
use crate::logger;

struct FileState {
    last_size: u64,
    stable_ticks: u32,
}

struct ActiveRule {
    path: PathBuf,
    target_str: String,
    target_peer: Option<PeerRef>,
    auto_delete: bool,
}

// Global semaphore for controlling concurrent uploads
pub static UPLOAD_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub fn get_upload_semaphore() -> Arc<Semaphore> {
    UPLOAD_SEMAPHORE.get_or_init(|| {
        let config = crate::config::Config::load();
        let limit = config.max_concurrent_uploads.unwrap_or(3);
        logger::info(&format!("Watcher: Initializing upload semaphore with limit = {}", limit));
        Arc::new(Semaphore::new(limit))
    }).clone()
}

fn parse_peer_id(id: i64) -> PeerId {
    if id >= 0 {
        PeerId::user(id)
    } else {
        let abs_id = id.abs();
        let id_str = abs_id.to_string();
        if id_str.starts_with("100") {
            let bare_id = id_str["100".len()..].parse::<i64>().unwrap_or(abs_id);
            PeerId::channel(bare_id)
        } else {
            PeerId::chat(abs_id)
        }
    }
}

pub async fn resolve_peer(client: &Client, target: &str) -> Result<PeerRef, String> {
    if target.eq_ignore_ascii_case("me") || target.eq_ignore_ascii_case("saved") || target.eq_ignore_ascii_case("saved_messages") {
        let me = client.get_me().await.map_err(|e| e.to_string())?;
        return Ok(PeerRef::from(grammers_client::types::Peer::User(me)));
    }

    if target.starts_with('@') {
        let username = &target[1..];
        if let Some(peer) = client.resolve_username(username).await.map_err(|e| e.to_string())? {
            return Ok(PeerRef::from(peer));
        }
        if let Some(peer) = client.resolve_username(target).await.map_err(|e| e.to_string())? {
            return Ok(PeerRef::from(peer));
        }
        return Err(format!("Username not found: {}", target));
    }

    if let Ok(id) = target.parse::<i64>() {
        let target_peer_id = parse_peer_id(id);
        let mut dialogs = client.iter_dialogs();
        while let Some(dialog) = dialogs.next().await.map_err(|e| e.to_string())? {
            let peer = dialog.peer;
            if peer.id() == target_peer_id {
                return Ok(PeerRef::from(peer));
            }
        }
        return Err(format!("Chat ID {} not found in dialogs.", id));
    }

    if let Some(peer) = client.resolve_username(target).await.map_err(|e| e.to_string())? {
        return Ok(PeerRef::from(peer));
    }

    Err(format!("Could not resolve target: {}", target))
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

async fn resume_pending_downloads(client: Client, download_dir: PathBuf) {
    if !download_dir.exists() {
        return;
    }

    let mut dir_entries = match fs::read_dir(&download_dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = dir_entries.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => ext.to_lowercase(),
            None => continue,
        };
        if ext == "meta" || ext == "uploaded" || path.file_name().and_then(|n| n.to_str()).unwrap_or("").starts_with('.') {
            continue;
        }

        let meta_path = path.with_extension(format!("{}.meta", ext));
        if !meta_path.exists() {
            continue;
        }

        let meta_content = match fs::read_to_string(&meta_path).await {
            Ok(content) => content,
            Err(_) => continue,
        };

        #[derive(serde::Deserialize)]
        struct DownloadMeta {
            url: String,
            target: String,
        }

        let meta: DownloadMeta = match serde_json::from_str(&meta_content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        logger::info(&format!(
            "Watcher: Found leftover downloaded file {:?}. Resuming upload to target '{}'...",
            path, meta.target
        ));

        let target_chat = match resolve_peer(&client, &meta.target).await {
            Ok(peer) => peer,
            Err(e) => {
                logger::error(&format!(
                    "Watcher: Failed to resolve target '{}' for leftover file {:?}: {}",
                    meta.target, path, e
                ));
                continue;
            }
        };

        let upload_filename = path.file_name().unwrap().to_string_lossy().to_string();
        let upload_task_id = format!("ul_resume_{}", upload_filename);
        let file_size = fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);

        crate::ipc::update_task(
            &upload_task_id,
            crate::ipc::TaskType::Upload,
            &upload_filename,
            crate::ipc::TaskStatus::Pending,
        );

        let client_up = client.clone();
        let path_up = path.clone();
        let meta_path_up = meta_path.clone();
        let target_chat_up = target_chat.clone();
        let upload_task_id_spawn = upload_task_id.clone();
        let upload_filename_spawn = upload_filename.clone();
        let target_str = meta.target.clone();
        let url_str = meta.url.clone();

        tokio::spawn(async move {
            let semaphore = get_upload_semaphore();
            let _permit = semaphore.acquire().await;

            logger::info(&format!("Watcher: Upload queue slot acquired for leftover {:?}", path_up));

            crate::ipc::update_task(
                &upload_task_id_spawn,
                crate::ipc::TaskType::Upload,
                &upload_filename_spawn,
                crate::ipc::TaskStatus::Uploading {
                    progress: 0.0,
                    speed: "2.4 MiB/s".to_string(),
                    eta: "--s".to_string(),
                },
            );

            let upload_task_id_clone = upload_task_id_spawn.clone();
            let upload_filename_clone = upload_filename_spawn.clone();
            let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

            tokio::spawn(async move {
                let speed_bytes = 2_500_000_u64;
                let mut elapsed = 0.0;
                let tick_sec = 0.5;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                            elapsed += tick_sec;
                            let uploaded = (elapsed * speed_bytes as f64) as u64;
                            let progress = if file_size > 0 {
                                ((uploaded as f64 / file_size as f64) * 100.0).min(95.0)
                            } else {
                                95.0
                            };
                            let speed_str = "2.4 MiB/s".to_string();
                            let eta_str = if file_size > uploaded {
                                format!("{}s", ((file_size - uploaded) / speed_bytes).max(1))
                            } else {
                                "1s".to_string()
                            };

                            crate::ipc::update_task(
                                &upload_task_id_clone,
                                crate::ipc::TaskType::Upload,
                                &upload_filename_clone,
                                crate::ipc::TaskStatus::Uploading {
                                    progress,
                                    speed: speed_str,
                                    eta: eta_str,
                                },
                            );
                        }
                        _ = &mut cancel_rx => {
                            break;
                        }
                    }
                }
            });

            let mut upload_result = Err(std::io::Error::new(std::io::ErrorKind::Other, "Upload not started"));
            for attempt in 1..=3 {
                logger::info(&format!("Watcher: Uploading leftover file {:?} (attempt {})", path_up, attempt));
                let client_up_inner = client_up.clone();
                let path_up_inner = path_up.clone();
                let target_chat_up_inner = target_chat_up.clone();
                let caption_inner = format!("[Uploaded] {}", url_str);
                let res = async {
                    let uploaded = client_up_inner.upload_file(&path_up_inner).await?;
                    client_up_inner.send_message(target_chat_up_inner, InputMessage::new().file(uploaded).text(caption_inner)).await
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                }.await;

                if res.is_ok() {
                    upload_result = res;
                    break;
                } else {
                    logger::warn(&format!("Watcher: Attempt {} to upload leftover file {:?} failed: {:?}", attempt, path_up, res.unwrap_err()));
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }

            let _ = cancel_tx.send(());

            match upload_result {
                Ok(_) => {
                    logger::info(&format!(
                        "Watcher: Leftover file {:?} uploaded successfully to target '{}'",
                        path_up, target_str
                    ));

                    crate::ipc::update_task(
                        &upload_task_id_spawn,
                        crate::ipc::TaskType::Upload,
                        &upload_filename_spawn,
                        crate::ipc::TaskStatus::Completed,
                    );
                    let upload_task_id_rm = upload_task_id_spawn.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        crate::ipc::remove_task(&upload_task_id_rm);
                    });

                    let _ = fs::remove_file(&path_up).await;
                    let _ = fs::remove_file(&meta_path_up).await;
                }
                Err(e) => {
                    logger::error(&format!("Watcher: Failed to upload leftover file {:?}: {:?}", path_up, e));

                    crate::ipc::update_task(
                        &upload_task_id_spawn,
                        crate::ipc::TaskType::Upload,
                        &upload_filename_spawn,
                        crate::ipc::TaskStatus::Failed(e.to_string()),
                    );
                    let upload_task_id_rm = upload_task_id_spawn.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        crate::ipc::remove_task(&upload_task_id_rm);
                    });
                }
            }
        });
    }
}

pub async fn run_watcher(client: Client, config: Config) {
    let mut rules = Vec::new();

    // 1. Gather all watch rules
    for rule in config.get_watch_rules() {
        if !rule.path.trim().is_empty() && !rule.target.trim().is_empty() {
            let auto_del = rule.get_auto_delete(config.auto_delete);
            rules.push(ActiveRule {
                path: expand_tilde(&rule.path),
                target_str: rule.target,
                target_peer: None,
                auto_delete: auto_del,
            });
        }
    }

    if rules.is_empty() {
        logger::error("Watcher: No watch directories configured. Exiting watcher.");
        return;
    }

    let auto_delete = config.auto_delete.unwrap_or(false);
    logger::info(&format!(
        "Watcher: Monitoring started on {} directories (auto_delete: {})",
        rules.len(),
        auto_delete
    ));

    // Resume pending uploads from download_dir
    let download_dir = expand_tilde(config.get_download_dir().to_str().unwrap_or(""));
    tokio::spawn(resume_pending_downloads(client.clone(), download_dir));

    let mut file_states: HashMap<PathBuf, FileState> = HashMap::new();
    let uploading_files: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let mut scanned_files = HashSet::new();

        for rule in &mut rules {
            // Resolve target_peer if not done yet
            if rule.target_peer.is_none() {
                match resolve_peer(&client, &rule.target_str).await {
                    Ok(peer) => {
                        logger::info(&format!(
                            "Watcher: Successfully resolved target '{}' for path {:?}",
                            rule.target_str, rule.path
                        ));
                        rule.target_peer = Some(peer);
                    }
                    Err(e) => {
                        logger::warn(&format!(
                            "Watcher: Failed to resolve target '{}' for path {:?}: {}. Retrying next tick.",
                            rule.target_str, rule.path, e
                        ));
                        continue;
                    }
                }
            }

            let target_chat = rule.target_peer.as_ref().unwrap().clone();

            // Ensure path exists
            if !rule.path.exists() {
                logger::warn(&format!(
                    "Watcher: Directory {:?} does not exist. Creating it...",
                    rule.path
                ));
                if let Err(e) = fs::create_dir_all(&rule.path).await {
                    logger::error(&format!("Watcher: Failed to create directory {:?}: {}", rule.path, e));
                    continue;
                }
            }

            // Perform directory scan
            let mut dir_entries = match fs::read_dir(&rule.path).await {
                Ok(entries) => entries,
                Err(e) => {
                    logger::error(&format!("Watcher: Failed to read directory {:?}: {}", rule.path, e));
                    continue;
                }
            };

            while let Ok(Some(entry)) = dir_entries.next_entry().await {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                // If this file is currently uploading, skip it
                if uploading_files.lock().unwrap().contains(&path) {
                    continue;
                }

                // Exclude hidden files
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name.ends_with(".uploaded") {
                        continue;
                    }
                }

                // Get metadata
                let metadata = match fs::metadata(&path).await {
                    Ok(meta) => meta,
                    Err(_) => continue,
                };

                let size = metadata.len();
                scanned_files.insert(path.clone());

                let state = file_states.entry(path.clone()).or_insert(FileState {
                    last_size: size,
                    stable_ticks: 0,
                });

                if state.last_size == size && size > 0 {
                    state.stable_ticks += 1;
                } else {
                    state.last_size = size;
                    state.stable_ticks = 0;
                }

                // If file size is stable for 2 ticks (approx 10s), it's ready to upload
                if state.stable_ticks >= 2 {
                    logger::info(&format!("Watcher: File ready to upload: {:?}", path));
                    let upload_filename = path.file_name().unwrap().to_string_lossy().to_string();
                    let upload_task_id = format!("ul_watch_{}", upload_filename);
                    let file_size = fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);

                    // 1. Mark file as currently uploading to prevent duplicate tasks
                    uploading_files.lock().unwrap().insert(path.clone());

                    // 2. Register task as Pending (queued) in IPC
                    crate::ipc::update_task(
                        &upload_task_id,
                        crate::ipc::TaskType::Upload,
                        &upload_filename,
                        crate::ipc::TaskStatus::Pending,
                    );

                    // 3. Spawn asynchronous upload handler
                    let client_up = client.clone();
                    let path_up = path.clone();
                    let target_chat_up = target_chat.clone();
                    let upload_task_id_spawn = upload_task_id.clone();
                    let upload_filename_spawn = upload_filename.clone();
                    let rule_target_str = rule.target_str.clone();
                    let rule_auto_delete = rule.auto_delete;
                    let uploading_files_clone = uploading_files.clone();

                    tokio::spawn(async move {
                        // Wait for a slot in the semaphore queue
                        let semaphore = get_upload_semaphore();
                        let _permit = semaphore.acquire().await;

                        logger::info(&format!("Watcher: Upload queue slot acquired for {:?}", path_up));

                        // Update status to Uploading
                        crate::ipc::update_task(
                            &upload_task_id_spawn,
                            crate::ipc::TaskType::Upload,
                            &upload_filename_spawn,
                            crate::ipc::TaskStatus::Uploading {
                                progress: 0.0,
                                speed: "2.4 MiB/s".to_string(),
                                eta: "--s".to_string(),
                            },
                        );

                        // Spawn simulation task
                        let upload_task_id_clone = upload_task_id_spawn.clone();
                        let upload_filename_clone = upload_filename_spawn.clone();
                        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

                        tokio::spawn(async move {
                            let speed_bytes = 2_500_000_u64; // ~2.4 MiB/s
                            let mut elapsed = 0.0;
                            let tick_sec = 0.5;
                            loop {
                                tokio::select! {
                                    _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                                        elapsed += tick_sec;
                                        let uploaded = (elapsed * speed_bytes as f64) as u64;
                                        let progress = if file_size > 0 {
                                            ((uploaded as f64 / file_size as f64) * 100.0).min(95.0)
                                        } else {
                                            95.0
                                        };
                                        let speed_str = "2.4 MiB/s".to_string();
                                        let eta_str = if file_size > uploaded {
                                            format!("{}s", ((file_size - uploaded) / speed_bytes).max(1))
                                        } else {
                                            "1s".to_string()
                                        };

                                        crate::ipc::update_task(
                                            &upload_task_id_clone,
                                            crate::ipc::TaskType::Upload,
                                            &upload_filename_clone,
                                            crate::ipc::TaskStatus::Uploading {
                                                progress,
                                                speed: speed_str,
                                                eta: eta_str,
                                            },
                                        );
                                    }
                                    _ = &mut cancel_rx => {
                                        break;
                                    }
                                }
                            }
                        });

                        // Upload file
                        let upload_result = async {
                            let uploaded = client_up.upload_file(&path_up).await?;
                            client_up.send_message(target_chat_up, InputMessage::new().file(uploaded)).await
                                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                        }.await;

                        let _ = cancel_tx.send(());

                        match upload_result {
                            Ok(_) => {
                                logger::info(&format!(
                                    "Watcher: Uploaded successfully via MTProto to target '{}': {:?}",
                                    rule_target_str, path_up
                                ));

                                crate::ipc::update_task(
                                    &upload_task_id_spawn,
                                    crate::ipc::TaskType::Upload,
                                    &upload_filename_spawn,
                                    crate::ipc::TaskStatus::Completed,
                                );
                                let upload_task_id_rm = upload_task_id_spawn.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    crate::ipc::remove_task(&upload_task_id_rm);
                                });

                                if rule_auto_delete {
                                    if let Err(e) = fs::remove_file(&path_up).await {
                                        logger::error(&format!("Watcher: Failed to delete file {:?}: {}", path_up, e));
                                    } else {
                                        logger::info(&format!("Watcher: Deleted local file: {:?}", path_up));
                                    }
                                } else {
                                    let mut new_path = path_up.clone();
                                    if let Some(ext) = path_up.extension().and_then(|e| e.to_str()) {
                                        new_path.set_extension(format!("{}.uploaded", ext));
                                    } else {
                                        new_path.set_extension("uploaded");
                                    }

                                    if let Err(e) = fs::rename(&path_up, &new_path).await {
                                        logger::error(&format!(
                                            "Watcher: Failed to rename file {:?} -> {:?}: {}",
                                            path_up, new_path, e
                                        ));
                                    } else {
                                        logger::info(&format!("Watcher: Marked file as uploaded: {:?}", new_path));
                                    }
                                }
                            }
                            Err(e) => {
                                logger::error(&format!("Watcher: Failed to upload file {:?}: {:?}", path_up, e));

                                crate::ipc::update_task(
                                    &upload_task_id_spawn,
                                    crate::ipc::TaskType::Upload,
                                    &upload_filename_spawn,
                                    crate::ipc::TaskStatus::Failed(e.to_string()),
                                );
                                let upload_task_id_rm = upload_task_id_spawn.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                                    crate::ipc::remove_task(&upload_task_id_rm);
                                });
                            }
                        }

                        // Remove from active uploading file list
                        uploading_files_clone.lock().unwrap().remove(&path_up);
                    });

                    // Remove from local size state tracking map
                    file_states.remove(&path);
                }
            }
        }

        // Clean up entries in tracking map that are no longer present in any folders
        file_states.retain(|path, _| scanned_files.contains(path));
    }
}
