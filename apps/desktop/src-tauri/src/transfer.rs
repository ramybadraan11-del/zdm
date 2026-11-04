//! Separate transfer windows.
//! Developed by Ramy Badraan
//!
//! The main window is a manager: a list, a toolbar, settings. A running transfer
//! deserves its own small window — that is what IDM does, and it is what the user
//! asked for: press start, get a focused window with the segment ribbon and the
//! three controls, nothing else competing for attention.
//!
//! Design notes:
//!  * One window per download id, keyed `transfer-<id>`, so reopening focuses the
//!    existing window instead of spawning duplicates.
//!  * Frameless with our own title bar: the OS decoration would fight the design.
//!  * Always-on-top is opt-in per download (default off) — a download window that
//!    refuses to go behind is a nuisance.
//!  * Closing the window pauses nothing: the transfer keeps running in the main
//!    app, which is how a background downloader should behave.

use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

/// Tracks which download each transfer window belongs to.
#[derive(Default)]
pub struct TransferRegistry {
    /// window label → download id
    windows: parking_lot::Mutex<HashMap<String, String>>,
}

impl TransferRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn window_label(download_id: &str) -> String {
        // Labels must be stable and URL-safe; the id is a UUID.
        format!("transfer-{}", download_id.replace(|c: char| !c.is_ascii_alphanumeric(), ""))
    }

    fn register(&self, label: &str, download_id: &str) {
        self.windows.lock().insert(label.to_string(), download_id.to_string());
    }

    pub fn forget(&self, label: &str) {
        self.windows.lock().remove(label);
    }

    /// Number of windows this registry is tracking (tests and diagnostics).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.windows.lock().len()
    }
}

/// Open (or focus) the transfer window for one download.
pub fn open(
    app: &AppHandle,
    registry: &Arc<TransferRegistry>,
    download_id: &str,
    filename: &str,
    always_on_top: bool,
) -> Result<String, String> {
    let label = TransferRegistry::window_label(download_id);

    // Focus an existing window rather than opening a second one for the same file.
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return Ok(label);
    }

    let title = if filename.is_empty() {
        "Transfer".to_string()
    } else {
        format!("Transfer — {filename}")
    };

    // The frontend reads the id from the query string, so the same bundle can
    // render either the manager or a transfer window.
    let url = WebviewUrl::App(format!("index.html?window=transfer&id={download_id}").into());

    let window = WebviewWindowBuilder::new(app, &label, url)
        .title(&title)
        // Measured: header 44 + body content 99 + footer 48 = 191 CSS px. 224
        // gives the ribbon room to breathe without leaving a void. Height is
        // locked so the ribbon never reflows while the download runs.
        .inner_size(460.0, 224.0)
        .min_inner_size(460.0, 224.0)
        .max_inner_size(460.0, 224.0)
        .resizable(false)
        .decorations(false)
        .transparent(false)
        .always_on_top(always_on_top)
        .skip_taskbar(false)
        .center()
        .visible(false)
        .build()
        .map_err(|e| format!("Could not open the transfer window: {e}"))?;

    registry.register(&label, download_id);

    // Forget the mapping when the window goes away so a later start reopens cleanly.
    let registry_clone = Arc::clone(registry);
    let label_clone = label.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            registry_clone.forget(&label_clone);
        }
    });

    let _ = window.show();
    let _ = window.set_focus();

    Ok(label)
}

/// Close a transfer window (used when a download is removed).
pub fn close(app: &AppHandle, registry: &Arc<TransferRegistry>, download_id: &str) {
    let label = TransferRegistry::window_label(download_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
    registry.forget(&label);
}

/// Bring every open transfer window to the front (tray menu action).
pub fn focus_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with("transfer-") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

/// How many transfer windows are currently open.
pub fn count(app: &AppHandle) -> usize {
    app.webview_windows()
        .keys()
        .filter(|label| label.starts_with("transfer-"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_stable_and_url_safe() {
        let id = "24316dc4-17a5-4dcb-a3d7-482783c2107c";
        let label = TransferRegistry::window_label(id);
        assert_eq!(label, "transfer-24316dc417a54dcba3d7482783c2107c");
        // Same input, same label: reopening focuses instead of duplicating.
        assert_eq!(label, TransferRegistry::window_label(id));
        assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn different_downloads_get_different_labels() {
        assert_ne!(
            TransferRegistry::window_label("aaa"),
            TransferRegistry::window_label("bbb")
        );
    }

    #[test]
    fn registry_tracks_and_forgets() {
        let registry = TransferRegistry::new();
        registry.register("transfer-x", "x");
        assert_eq!(registry.len(), 1);
        registry.forget("transfer-x");
        assert_eq!(registry.len(), 0);
    }
}
