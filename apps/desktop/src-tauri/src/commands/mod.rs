//! ZDM Tauri IPC Commands
//! Developed by Ramy Badraan

use crate::engine::manager::DownloadManager;
use crate::engine::types::{DownloadItem, EngineStats, NewDownloadRequest, UrlMetadata};
use crate::video::{query_video, VideoInfo};
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
