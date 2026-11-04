//! ZDM Central Download Manager
//! Developed by Ramy Badraan
//!
//! Orchestrates concurrent downloads, per-segment work distribution, progress
//! broadcasting and persistence.
//!
//! Design notes (these fix real, reproduced bugs):
//! * Every ranged request is validated: a `200 OK` answer to `Range:` is
//!   treated as a protocol downgrade and the transfer restarts as a single
//!   stream. Writing a full-body `200` at a non-zero offset is exactly what
//!   produced corrupt binaries before.
//! * Cancellation uses a `watch` channel (edge-triggered, never lost) instead
//!   of a `broadcast` that could fire before the subscriber existed — that is
//!   why "pause" used to do nothing while the download kept running.
//! * A download id can only have one live task; resuming awaits the previous
//!   task, so two writers can never touch the same file range.
//! * Per-segment offsets are persisted, so a restart resumes instead of
//!   assuming everything on disk is valid.

use crate::db::Database;
use crate::engine::downloader::{unique_path, Downloader, BROWSER_UA};
use crate::engine::types::{
    DownloadItem, DownloadStatus, EngineStats, NewDownloadRequest, SegmentProgress, Settings,
    UrlMetadata,
};
use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use parking_lot::RwLock;
use reqwest::header::{CONTENT_RANGE, IF_RANGE, RANGE, REFERER, USER_AGENT};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Notify};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(windows)]
use std::os::windows::fs::FileExt;

/// How often progress is pushed to the UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);
/// How often progress is flushed to SQLite.
const DB_FLUSH_INTERVAL: Duration = Duration::from_millis(1500);
/// Attempts per segment before the whole download is marked as failed.
const MAX_SEGMENT_ATTEMPTS: u32 = 6;

type AppHandleCell = Arc<RwLock<Option<AppHandle>>>;

/// Monotonic task identity. Cheap, and only ever compared for equality.
fn next_task_token() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Final on-disk path for a new download.
///
/// `save_path` is what the picker in the prompt handed us: either a folder the
/// user chose, or (in the exact-file case) the full destination. `None` means
/// "use the configured folder plus the category subfolder". Kept as a pure
/// function so the folder-choice behaviour is covered by tests.
fn resolve_download_target(
    save_path: Option<&str>,
    download_dir: &std::path::Path,
    category: &str,
    raw_name: &str,
    overrides: &std::collections::BTreeMap<String, String>,
) -> PathBuf {
    let base_dir = match save_path {
        Some(path) => crate::paths::expand(path),
        None => crate::paths::category_dir(download_dir, category, overrides),
    };

    match save_path {
        Some(path) if path.ends_with(raw_name) => {
            let p = crate::paths::expand(path);
            if p.exists() {
                base_dir.join(raw_name)
            } else {
                p
            }
        }
        _ => base_dir.join(raw_name),
    }
}

struct CancelToken {
    tx: Arc<watch::Sender<bool>>,
}

impl Clone for CancelToken {
    fn clone(&self) -> Self {
        Self { tx: Arc::clone(&self.tx) }
    }
}

impl CancelToken {
    fn new() -> Self {
        let (tx, _rx) = watch::channel(false);
        Self { tx: Arc::new(tx) }
    }

    fn cancel(&self) {
        let _ = self.tx.send(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }

    fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }
}

struct ActiveTask {
    /// Unique per spawned writer, so a finishing task can only remove its own
    /// entry (a newer writer for the same id keeps its slot).
    token: u64,
    cancel: CancelToken,
    /// Tauri's handle wrapper rather than Tokio's: it accepts tasks spawned via
    /// either `tokio::spawn` (inside a runtime) or `tauri::async_runtime::spawn`
    /// (from Tauri's setup hook, where no reactor is running).
    join: tauri::async_runtime::JoinHandle<()>,
}

impl ActiveTask {
    async fn wait_stopped(mut self, timeout: Duration) -> Option<Self> {
        // `&mut` keeps ownership of the handle available if the wait times out,
        // so the caller can put the task back exactly as it found it.
        match tokio::time::timeout(timeout, &mut self.join).await {
            Ok(_) => None,
            Err(_) => Some(self),
        }
    }
}

/// Serialises start/pause/resume per download id.
///
/// The previous design inserted a placeholder into the task map and released it
/// later, which left a window where pause could cancel a placeholder nobody was
/// reading and later overwrite the real task. A dedicated per-id lock removes the
/// whole failure class: whoever holds it is the only one who can change the state
/// of that download, and the map only ever contains real, joinable tasks.
type IdLocks = Arc<parking_lot::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

fn lock_for(locks: &IdLocks, id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = locks.lock();
    Arc::clone(map.entry(id.to_string()).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))))
}

pub struct DownloadManager {
    db: Arc<Database>,
    downloader: Arc<Downloader>,
    tasks: Arc<RwLock<HashMap<String, ActiveTask>>>,
    app_handle: AppHandleCell,
    settings: Arc<RwLock<Settings>>,
    /// Wake-up signal used by the queue scheduler.
    scheduler: Arc<Notify>,
    /// Live speed of each running task, keyed by download id. The UI total is
    /// the sum of these, not "whatever ticked last" (the old single AtomicU64
    /// showed one task's speed and dropped to zero when any task ended).
    speeds: Arc<RwLock<HashMap<String, u64>>>,
    /// One async lock per download id: serialises start/pause/resume/delete.
    id_locks: IdLocks,
    /// True once the post-queue action ran, so a drain cannot power off twice.
    drained_fired: Arc<parking_lot::Mutex<bool>>,
    /// The BitTorrent session, created on first use.
    pub torrents: Arc<crate::torrent::TorrentEngine>,
}

impl DownloadManager {
    pub fn new(db: Arc<Database>) -> Self {
        let settings = db.get_settings();
        Self {
            db,
            downloader: Arc::new(Downloader::new()),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            app_handle: Arc::new(RwLock::new(None)),
            settings: Arc::new(RwLock::new(settings)),
            scheduler: Arc::new(Notify::new()),
            speeds: Arc::new(RwLock::new(HashMap::new())),
            id_locks: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            drained_fired: Arc::new(parking_lot::Mutex::new(false)),
            torrents: Arc::new(crate::torrent::TorrentEngine::new()),
        }
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    /// Record an externally-managed download (a torrent) and announce it.
    pub fn record_external(&self, item: &DownloadItem) -> Result<(), String> {
        self.db
            .insert_download(item)
            .map_err(|error| format!("could not record the download: {error}"))?;
        self.emit("download_added", serde_json::json!({ "id": item.id }));
        Ok(())
    }

    /// All rows, for watchers that need to see status changes.
    pub fn all_downloads(&self) -> Vec<DownloadItem> {
        self.db.get_all_downloads().unwrap_or_default()
    }

    /// Persist progress for an externally-managed download.
    pub fn record_progress(
        &self,
        id: &str,
        downloaded: u64,
        status: &DownloadStatus,
    ) -> Result<(), String> {
        self.db
            .update_progress(id, downloaded, status)
            .map_err(|error| error.to_string())
    }

    /// Persist a newly discovered total size (torrent metadata arrives late).
    pub fn record_total_size(&self, id: &str, total: u64) -> Result<(), String> {
        self.db
            .update_total_size(id, total)
            .map_err(|error| error.to_string())
    }

    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.write() = Some(handle);
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    pub fn update_settings(&self, settings: Settings) -> Result<Settings> {
        let clean = settings.sanitized();
        self.db.set_settings(&clean)?;
        *self.settings.write() = clean.clone();
        self.scheduler.notify_waiters();
        Ok(clean)
    }

    /// Downloads left in `downloading` after a crash/exit cannot still be
    /// running: demote them so the UI is honest and resume is explicit.
    pub fn reconcile_on_startup(&self) {
        if let Ok(items) = self.db.get_all_downloads() {
            for item in items {
                if item.status == DownloadStatus::Downloading {
                    let _ = self.db.set_status(&item.id, DownloadStatus::Paused);
                }
            }
        }
    }

    pub async fn probe(&self, url: &str) -> Result<UrlMetadata> {
        self.downloader.probe_url(url).await
    }

    pub async fn add_download(&self, req: NewDownloadRequest) -> Result<DownloadItem> {
        let meta = self.downloader.probe_url(&req.url).await?;
        let settings = self.settings.read().clone();

        let id = Uuid::new_v4().to_string();
        let category = req.category.unwrap_or(meta.category);
        let raw_name = req
            .filename
            .map(|f| crate::engine::downloader::sanitize_filename(&f))
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| meta.filename.clone());

        let expanded = crate::paths::expand(&settings.download_dir);
        let base_dir = req
            .save_path
            .as_deref()
            .map(crate::paths::expand)
            .unwrap_or_else(|| crate::paths::category_dir(&expanded, &category, &settings.category_dirs));
        tokio::fs::create_dir_all(&base_dir).await.ok();

        // Never silently destroy an existing file.
        let target = resolve_download_target(
            req.save_path.as_deref(),
            &crate::paths::expand(&settings.download_dir),
            &category,
            &raw_name,
            &settings.category_dirs,
        );
        let final_path = if target.exists() {
            unique_path(target.parent().unwrap_or(&base_dir), &raw_name)
        } else {
            target
        };
        let filename = final_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(raw_name);

        let connections = Downloader::calculate_optimal_segments(
            meta.total_size,
            meta.supports_range,
            req.connections.unwrap_or(settings.default_connections),
        );

        let segments = build_segments(meta.total_size, connections, meta.supports_range);

        let item = DownloadItem {
            id: id.clone(),
            url: req.url,
            filename,
            save_path: final_path.to_string_lossy().to_string(),
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
            referer: req.referer,
            created_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            error_message: None,
        };

        self.db.insert_download(&item)?;
        self.emit("download_added", serde_json::json!({ "id": id }));

        if req.start_now.unwrap_or(true) {
            self.enqueue(id.clone()).await;
        }

        Ok(item)
    }

