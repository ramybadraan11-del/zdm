//! ZDM Central Download Manager
//! Developed by Ramy Badraan
//!
//! Orchestrates concurrent tasks, work distribution, progress broadcast,
//! and SQLite persistence.

use crate::db::Database;
use crate::engine::downloader::Downloader;
use crate::engine::types::{DownloadItem, DownloadStatus, EngineStats, NewDownloadRequest, SegmentProgress, UrlMetadata};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use parking_lot::RwLock;
use reqwest::header::{RANGE, USER_AGENT};
use reqwest::Client;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[cfg(windows)]
use std::os::windows::fs::FileExt;

pub struct DownloadManager {
    db: Arc<Database>,
    downloader: Arc<Downloader>,
    active_cancels: Arc<RwLock<HashMap<String, broadcast::Sender<()>>>>,
    app_handle: Arc<RwLock<Option<AppHandle>>>,
}

impl DownloadManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            downloader: Arc::new(Downloader::new()),
            active_cancels: Arc::new(RwLock::new(HashMap::new())),
            app_handle: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.write() = Some(handle);
    }

    pub async fn probe(&self, url: &str) -> Result<UrlMetadata> {
        self.downloader.probe_url(url).await
    }

    pub async fn add_download(&self, req: NewDownloadRequest) -> Result<DownloadItem> {
        let meta = self.downloader.probe_url(&req.url).await?;

        let id = Uuid::new_v4().to_string();
        let filename = req.filename.unwrap_or(meta.filename);
        let category = req.category.unwrap_or(meta.category);

        // Resolve default download directory (~/Downloads/ZDM/<Category>)
        let download_dir = if let Some(path) = req.save_path {
            PathBuf::from(path)
        } else {
            let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
            let mut p = PathBuf::from(home);
            p.push("Downloads");
            p.push("ZDM");
            p.push(&category);
            p
        };

        let _ = tokio::fs::create_dir_all(&download_dir).await;
        let save_path = download_dir.join(&filename).to_string_lossy().to_string();

        let connections = Downloader::calculate_optimal_segments(
            meta.total_size,
            meta.supports_range,
            req.connections.unwrap_or(8),
        );

        let mut segments = Vec::new();
        if meta.supports_range && meta.total_size > 0 && connections > 1 {
            let chunk_size = meta.total_size / (connections as u64);
            for i in 0..connections {
                let start_byte = i as u64 * chunk_size;
                let end_byte = if i == connections - 1 {
                    meta.total_size - 1
                } else {
                    (i as u64 + 1) * chunk_size - 1
                };
                let total_bytes = end_byte - start_byte + 1;

                segments.push(SegmentProgress {
                    index: i,
                    start_byte,
                    end_byte,
                    downloaded_bytes: 0,
                    total_bytes,
                    speed_bps: 0,
                    is_active: false,
                    is_finished: false,
                });
            }
        } else {
            // Single connection
            segments.push(SegmentProgress {
                index: 0,
                start_byte: 0,
                end_byte: if meta.total_size > 0 { meta.total_size - 1 } else { 0 },
                downloaded_bytes: 0,
                total_bytes: meta.total_size,
                speed_bps: 0,
                is_active: false,
                is_finished: false,
            });
        }

        let item = DownloadItem {
            id: id.clone(),
            url: req.url,
            filename,
            save_path,
            total_size: meta.total_size,
            downloaded_size: 0,
            status: DownloadStatus::Queued,
            category,
            speed_bps: 0,
            eta_seconds: 0,
            progress_percent: 0.0,
            connections,
            segments,
            supports_range: meta.supports_range,
            etag: meta.etag,
            referer: None,
            created_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            error_message: None,
        };

        self.db.insert_download(&item)?;

        if req.start_now.unwrap_or(true) {
            let id_clone = id.clone();
            self.start_download(&id_clone).await?;
        }

        Ok(item)
    }

    pub async fn start_download(&self, id: &str) -> Result<()> {
        let downloads = self.db.get_all_downloads()?;
        let item = downloads
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow!("Download not found: {}", id))?;

        if item.status == DownloadStatus::Downloading {
            return Ok(());
        }

        let (cancel_tx, _) = broadcast::channel(1);
        self.active_cancels.write().insert(id.to_string(), cancel_tx.clone());

        self.db.set_status(id, DownloadStatus::Downloading)?;

        let db_clone = Arc::clone(&self.db);
        let active_cancels_clone = Arc::clone(&self.active_cancels);
        let app_handle_clone = Arc::clone(&self.app_handle);
        let id_str = id.to_string();

        tokio::spawn(async move {
            let res = Self::run_download_loop(
                item,
                cancel_tx,
                Arc::clone(&db_clone),
                Arc::clone(&app_handle_clone),
            )
            .await;

            active_cancels_clone.write().remove(&id_str);

            match res {
                Ok(true) => {
                    let now = chrono::Utc::now().timestamp();
                    let _ = db_clone.mark_completed(&id_str, now);
                    Self::static_emit_event(&app_handle_clone, "download_completed", serde_json::json!({ "id": id_str }));
                }
                Ok(false) => {
                    let _ = db_clone.set_status(&id_str, DownloadStatus::Paused);
                    Self::static_emit_event(&app_handle_clone, "download_paused", serde_json::json!({ "id": id_str }));
                }
                Err(err) => {
                    let _ = db_clone.set_status(&id_str, DownloadStatus::Error);
                    Self::static_emit_event(
                        &app_handle_clone,
                        "download_error",
                        serde_json::json!({ "id": id_str, "error": err.to_string() }),
                    );
                }
            }
        });

        Ok(())
    }

    pub async fn pause_download(&self, id: &str) -> Result<()> {
        if let Some(tx) = self.active_cancels.write().remove(id) {
            let _ = tx.send(());
        }
        self.db.set_status(id, DownloadStatus::Paused)?;
        self.emit_event("download_paused", serde_json::json!({ "id": id }));
        Ok(())
    }

    pub async fn resume_download(&self, id: &str) -> Result<()> {
        self.start_download(id).await
    }

    pub async fn delete_download(&self, id: &str, delete_file: bool) -> Result<()> {
        self.pause_download(id).await?;

        if delete_file {
            if let Ok(downloads) = self.db.get_all_downloads() {
                if let Some(item) = downloads.iter().find(|d| d.id == id) {
                    let _ = tokio::fs::remove_file(&item.save_path).await;
                }
            }
        }

        self.db.delete_download(id)?;
        self.emit_event("download_deleted", serde_json::json!({ "id": id }));
        Ok(())
    }

    pub fn get_all_downloads(&self) -> Result<Vec<DownloadItem>> {
        Ok(self.db.get_all_downloads()?)
    }

    pub fn get_stats(&self) -> EngineStats {
        let downloads = self.db.get_all_downloads().unwrap_or_default();
        let mut active_count = 0;
        let mut queued_count = 0;
        let mut completed_count = 0;

        for d in &downloads {
            match d.status {
                DownloadStatus::Downloading => active_count += 1,
                DownloadStatus::Queued => queued_count += 1,
                DownloadStatus::Completed => completed_count += 1,
                _ => {}
            }
        }

        EngineStats {
            total_speed_bps: 0,
            active_count,
            queued_count,
            completed_count,
            disk_free_bytes: 142 * 1024 * 1024 * 1024,
        }
    }

    fn emit_event<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        Self::static_emit_event(&self.app_handle, event, payload);
    }

    fn static_emit_event<S: serde::Serialize + Clone>(
        app_handle: &Arc<RwLock<Option<AppHandle>>>,
        event: &str,
        payload: S,
    ) {
        if let Some(app) = app_handle.read().as_ref() {
            let _ = app.emit(event, payload);
        }
    }

    /// Primary execution loop: manages preallocation, workers, and direct-to-disk writes
    async fn run_download_loop(
        item: DownloadItem,
        cancel_rx: broadcast::Sender<()>,
        db: Arc<Database>,
        app_handle: Arc<RwLock<Option<AppHandle>>>,
    ) -> Result<bool> {
        let save_path = PathBuf::from(&item.save_path);
        if let Some(parent) = save_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        // Open or create the target file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&save_path)
            .map_err(|e| anyhow!("Failed to open file: {}", e))?;

        // Preallocate disk space if total size is known
        if item.total_size > 0 {
            let _ = file.set_len(item.total_size);
        }

        let file = Arc::new(file);
        let total_downloaded = Arc::new(AtomicU64::new(item.downloaded_size));
        let is_cancelled = Arc::new(AtomicBool::new(false));

        let mut worker_handles = Vec::new();
        let segment_count = item.segments.len();
        let shared_segments: Arc<RwLock<Vec<SegmentProgress>>> = Arc::new(RwLock::new(item.segments.clone()));

        // Spawn parallel segment workers
        for seg_idx in 0..segment_count {
            let seg = shared_segments.read()[seg_idx].clone();
            if seg.is_finished || (seg.total_bytes > 0 && seg.downloaded_bytes >= seg.total_bytes) {
                continue;
            }

            let file_clone = Arc::clone(&file);
            let total_dl_clone = Arc::clone(&total_downloaded);
            let cancel_clone = Arc::clone(&is_cancelled);
            let segs_ref = Arc::clone(&shared_segments);
            let url = item.url.clone();
            let supports_range = item.supports_range;

            let handle = tokio::spawn(async move {
                let client = Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .unwrap_or_default();

                let mut req = client.get(&url)
                    .header(USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36");

                let start_offset = seg.start_byte + seg.downloaded_bytes;
                if supports_range && seg.end_byte > 0 {
                    req = req.header(RANGE, format!("bytes={}-{}", start_offset, seg.end_byte));
                }

                let resp = match req.send().await {
                    Ok(r) => r,
                    Err(_) => return false,
                };

                if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    return false;
                }

                let mut stream = resp.bytes_stream();
                let mut current_offset = start_offset;

                while let Some(chunk_res) = stream.next().await {
                    if cancel_clone.load(Ordering::Relaxed) {
                        return false;
                    }

                    let chunk = match chunk_res {
                        Ok(c) => c,
                        Err(_) => return false,
                    };

                    let chunk_len = chunk.len() as u64;

                    // Direct write to file at exact offset (zero temporary files!)
                    #[cfg(unix)]
                    {
                        if let Err(_) = file_clone.write_all_at(&chunk, current_offset) {
                            return false;
                        }
                    }

                    #[cfg(windows)]
                    {
                        if let Err(_) = file_clone.seek_write(&chunk, current_offset) {
                            return false;
                        }
                    }

                    current_offset += chunk_len;
                    total_dl_clone.fetch_add(chunk_len, Ordering::Relaxed);

                    // Update segment in memory
                    {
                        let mut lock = segs_ref.write();
                        if let Some(s) = lock.get_mut(seg_idx) {
                            s.downloaded_bytes += chunk_len;
                            s.is_active = true;
                            if s.total_bytes > 0 && s.downloaded_bytes >= s.total_bytes {
                                s.is_finished = true;
                                s.is_active = false;
                            }
                        }
                    }
                }

                true
            });

            worker_handles.push(handle);
        }

        // Monitoring and UI update loop (every 100ms)
        let mut last_instant = Instant::now();
        let mut last_bytes = total_downloaded.load(Ordering::Relaxed);
        let mut rx = cancel_rx.subscribe();
        let mut db_sync_counter = 0;

        loop {
            tokio::select! {
                _ = rx.recv() => {
                    is_cancelled.store(true, Ordering::Relaxed);
                    let dl = total_downloaded.load(Ordering::Relaxed);
                    let _ = db.update_progress(&item.id, dl, &DownloadStatus::Paused);
                    return Ok(false);
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_instant).as_secs_f64();
                    let current_bytes = total_downloaded.load(Ordering::Relaxed);
                    let delta_bytes = current_bytes.saturating_sub(last_bytes);

                    let speed_bps = if elapsed > 0.0 {
                        (delta_bytes as f64 / elapsed) as u64
                    } else {
                        0
                    };

                    last_instant = now;
                    last_bytes = current_bytes;

                    let remaining = item.total_size.saturating_sub(current_bytes);
                    let eta_seconds = if speed_bps > 0 {
                        remaining / speed_bps
                    } else {
                        0
                    };

                    let percent = if item.total_size > 0 {
                        (current_bytes as f32 / item.total_size as f32) * 100.0
                    } else {
                        0.0
                    };

                    let segs_snapshot = shared_segments.read().clone();

                    Self::static_emit_event(&app_handle, "download_progress", serde_json::json!({
                        "id": item.id,
                        "downloaded_bytes": current_bytes,
                        "total_bytes": item.total_size,
                        "speed_bps": speed_bps,
                        "eta_seconds": eta_seconds,
                        "progress_percent": percent,
                        "segments": segs_snapshot,
                    }));

                    db_sync_counter += 1;
                    if db_sync_counter >= 10 {
                        db_sync_counter = 0;
                        let _ = db.update_progress(&item.id, current_bytes, &DownloadStatus::Downloading);
                    }

                    if item.total_size > 0 && current_bytes >= item.total_size {
                        break;
                    }

                    let all_done = worker_handles.iter().all(|h| h.is_finished());
                    if all_done {
                        break;
                    }
                }
            }
        }

        for h in worker_handles {
            let _ = h.await;
        }

        let final_bytes = total_downloaded.load(Ordering::Relaxed);
        let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Completed);

        Ok(true)
    }
}
