use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use grammers_client::Client;
use grammers_session::defs::PeerRef;
use grammers_client::types::InputMessage;

use crate::config::Config;
use crate::logger;

struct FileState {
    last_size: u64,
    stable_ticks: u32,
}

pub async fn run_watcher(client: Client, config: Config) {
    let watch_dir_str = match &config.watch_dir {
        Some(d) => d,
        None => {
            logger::error("Watcher: watch_dir is not configured. Exiting watcher.");
            return;
        }
    };

    let watch_path = PathBuf::from(watch_dir_str);
    if !watch_path.exists() {
        logger::warn(&format!(
            "Watcher: Directory {:?} does not exist. Creating it...",
            watch_path
        ));
        if let Err(e) = fs::create_dir_all(&watch_path).await {
            logger::error(&format!("Watcher: Failed to create watch_dir: {}", e));
            return;
        }
    }

    let auto_delete = config.auto_delete.unwrap_or(false);
    logger::info(&format!(
        "Watcher: Monitoring started on {:?} (auto_delete: {})",
        watch_path, auto_delete
    ));

    // Resolve target chat: "me" (Saved Messages)
    let me = match client.get_me().await {
        Ok(user) => user,
        Err(e) => {
            logger::error(&format!("Watcher: Failed to get self user: {}", e));
            return;
        }
    };
    let target_chat: PeerRef = PeerRef::from(grammers_client::types::Peer::User(me));

    let mut file_states: HashMap<PathBuf, FileState> = HashMap::new();

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Perform directory scan
        let mut dir_entries = match fs::read_dir(&watch_path).await {
            Ok(entries) => entries,
            Err(e) => {
                logger::error(&format!("Watcher: Failed to read watch_dir: {}", e));
                continue;
            }
        };

        let mut scanned_files = std::collections::HashSet::new();

        while let Ok(Some(entry)) = dir_entries.next_entry().await {
            let path = entry.path();
            if !path.is_file() {
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

                crate::ipc::update_task(
                    &upload_task_id,
                    crate::ipc::TaskType::Upload,
                    &upload_filename,
                    crate::ipc::TaskStatus::Uploading {
                        progress: 0.0,
                        speed: "2.4 MiB/s".to_string(),
                        eta: "--s".to_string(),
                    },
                );

                // Spawn background simulation task
                let upload_task_id_clone = upload_task_id.clone();
                let upload_filename_clone = upload_filename.clone();
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

                // Upload and send file via grammers MTProto
                let client_up = client.clone();
                let path_up = path.clone();
                let target_chat_up = target_chat.clone();
                let upload_result = async {
                    let uploaded = client_up.upload_file(&path_up).await?;
                    client_up.send_message(target_chat_up, InputMessage::new().file(uploaded)).await
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
                }.await;

                let _ = cancel_tx.send(());

                match upload_result {
                    Ok(_) => {
                        logger::info(&format!(
                            "Watcher: Uploaded successfully via MTProto: {:?}",
                            path
                        ));

                        crate::ipc::update_task(
                            &upload_task_id,
                            crate::ipc::TaskType::Upload,
                            &upload_filename,
                            crate::ipc::TaskStatus::Completed,
                        );
                        let upload_task_id_rm = upload_task_id.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            crate::ipc::remove_task(&upload_task_id_rm);
                        });

                        if auto_delete {
                            if let Err(e) = fs::remove_file(&path).await {
                                logger::error(&format!("Watcher: Failed to delete file {:?}: {}", path, e));
                            } else {
                                logger::info(&format!("Watcher: Deleted local file: {:?}", path));
                            }
                        } else {
                            let mut new_path = path.clone();
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                new_path.set_extension(format!("{}.uploaded", ext));
                            } else {
                                new_path.set_extension("uploaded");
                            }

                            if let Err(e) = fs::rename(&path, &new_path).await {
                                logger::error(&format!(
                                    "Watcher: Failed to rename file {:?} -> {:?}: {}",
                                    path, new_path, e
                                ));
                            } else {
                                logger::info(&format!("Watcher: Marked file as uploaded: {:?}", new_path));
                            }
                        }
                    }
                    Err(e) => {
                        logger::error(&format!("Watcher: Failed to upload file {:?}: {:?}", path, e));
                        
                        crate::ipc::update_task(
                            &upload_task_id,
                            crate::ipc::TaskType::Upload,
                            &upload_filename,
                            crate::ipc::TaskStatus::Failed(e.to_string()),
                        );
                        let upload_task_id_rm = upload_task_id.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            crate::ipc::remove_task(&upload_task_id_rm);
                        });

                        // Reset ticks so we wait before retrying next time
                        state.stable_ticks = 0;
                    }
                }

                // Remove from state mapping so it doesn't get processed again
                file_states.remove(&path);
            }
        }

        // Clean up entries in tracking map that are no longer present in folder
        file_states.retain(|path, _| scanned_files.contains(path));
    }
}