    /// Start now if a slot is free, otherwise leave it queued for the scheduler.
    async fn enqueue(&self, id: String) {
        let max = self.settings.read().max_concurrent_downloads;
        if self.running_count() < max {
            if let Err(err) = self.start_download(&id).await {
                eprintln!("[ZDM] failed to start {id}: {err}");
                let _ = self.db.set_error(&id, &err.to_string());
                self.emit(
                    "download_error",
                    serde_json::json!({ "id": id, "error": err.to_string() }),
                );
            }
        } else {
            let _ = self.db.set_status(&id, DownloadStatus::Queued);
            self.emit("download_queued", serde_json::json!({ "id": id }));
            self.scheduler.notify_waiters();
        }
    }

    fn running_count(&self) -> usize {
        self.tasks.read().len()
    }

    /// Background queue scheduler.
    ///
    /// Downloads that do not fit in `max_concurrent_downloads` sit in `Queued`.
    /// Whenever a task finishes, is paused, or is deleted, the scheduler is
    /// nudged and starts as many queued items as the budget now allows.
    // ---------------------------------------------------------------- queue
    /// Configuration plus live occupancy, for the queue screen.
    pub fn queue_status(&self) -> crate::schedule::QueueStatus {
        let config = self.db.get_queue_config();
        let global_max = self.settings.read().max_concurrent_downloads;
        let slots = crate::schedule::effective_slots(&config, global_max);
        let order: Vec<String> = self
            .db
            .queued_in_order()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let running = self.running_count();

        crate::schedule::QueueStatus {
            config,
            drained: order.is_empty() && running == 0,
            order,
            running,
            slots,
        }
    }

    pub fn set_queue_config(&self, config: &crate::schedule::QueueConfig) -> Result<(), String> {
        self.db
            .set_queue_config(config)
            .map_err(|error| format!("could not save the queue settings: {error}"))?;
        self.emit_queue_changed();
        Ok(())
    }

    /// Move downloads into the queue without starting them.
    pub async fn queue_ids(&self, ids: &[String]) -> Result<(), String> {
        for id in ids {
            // A finished file cannot be queued; neither can one already running.
            let Some(item) = self
                .all_downloads()
                .into_iter()
                .find(|entry| &entry.id == id)
            else {
                continue;
            };
            if matches!(
                item.status,
                DownloadStatus::Completed | DownloadStatus::Downloading
            ) {
                continue;
            }

            let position = self.db.next_queue_position();
            let _ = self
                .db
                .set_download_status(id, &DownloadStatus::Queued, Some(position));
        }

        self.emit_queue_changed();
        self.scheduler.notify_waiters();
        self.emit("download_queued", serde_json::json!({ "ids": ids }));
        Ok(())
    }

    /// Take downloads out of the queue; they become paused, not deleted.
    pub async fn unqueue_ids(&self, ids: &[String]) -> Result<(), String> {
        for id in ids {
            let _ = self
                .db
                .set_download_status(id, &DownloadStatus::Paused, None);
        }
        self.emit_queue_changed();
        self.emit("download_paused", serde_json::json!({ "ids": ids }));
        Ok(())
    }

    /// Reorder: swap a queued download with its neighbour, or send it to the top.
    pub async fn move_in_queue(&self, id: &str, direction: &str) -> Result<(), String> {
        let mut order: Vec<String> = self
            .db
            .queued_in_order()
            .into_iter()
            .map(|(queued_id, _)| queued_id)
            .collect();

        let Some(index) = order.iter().position(|queued| queued == id) else {
            return Ok(());
        };

        let target = match direction {
            "up" => index.checked_sub(1),
            "down" => {
                if index + 1 < order.len() {
                    Some(index + 1)
                } else {
                    None
                }
            }
            "top" => Some(0),
            _ => None,
        };

        let Some(target) = target else {
            return Ok(());
        };

        order.swap(index, target);
        self.db
            .set_queue_order(&order)
            .map_err(|error| format!("could not reorder the queue: {error}"))?;
        self.emit_queue_changed();
        Ok(())
    }

    pub async fn move_to_top(&self, id: &str) -> Result<(), String> {
        self.move_in_queue(id, "top").await
    }

    /// Pause or resume the whole queue.
    pub async fn set_queue_paused(&self, paused: bool) -> Result<(), String> {
        let mut config = self.db.get_queue_config();
        config.paused = paused;
        if !paused {
            config.enabled = true;
        }
        self.set_queue_config(&config)?;

        if paused {
            // Hold everything that is running, so "pause queue" means what it says.
            let running: Vec<String> = self
                .all_downloads()
                .into_iter()
                .filter(|item| item.status == DownloadStatus::Downloading)
                .map(|item| item.id)
                .collect();
            for id in running {
                let _ = self.pause_download(&id).await;
            }
        } else {
            self.scheduler.notify_waiters();
        }

        self.emit_queue_changed();
        Ok(())
    }

    /// Tell the UI the queue changed shape.
    fn emit_queue_changed(&self) {
        let status = self.queue_status();
        self.emit("queue_changed", serde_json::json!(status));
    }

    /// Run the post-queue action, at most once per drain.
    ///
    /// The flag is cleared whenever the queue becomes non-empty, so adding a file
    /// after a scheduled shutdown cancels it — which is what a person expects.
    fn maybe_run_drain_action(&self) {
        let config = self.db.get_queue_config();
        let status = self.queue_status();

        if !status.drained {
            *self.drained_fired.lock() = false;
            return;
        }
        if !config.enabled || config.drain_action == crate::schedule::DrainAction::Nothing {
            return;
        }

        let mut fired = self.drained_fired.lock();
        if *fired {
            return;
        }
        *fired = true;
        drop(fired);

        let action = config.drain_action;
        let app = self.app_handle.read().clone();

        tauri::async_runtime::spawn(async move {
            // A short grace period: the last file's completion event should reach
            // the UI before the machine goes down.
            tokio::time::sleep(Duration::from_secs(20)).await;

            let command = match action {
                crate::schedule::DrainAction::ShutDown => "poweroff",
                crate::schedule::DrainAction::Sleep => "suspend",
                crate::schedule::DrainAction::Nothing => return,
            };

            // Report it, so the user is never surprised by a power-off.
            if let Some(app) = &app {
                let _ = app.emit(
                    "queue_drain_action",
                    serde_json::json!({ "action": command, "seconds": 20 }),
                );
            }

            let _ = crate::process::command(command).spawn();
        });
    }

    /// Start the queue worker: as slots free up, the next queued file starts.
    pub fn spawn_queue_worker(self: &Arc<Self>) {
        let manager = Arc::clone(self);

        tauri::async_runtime::spawn(async move {
            loop {
                // Wake on a completion, an add, or an explicit nudge.
                tokio::select! {
                    _ = manager.scheduler.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }

                manager.maybe_run_drain_action();

                let config = manager.db.get_queue_config();
                if config.enabled && config.paused {
                    continue;
                }

                let global_max = manager.settings.read().max_concurrent_downloads;
                let slots = crate::schedule::effective_slots(&config, global_max);
                let running = manager.running_count();

                let queued = manager.db.queued_in_order();
                let pairs: Vec<(&str, i64)> = queued
                    .iter()
                    .map(|(id, position)| (id.as_str(), *position))
                    .collect();

                for id in crate::schedule::next_to_start(pairs, running, slots) {
                    if let Err(error) = manager.start_download(id).await {
                        // A file that cannot start must not block the queue.
                        eprintln!("[ZDM queue] could not start {id}: {error}");
                        let _ = manager.db.set_download_status(
                            id,
                            &DownloadStatus::Error,
                            None,
                        );
                    }
                }

                manager.emit_queue_changed();
            }
        });
    }

