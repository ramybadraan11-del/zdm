//! ZDM Tauri Commands
//! Developed by Ramy Badraan
//!
//! Thin, typed wrappers over the engine. Every command returns `Result<_, String>`
//! because that is what the frontend can display; the engine keeps `anyhow`.

use crate::browser_installer::{
    get_installed_browsers_list, launch_browser_with_extension, repair_browser_integration,
    BrowserStatus, IntegrationReport,
};
use crate::engine::manager::DownloadManager;
use crate::engine::types::{
    DownloadItem, DownloadStatus, EngineStats, NewDownloadRequest, Settings, UrlMetadata,
};
use crate::video::{query_playlist, query_video, PlaylistInfo, VideoInfo};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CliPayload {
    pub kind: String,
    pub value: Option<String>,
}

pub struct PendingCli(pub std::sync::Mutex<Option<CliPayload>>);

#[tauri::command]
pub fn get_pending_cli(state: State<'_, PendingCli>) -> Option<CliPayload> {
    state.0.lock().unwrap().take()
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub developer: String,
    pub license: String,
    pub architecture: String,
    pub os: String,
    pub engine: String,
    pub data_dir: String,
    pub download_dir: String,
    pub platform: String,
}

#[tauri::command]
pub async fn probe_url(
    url: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<UrlMetadata, String> {
    // A torrent has no Content-Length to probe over HTTP: `https://…/x.torrent`
    // would be fetched as an ordinary file and reported with the wrong size and
    // type. Describe it honestly instead, so the dialog can offer "Add torrent".
    if crate::torrent::looks_like_torrent(&url) {
        let filename = crate::torrent::display_name_of(&url)
            .or_else(|| crate::torrent::magnet_display_name(&url))
            .unwrap_or_else(|| "torrent.torrent".to_string());
        return Ok(UrlMetadata {
            url,
            filename,
            total_size: 0,
            supports_range: false,
            mime_type: "application/x-bittorrent".to_string(),
            category: "Torrent".to_string(),
            etag: None,
        });
    }

    manager.probe(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_download(
    req: NewDownloadRequest,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<DownloadItem, String> {
    // A magnet or .torrent capture is not an HTTP transfer: it has no byte ranges
    // and no connections to multiply, so it goes to the BitTorrent engine.
    if crate::torrent::looks_like_torrent(&req.url) {
        return Err("This is a torrent — use Add torrent.".to_string());
    }

    manager.add_download(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_download(
    id: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    manager.start_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_download(
    id: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    manager.pause_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_download(
    id: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    manager.resume_download(&id).await.map_err(|e| e.to_string())
}

/// Abort an in-flight transfer and keep the partial file, so the user can decide
/// later. Deleting the file is a separate, explicit action (`delete_download`).
#[tauri::command]
pub async fn cancel_download(
    id: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    // A torrent is stopped through its session: it has no task in the HTTP table,
    // so the generic path below would only mark the row cancelled while the
    // BitTorrent transfer kept writing.
    if let Some(hash) = id.strip_prefix("torrent-") {
        // Stop the transfer first, so no piece is written after the row says
        // "cancelled". `cancel_download` below only touches the HTTP task table
        // and the row, neither of which owns a BitTorrent transfer.
        let _ = manager.torrents.set_paused(hash, true).await;
        return manager
            .cancel_download(&id)
            .await
            .map_err(|e| e.to_string());
    }

    manager
        .cancel_download(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_download(
    id: String,
    delete_file: bool,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    // A torrent is owned by the BitTorrent session, not by the row: deleting the
    // row alone left the transfer running in the background, still writing to
    // disk, with no card to pause or cancel it.
    //
    // The files must also be deleted by the session, not by the row: a torrent's
    // `save_path` is a folder (often with many files), so the single-file removal
    // inside `delete_download` could only ever fail, and "delete with file" left
    // the payload on disk while the card disappeared.
    if let Some(hash) = id.strip_prefix("torrent-") {
        manager
            .torrents
            .remove(hash, delete_file)
            .await
            .map_err(|error| format!("could not remove the torrent: {error}"))?;

        // The payload is already handled; only the row is left to drop.
        return manager
            .delete_download(&id, false)
            .await
            .map_err(|e| e.to_string());
    }

    manager
        .delete_download(&id, delete_file)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_downloads(
    ids: Vec<String>,
    delete_files: bool,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    for id in ids {
        if let Some(hash) = id.strip_prefix("torrent-") {
            let _ = manager.torrents.remove(hash, delete_files).await;
            let _ = manager.delete_download(&id, false).await;
        } else {
            let _ = manager.delete_download(&id, delete_files).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn update_download_url(
    id: String,
    new_url: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    let trimmed = new_url.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    manager.update_download_url(&id, trimmed).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restart_download(
    id: String,
    app: tauri::AppHandle,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<(), String> {
    if let Some(hash) = id.strip_prefix("torrent-").map(|s| s.to_string()) {
        let _ = manager.torrents.remove(&hash, true).await;
        if let Ok(Some(item)) = manager.db().get_download(&id) {
            let _ = manager.db().reset_download_progress(&id);
            let folder = item.save_path.clone();
            let url = item.url.clone();
            let manager_clone = Arc::clone(&manager);
            let id_clone = id.clone();
            let app_clone = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = manager_clone.torrents.add(&url, &folder, false).await {
                    eprintln!("[ZDM torrent] restart add failed: {e}");
                    let _ = manager_clone.record_progress(&id_clone, 0, &DownloadStatus::Error);
                    let _ = app_clone.emit("download_error", serde_json::json!({ "id": id_clone, "error": e }));
                }
            });
            crate::commands::spawn_torrent_watch(app, Arc::clone(&manager), id, hash);
        }
        return Ok(());
    }

    manager.restart_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_all(manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    let ids: Vec<String> = manager
        .get_all_downloads()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|d| {
            matches!(
                d.status,
                crate::engine::types::DownloadStatus::Downloading
                    | crate::engine::types::DownloadStatus::Queued
            )
        })
        .map(|d| d.id)
        .collect();

    for id in ids {
        // A torrent lives in the BitTorrent session, not in the HTTP task table:
        // `pause_download` would find no task, label the row paused, and leave the
        // transfer running. The Space shortcut and the tray "pause all" must stop
        // every lane, so torrents take their own route.
        if id.starts_with("torrent-") {
            let hash = id.trim_start_matches("torrent-");
            let _ = manager.torrents.set_paused(hash, true).await;
            let _ = manager.record_progress(
                &id,
                manager
                    .get_all_downloads()
                    .ok()
                    .and_then(|items| items.into_iter().find(|item| item.id == id))
                    .map(|item| item.downloaded_size)
                    .unwrap_or(0),
                &crate::engine::types::DownloadStatus::Paused,
            );
            continue;
        }

        let _ = manager.pause_download(&id).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_downloads(
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<Vec<DownloadItem>, String> {
    manager.get_all_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_stats(manager: State<'_, Arc<DownloadManager>>) -> Result<EngineStats, String> {
    Ok(manager.get_stats())
}

#[tauri::command]
pub async fn query_video_info(url: String) -> Result<VideoInfo, String> {
    query_video(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_video_download(
    app: tauri::AppHandle,
    registry: State<'_, std::sync::Arc<crate::transfer::TransferRegistry>>,
    url: String,
    format_id: String,
    title: String,
    ext: String,
    is_audio: bool,
    est_size: Option<u64>,
    custom_filename: Option<String>,
    force_overwrite: Option<bool>,
    open_window: Option<bool>,
    folder: Option<String>,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<DownloadItem, String> {
    let item = manager
        .start_video_download(
            &url,
            &format_id,
            &title,
            &ext,
            is_audio,
            est_size,
            custom_filename,
            force_overwrite.unwrap_or(false),
            folder,
        )
        .await
        .map_err(|e| e.to_string())?;

    // A video started from the prompt must keep its own transfer window; without
    // this the prompt closed and the download continued invisibly inside the
    // manager, which is what the user complained about.
    if open_window.unwrap_or(true) {
        let _ = crate::transfer::open(&app, &registry, &item.id, &item.filename, false);
    }

    crate::prompt::close_all(&app);
    Ok(item)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistDownloadEntry {
    pub url: String,
    pub title: String,
}

#[tauri::command]
pub async fn query_playlist_info(url: String) -> Result<PlaylistInfo, String> {
    query_playlist(&url).await.map_err(|e| e.to_string())
}

/// Download every video in a playlist, one after another.
///
/// The list was already analysed, so it is reused instead of fetched again.
/// Downloads run sequentially on purpose: firing a whole playlist at the source
/// at once invites rate limiting and ignores the user's queue expectations.
#[tauri::command]
pub async fn start_playlist_download(
    entries: Vec<PlaylistDownloadEntry>,
    height: u64,
    folder: Option<String>,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<usize, String> {
    if entries.is_empty() {
        return Err("The playlist has no videos to download.".to_string());
    }
    if height == 0 {
        return Err("Choose a quality first.".to_string());
    }

    let manager = manager.inner().clone();
    let folder = folder.filter(|path| !path.trim().is_empty());
    let count = entries.len();
    let format_id = format!("video_{height}");

    tauri::async_runtime::spawn(async move {
        for entry in entries {
            let title = if entry.title.trim().is_empty() {
                "video".to_string()
            } else {
                entry.title.trim().to_string()
            };

            let item = match manager
                .start_video_download(
                    &entry.url,
                    &format_id,
                    &title,
                    "mp4",
                    false,
                    None,
                    Some(title.clone()),
                    false,
                    folder.clone(),
                )
                .await
            {
                Ok(item) => item,
                Err(error) => {
                    eprintln!("[ZDM playlist] {}: {error}", entry.url);
                    continue;
                }
            };

            // Wait for this video before starting the next one.
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                let status = manager
                    .db()
                    .get_download(&item.id)
                    .ok()
                    .flatten()
                    .map(|download| download.status);
                match status {
                    Some(DownloadStatus::Completed)
                    | Some(DownloadStatus::Error)
                    | Some(DownloadStatus::Cancelled)
                    | None => break,
                    _ => {}
                }
            }
        }
    });

    Ok(count)
}

#[derive(Serialize)]
pub struct FileConflictInfo {
    pub exists: bool,
    pub existing_path: String,
    pub existing_size: u64,
    pub suggested_name: String,
}

#[tauri::command]
pub fn check_file_conflict(
    filename: String,
    category: String,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<FileConflictInfo, String> {
    let settings = manager.settings();
    let dir = crate::paths::category_dir(
        &crate::paths::expand(&settings.download_dir),
        &category,
        &settings.category_dirs,
    );
    let target = dir.join(&filename);

    if target.exists() {
        let size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
        let unique = crate::engine::downloader::unique_path(&dir, &filename);
        let suggested = unique
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(filename);
        Ok(FileConflictInfo {
            exists: true,
            existing_path: target.to_string_lossy().to_string(),
            existing_size: size,
            suggested_name: suggested,
        })
    } else {
        Ok(FileConflictInfo {
            exists: false,
            existing_path: target.to_string_lossy().to_string(),
            existing_size: 0,
            suggested_name: filename,
        })
    }
}

#[tauri::command]
pub async fn update_yt_dlp_engine() -> Result<String, String> {
    let path = crate::video::ensure_yt_dlp()
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_settings(manager: State<'_, Arc<DownloadManager>>) -> Settings {
    manager.settings()
}

#[tauri::command]
pub fn save_settings(
    settings: Settings,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<Settings, String> {
    manager.update_settings(settings).map_err(|e| e.to_string())
}

/// Ensure ffmpeg exists, downloading it if the machine has none.
#[tauri::command]
pub async fn install_ffmpeg() -> Result<String, String> {
    let dir = crate::ffmpeg::ensure().await.map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_browsers() -> Vec<BrowserStatus> {
    get_installed_browsers_list()
}

#[tauri::command]
pub async fn register_all_browsers() -> Result<IntegrationReport, String> {
    // Off the UI thread: this writes files and, on Windows, shells out to `reg`.
    tauri::async_runtime::spawn_blocking(crate::browser_installer::repair_now)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn repair_browser(browser_id: String) -> Result<IntegrationReport, String> {
    Ok(repair_browser_integration(&browser_id))
}

#[tauri::command]
pub fn launch_browser_with_ext(browser_id: String) -> Result<(), String> {
    launch_browser_with_extension(&browser_id)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    open_path(&path, false)
}

#[tauri::command]
pub fn open_file_folder(path: String) -> Result<(), String> {
    open_path(&path, true)
}


/// Open a folder in Explorer.
///
/// `raw_arg` is used so the path is handed over exactly as written; `arg` would
/// wrap it in a second layer of quotes.
#[cfg(windows)]
fn spawn_explorer_open(folder: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let mut cmd = crate::process::std_command("explorer.exe");
    // Quoted for spaces, unquoted switch: `"C:\a b"`.
    cmd.raw_arg(format!("\"{folder}\""));
    cmd.creation_flags(0x0000_0008); // DETACHED_PROCESS
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open the folder in Explorer: {e}"))
}

/// Reveal one file in Explorer with the file selected.
///
/// The switch and the quoted path must form a single raw argument:
/// `/select,"C:\a b\file.mp4"`.
#[cfg(windows)]
fn spawn_explorer_select(file: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let mut cmd = crate::process::std_command("explorer.exe");
    cmd.raw_arg(format!("/select,\"{file}\""));
    cmd.creation_flags(0x0000_0008);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not show the file in Explorer: {e}"))
}

/// Open a path in the platform file manager, without ever spawning a console
/// window on Windows.
fn open_path(path: &str, reveal_parent: bool) -> Result<(), String> {
    let p = crate::paths::expand(path);
    let target = if reveal_parent {
        if p.is_file() {
            p.parent().unwrap_or(&p).to_path_buf()
        } else {
            p.clone()
        }
    } else {
        p.clone()
    };

    if !target.exists() {
        return Err(format!("Path does not exist: {}", target.display()));
    }

    #[cfg(windows)]
    {
        // Explorer is a shell, not a normal program — three details decide
        // whether it works or silently opens Documents instead:
        //
        //  1. `Command::arg` applies Windows quoting to the WHOLE argument, so
        //     `/select,C:\a b\file.mp4` becomes `"/select,C:\a b\file.mp4"`.
        //     Explorer cannot parse that and falls back to Documents. The switch
        //     must reach it unquoted, with only the path quoted.
        //  2. `/select,` needs a backslash path; a forward slash breaks it.
        //  3. It returns a non-zero exit code even on success.
        let file_path = p.to_string_lossy().replace('/', "\\");

        if reveal_parent && p.is_file() {
            if file_path.contains(',') {
                // A comma would split the /select argument; open the folder.
                let folder = target.to_string_lossy().replace('/', "\\");
                spawn_explorer_open(&folder)?;
            } else {
                spawn_explorer_select(&file_path)?;
            }
        } else {
            let folder = target.to_string_lossy().replace('/', "\\");
            spawn_explorer_open(&folder)?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        crate::process::std_command("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = crate::process::std_command("open");
        if reveal_parent && p.is_file() {
            cmd.arg("-R").arg(&p);
        } else {
            cmd.arg(&target);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_app_info(manager: State<'_, Arc<DownloadManager>>) -> AppInfo {
    AppInfo {
        name: "Zyzoom Download Manager (ZDM)".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        developer: "Ramy Badraan".to_string(),
        license: "Proprietary".to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        engine: "Tauri 2 • Tokio • Multi-segment direct-write".to_string(),
        data_dir: crate::paths::data_dir().to_string_lossy().to_string(),
        download_dir: manager.settings().download_dir,
        platform: if cfg!(windows) {
            "windows".to_string()
        } else if cfg!(target_os = "macos") {
            "macos".to_string()
        } else {
            "linux".to_string()
        },
    }
}

#[tauri::command]
pub async fn reveal_in_folder(path: String) -> Result<(), String> {
    open_file_folder(path)
}

/// What the browser capture can do right now: bridge up, capture enabled, and
/// which extensions are actually passed through. Surfacing this in Settings
/// turns "it silently doesn't download images anymore" into a visible list.
#[derive(Serialize)]
pub struct CaptureStatus {
    pub bridge_running: bool,
    pub bridge_port: u16,
    pub capture_downloads: bool,
    pub sniffer_enabled: bool,
    pub intercept_extensions: Vec<String>,
}

#[tauri::command]
pub fn capture_status(manager: State<'_, Arc<DownloadManager>>) -> CaptureStatus {
    let settings = manager.settings();
    CaptureStatus {
        bridge_running: crate::ipc::bridge_started(),
        bridge_port: crate::ipc::BRIDGE_PORT,
        capture_downloads: settings.capture_downloads,
        sniffer_enabled: settings.sniffer_enabled,
        intercept_extensions: settings.intercept_extensions.clone(),
    }
}

/// Diagnostic surface used by the Settings screen: tells the user, in plain
/// language, whether tray, autostart and browser integration are actually wired.
#[derive(Serialize)]
pub struct Diagnostics {
    pub tray_available: bool,
    /// Whether a usable ffmpeg was found. Video merging needs it, so the UI
    /// reports its absence instead of letting downloads fail mysteriously.
    pub ffmpeg_available: bool,
    /// Number of open transfer windows, so the user (and support) can tell whether
    /// a "missing" window is actually open behind another app.
    pub transfer_windows: usize,
    pub ffmpeg_path: Option<String>,
    pub autostart_enabled: bool,
    pub autostart_kind: String,
    pub browser_report: IntegrationReport,
    pub data_dir: String,
    pub log_file: String,
    pub desktop: String,
}

#[tauri::command]
pub fn get_diagnostics(app: tauri::AppHandle) -> Diagnostics {
    let autostart = crate::autostart::status();
    Diagnostics {
        tray_available: app.tray_by_id("main").is_some(),
        ffmpeg_available: crate::ffmpeg::is_available(),
        transfer_windows: crate::transfer::count(&app),
        ffmpeg_path: crate::ffmpeg::find(),
        autostart_enabled: autostart.enabled,
        autostart_kind: autostart.kind,
        // Cached: recomputing here rewrote every browser policy on each visit and
        // made the Settings screen freeze.
        browser_report: crate::browser_installer::cached_report(),
        data_dir: crate::paths::data_dir().to_string_lossy().to_string(),
        log_file: crate::paths::log_path().to_string_lossy().to_string(),
        desktop: std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_else(|_| "unknown".to_string()),
    }
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    crate::autostart::set_enabled(enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

/// Open (or focus) a separate transfer window for one download.
///
/// The main window stays a manager; a running transfer gets its own small window
/// with the segment ribbon, which is what makes parallel downloading legible.
#[tauri::command]
pub fn open_transfer_window(
    app: tauri::AppHandle,
    registry: State<'_, std::sync::Arc<crate::transfer::TransferRegistry>>,
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
    always_on_top: Option<bool>,
) -> Result<String, String> {
    let filename = manager
        .get_all_downloads()
        .ok()
        .and_then(|items| items.into_iter().find(|d| d.id == id))
        .map(|d| d.filename)
        .unwrap_or_default();

    crate::transfer::open(&app, &registry, &id, &filename, always_on_top.unwrap_or(false))
}

/// Bring the manager window forward (the prompt offers this, but never does it
/// automatically — that was the "downloading opens the whole app" complaint).
#[tauri::command]
pub fn show_manager(app: tauri::AppHandle) {
    crate::prompt::close_all(&app);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Close every floating prompt (after the user chose an action).
#[tauri::command]
pub fn close_prompt(app: tauri::AppHandle) {
    crate::prompt::close_all(&app);
}

/// Start a download from the floating prompt and open its transfer window.
///
/// This is the only path that begins a transfer from the prompt; the prompt never
/// writes anything until the user presses start.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_download_from_prompt(
    app: tauri::AppHandle,
    registry: State<'_, std::sync::Arc<crate::transfer::TransferRegistry>>,
    manager: State<'_, Arc<DownloadManager>>,
    url: String,
    filename: Option<String>,
    folder: Option<String>,
    connections: Option<usize>,
    open_window: Option<bool>,
    referer: Option<String>,
) -> Result<String, String> {
    let req = NewDownloadRequest {
        url,
        filename,
        save_path: folder,
        connections,
        category: None,
        start_now: Some(true),
        referer,
    };

    let item = manager.add_download(req).await.map_err(|e| e.to_string())?;

    if open_window.unwrap_or(true) {
        let _ = crate::transfer::open(&app, &registry, &item.id, &item.filename, false);
    }

    // The prompt has done its job.
    crate::prompt::close_all(&app);

    Ok(item.id)
}

/// Queue the download without starting it ("later").
#[tauri::command]
pub async fn queue_download_from_prompt(
    app: tauri::AppHandle,
    manager: State<'_, Arc<DownloadManager>>,
    url: String,
    filename: Option<String>,
    folder: Option<String>,
    connections: Option<usize>,
    referer: Option<String>,
) -> Result<String, String> {
    let req = NewDownloadRequest {
        url,
        filename,
        save_path: folder,
        connections,
        category: None,
        start_now: Some(false),
        referer,
    };

    let item = manager.add_download(req).await.map_err(|e| e.to_string())?;
    crate::prompt::close_all(&app);
    Ok(item.id)
}

/// Current queue: configuration, order and occupancy.
#[tauri::command]
pub fn queue_status(manager: State<'_, Arc<DownloadManager>>) -> crate::schedule::QueueStatus {
    manager.queue_status()
}

/// Change queue behaviour (enabled, parallelism, drain action, paused).
#[tauri::command]
pub fn save_queue_config(
    manager: State<'_, Arc<DownloadManager>>,
    config: crate::schedule::QueueConfig,
) -> Result<crate::schedule::QueueStatus, String> {
    manager.set_queue_config(&config)?;
    Ok(manager.queue_status())
}

/// Add downloads to the queue without starting them.
#[tauri::command]
pub async fn queue_downloads(
    manager: State<'_, Arc<DownloadManager>>,
    ids: Vec<String>,
) -> Result<(), String> {
    manager.queue_ids(&ids).await
}

/// Remove downloads from the queue (they stay as paused downloads).
#[tauri::command]
pub async fn unqueue_downloads(
    manager: State<'_, Arc<DownloadManager>>,
    ids: Vec<String>,
) -> Result<(), String> {
    manager.unqueue_ids(&ids).await
}

/// Move one queued download up or down the list.
#[tauri::command]
pub async fn move_in_queue(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
    direction: String,
) -> Result<(), String> {
    manager.move_in_queue(&id, &direction).await
}

/// Put a queued download at the front.
#[tauri::command]
pub async fn move_to_top(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), String> {
    manager.move_to_top(&id).await
}

/// Start the queue (or resume it after a pause).
#[tauri::command]
pub async fn start_queue(manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.set_queue_paused(false).await
}

/// Hold the queue: running transfers pause, nothing new starts.
#[tauri::command]
pub async fn pause_queue(manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.set_queue_paused(true).await
}

/// Start a magnet link or .torrent file.
///
/// A torrent is not an HTTP transfer, so it does not go through `add_download`:
/// there are no byte ranges and no connections to multiply. It still gets a row in
/// the same table and emits the same `download_progress` event, so the lane card,
/// the transfer window and the tray need no special case.
#[tauri::command]
pub async fn add_torrent(
    app: tauri::AppHandle,
    manager: State<'_, Arc<DownloadManager>>,
    url: String,
    save_path: Option<String>,
    paused: Option<bool>,
) -> Result<DownloadItem, String> {
    let settings = manager.settings();
    let folder = save_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| settings.download_dir.clone());

    // A recognisable name before the metadata arrives; the real folder name is
    // only known once the magnet resolves, and the row is updated then.
    let filename = {
        let trimmed = url.trim();
        if crate::torrent::looks_like_torrent_path(trimmed) {
            // A filesystem path: take the last component after EITHER separator.
            // `rsplit('/')` alone left the whole `C:\dir\a.torrent` string on
            // Windows, which then showed up as the download's name.
            crate::torrent::display_name_of(trimmed).unwrap_or_else(|| "torrent".to_string())
        } else if trimmed.to_ascii_lowercase().starts_with("magnet:") {
            // A magnet: its last `:`-separated part is not the name — `xt` holds the
            // info hash, and slicing that to 16 chars panicked on a short or
            // `btmh`-only magnet. Use the display name when the link carries one.
            crate::torrent::magnet_display_name(trimmed)
                .unwrap_or_else(|| "torrent.torrent".to_string())
        } else {
            // An `http(s)` link to a .torrent file, or a bare info-hash. The URL's
            // last path component is the real name (`…/live.torrent`); before this
            // branch existed such a link fell into the magnet case and every remote
            // torrent was named "torrent.torrent".
            crate::torrent::display_name_of(trimmed)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "torrent.torrent".to_string())
        }
    };

    let is_paused = paused.unwrap_or(false);
    let extracted_hash = crate::torrent::extract_info_hash(&url);

    if let Some(info_hash) = extracted_hash {
        let id = format!("torrent-{info_hash}");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let item = DownloadItem {
            id: id.clone(),
            url: url.clone(),
            filename,
            save_path: folder.clone(),
            total_size: 0,
            downloaded_size: 0,
            status: if is_paused {
                DownloadStatus::Paused
            } else {
                DownloadStatus::Downloading
            },
            category: "Torrent".to_string(),
            supports_range: false,
            etag: None,
            referer: None,
            connections: 1,
            speed_bps: 0,
            eta_seconds: 0,
            progress_percent: 0.0,
            created_at: now as i64,
            finished_at: None,
            error_message: None,
            segments: Vec::new(),
        };

        manager.record_external(&item)?;

        // Add torrent session in the background so the UI never freezes waiting for DHT resolution!
        let manager_clone = Arc::clone(&manager);
        let url_clone = url.clone();
        let folder_clone = folder.clone();
        let app_clone = app.clone();
        let id_clone = id.clone();
        tauri::async_runtime::spawn(async move {
            // Honour a pause the user issued while metadata was still resolving.
            let paused = manager_clone
                .all_downloads()
                .into_iter()
                .find(|d| d.id == id_clone)
                .map(|d| d.status == DownloadStatus::Paused)
                .unwrap_or(is_paused);
            if let Err(error) = manager_clone.torrents.add(&url_clone, &folder_clone, paused).await {
                eprintln!("[ZDM torrent] add failed in background: {error}");
                let _ = manager_clone.record_progress(&id_clone, 0, &DownloadStatus::Error);
                let _ = app_clone.emit("download_error", serde_json::json!({ "id": id_clone, "error": error }));
            }
        });

        crate::commands::spawn_torrent_watch(app, Arc::clone(&manager), id, info_hash);
        return Ok(item);
    }

    let info_hash = manager
        .torrents
        .add(&url, &folder, is_paused)
        .await
        .map_err(|error| error.to_string())?;

    let id = format!("torrent-{info_hash}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let item = DownloadItem {
        id: id.clone(),
        url: url.clone(),
        filename,
        save_path: folder,
        total_size: 0,
        downloaded_size: 0,
        status: if is_paused {
            DownloadStatus::Paused
        } else {
            DownloadStatus::Downloading
        },
        category: "Torrent".to_string(),
        supports_range: false,
        etag: None,
        referer: None,
        connections: 1,
        speed_bps: 0,
        eta_seconds: 0,
        progress_percent: 0.0,
        created_at: now as i64,
        finished_at: None,
        error_message: None,
        segments: Vec::new(),
    };

    manager.record_external(&item)?;

    // Report progress until it finishes or is removed.
    crate::commands::spawn_torrent_watch(app, Arc::clone(&manager), id.clone(), info_hash);
    Ok(item)
}

/// Add raw .torrent file bytes directly (drag & drop support).
#[tauri::command]
pub async fn add_torrent_bytes(
    app: tauri::AppHandle,
    manager: State<'_, Arc<DownloadManager>>,
    name: String,
    bytes: Vec<u8>,
    save_path: Option<String>,
    paused: Option<bool>,
) -> Result<DownloadItem, String> {
    let settings = manager.settings();
    let folder = save_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| settings.download_dir.clone());

    let is_paused = paused.unwrap_or(false);
    let info_hash = crate::torrent::extract_info_hash_from_bytes(&bytes)
        .ok_or_else(|| "Could not parse torrent file metadata".to_string())?;

    // Persist the torrent file so it can be resumed after restarts
    let torrents_dir = crate::paths::data_dir().join("torrents");
    let _ = std::fs::create_dir_all(&torrents_dir);
    let saved_file = torrents_dir.join(format!("{info_hash}.torrent"));
    let _ = std::fs::write(&saved_file, &bytes);

    let filename = if name.trim().is_empty() {
        format!("{info_hash}.torrent")
    } else {
        name
    };

    let id = format!("torrent-{info_hash}");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let item = DownloadItem {
        id: id.clone(),
        url: saved_file.to_string_lossy().to_string(),
        filename,
        save_path: folder.clone(),
        total_size: 0,
        downloaded_size: 0,
        status: if is_paused {
            DownloadStatus::Paused
        } else {
            DownloadStatus::Downloading
        },
        category: "Torrent".to_string(),
        supports_range: false,
        etag: None,
        referer: None,
        connections: 1,
        speed_bps: 0,
        eta_seconds: 0,
        progress_percent: 0.0,
        created_at: now as i64,
        finished_at: None,
        error_message: None,
        segments: Vec::new(),
    };

    manager.record_external(&item)?;

    let manager_clone = Arc::clone(&manager);
    let app_clone = app.clone();
    let id_clone = id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = manager_clone.torrents.add_bytes(bytes, &folder, is_paused).await {
            eprintln!("[ZDM torrent] add_bytes failed: {error}");
            let _ = manager_clone.record_progress(&id_clone, 0, &DownloadStatus::Error);
            let _ = app_clone.emit("download_error", serde_json::json!({ "id": id_clone, "error": error }));
        }
    });

    crate::commands::spawn_torrent_watch(app, Arc::clone(&manager), id, info_hash);
    Ok(item)
}

/// Poll one torrent and push its progress through the normal event.
/// Rows that already have a watcher, so a resume cannot start a second one.
static WATCHING: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
    std::sync::Mutex::new(None);

/// Record which rows currently have a watcher, and report whether this call is
/// allowed to start one.
fn claim_watcher(id: &str) -> bool {
    let mut guard = match WATCHING.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .get_or_insert_with(std::collections::HashSet::new)
        .insert(id.to_string())
}

/// Release a row's watcher slot on every way out of the loop.
fn release_watcher(id: &str) {
    let mut guard = match WATCHING.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(watching) = guard.as_mut() {
        watching.remove(id);
    }
}

fn spawn_torrent_watch(
    app: tauri::AppHandle,
    manager: Arc<DownloadManager>,
    id: String,
    info_hash: String,
) {
    // One watcher per row. Resuming a torrent after a restart re-adds it and calls
    // this again; without the guard the second watcher emits a duplicate
    // `download_progress` for the same row every second and both race on
    // completion.
    if !claim_watcher(&id) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;

        // Release the slot however this loop ends: every `return` below is a way
        // out, and a leaked slot would make the row un-resumable.
        struct Release(String);
        impl Drop for Release {
            fn drop(&mut self) {
                release_watcher(&self.0);
            }
        }
        let _release = Release(id.clone());

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

            // Stop when the row is gone (deleted or cancelled by the user).
            let Some(row) = manager
                .all_downloads()
                .into_iter()
                .find(|entry| entry.id == id)
            else {
                return;
            };

            if row.status == DownloadStatus::Cancelled {
                let _ = manager.torrents.remove(&info_hash, false).await;
                return;
            }

            // Stats from live session. While metadata resolves over DHT/trackers, stats may be None.
            let stats_opt = manager.torrents.stats(&info_hash).await;
            if let Some(stats) = stats_opt {
                let percent = if stats.total > 0 {
                    (stats.downloaded as f64 / stats.total as f64) * 100.0
                } else {
                    0.0
                };

                // Update the row's size once metadata has resolved.
                if stats.total > 0 && row.total_size == 0 {
                    let _ = manager.record_total_size(&id, stats.total);
                }

                if let Some(reason) = stats.error {
                    let _ = manager.record_progress(&id, stats.downloaded, &DownloadStatus::Error);
                    let _ = app.emit(
                        "download_error",
                        serde_json::json!({ "id": id, "error": reason }),
                    );
                    return;
                }

                let next_status = if stats.finished {
                    DownloadStatus::Completed
                } else if row.status == DownloadStatus::Paused {
                    DownloadStatus::Paused
                } else {
                    DownloadStatus::Downloading
                };

                let _ = manager.record_progress(&id, stats.downloaded, &next_status);

                let _ = app.emit(
                    "download_progress",
                    serde_json::json!({
                        "id": id,
                        "downloaded_bytes": stats.downloaded,
                        "total_bytes": stats.total,
                        "speed_bps": stats.down_bps,
                        "eta_seconds": if stats.down_bps > 0 && stats.total > stats.downloaded {
                            (stats.total - stats.downloaded) / stats.down_bps
                        } else { 0 },
                        "progress_percent": percent,
                        "segments": [],
                        "peers": stats.peers,
                    }),
                );

                if stats.finished {
                    let _ = manager.record_progress(
                        &id,
                        stats.downloaded.max(stats.total),
                        &DownloadStatus::Completed,
                    );

                    let _ = app.emit("download_completed", serde_json::json!({
                        "id": id,
                        "filename": row.filename,
                        "save_path": row.save_path,
                    }));
                    return;
                }
            } else if row.status == DownloadStatus::Downloading {
                // Heartbeat while session is resolving metadata or connecting
                let _ = app.emit(
                    "download_progress",
                    serde_json::json!({
                        "id": id,
                        "downloaded_bytes": row.downloaded_size,
                        "total_bytes": row.total_size,
                        "speed_bps": 0,
                        "eta_seconds": 0,
                        "progress_percent": 0.0,
                        "segments": [],
                        "peers": 0,
                    }),
                );
            }
        }
    });
}

/// Pause or resume a torrent by its download id.
#[tauri::command]
pub async fn pause_torrent(
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), String> {
    let hash = id.trim_start_matches("torrent-").to_string();
    manager.torrents.set_paused(&hash, true).await
}

/// Resume a torrent, re-adding it when the session does not know it yet.
///
/// The BitTorrent session lives for one process: after a restart the row is still
/// in the database but the session has no knowledge of it, so `resume_torrent`
/// used to fail with "that torrent is no longer known" and the download could
/// never be restarted from the UI. Re-adding from the row's own source (a magnet
/// or the original `.torrent` path) restores it, and `overwrite` lets the engine
/// pick up the bytes already on disk.
#[tauri::command]
pub async fn resume_torrent(
    app: tauri::AppHandle,
    manager: State<'_, Arc<DownloadManager>>,
    id: String,
) -> Result<(), String> {
    let hash = id.trim_start_matches("torrent-").to_string();

    if manager.torrents.stats(&hash).await.is_some() {
        return manager.torrents.set_paused(&hash, false).await;
    }

    // Not in this session: restore it from the row.
    let row = manager
        .get_all_downloads()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or("that download is no longer in the list")?;

    let new_hash = manager
        .torrents
        .add(&row.url, &row.save_path, false)
        .await
        .map_err(|error| format!("could not resume the torrent: {error}"))?;

    // The info hash is the row's identity. A `.torrent` path that now resolves to
    // a different hash (the file was replaced) would leave two rows pointing at
    // one transfer, so say so instead of watching the wrong one.
    if new_hash != hash {
        return Err(
            "that torrent file no longer matches the download; add it again".to_string()
        );
    }

    crate::commands::spawn_torrent_watch(app, Arc::clone(&manager), id, new_hash);
    Ok(())
}

#[tauri::command]
pub fn close_transfer_window(
    app: tauri::AppHandle,
    registry: State<'_, std::sync::Arc<crate::transfer::TransferRegistry>>,
    id: String,
) {
    crate::transfer::close(&app, &registry, &id);
}

#[tauri::command]
pub fn set_window_always_on_top(app: tauri::AppHandle, label: String, enabled: bool) {
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_always_on_top(enabled);
    }
}

#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

#[cfg(test)]
mod tests {
    /// Windows paths must be normalised to backslashes before being passed to
    /// `explorer.exe /select,`. A forward slash makes Explorer ignore the target
    /// and open Documents instead — the reported bug.
    #[test]
    fn windows_paths_use_backslashes() {
        let raw = "/home/ramy/Downloads/ZDM/Video/clip.mp4";
        let win = raw.replace('/', "\\");
        assert!(win.contains('\\'));
        assert!(!win.contains('/'));
    }

    /// The exact shape Explorer needs. `Command::arg` would wrap the whole
    /// argument in quotes (`"/select,C:\\x.mp4"`), which Explorer rejects and
    /// answers by opening Documents — the reported bug. `raw_arg` must produce
    /// `/select,"C:\\x.mp4"` instead.
    #[test]
    fn select_argument_shape_is_exact() {
        let file = "C:\\Users\\me\\Downloads\\clip.mp4";
        let rendered = format!("/select,\"{file}\"");

        assert_eq!(rendered, "/select,\"C:\\Users\\me\\Downloads\\clip.mp4\"");
        // The switch must NOT be inside the quotes.
        assert!(!rendered.starts_with("\"/select"), "the switch must be unquoted");
        assert!(rendered.starts_with("/select,\""), "the path must be quoted");
        // Only the switch keeps a forward slash; the path must be all backslashes.
        let path_part = rendered.trim_start_matches("/select,").trim_matches('"');
        assert!(!path_part.contains('/'), "Windows paths need backslashes");
    }

    /// Opening a folder quotes only the path.
    #[test]
    fn open_folder_argument_shape_is_exact() {
        let folder = "C:\\Users\\me\\Downloads\\ZDM\\Video";
        let rendered = format!("\"{folder}\"");
        assert_eq!(rendered, "\"C:\\Users\\me\\Downloads\\ZDM\\Video\"");
        assert!(!rendered.contains('/'));
    }

    /// A comma inside `/select,` breaks Explorer's argument parsing, so the code
    /// falls back to opening the containing folder.
    #[test]
    fn commas_are_detected_for_the_select_fallback() {
        let with_comma = "C:\\Users\\me\\My, File.mp4";
        assert!(with_comma.contains(','), "must take the folder fallback");
        let normal = "C:\\Users\\me\\file.mp4";
        assert!(!normal.contains(','));
    }
}
