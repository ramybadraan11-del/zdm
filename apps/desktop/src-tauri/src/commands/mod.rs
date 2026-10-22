//! ZDM Tauri IPC Commands
//! Developed by Ramy Badraan

use crate::browser_installer::{get_installed_browsers_list, install_browser_integrations, launch_browser, BrowserStatus};
use crate::engine::manager::DownloadManager;
use crate::engine::types::{DownloadItem, EngineStats, NewDownloadRequest, UrlMetadata};
use crate::video::{download_with_yt_dlp, ensure_yt_dlp, query_video, VideoInfo};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tauri::State;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub developer: String,
    pub license: String,
    pub architecture: String,
    pub os: String,
    pub engine: String,
}

#[tauri::command]
pub async fn probe_url(url: String, manager: State<'_, Arc<DownloadManager>>) -> Result<UrlMetadata, String> {
    manager.probe(&url).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_download(req: NewDownloadRequest, manager: State<'_, Arc<DownloadManager>>) -> Result<DownloadItem, String> {
    manager.add_download(req).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_download(id: String, manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.start_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_download(id: String, manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.pause_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_download(id: String, manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.resume_download(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_download(id: String, delete_file: bool, manager: State<'_, Arc<DownloadManager>>) -> Result<(), String> {
    manager.delete_download(&id, delete_file).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_downloads(manager: State<'_, Arc<DownloadManager>>) -> Result<Vec<DownloadItem>, String> {
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
pub async fn download_video_stream(url: String, format_id: String, save_path: String) -> Result<(), String> {
    download_with_yt_dlp(&url, &format_id, &save_path).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct FileConflictInfo {
    pub exists: bool,
    pub existing_path: String,
    pub existing_size: u64,
    pub suggested_name: String,
}

#[tauri::command]
pub fn check_file_conflict(filename: String, category: String) -> Result<FileConflictInfo, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    let save_dir = std::path::PathBuf::from(home).join("Downloads").join("ZDM").join(&category);
    let target_file = save_dir.join(&filename);

    if target_file.exists() {
        let size = std::fs::metadata(&target_file).map(|m| m.len()).unwrap_or(0);
        let stem = target_file.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = target_file.extension().and_then(|s| s.to_str()).unwrap_or("");
        let mut counter = 1;
        let mut suggested = format!("{} ({}).{}", stem, counter, ext);
        while save_dir.join(&suggested).exists() {
            counter += 1;
            suggested = format!("{} ({}).{}", stem, counter, ext);
        }

        Ok(FileConflictInfo {
            exists: true,
            existing_path: target_file.to_string_lossy().to_string(),
            existing_size: size,
            suggested_name: suggested,
        })
    } else {
        Ok(FileConflictInfo {
            exists: false,
            existing_path: target_file.to_string_lossy().to_string(),
            existing_size: 0,
            suggested_name: filename,
        })
    }
}

#[tauri::command]
pub async fn start_video_download(
    url: String,
    format_id: String,
    title: String,
    ext: String,
    is_audio: bool,
    est_size: Option<u64>,
    custom_filename: Option<String>,
    force_overwrite: Option<bool>,
    manager: State<'_, Arc<DownloadManager>>,
) -> Result<DownloadItem, String> {
    manager
        .start_video_download(
            &url,
            &format_id,
            &title,
            &ext,
            is_audio,
            est_size,
            custom_filename,
            force_overwrite.unwrap_or(false),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_yt_dlp_engine() -> Result<String, String> {
    let path = ensure_yt_dlp().await.map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn get_browsers() -> Vec<BrowserStatus> {
    get_installed_browsers_list()
}

#[tauri::command]
pub fn register_all_browsers() -> Result<(), String> {
    install_browser_integrations();
    Ok(())
}

#[tauri::command]
pub fn launch_browser_with_ext(browser_id: String) -> Result<(), String> {
    launch_browser(&browser_id)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_file_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let dir = if p.is_file() {
        p.parent().unwrap_or(p)
    } else {
        p
    };

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(dir)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "Zyzoom Download Manager (ZDM)".to_string(),
        version: "1.0.0 Pro".to_string(),
        developer: "Ramy Badraan".to_string(),
        license: "Proprietary".to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
        engine: "Tauri 2 + Tokio Async 32-Stream Direct-Write".to_string(),
    }
}