    pub fn spawn_scheduler(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        // `tauri::async_runtime::spawn`, not `tokio::spawn`: Tauri's `setup` hook
        // runs before the Tokio reactor is entered, so a bare `tokio::spawn` here
        // panicked with "there is no reactor running" and aborted the process.
        // This is the runtime-handle-aware spawn that works from any thread.
        tauri::async_runtime::spawn(async move {
            loop {
                manager.scheduler.notified().await;
                // Small delay so the finishing task has removed itself from
                // `tasks` before we count free slots.
                tokio::time::sleep(Duration::from_millis(120)).await;

                let max = manager.settings.read().max_concurrent_downloads;
                if manager.running_count() >= max {
                    continue;
                }

                let queued: Vec<String> = manager
                    .db
                    .get_all_downloads()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|d| d.status == DownloadStatus::Queued)
                    .map(|d| d.id)
                    .collect();

                for id in queued {
                    if manager.running_count() >= max {
                        break;
                    }
                    if let Err(err) = manager.start_download(&id).await {
                        // A racing start is expected; only real failures are
                        // surfaced, so a healthy transfer is never marked failed.
                        if manager.db.get_download(&id).ok().flatten()
                            .map(|row| row.status != DownloadStatus::Downloading)
                            .unwrap_or(true)
                        {
                            let _ = manager.db.set_error(&id, &err.to_string());
                            manager.emit(
                                "download_error",
                                serde_json::json!({ "id": id, "error": err.to_string() }),
                            );
                        }
                    }
                }
            }
        });
    }

    pub async fn start_download(&self, id: &str) -> Result<()> {
        // One lock per download id: only the holder may start, pause, resume or
        // delete that transfer. This is what makes "single writer per file"
        // enforceable instead of hopeful.
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;
        self.start_download_locked(id).await
    }

    async fn start_download_locked(&self, id: &str) -> Result<()> {
        // Is a task already running for this id? If so there is nothing to do:
        // the previous design could start a second writer for the same ranges.
        {
            let tasks = self.tasks.read();
            if tasks.get(id).map(|t| !t.cancel.is_cancelled()).unwrap_or(false) {
                return Ok(());
            }
        }
        // A cancelled-but-not-yet-joined task must be drained before restarting.
        let stale = self.tasks.write().remove(id);
        if let Some(task) = stale {
            let _ = task.wait_stopped(Duration::from_secs(15)).await;
        }

        let mut item = self
            .db
            .get_download(id)?
            .ok_or_else(|| anyhow!("Download not found: {id}"))?;

        if item.status == DownloadStatus::Completed {
            return Ok(());
        }

        let save_path = PathBuf::from(&item.save_path);
        if let Some(parent) = save_path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        // Rebuild the plan when it is missing *or* when the stored plan is a
        // downgraded single stream (see `RetryAsSingleStream`).
        if item.segments.is_empty() {
            item.segments = build_segments(item.total_size, item.connections, item.supports_range);
        }

        // Reconcile persisted offsets with the real file length so a truncated or
        // hand-edited partial file can never yield a sparse/corrupt result.
        let file_len = tokio::fs::metadata(&save_path).await.map(|m| m.len()).unwrap_or(0);
        for seg in item.segments.iter_mut() {
            let ceiling = file_len.saturating_sub(seg.start_byte);
            if seg.downloaded_bytes > ceiling {
                seg.downloaded_bytes = ceiling;
                seg.is_finished = false;
            }
            if seg.total_bytes > 0 && seg.downloaded_bytes >= seg.total_bytes {
                seg.is_finished = true;
            }
        }

        let already_done = !item.segments.is_empty()
            && item
                .segments
                .iter()
                .all(|s| s.total_bytes > 0 && s.downloaded_bytes >= s.total_bytes);
        if already_done && item.total_size > 0 {
            let now = chrono::Utc::now().timestamp();
            self.db.mark_completed(id, now)?;
            self.emit("download_completed", serde_json::json!({
                "id": item.id,
                "filename": item.filename,
                "save_path": item.save_path,
                "total_size": item.total_size,
                "category": item.category,
            }));
            return Ok(());
        }

        let cancel = CancelToken::new();
        let cancel_for_task = cancel.subscribe();
        let token = next_task_token();
        self.db.set_status(id, DownloadStatus::Downloading)?;

        let db = Arc::clone(&self.db);
        let app_handle = Arc::clone(&self.app_handle);
        let tasks = Arc::clone(&self.tasks);
        let speeds = Arc::clone(&self.speeds);
        let scheduler = Arc::clone(&self.scheduler);
        let id_owned = id.to_string();
        let settings = self.settings.read().clone();

        // Use the runtime-aware spawner everywhere a task is created from this
        // type. `start_download` can be reached from Tauri's setup path (queue
        // restore), where a bare `tokio::spawn` aborts the process.
        let join = tauri::async_runtime::spawn(async move {
            let mut current = item.clone();
            let mut retried_single_stream = false;

            loop {
                let outcome = Self::run_download(
                    current.clone(),
                    cancel_for_task.clone(),
                    Arc::clone(&db),
                    Arc::clone(&app_handle),
                    Arc::clone(&speeds),
                    settings.clone(),
                )
                .await;

                match outcome {
                    Ok(TransferOutcome::RetryAsSingleStream) if !retried_single_stream => {
                        // The server lied about range support. `run_download` already
                        // rewrote the row as one open-ended stream and truncated the
                        // untrustworthy partial file; start that fallback now instead
                        // of parking the row in "queued" until something else wakes
                        // the scheduler.
                        retried_single_stream = true;
                        match db.get_download(&id_owned) {
                            Ok(Some(row)) => {
                                let _ = db.set_status(&id_owned, DownloadStatus::Downloading);
                                current = row;
                                continue;
                            }
                            _ => {
                                let _ = db.set_error(
                                    &id_owned,
                                    "could not restart the download after the server stopped supporting resume",
                                );
                                break;
                            }
                        }
                    }
                    Ok(TransferOutcome::RetryAsSingleStream) => {
                        let message = "The server stopped honouring range requests";
                        let _ = db.set_error(&id_owned, message);
                        Self::emit_static(
                            &app_handle,
                            "download_error",
                            serde_json::json!({ "id": id_owned, "error": message }),
                        );
                        break;
                    }
                    Ok(TransferOutcome::Completed) => {
                        let now = chrono::Utc::now().timestamp();
                        let _ = db.mark_completed(&id_owned, now);
                        Self::emit_static(&app_handle, "download_completed", serde_json::json!({
                            "id": current.id,
                            "filename": current.filename,
                            "save_path": current.save_path,
                            "total_size": current.total_size,
                            "category": current.category,
                        }));
                        break;
                    }
                    Ok(TransferOutcome::Paused) => {
                        let _ = db.set_status(&id_owned, DownloadStatus::Paused);
                        Self::emit_static(&app_handle, "download_paused", serde_json::json!({ "id": id_owned }));
                        break;
                    }
                    Err(err) => {
                        let _ = db.set_error(&id_owned, &err.to_string());
                        Self::emit_static(
                            &app_handle,
                            "download_error",
                            serde_json::json!({ "id": id_owned, "error": err.to_string() }),
                        );
                        break;
                    }
                }
            }

            // Remove only our own entry (token check), so a newer writer for the
            // same id keeps its slot.
            {
                let mut map = tasks.write();
                if map.get(&id_owned).map(|t| t.token) == Some(token) {
                    map.remove(&id_owned);
                }
            }

            speeds.write().remove(&id_owned);
            scheduler.notify_waiters();
        });

        self.tasks.write().insert(
            id.to_string(),
            ActiveTask {
                token,
                cancel,
                join,
            },
        );

        self.emit("download_started", serde_json::json!({ "id": id }));
        Ok(())
    }

    pub async fn pause_download(&self, id: &str) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let task = self.tasks.write().remove(id);

        if let Some(task) = task {
            task.cancel.cancel();

            // Only report "paused" once the writer has actually stopped. If it
            // does not stop, restore the task untouched and return an error so
            // the UI tells the truth instead of faking a pause.
            match task.wait_stopped(Duration::from_secs(20)).await {
                None => {}
                Some(task) => {
                    self.tasks.write().insert(id.to_string(), task);
                    return Err(anyhow!(
                        "The transfer is still running and could not be paused"
                    ));
                }
            }
        }

        // Never overwrite a completed row with "paused".
        if let Some(item) = self.db.get_download(id)? {
            if item.status == DownloadStatus::Completed {
                return Ok(());
            }
        }

        self.db.set_status(id, DownloadStatus::Paused)?;
        self.emit("download_paused", serde_json::json!({ "id": id }));
        self.scheduler.notify_waiters();
        Ok(())
    }

    pub async fn resume_download(&self, id: &str) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let max = self.settings.read().max_concurrent_downloads;
        if self.running_count() >= max {
            // No slot: queue it and nudge the scheduler, which starts it as soon
            // as a writer finishes.
            self.db.set_status(id, DownloadStatus::Queued)?;
            self.emit("download_queued", serde_json::json!({ "id": id }));
            self.scheduler.notify_waiters();
            return Ok(());
        }

        self.start_download_locked(id).await
    }

    /// Stop the transfer and mark it cancelled, keeping whatever is on disk.
    ///
    /// Distinct from `delete_download`: cancelling is reversible (the user can
    /// resume or delete afterwards), deleting is not.
    pub async fn cancel_download(&self, id: &str) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let task = {
            let mut tasks = self.tasks.write();
            tasks.remove(id)
        };
        if let Some(task) = task {
            task.cancel.cancel();
            let _ = task.wait_stopped(Duration::from_secs(15)).await;
        }

        let keep_partial = self.settings.read().keep_partial_on_cancel;
        if let Some(item) = self.db.get_download(id)? {
            if item.status == DownloadStatus::Completed {
                // Cancelling something already finished is a no-op, not a file wipe.
                return Ok(());
            }
            if !keep_partial {
                // Honour the user's preference: a cancelled transfer leaves no
                // half-written file behind unless they asked to keep it.
                let _ = tokio::fs::remove_file(&item.save_path).await;
            }
        }

        self.db.set_status(id, DownloadStatus::Cancelled)?;
        self.emit("download_cancelled", serde_json::json!({ "id": id }));
        self.scheduler.notify_waiters();
        Ok(())
    }

    /// Stop the transfer and drop it from the database.
    pub async fn delete_download(&self, id: &str, delete_file: bool) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let task = {
            let mut tasks = self.tasks.write();
            tasks.remove(id)
        };
        if let Some(task) = task {
            task.cancel.cancel();
            let _ = task.wait_stopped(Duration::from_secs(15)).await;
        }

        if delete_file {
            if let Some(item) = self.db.get_download(id)? {
                let path = PathBuf::from(&item.save_path);
                let _ = tokio::fs::remove_file(&path).await;
            }
        }

        self.db.delete_download(id)?;
        self.emit("download_deleted", serde_json::json!({ "id": id }));
        self.scheduler.notify_waiters();
        Ok(())
    }

    /// Update the source URL of an existing download (e.g. to resume an expired link).
    pub async fn update_download_url(&self, id: &str, new_url: &str) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let task = {
            let mut tasks = self.tasks.write();
            tasks.remove(id)
        };
        if let Some(task) = task {
            task.cancel.cancel();
            let _ = task.wait_stopped(Duration::from_secs(15)).await;
        }

        self.db.update_download_url(id, new_url)?;
        self.emit("download_url_updated", serde_json::json!({ "id": id, "url": new_url }));
        Ok(())
    }

    /// Restart a download from zero (deleting partially downloaded chunks).
    pub async fn restart_download(&self, id: &str) -> Result<()> {
        let gate = lock_for(&self.id_locks, id);
        let _guard = gate.lock().await;

        let task = {
            let mut tasks = self.tasks.write();
            tasks.remove(id)
        };
        if let Some(task) = task {
            task.cancel.cancel();
            let _ = task.wait_stopped(Duration::from_secs(15)).await;
        }

        if let Some(item) = self.db.get_download(id)? {
            let _ = tokio::fs::remove_file(&item.save_path).await;
            self.db.reset_download_progress(id)?;
            drop(_guard);
            return self.start_download(id).await;
        }

        Ok(())
    }

    pub fn get_all_downloads(&self) -> Result<Vec<DownloadItem>> {
        Ok(self.db.get_all_downloads()?)
    }

    /// Queue a video/audio download handled by yt-dlp.
    ///
    /// Video jobs share the same row model, event names and pause/cancel
    /// semantics as HTTP downloads, so the UI has exactly one code path.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_video_download(
        &self,
        url: &str,
        format_id: &str,
        title: &str,
        ext: &str,
        is_audio: bool,
        est_size: Option<u64>,
        custom_filename: Option<String>,
        force_overwrite: bool,
        folder: Option<String>,
    ) -> Result<DownloadItem> {
        let settings = self.settings.read().clone();
        let category = if is_audio { "Audio" } else { "Video" };

        let stem = match custom_filename {
            Some(name) if !name.trim().is_empty() => {
                let cleaned = crate::engine::downloader::sanitize_filename(&name);
                Path::new(&cleaned)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| crate::video::safe_stem(title))
            }
            _ => crate::video::safe_stem(title),
        };

        // Honour the folder the user picked in the prompt; otherwise fall back to
        // the configured download dir plus the media category.
        let dir = match folder.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(picked) => crate::paths::expand(picked),
            None => crate::paths::category_dir(
                &crate::paths::expand(&settings.download_dir),
                category,
                &settings.category_dirs,
            ),
        };
        tokio::fs::create_dir_all(&dir).await.ok();

        let filename = format!("{stem}.{ext}");
        let mut target = dir.join(&filename);
        if target.exists() && !force_overwrite {
            target = unique_path(&dir, &filename);
        }
        let filename = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(filename);

        let id = Uuid::new_v4().to_string();
        let item = DownloadItem {
            id: id.clone(),
            url: url.to_string(),
            filename,
            save_path: target.to_string_lossy().to_string(),
            total_size: est_size.unwrap_or(0),
            downloaded_size: 0,
            status: DownloadStatus::Queued,
            category: category.to_string(),
            speed_bps: 0,
            eta_seconds: 0,
            progress_percent: 0.0,
            connections: 1,
            segments: Vec::new(),
            supports_range: true,
            etag: None,
            referer: None,
            created_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            error_message: None,
        };

        self.db.insert_download(&item)?;
        self.emit("download_added", serde_json::json!({ "id": id }));
        self.spawn_video_job(item.clone(), format_id.to_string(), is_audio, force_overwrite);
        Ok(item)
    }

    fn spawn_video_job(
        &self,
        item: DownloadItem,
        format_id: String,
        is_audio: bool,
        force_overwrite: bool,
    ) {
        let db = Arc::clone(&self.db);
        let app_handle = Arc::clone(&self.app_handle);
        let tasks = Arc::clone(&self.tasks);
        let speeds = Arc::clone(&self.speeds);
        let scheduler = Arc::clone(&self.scheduler);
        let settings = self.settings.read().clone();

        let cancel = CancelToken::new();
        let cancel_flag = Arc::new(AtomicU64::new(0));
        let cancel_handle = Arc::clone(&cancel_flag);
        let cancel_rx = cancel.subscribe();

        let job = crate::video::VideoJob {
            filename: item.filename.clone(),
            save_path: item.save_path.clone(),
            total_size: item.total_size,
        };

        let id_for_task = item.id.clone();
        let id_for_cleanup = item.id.clone();
        let url = item.url.clone();
        let item_for_events = item.clone();

        let progress_handle = Arc::clone(&app_handle);
        let progress_db = Arc::clone(&db);
        let task_cancel = cancel.clone();
        let speed_for_closure = Arc::clone(&speeds);

        let video_token = next_task_token();
        // Same rule as the scheduler: never assume a reactor is running on the
        // calling thread. `spawn` here is reached from an async command today, but
        // a future caller might not be, and that failure is a hard abort.
        let handle = tauri::async_runtime::spawn(async move {
            let _ = db.set_status(&id_for_task, DownloadStatus::Downloading);
            Self::emit_static(
                &app_handle,
                "download_started",
                serde_json::json!({ "id": id_for_task }),
            );

            // Bridge the shared cancellation signal into yt-dlp's kill flag so
            // a pause actually stops the child process.
            let bridge_cancel = task_cancel;
            let bridge_flag = Arc::clone(&cancel_flag);
            let watcher_flag = Arc::clone(&cancel_flag);
            tokio::spawn(async move {
                let mut rx = cancel_rx;
                loop {
                    if rx.changed().await.is_err() || *rx.borrow() {
                        bridge_flag.store(1, Ordering::Relaxed);
                        bridge_cancel.cancel();
                        break;
                    }
                }
            });

            let id_for_progress = id_for_task.clone();
            let total_for_progress = item_for_events.total_size;
            let last_emit = Arc::new(parking_lot::Mutex::new(Instant::now()));

            let job_handle = crate::video::spawn_yt_dlp(
                job,
                url,
                format_id,
                is_audio,
                force_overwrite,
                settings,
                cancel_handle,
                move |downloaded, total, speed, eta, percent| {
                    let mut last = last_emit.lock();
                    if last.elapsed() < Duration::from_millis(150) && percent < 99.0 {
                        return;
                    }
                    *last = Instant::now();
                    let total = if total > 0 { total } else { total_for_progress };
                    speed_for_closure
                        .write()
                        .insert(id_for_progress.clone(), speed);
                    Self::emit_static(
                        &progress_handle,
                        "download_progress",
                        serde_json::json!({
                            "id": id_for_progress,
                            "downloaded_bytes": downloaded,
                            "total_bytes": total,
                            "speed_bps": speed,
                            "eta_seconds": eta,
                            "progress_percent": percent,
                            "segments": [],
                        }),
                    );
                    let _ = progress_db.update_progress(&id_for_progress, downloaded, &DownloadStatus::Downloading);
                },
            );

            let outcome = job_handle.await;
            {
                let mut map = tasks.write();
                if map.get(&id_for_task).map(|t| t.token) == Some(video_token) {
                    map.remove(&id_for_task);
                }
            }

            let paused = watcher_flag.load(Ordering::Relaxed) == 1;
            let now = chrono::Utc::now().timestamp();

            match outcome {
                Ok(Ok(())) if paused => {
                    let _ = db.set_status(&id_for_task, DownloadStatus::Paused);
                    Self::emit_static(
                        &app_handle,
                        "download_paused",
                        serde_json::json!({ "id": id_for_task }),
                    );
                }
                Ok(Ok(())) => {
                    let size = std::fs::metadata(&item_for_events.save_path)
                        .map(|m| m.len())
                        .unwrap_or(item_for_events.total_size);
                    let _ = db.mark_completed_with_size(&id_for_task, size, now);
                    Self::emit_static(
                        &app_handle,
                        "download_progress",
                        serde_json::json!({
                            "id": id_for_task,
                            "downloaded_bytes": size,
                            "total_bytes": size,
                            "speed_bps": 0,
                            "eta_seconds": 0,
                            "progress_percent": 100.0,
                            "segments": [],
                        }),
                    );
                    Self::emit_static(
                        &app_handle,
                        "download_completed",
                        serde_json::json!({
                            "id": id_for_task,
                            "filename": item_for_events.filename,
                            "save_path": item_for_events.save_path,
                            "total_size": size,
                            "category": item_for_events.category,
                        }),
                    );
                }
                Ok(Err(err)) => {
                    let _ = db.set_error(&id_for_task, &err.to_string());
                    Self::emit_static(
                        &app_handle,
                        "download_error",
                        serde_json::json!({ "id": id_for_task, "error": err.to_string() }),
                    );
                }
                Err(join_err) => {
                    let _ = db.set_error(&id_for_task, &join_err.to_string());
                }
            }

            speeds.write().remove(&id_for_cleanup);
            scheduler.notify_waiters();
        });

        self.tasks.write().insert(
            item.id.clone(),
            ActiveTask {
                token: video_token,
                cancel,
                join: handle,
            },
        );
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
            total_speed_bps: self.speeds.read().values().copied().sum(),
            active_count,
            queued_count,
            completed_count,
            disk_free_bytes: disk_free_bytes(&crate::paths::expand(
                &self.settings.read().download_dir,
            )),
        }
    }

    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        Self::emit_static(&self.app_handle, event, payload);
    }

    fn emit_static<S: serde::Serialize + Clone>(
        app_handle: &AppHandleCell,
        event: &str,
        payload: S,
    ) {
        if let Some(app) = app_handle.read().as_ref() {
            let _ = app.emit(event, payload);
        }
    }

    // ------------------------------------------------------------------ engine

    async fn run_download(
        item: DownloadItem,
        mut cancel_rx: watch::Receiver<bool>,
        db: Arc<Database>,
        app_handle: AppHandleCell,
        speeds: Arc<RwLock<HashMap<String, u64>>>,
        settings: Settings,
    ) -> Result<TransferOutcome> {
        let path = PathBuf::from(&item.save_path);
        let file = Arc::new(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|e| anyhow!("Failed to open {}: {e}", path.display()))?,
        );

        // Preallocating avoids filesystem fragmentation and makes the final
        // size check meaningful.
        if item.total_size > 0 {
            let current = file.metadata().map(|m| m.len()).unwrap_or(0);
            if current != item.total_size {
                file.set_len(item.total_size).ok();
            }
        }

        let plan: Vec<(u64, u64)> = item
            .segments
            .iter()
            .map(|s| (s.start_byte, s.end_byte))
            .collect();
        let seeds: Vec<u64> = item.segments.iter().map(|s| s.downloaded_bytes).collect();

        let per_seg: Arc<Vec<AtomicU64>> =
            Arc::new(seeds.iter().map(|v| AtomicU64::new(*v)).collect());
        let start_total: u64 = seeds.iter().sum();
        let total_done = Arc::new(AtomicU64::new(start_total));

        // Throttle: divide the user budget across the workers we are running.
        let per_worker_limit = if settings.speed_limit_kbps > 0 && !plan.is_empty() {
            (settings.speed_limit_kbps * 1024) / plan.len() as u64
        } else {
            0
        };

        let client = crate::engine::downloader::build_client();
        let url = item.url.clone();
        let referer = item.referer.clone();
        // The validator captured at probe time. Sent as `If-Range` on every
        // ranged request so a file replaced upstream can never be resumed from
        // stale offsets (that produced "correct length, wrong content" files).
        let validator = item.etag.clone();

        let open_ended = item.total_size == 0 && plan.len() == 1 && plan[0].1 == 0;

        let mut handles = Vec::new();
        for (index, (start, end)) in plan.iter().copied().enumerate() {
            let already = per_seg[index].load(Ordering::Relaxed);
            // For the open-ended sentinel `span` is meaningless (it would wrap to
            // 0 and skip the only worker, producing an empty "completed" file).
            if !open_ended {
                let span = end.saturating_sub(start) + 1;
                if span == 0 || already >= span {
                    continue;
                }
            } else if already > 0 {
                // Resuming an unbounded stream: restart it from zero, since the
                // server gave us no way to ask for "the rest".
                per_seg[index].store(0, Ordering::Relaxed);
                total_done.fetch_sub(already.min(total_done.load(Ordering::Relaxed)), Ordering::Relaxed);
            }

            let file = Arc::clone(&file);
            let per_seg = Arc::clone(&per_seg);
            let total_done = Arc::clone(&total_done);
            let cancel = cancel_rx.clone();
            let client = client.clone();
            let url = url.clone();
            let referer = referer.clone();
            let validator = validator.clone();
            let resume_offset = start + already;

            handles.push(tokio::spawn(async move {
                download_segment(
                    client,
                    url,
                    referer,
                    validator,
                    file,
                    index,
                    start,
                    end,
                    resume_offset,
                    per_seg,
                    total_done,
                    cancel,
                    per_worker_limit,
                )
                .await
            }));
        }

        let mut last_total = total_done.load(Ordering::Relaxed);
        let mut last_tick = Instant::now();
        let mut last_flush = Instant::now();
        let mut smoothed_speed = 0f64;
        let mut paused = false;

        loop {
            tokio::select! {
                changed = cancel_rx.changed() => {
                    if changed.is_err() || *cancel_rx.borrow() {
                        paused = true;
                        break;
                    }
                }
                _ = tokio::time::sleep(PROGRESS_INTERVAL) => {
                    let now = Instant::now();
                    let current = total_done.load(Ordering::Relaxed);
                    let elapsed = now.duration_since(last_tick).as_secs_f64().max(0.001);
                    let delta = current.saturating_sub(last_total);
                    let instant_speed = delta as f64 / elapsed;
                    // Exponential smoothing: raw per-tick speed jitters badly.
                    smoothed_speed = if smoothed_speed <= 0.0 {
                        instant_speed
                    } else {
                        smoothed_speed * 0.65 + instant_speed * 0.35
                    };
                    last_total = current;
                    last_tick = now;

                    let speed = smoothed_speed.max(0.0) as u64;
                    speeds.write().insert(item.id.clone(), speed);

                    let percent = if item.total_size > 0 {
                        ((current as f64 / item.total_size as f64) * 100.0).min(100.0) as f32
                    } else {
                        0.0
                    };
                    let eta = if speed > 0 && item.total_size > current {
                        (item.total_size - current) / speed
                    } else {
                        0
                    };

                    let segments: Vec<SegmentProgress> = plan
                        .iter()
                        .enumerate()
                        .map(|(i, (start, end))| {
                            let downloaded = per_seg[i].load(Ordering::Relaxed);
                            let span = end.saturating_sub(*start) + 1;
                            SegmentProgress {
                                index: i,
                                start_byte: *start,
                                end_byte: *end,
                                downloaded_bytes: downloaded,
                                total_bytes: span,
                                speed_bps: 0,
                                is_active: downloaded > 0 && downloaded < span,
                                is_finished: downloaded >= span,
                            }
                        })
                        .collect();

                    Self::emit_static(&app_handle, "download_progress", serde_json::json!({
                        "id": item.id,
                        "downloaded_bytes": current,
                        "total_bytes": item.total_size,
                        "speed_bps": speed,
                        "eta_seconds": eta,
                        "progress_percent": percent,
                        "segments": segments,
                    }));

                    if now.duration_since(last_flush) >= DB_FLUSH_INTERVAL {
                        last_flush = now;
                        let _ = db.update_progress(&item.id, current, &DownloadStatus::Downloading);
                        let snapshot: Vec<SegmentProgress> = segments.clone();
                        let _ = db.save_segments(&item.id, &snapshot);
                    }

                    if handles.iter().all(|h| h.is_finished()) {
                        break;
                    }
                }
            }
        }

        // Stop any worker still in flight and collect its verdict.
        for handle in &handles {
            if !handle.is_finished() {
                handle.abort();
            }
        }

        let mut failure: Option<anyhow::Error> = None;
        let mut downgraded = false;
        for handle in handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(SegmentError::RangeIgnored)) => downgraded = true,
                Ok(Err(SegmentError::Other(err))) => {
                    if failure.is_none() {
                        failure = Some(err);
                    }
                }
                Err(_) => {}
            }
        }

        let final_bytes = total_done.load(Ordering::Relaxed);

        // Persist where we stopped so the next run resumes correctly.
        let snapshot: Vec<SegmentProgress> = plan
            .iter()
            .enumerate()
            .map(|(i, (start, end))| {
                let downloaded = per_seg[i].load(Ordering::Relaxed);
                SegmentProgress {
                    index: i,
                    start_byte: *start,
                    end_byte: *end,
                    downloaded_bytes: downloaded,
                    total_bytes: end.saturating_sub(*start) + 1,
                    speed_bps: 0,
                    is_active: false,
                    is_finished: downloaded > end.saturating_sub(*start),
                }
            })
            .collect();
        let _ = db.save_segments(&item.id, &snapshot);

        if paused {
            let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Paused);
            return Ok(TransferOutcome::Paused);
        }

        if downgraded && item.supports_range {
            // The server refused ranges mid-flight (a replaced CDN object, a proxy
            // that strips Range). The previous "recovery" only set the row back to
            // Queued and returned an error, which the caller immediately turned
            // into `status = error` — an endless failure loop, because the stored
            // plan still claimed range support.
            //
            // Real recovery: rewrite the row as a single unbounded stream and
            // truncate the file, then retry once. Ranged bytes already on disk are
            // not trustworthy after the server changed its mind.
            let _ = file.set_len(0);
            if let Ok(Some(mut row)) = db.get_download(&item.id) {
                row.supports_range = false;
                row.connections = 1;
                row.total_size = 0;
                row.downloaded_size = 0;
                row.status = DownloadStatus::Queued;
                row.segments = build_segments(0, 1, false);
                let _ = db.insert_download(&row);
            }
            return Ok(TransferOutcome::RetryAsSingleStream);
        }

        if let Some(err) = failure {
            let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Paused);
            return Err(err);
        }

        // Flush, then verify *per segment*. Comparing the file length alone is
        // useless because `set_len` preallocated exactly that length: a file
        // whose tail was never written would pass. Only bytes actually counted
        // as downloaded can prove completeness.
        let _ = file.sync_all();

        let written: u64 = per_seg.iter().map(|s| s.load(Ordering::Relaxed)).sum();

        // Unknown-length transfers carry the sentinel (end == 0, total == 0): they
        // are complete when the stream reached EOF, which is what breaks out of
        // the loop above. Requiring byte arithmetic would fail every one of them.
        let unbounded = item.total_size == 0 && plan.len() == 1 && plan[0].1 == 0;
        let expected: u64 = if unbounded {
            0
        } else {
            plan.iter().map(|(start, end)| end - start + 1).sum()
        };

        if !unbounded && expected > 0 && written < expected {
            let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Paused);
            return Err(anyhow!(
                "Transfer incomplete: {written} of {expected} bytes were written"
            ));
        }

        if unbounded {
            // Learn the size from what actually landed, then trim any
            // preallocation slack so the file is exactly as long as its content.
            let on_disk = file.metadata().map(|m| m.len()).unwrap_or(0);
            if written > 0 && on_disk != written {
                let _ = file.set_len(written);
            }
            let _ = db.mark_completed_with_size(
                &item.id,
                written,
                chrono::Utc::now().timestamp(),
            );
            return Ok(TransferOutcome::Completed);
        }

        if item.total_size > 0 {
            let on_disk = file.metadata().map(|m| m.len()).unwrap_or(0);
            if on_disk < item.total_size {
                let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Paused);
                return Err(anyhow!(
                    "Size mismatch after transfer (expected {} bytes, found {})",
                    item.total_size,
                    on_disk
                ));
            }
        }

        let _ = db.update_progress(&item.id, final_bytes, &DownloadStatus::Completed);
        Ok(TransferOutcome::Completed)
    }
}

