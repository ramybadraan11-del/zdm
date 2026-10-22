// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Zyzoom Download Manager (ZDM)
//! Developed by Ramy Badraan
//!
//! High-Performance Multi-Segment Download Accelerator for Linux & Windows.

mod commands;
mod db;
mod engine;
mod ipc;
mod video;

use db::Database;
use engine::DownloadManager;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    env_logger::init();

    // Setup ZDM data directory (~/.local/share/zdm or %APPDATA%/zdm)
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let mut data_dir = PathBuf::from(home);
    data_dir.push(".local");
    data_dir.push("share");
    data_dir.push("zdm");
    let _ = fs::create_dir_all(&data_dir);

    let db_path = data_dir.join("zdm.db");
    let database = Arc::new(Database::new(&db_path).expect("Failed to initialize ZDM database"));
    let download_manager = Arc::new(DownloadManager::new(database));

    // Start local IPC server for browser extension captures
    ipc::start_ipc_server(Arc::clone(&download_manager));

    let manager_for_setup = Arc::clone(&download_manager);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(download_manager)
        .setup(move |app| {
            manager_for_setup.set_app_handle(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_url,
            commands::add_download,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::delete_download,
            commands::get_downloads,
            commands::get_stats,
            commands::query_video_info,
            commands::open_file,
            commands::open_file_folder,
            commands::get_app_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ZDM application");
}
