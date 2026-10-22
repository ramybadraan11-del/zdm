// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Zyzoom Download Manager (ZDM)
//! Developed by Ramy Badraan
//!
//! High-Performance Multi-Segment Download Accelerator for Linux & Windows.

mod browser_installer;
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
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

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

    // Automatically register browser extensions and native messaging manifests
    browser_installer::install_browser_integrations();

    let manager_for_setup = Arc::clone(&download_manager);

    // Check if launched with --minimized or --background (autostart)
    let is_minimized_launch = std::env::args().any(|arg| arg == "--minimized" || arg == "--background");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(download_manager)
        .setup(move |app| {
            manager_for_setup.set_app_handle(app.handle().clone());

            // Start local IPC server for browser extension captures inside Tauri runtime
            ipc::start_ipc_server(Arc::clone(&manager_for_setup));

            let main_window = app.get_webview_window("main").expect("Failed to get main window");

            if is_minimized_launch {
                let _ = main_window.hide();
            }

            // Create Tray Menu
            let title_item = MenuItem::with_id(app, "title", "⚡ Zyzoom Download Manager", false, None::<&str>)?;
            let dev_item = MenuItem::with_id(app, "dev", "👤 Developed by Ramy Badraan", false, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let toggle_item = MenuItem::with_id(app, "toggle_window", "إظهار / إخفاء النافذة", true, None::<&str>)?;
            let add_item = MenuItem::with_id(app, "add_url", "+ إضافة رابط جديد...", true, None::<&str>)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "الخروج من البرنامج", true, None::<&str>)?;

            let tray_menu = Menu::with_items(
                app,
                &[
                    &title_item,
                    &dev_item,
                    &sep1,
                    &toggle_item,
                    &add_item,
                    &sep2,
                    &quit_item,
                ],
            )?;

            // Build System Tray Icon (Waybar, Hyprland, KDE, GNOME, Windows taskbar)
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Zyzoom Download Manager (ZDM) • Developed by Ramy Badraan")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app_handle, event| match event.id().as_ref() {
                    "toggle_window" => {
                        if let Some(win) = app_handle.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.unminimize();
                                let _ = win.set_focus();
                            }
                        }
                    }
                    "add_url" => {
                        if let Some(win) = app_handle.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            let _ = win.emit("open_add_modal", ());
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.unminimize();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // When closing window, hide to tray instead of terminating (IDM behavior)
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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