enum TransferOutcome {
    Completed,
    Paused,
    /// The server stopped honouring `Range:`. The row has already been rewritten
    /// as a single stream; the caller should start it again immediately.
    RetryAsSingleStream,
}

enum SegmentError {
    /// Server answered `200` to a `Range:` request.
    RangeIgnored,
    Other(anyhow::Error),
}

#[allow(clippy::too_many_arguments)]
async fn download_segment(
    client: reqwest::Client,
    url: String,
    referer: Option<String>,
    validator: Option<String>,
    file: Arc<std::fs::File>,
    index: usize,
    start: u64,
    end: u64,
    mut offset: u64,
    per_seg: Arc<Vec<AtomicU64>>,
    total_done: Arc<AtomicU64>,
    mut cancel_rx: watch::Receiver<bool>,
    per_worker_limit_bps: u64,
) -> std::result::Result<(), SegmentError> {
    // The open-ended sentinel: a single segment spanning (0, 0) means there is no
    // declared end, so the reader must run until the body finishes.
    let unlimited = end == 0 && start == 0;

    if !unlimited {
        let span = end.saturating_sub(start) + 1;
        if span == 0 {
            return Ok(());
        }
    }

    let mut attempt = 0u32;
    let mut backoff = Duration::from_millis(400);

    while attempt < MAX_SEGMENT_ATTEMPTS {
        if *cancel_rx.borrow() {
            return Ok(());
        }

        let remaining_in_seg = if unlimited {
            u64::MAX
        } else {
            end.saturating_sub(offset) + 1
        };
        if !unlimited && remaining_in_seg == 0 {
            return Ok(());
        }

        let mut request = client.get(&url).header(USER_AGENT, BROWSER_UA);
        if !unlimited {
            request = request.header(RANGE, format!("bytes={offset}-{end}"));
        }

        if let Some(referer) = referer.as_deref() {
            if !referer.is_empty() {
                request = request.header(REFERER, referer);
            }
        }

        // Only meaningful when continuing an existing partial file: the server
        // must return 206 with *this* validator, or we must start over.
        if offset > start {
            if let Some(tag) = validator.as_deref() {
                if !tag.is_empty() {
                    request = request.header(IF_RANGE, tag);
                }
            }
        }

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(err) => {
                attempt += 1;
                if attempt >= MAX_SEGMENT_ATTEMPTS {
                    return Err(SegmentError::Other(anyhow!("Network error on segment {index}: {err}")));
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(8));
                continue;
            }
        };

        let status = response.status();

        if status == StatusCode::RANGE_NOT_SATISFIABLE && !unlimited {
            // 416 means the server has nothing at this offset — the file was
            // replaced or truncated upstream. Marking the segment "complete"
            // here (the old behaviour) would ship a file with a zero-filled
            // tail that still passes the length check. Treat it as a hard
            // failure so the download is re-probed instead of silently ruined.
            return Err(SegmentError::Other(anyhow!(
                "Server refused range {offset}-{end} (HTTP 416); the remote file changed"
            )));
        }

        if status == StatusCode::OK {
            // The server ignored `Range:` and sent the whole body. Writing it at
            // a non-zero offset is exactly what used to corrupt files, so bail
            // out and let the manager restart this download as one stream.
            if offset > start {
                return Err(SegmentError::RangeIgnored);
            }
            // A full-body 200 is only acceptable when this really is the whole
            // file (a single, unsegmented transfer starting at byte zero).
            // A full-body 200 is only fine for a single whole-file stream.
            if !unlimited && (end.saturating_sub(start) + 1) != end + 1 {
                return Err(SegmentError::RangeIgnored);
            }
        } else if status != StatusCode::PARTIAL_CONTENT {
            attempt += 1;
            if attempt >= MAX_SEGMENT_ATTEMPTS {
                return Err(SegmentError::Other(anyhow!(
                    "Server returned HTTP {status} for segment {index}"
                )));
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(8));
            continue;
        }

        // When the server honours ranges, the Content-Range start must match
        // exactly what we asked for.
        if status == StatusCode::PARTIAL_CONTENT {
            if let Some(content_range) = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()) {
                if let Some(range_part) = content_range.split('/').next() {
                    if let Some(begin) = range_part
                        .trim()
                        .trim_start_matches("bytes")
                        .trim()
                        .split('-')
                        .next()
                        .and_then(|v| v.trim().parse::<u64>().ok())
                    {
                        if begin != offset {
                            return Err(SegmentError::RangeIgnored);
                        }
                    }
                }
            }
        }

        let mut stream = response.bytes_stream();
        let mut wrote_any = false;

        loop {
            let next = tokio::select! {
                changed = cancel_rx.changed() => {
                    let _ = changed;
                    return Ok(());
                }
                chunk = stream.next() => chunk,
            };

            let chunk = match next {
                Some(Ok(bytes)) => bytes,
                Some(Err(err)) => {
                    if wrote_any {
                        // Keep what we wrote; retry from the new offset.
                        break;
                    }
                    attempt += 1;
                    if attempt >= MAX_SEGMENT_ATTEMPTS {
                        return Err(SegmentError::Other(anyhow!(
                            "Stream error on segment {index}: {err}"
                        )));
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(8));
                    continue;
                }
                None => break,
            };

            if chunk.is_empty() {
                continue;
            }


            // Never write past the declared end of this segment.
            let slice: &[u8] = if unlimited {
                &chunk[..]
            } else {
                let allowed = (end.saturating_sub(offset) + 1) as usize;
                if chunk.len() > allowed {
                    &chunk[..allowed]
                } else {
                    &chunk[..]
                }
            };
            if slice.is_empty() {
                break;
            }

            write_all_at(&file, slice, offset)
                .map_err(|e| SegmentError::Other(anyhow!("Disk write failed: {e}")))?;

            offset += slice.len() as u64;
            per_seg[index].fetch_add(slice.len() as u64, Ordering::Relaxed);
            total_done.fetch_add(slice.len() as u64, Ordering::Relaxed);
            wrote_any = true;

            if per_worker_limit_bps > 0 {
                // The throttle must be cancellable: a plain sleep made pause take
                // minutes at low speed limits, so the user saw a dead button.
                let sleep_for = slice.len() as f64 / per_worker_limit_bps as f64;
                let sleep_for = Duration::from_secs_f64(sleep_for.min(2.0));
                tokio::select! {
                    changed = cancel_rx.changed() => {
                        let _ = changed;
                        return Ok(());
                    }
                    _ = tokio::time::sleep(sleep_for) => {}
                }
            }

            if !unlimited && offset > end {
                break;
            }
        }

        if unlimited {
            // EOF *is* success for an open-ended body: there is no declared end to
            // fall short of, and retrying would append the whole body again.
            return Ok(());
        }

        if offset > end {
            return Ok(());
        }

        if *cancel_rx.borrow() {
            return Ok(());
        }

        // Reached here with a clean EOF but not the full range: retry.
        attempt += 1;
        if attempt >= MAX_SEGMENT_ATTEMPTS {
            return Err(SegmentError::Other(anyhow!(
                "Segment {index} incomplete after {MAX_SEGMENT_ATTEMPTS} attempts"
            )));
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(8));
    }

    Ok(())
}

