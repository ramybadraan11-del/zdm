//! ZDM Local IPC Bridge
//! Developed by Ramy Badraan
//!
//! Listens on 127.0.0.1:46800 for incoming captures from zdm-host / browser extensions.

use crate::engine::manager::DownloadManager;
use crate::engine::types::NewDownloadRequest;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;

pub fn start_ipc_server(manager: Arc<DownloadManager>, app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind("127.0.0.1:46800").await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[ZDM IPC] Failed to bind 127.0.0.1:46800: {}", e);
                return;
            }
        };

        println!("[ZDM IPC] Listening for browser extension captures on 127.0.0.1:46800");

        loop {
            if let Ok((socket, _)) = listener.accept().await {
                let mgr = Arc::clone(&manager);
                let app = app_handle.clone();

                tauri::async_runtime::spawn(async move {
                    let mut reader = BufReader::new(socket);
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.is_ok() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                            let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("add_download");
                            let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("");

                            if !url.is_empty() {
                                // Bring ZDM window to the foreground
                                if let Some(win) = app.get_webview_window("main") {
                                    let _ = win.show();
                                    let _ = win.unminimize();
                                    let _ = win.set_focus();
                                }

                                if msg_type == "open_video_modal" {
                                    let _ = app.emit("open_video_modal_with_url", serde_json::json!({ "url": url }));
                                } else {
                                    let filename = val.get("filename").and_then(|f| f.as_str()).map(|s| s.to_string());
                                    let req = NewDownloadRequest {
                                        url: url.to_string(),
                                        filename,
                                        save_path: None,
                                        connections: Some(8),
                                        category: None,
                                        start_now: Some(true),
                                    };
                                    let _ = mgr.add_download(req).await;
                                }
                            }
                        }
                    }
                });
            }
        }
    });
}
