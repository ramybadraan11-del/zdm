//! ZDM System Tray
//! Developed by Ramy Badraan
//!
//! On Windows the tray always works. On Linux it depends entirely on whether
//! the session exports an StatusNotifier/AppIndicator host:
//!
//! * Hyprland/sway/i3 need a tray host such as `waybar` (with the `tray` module)
//!   or `sfwbar`. Without one, no application can show an icon — including ours.
//! * GNOME needs the "AppIndicator" extension.
//!
//! We therefore build the icon defensively: if it cannot be created we record
//! that fact, keep the app fully usable, and let the Settings screen explain
//! what to install instead of leaving the user staring at a missing icon.

use crate::engine::manager::DownloadManager;
use crate::engine::types::DownloadStatus;
use crate::i18n::TrayStrings;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub const TRAY_ID: &str = "main";

pub fn build(
    app: &AppHandle,
    manager: Arc<DownloadManager>,
    strings: TrayStrings,
) -> Result<(), Box<dyn std::error::Error>> {
    let toggle = MenuItem::with_id(app, "toggle", &strings.show_hide, true, None::<&str>)?;
    let add_url = MenuItem::with_id(app, "add_url", &strings.add_url, true, None::<&str>)?;
    let video = MenuItem::with_id(app, "video", &strings.download_video, true, None::<&str>)?;
    let pause_all = MenuItem::with_id(app, "pause_all", &strings.pause_all, true, None::<&str>)?;
    let transfers = MenuItem::with_id(app, "transfers", &strings.show_transfers, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", &strings.settings, true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", &strings.about, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", &strings.quit, true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &add_url,
            &video,
            &sep1,
            &transfers,
            &pause_all,
            &settings,
            &about,
            &sep2,
            &quit,
        ],
    )?;

    let manager_menu = Arc::clone(&manager);
    let manager_click = Arc::clone(&manager);

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("no default window icon")?;

    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(&strings.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "toggle" => toggle_window(app),
            "add_url" => {
                reveal(app);
                let _ = app.emit("open_add_modal", ());
            }
            "video" => {
                reveal(app);
                let _ = app.emit("open_video_modal", ());
            }
            "transfers" => {
                crate::transfer::focus_all(app);
            }
            "pause_all" => {
                let manager = Arc::clone(&manager_menu);
                tauri::async_runtime::spawn(async move {
                    if let Ok(items) = manager.get_all_downloads() {
                        for item in items {
                            if matches!(
                                item.status,
                                DownloadStatus::Downloading | DownloadStatus::Queued
                            ) {
                                let _ = manager.pause_download(&item.id).await;
                            }
                        }
                    }
                });
            }
            "settings" => {
                reveal(app);
                let _ = app.emit("navigate", "settings");
            }
            "about" => {
                reveal(app);
                let _ = app.emit("open_about_modal", ());
            }
            "quit" => {
                // Quit must really terminate. `AppHandle::exit` raises
                // `RunEvent::ExitRequested`, and the run loop prevents exit while
                // `EXITING` is false (that is what keeps the tray app alive when a
                // window is closed) — so the flag has to be set first, or the
                // process could never be quit at all.
                crate::request_exit();
                let handle = app.clone();
                let manager = Arc::clone(&manager_menu);
                tauri::async_runtime::spawn(async move {
                    if let Ok(items) = manager.get_all_downloads() {
                        for item in items {
                            if item.status == DownloadStatus::Downloading {
                                let _ = manager.pause_download(&item.id).await;
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    handle.exit(0);
                });
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
                let _ = &manager_click;
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    // Keep the handle alive for the lifetime of the app.
    std::mem::forget(tray);
    Ok(())
}

pub fn toggle_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }
}

pub fn reveal(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Whether Linux exposes any tray host at all. Used for honest diagnostics.
pub fn session_has_tray_host() -> bool {
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
    #[cfg(target_os = "linux")]
    {
        // A StatusNotifier watcher registers a well-known name on the session
        // bus; if it exists, an icon will be rendered somewhere.
        if let Ok(out) = std::process::Command::new("busctl")
            .args(["--user", "list", "--no-pager", "--no-legend"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("StatusNotifierWatcher") || text.contains("org.kde.StatusNotifierWatcher") {
                return true;
            }
        }
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg("pgrep -x waybar >/dev/null || pgrep -x sfwbar >/dev/null || pgrep -x trayer >/dev/null || pgrep -x stalonetray >/dev/null")
            .output()
        {
            return out.status.success();
        }
        false
    }
}