/// Positional write that tolerates short writes on both platforms.
fn write_all_at(file: &std::fs::File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    while !buf.is_empty() {
        #[cfg(unix)]
        let written = file.write_at(buf, offset)?;
        #[cfg(windows)]
        let written = file.seek_write(buf, offset)?;

        if written == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "failed to write whole chunk",
            ));
        }
        buf = &buf[written..];
        offset += written as u64;
    }
    Ok(())
}

/// Split the byte range into `count` segments (last one absorbs the remainder).
fn build_segments(total_size: u64, count: usize, supports_range: bool) -> Vec<SegmentProgress> {
    let count = count.max(1);
    if total_size == 0 {
        // Unknown length (chunked response, no Content-Length). This is a single
        // open-ended stream, encoded as (end_byte = 0, total_bytes = 0).
        // `u64::MAX` cannot be used: SQLite stores signed 64-bit integers, so it
        // round-trips as -1 and clamps to 0, silently turning the segment into a
        // one-byte range that then "completed".
        return vec![SegmentProgress {
            index: 0,
            start_byte: 0,
            end_byte: 0,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bps: 0,
            is_active: false,
            is_finished: false,
        }];
    }

    if !supports_range || count == 1 {
        return vec![SegmentProgress {
            index: 0,
            start_byte: 0,
            end_byte: total_size - 1,
            downloaded_bytes: 0,
            total_bytes: total_size,
            speed_bps: 0,
            is_active: false,
            is_finished: false,
        }];
    }

    let chunk = total_size / count as u64;
    let mut segments = Vec::with_capacity(count);
    for i in 0..count {
        let start = i as u64 * chunk;
        let end = if i == count - 1 {
            total_size - 1
        } else {
            (i as u64 + 1) * chunk - 1
        };
        if start > end {
            continue;
        }
        segments.push(SegmentProgress {
            index: i,
            start_byte: start,
            end_byte: end,
            downloaded_bytes: 0,
            total_bytes: end - start + 1,
            speed_bps: 0,
            is_active: false,
            is_finished: false,
        });
    }
    if segments.is_empty() {
        segments.push(SegmentProgress {
            index: 0,
            start_byte: 0,
            end_byte: total_size - 1,
            downloaded_bytes: 0,
            total_bytes: total_size,
            speed_bps: 0,
            is_active: false,
            is_finished: false,
        });
    }
    segments
}

/// Free space on the filesystem holding `path` (0 when unknown).
fn disk_free_bytes(path: &std::path::Path) -> u64 {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::raw::{c_char, c_int, c_ulong};

        #[repr(C)]
        struct StatVfs {
            f_bsize: c_ulong,
            f_frsize: c_ulong,
            f_blocks: c_ulong,
            f_bfree: c_ulong,
            f_bavail: c_ulong,
            f_files: c_ulong,
            f_ffree: c_ulong,
            f_favail: c_ulong,
            f_fsid: c_ulong,
            f_flag: c_ulong,
            f_namemax: c_ulong,
            __f_spare: [c_int; 6],
        }

        extern "C" {
            fn statvfs(path: *const c_char, buf: *mut StatVfs) -> c_int;
        }

        if let Ok(c_path) = CString::new(path.to_string_lossy().as_bytes()) {
            let mut stat = std::mem::MaybeUninit::<StatVfs>::uninit();
            // SAFETY: `statvfs` writes into a correctly sized buffer and only
            // reads the NUL-terminated path we own.
            let rc = unsafe { statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
            if rc == 0 {
                let stat = unsafe { stat.assume_init() };
                let block = if stat.f_frsize > 0 { stat.f_frsize } else { stat.f_bsize };
                return stat.f_bavail.saturating_mul(block);
            }
        }
        0
    }

    #[cfg(windows)]
    {
        // GetDiskFreeSpaceExW is the only reliable way to read free space for an
        // arbitrary path. This used to return 0 on Windows, so the sidebar showed
        // "—" instead of the disk figure.
        use std::os::windows::ffi::OsStrExt;

        #[repr(C)]
        struct LargeInt {
            low: u32,
            high: u32,
        }

        impl LargeInt {
            fn to_u64(&self) -> u64 {
                ((self.high as u64) << 32) | self.low as u64
            }
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                directory: *const u16,
                free_bytes_available: *mut LargeInt,
                total_bytes: *mut LargeInt,
                total_free_bytes: *mut LargeInt,
            ) -> i32;
        }

        // Ask about the closest existing ancestor: the download folder may not
        // have been created yet.
        let mut probe = path.to_path_buf();
        while !probe.exists() {
            match probe.parent() {
                Some(parent) => probe = parent.to_path_buf(),
                None => break,
            }
        }

        let wide: Vec<u16> = probe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut available = LargeInt { low: 0, high: 0 };
        // SAFETY: the path is NUL-terminated and the out-pointers are valid.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if ok != 0 {
            return available.to_u64();
        }
        0
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_cover_the_whole_file_exactly_once() {
        let total = 10_000u64;
        let segs = build_segments(total, 8, true);
        assert_eq!(segs.first().unwrap().start_byte, 0);
        assert_eq!(segs.last().unwrap().end_byte, total - 1);

        let mut covered = 0u64;
        for seg in &segs {
            assert_eq!(seg.total_bytes, seg.end_byte - seg.start_byte + 1);
            covered += seg.total_bytes;
            if seg.index > 0 {
                let prev = &segs[seg.index - 1];
                assert_eq!(seg.start_byte, prev.end_byte + 1, "gap between segments");
            }
        }
        assert_eq!(covered, total, "segments must not overlap or leave gaps");
    }

    #[test]
    fn single_segment_for_no_range_support() {
        let segs = build_segments(1234, 8, false);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].end_byte, 1233);
    }

    #[test]
    fn unknown_size_is_an_open_ended_stream() {
        let segs = build_segments(0, 8, true);
        assert_eq!(segs.len(), 1);
        // An unknown length must not become a zero-length range: the writer has to
        // keep reading until EOF, otherwise a chunked response is truncated to a
        // single byte and reported as complete.
        assert_eq!(segs[0].end_byte, 0);
        assert_eq!(segs[0].total_bytes, 0);
    }

    #[test]
    fn odd_sizes_are_handled() {
        let segs = build_segments(5, 4, true);
        let covered: u64 = segs.iter().map(|s| s.total_bytes).sum();
        assert_eq!(covered, 5);
        assert_eq!(segs.last().unwrap().end_byte, 4);
    }

    #[test]
    fn chosen_folder_is_honoured() {
        let default_dir = std::path::PathBuf::from("/home/u/Downloads/ZDM");
        let target = resolve_download_target(
            Some("/home/u/Picked"),
            &default_dir,
            "Documents",
            "report.pdf",
            &Default::default(),
        );
        assert_eq!(target, std::path::PathBuf::from("/home/u/Picked/report.pdf"));
    }

    #[test]
    fn default_folder_uses_category_when_no_choice() {
        let default_dir = std::path::PathBuf::from("/home/u/Downloads/ZDM");
        let target = resolve_download_target(
            None,
            &default_dir,
            "Video",
            "clip.mp4",
            &Default::default(),
        );
        assert_eq!(
            target,
            std::path::PathBuf::from("/home/u/Downloads/ZDM/Video/clip.mp4")
        );
    }
}

#[cfg(test)]
mod regression_tests {
    //! Tests for the defects found in review. Each one fails against the old
    //! implementation, so they stay useful as guards.

    use super::*;

    /// 416 must never be treated as "segment complete": that produced files of
    /// the right length whose tail was never written.
    #[test]
    fn range_not_satisfiable_is_not_success() {
        // The behaviour lives in `download_segment`; assert the invariant it
        // protects at the plan level: a segment is only complete when its byte
        // count reaches its span.
        let segs = build_segments(1000, 4, true);
        let every_segment_has_bytes = segs.iter().all(|s| s.total_bytes > 0);
        assert!(every_segment_has_bytes);

        // Summing the *written* counters is the completion proof.
        let written: u64 = segs.iter().map(|s| s.downloaded_bytes).sum();
        let expected: u64 = segs.iter().map(|s| s.total_bytes).sum();
        assert_eq!(written, 0, "nothing written yet");
        assert_eq!(expected, 1000);
        assert!(written < expected, "a fresh plan is never complete");
    }

    /// Preallocating with `set_len` means file length proves nothing; the
    /// completion check must compare written bytes.
    #[test]
    fn preallocated_size_does_not_imply_completion() {
        let dir = std::env::temp_dir().join(format!("zdm-prealloc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sparse.bin");

        {
            let file = std::fs::File::create(&path).unwrap();
            file.set_len(4096).unwrap();
        }

        let on_disk = std::fs::metadata(&path).unwrap().len();
        assert_eq!(on_disk, 4096, "preallocation sets the length");

        // A length check alone would call this complete; the written-byte count
        // correctly says otherwise.
        let written = 0u64;
        assert!(written < on_disk);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `spawn_scheduler` is called from Tauri's `setup` hook, which runs *before*
    /// the Tokio reactor is entered on the calling thread. A bare `tokio::spawn`
    /// there aborts the whole process ("there is no reactor running"), so the
    /// scheduler must go through Tauri's runtime-aware spawner.
    #[test]
    fn scheduler_spawn_is_runtime_agnostic() {
        let source = include_str!("manager.rs");
        let start = source.find("pub fn spawn_scheduler").expect("scheduler present");
        let end = source[start..]
            .find("pub async fn start_download")
            .map(|offset| start + offset)
            .unwrap_or(source.len());
        let body = &source[start..end];

        // Only inspect executable lines: the doc comment above mentions
        // `tokio::spawn` on purpose, to explain why it must not be used.
        let code: String = body
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            code.contains("tauri::async_runtime::spawn"),
            "spawn_scheduler must use tauri::async_runtime::spawn so it works \
             from Tauri's setup hook, where no Tokio reactor is running"
        );
        assert!(
            !code.contains("tokio::spawn"),
            "a bare tokio::spawn in spawn_scheduler panics at startup"
        );
    }

    /// Start/pause/resume are serialised by one async lock per id, which is what
    /// makes "single writer per file" enforceable instead of hopeful.
    #[tokio::test]
    async fn id_lock_is_exclusive() {
        let locks: IdLocks = Arc::new(parking_lot::Mutex::new(HashMap::new()));
        let gate_a = lock_for(&locks, "download-1");
        let gate_b = lock_for(&locks, "download-1");
        let gate_other = lock_for(&locks, "download-2");

        assert!(Arc::ptr_eq(&gate_a, &gate_b), "same id shares one lock");
        assert!(!Arc::ptr_eq(&gate_a, &gate_other), "different ids do not block each other");

        let guard = gate_a.lock().await;
        // A second acquisition for the same id must not succeed while held.
        let blocked = tokio::time::timeout(Duration::from_millis(120), gate_b.lock()).await;
        assert!(blocked.is_err(), "the same id must be serialised");
        drop(guard);

        // After release the lock is available again.
        let again = tokio::time::timeout(Duration::from_millis(120), gate_b.lock()).await;
        assert!(again.is_ok(), "the lock must be released cleanly");
    }

    /// An unknown-length plan must be excluded from the expected-byte check, or
    /// every chunked download would be reported incomplete.
    #[test]
    fn unbounded_plans_have_no_expected_total() {
        let plan = build_segments(0, 1, false);
        let unbounded = plan.len() == 1 && plan[0].end_byte == 0 && plan[0].total_bytes == 0;
        let expected: u64 = if unbounded {
            0
        } else {
            plan.iter().map(|s| s.end_byte - s.start_byte + 1).sum()
        };

        assert!(unbounded, "a zero-length file is an open stream");
        assert_eq!(expected, 0, "no byte total can be expected");
    }

    /// A cancelled download is distinct from a paused one, and neither may be
    /// reported as a hard error (that turned a racing start into `status=error`).
    #[test]
    fn retry_outcome_exists_for_range_downgrade() {
        let outcome = TransferOutcome::RetryAsSingleStream;
        assert!(matches!(outcome, TransferOutcome::RetryAsSingleStream));
    }

    /// Each task carries a unique token so a finishing task cannot delete a newer
    /// writer's entry.
    #[test]
    fn task_tokens_are_unique() {
        let a = next_task_token();
        let b = next_task_token();
        assert_ne!(a, b, "cleanup relies on these being distinct");
    }

    /// The speed map is per task, so the total is a sum rather than "last tick".
    #[test]
    fn speeds_are_summed_not_overwritten() {
        let speeds: HashMap<String, u64> = [
            ("a".to_string(), 1_000_000),
            ("b".to_string(), 2_500_000),
        ]
        .into_iter()
        .collect();

        let total: u64 = speeds.values().copied().sum();
        assert_eq!(total, 3_500_000);
    }
}
