// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Zyzoom Download Manager (ZDM)
//! Developed by Ramy Badraan
//!
//! Tauri + Tokio download accelerator for Linux and Windows.

mod autostart;
mod browser_installer;
mod commands;
mod db;
mod engine;
mod extension_payload;
mod ffmpeg;
mod i18n;
mod ipc;
mod paths;
mod process;
mod prompt;
mod schedule;
mod torrent;

#[cfg(test)]
mod torrent_live;
mod transfer;
mod tray;
mod video;

#[cfg(test)]
mod testing;

use db::Database;
use engine::DownloadManager;
use i18n::Lang;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

/// Set once the user asked to exit, so window-close can really terminate.
static EXITING: AtomicBool = AtomicBool::new(false);

/// Mark the process as intentionally exiting, so the run loop stops vetoing the
/// exit. Called by the tray "Quit" item and by the final exit path.
pub(crate) fn request_exit() {
    EXITING.store(true, Ordering::SeqCst);
}

/// Fail with a readable message when there is no graphical session.
///
/// Without this the process dies inside GTK with a Rust panic and a core dump,
/// which tells the user nothing — especially confusing when the app is launched
/// from a login script on a headless box.
fn require_graphical_session() {
    #[cfg(target_os = "linux")]
    {
        let has_display = std::env::var("DISPLAY").map(|v| !v.trim().is_empty()).unwrap_or(false);
        let has_wayland = std::env::var("WAYLAND_DISPLAY")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

        if !has_display && !has_wayland {
            eprintln!(
                "ZDM needs a graphical session: neither DISPLAY nor WAYLAND_DISPLAY is set.\n\
                 If you are launching it from a service or an SSH session, start it from your \n\
                 desktop session instead (or run `systemctl --user start zdm.service`)."
            );
            std::process::exit(3);
        }
    }
}

use commands::{CliPayload, PendingCli};

fn main() {
    init_logging();
    require_graphical_session();
    if let Err(err) = paths::ensure_dirs() {
        eprintln!("[ZDM] cannot create data directories: {err}");
    }

    let db = match Database::new(paths::db_path()) {
        Ok(db) => Arc::new(db),
        Err(err) => {
            eprintln!("[ZDM] database unavailable: {err}");
            std::process::exit(1);
        }
    };

    let manager = Arc::new(DownloadManager::new(db));

    // `reconcile_on_startup` deliberately does NOT run here. It demotes every
    // `downloading` row to `paused`, and this point is reached by *every* process
    // — including the throwaway second instance that a double-clicked `.torrent`
    // (or a magnet link) spawns. That instance is killed moments later by the
    // single-instance plugin, but not before its reconcile had relabelled the
    // downloads the primary was still running: cards jumped to "paused" at their
    // current percentage and never moved again, which looked exactly like
    // "torrent support does not work". It now runs in `setup`, which only the
    // surviving primary reaches.

    // Autostart wiring is idempotent and never fatal.
    let _ = autostart::install();

    let start_hidden = std::env::args().any(|a| {
        matches!(
            a.as_str(),
            "--background" | "--minimized" | "--hidden" | "--tray" | "--silent"
        )
    });

    // CLI intents from desktop-file actions and "Open with" handlers.
    let cli = CliIntent::parse(std::env::args().skip(1));
    let initial_payload = match &cli {
        CliIntent::AddUrl(url) => Some(CliPayload {
            kind: "url".to_string(),
            value: Some(url.clone()),
        }),
        CliIntent::Video(url) => Some(CliPayload {
            kind: "video".to_string(),
            value: url.clone(),
        }),
        CliIntent::AddTorrent(path) => Some(CliPayload {
            kind: "torrent".to_string(),
            value: Some(path.clone()),
        }),
        CliIntent::PickTorrent => Some(CliPayload {
            kind: "pick_torrent".to_string(),
            value: None,
        }),
        CliIntent::None => None,
    };

    let manager_for_setup = Arc::clone(&manager);
    let manager_for_run = Arc::clone(&manager);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tray::reveal(app);
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }

            let mut opened_something = false;
            for arg in argv.iter().skip(1) {
                let lower = arg.trim().to_ascii_lowercase();

                if looks_like_torrent_argument(arg) {
                    let _ = app.emit("add_torrent_from_cli", serde_json::json!({ "path": arg }));
                    opened_something = true;
                } else if lower.starts_with("http://") || lower.starts_with("https://") {
                    let _ = app.emit("add_url_from_cli", serde_json::json!({ "url": arg }));
                    opened_something = true;
                } else if lower.starts_with("magnet:") {
                    let _ = app.emit("add_torrent_from_cli", serde_json::json!({ "path": arg }));
                    opened_something = true;
                }
            }

            if !opened_something && argv.iter().skip(1).any(|a| a == "--add-torrent" || a == "-t") {
                let _ = app.emit("pick_torrent_from_cli", serde_json::json!({}));
            }

            let _ = app.emit("focus_requested", ());
        }))
        .manage(Arc::clone(&manager))
        .manage(transfer::TransferRegistry::new())
        .manage(PendingCli(std::sync::Mutex::new(initial_payload)))
        .setup(move |app| {
            let handle = app.handle().clone();
            manager_for_setup.set_app_handle(handle.clone());

            // Only the process that survives the single-instance check reaches
            // `setup`, so this is the correct place to demote rows left in
            // `downloading` by a crash or a real exit. Doing it earlier marked the
            // primary's live downloads as paused whenever a second instance was
            // launched to hand over a `.torrent` file or a magnet link.
            manager_for_setup.reconcile_on_startup();

            // Local bridge for the browser extension (CORS-complete).
            ipc::start(Arc::clone(&manager_for_setup), handle.clone());

            // Background queue scheduler: which waiting download goes next.
            manager_for_setup.spawn_scheduler();

            // Download queue: feeds the engine as slots free up.
            manager_for_setup.spawn_queue_worker();

            // Tray: build it, but survive a session with no tray host.
            let settings = manager_for_setup.settings();
            let lang = Lang::detect(&settings.language);
            if let Err(err) = tray::build(app.handle(), Arc::clone(&manager_for_setup), i18n::tray_strings(lang)) {
                eprintln!("[ZDM] system tray unavailable: {err}");
                let _ = app
                    .handle()
                    .emit("tray_unavailable", serde_json::json!({ "reason": err.to_string() }));
            }

            if let Some(win) = app.get_webview_window("main") {
                // Show only when not started as a background agent, and always
                // give the window a chance to paint before appearing.
                if !start_hidden {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            // Forward CLI intents once the webview exists to receive them.
            let cli_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                match cli {
                    CliIntent::AddUrl(url) => {
                        let _ = cli_handle.emit("add_url_from_cli", serde_json::json!({ "url": url }));
                    }
                    CliIntent::Video(url) => {
                        let payload = match url {
                            Some(url) => serde_json::json!({ "url": url }),
                            None => serde_json::json!({}),
                        };
                        let _ = cli_handle.emit("open_video_modal_with_url", payload);
                    }
                    // A double-clicked .torrent starts downloading directly: the
                    // user already chose the file, so asking again adds nothing.
                    CliIntent::AddTorrent(path) => {
                        let _ = cli_handle.emit("add_torrent_from_cli", serde_json::json!({ "path": path }));
                    }
                    // The desktop action opened with no file: let the UI ask.
                    CliIntent::PickTorrent => {
                        let _ = cli_handle.emit("pick_torrent_from_cli", serde_json::json!({}));
                    }
                    CliIntent::None => {}
                }
            });

            // Materialise the extension payload compiled into the binary, so the
            // CRX/XPI can never be "not found" regardless of install layout.
            if let Some(crx) = extension_payload::ensure_extracted() {
                println!("[ZDM] extension payload ready at {}", crx.display());
            } else {
                eprintln!("[ZDM] this build carries no embedded extension payload");
            }

            // Browser integration is re-applied at startup, but only when it is
            // actually stale. Running the full pass unconditionally wrote every
            // manifest and rewrote every policy through `reg` on each launch,
            // which made startup hang on Windows. The stamp records the host
            // path, the extension version and the set of browsers found; if none
            // of those changed, there is nothing to redo.
            let browser_report = browser_installer::ensure_integration_fresh();
            if !browser_report.warnings.is_empty() {
                for warning in &browser_report.warnings {
                    eprintln!("[ZDM browser] {warning}");
                }
            }

            // Notify once the UI is ready (it asks for diagnostics itself).
            let _ = app.handle().emit("ready", serde_json::json!({
                "background": start_hidden,
                "tray": tray::session_has_tray_host(),
            }));

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window hides to tray (IDM behaviour) unless the user
            // chose "Quit" from the menu.
            WindowEvent::CloseRequested { api, .. }
                if !EXITING.load(Ordering::SeqCst) => {
                    api.prevent_close();
                    let _ = window.hide();
                }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_url,
            commands::add_download,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::delete_download,
            commands::pause_all,
            commands::get_downloads,
            commands::get_stats,
            commands::query_video_info,
            commands::start_video_download,
            commands::query_playlist_info,
            commands::start_playlist_download,
            commands::queue_status,
            commands::save_queue_config,
            commands::queue_downloads,
            commands::unqueue_downloads,
            commands::move_in_queue,
            commands::move_to_top,
            commands::start_queue,
            commands::pause_queue,
            commands::add_torrent,
            commands::add_torrent_bytes,
            commands::pause_torrent,
            commands::resume_torrent,
            commands::update_download_url,
            commands::restart_download,
            commands::delete_downloads,
            commands::get_pending_cli,
            commands::check_file_conflict,
            commands::update_yt_dlp_engine,
            commands::install_ffmpeg,
            commands::capture_status,
            commands::get_settings,
            commands::save_settings,
            commands::get_browsers,
            commands::register_all_browsers,
            commands::repair_browser,
            commands::launch_browser_with_ext,
            commands::open_file,
            commands::open_file_folder,
            commands::reveal_in_folder,
            commands::get_app_info,
            commands::get_diagnostics,
            commands::set_autostart,
            commands::path_exists,
            commands::show_main_window,
            commands::open_transfer_window,
            commands::close_transfer_window,
            commands::close_prompt,
            commands::show_manager,
            commands::start_download_from_prompt,
            commands::queue_download_from_prompt,
            commands::set_window_always_on_top,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("[ZDM] cannot create the application window: {err}");
            eprintln!("[ZDM] A graphical session is required (DISPLAY or WAYLAND_DISPLAY).");
            std::process::exit(1);
        });

    app.run(move |_app_handle: &AppHandle, event| {
        if let RunEvent::ExitRequested { api, .. } = &event {
            // Keep running in the tray unless the user truly quit.
            if !EXITING.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
        if let RunEvent::Exit = event {
            // Last chance to persist anything the engine still holds.
            let _ = manager_for_run.get_all_downloads();
            // Release torrent sockets and tracker connections instead of leaving
            // them for the OS to reap.
            let torrents = Arc::clone(&manager_for_run.torrents);
            tauri::async_runtime::block_on(async move {
                torrents.shutdown().await;
            });
        }
    });
}

/// Single-instance behaviour is provided by `tauri-plugin-single-instance`:
/// it is the official cross-platform guard, gives the second launch a callback
/// instead of a second engine, and therefore fixes both the duplicate tray icon
/// and the "two writers on one SQLite file" hazard on Windows.
/// See `.plugin(tauri_plugin_single_instance::init(...))` in `main`.
/// What the user asked for on the command line.
#[derive(Debug)]
enum CliIntent {
    AddUrl(String),
    Video(Option<String>),
    /// A `.torrent` file path from a double-click or `zdm file.torrent`.
    AddTorrent(String),
    /// `--add-torrent` with no path: the UI should ask for a file.
    PickTorrent,
    None,
}

impl CliIntent {
    fn parse(args: impl Iterator<Item = String>) -> Self {
        let mut intent = CliIntent::None;
        let mut pending: Option<&str> = None;

        for arg in args {
            // Flags the shell may pass alongside the file (Windows adds these when
            // the OS opens a document) must never be mistaken for a path.
            if arg.starts_with('-') {
                pending = match arg.as_str() {
                    "--add-url" | "-a" => Some("add"),
                    "--video" | "-v" => Some("video"),
                    "--add-torrent" | "-t" => Some("torrent"),
                    _ => None,
                };
                continue;
            }

            // A .torrent path wins over every other interpretation: this is the
            // registered file association firing (and `file://` URIs reach us the
            // same way on Linux).
            if looks_like_torrent_argument(&arg) {
                intent = CliIntent::AddTorrent(arg.clone());
                pending = None;
                continue;
            }

            // A magnet link is the torrent engine's job, and it must be handled
            // here as well as in the single-instance callback. This parser used to
            // check `http(s)` only, so `zdm "magnet:?xt=…"` on a cold start did
            // nothing at all, while the same link worked when ZDM was already
            // running — the exact asymmetry that made magnets look unsupported.
            if arg.trim().to_ascii_lowercase().starts_with("magnet:") {
                intent = CliIntent::AddTorrent(arg.clone());
                pending = None;
                continue;
            }

            match pending {
                Some(kind) => {
                    // The value for a flag that takes a URL or a torrent path.
                    if looks_like_torrent_argument(&arg) {
                        intent = CliIntent::AddTorrent(arg.clone());
                    } else if arg.starts_with("http") || arg.starts_with("www.") {
                        let url = normalise_url(&arg);
                        intent = if kind == "video" {
                            CliIntent::Video(Some(url))
                        } else {
                            CliIntent::AddUrl(url)
                        };
                    }
                    pending = None;
                }
                None => {
                    if arg.starts_with("http://") || arg.starts_with("https://") {
                        // An HTTP link to a `.torrent` file is a torrent, not a
                        // file transfer: `add_download` refuses it ("This is a
                        // torrent — use Add torrent"), so routing it through the
                        // URL path produced that error instead of a download.
                        intent = if looks_like_torrent_argument(&arg) {
                            CliIntent::AddTorrent(arg.clone())
                        } else {
                            CliIntent::AddUrl(arg.clone())
                        };
                    }
                }
            }
        }

        // Bare "--video" with no URL still opens the dialog.
        if matches!(intent, CliIntent::None) && pending == Some("video") {
            return CliIntent::Video(None);
        }
        // Bare "--add-torrent" (the desktop action) asks for a file instead.
        if matches!(intent, CliIntent::None) && pending == Some("torrent") {
            return CliIntent::PickTorrent;
        }
        intent
    }
}

/// Whether an argv entry names a `.torrent` file, in any form the OS may hand it
/// to us: `C:\a\b.torrent`, `file:///C:/a/b.torrent`, `/home/x/a.torrent`.
fn looks_like_torrent_argument(arg: &str) -> bool {
    let clean = arg.trim().trim_matches('"').trim_matches('\'');
    let lower = clean.to_ascii_lowercase();
    if lower.starts_with("magnet:") {
        return false;
    }
    let without_prefix = lower
        .strip_prefix("file://")
        .unwrap_or(&lower);
    without_prefix.ends_with(".torrent")
}

fn normalise_url(raw: &str) -> String {
    if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    }
}

fn init_logging() {
    // Keep logs in the data dir so a user can attach them to a bug report.
    let log_file = paths::log_path();
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        let target = std::sync::Arc::new(parking_lot::Mutex::new(file));
        let writer = LogWriter { inner: target };
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .target(env_logger::Target::Pipe(Box::new(writer)))
            .try_init();
        return;
    }
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
}

struct LogWriter {
    inner: std::sync::Arc<parking_lot::Mutex<std::fs::File>>,
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut *self.inner.lock(), buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut *self.inner.lock())
    }
}

#[cfg(test)]
mod cli_tests {
    use super::{normalise_url, CliIntent};

    fn parse(args: &[&str]) -> CliIntent {
        CliIntent::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_add_url() {
        match parse(&["--add-url", "https://example.com/f.zip"]) {
            CliIntent::AddUrl(url) => assert_eq!(url, "https://example.com/f.zip"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_video_with_and_without_url() {
        match parse(&["--video", "https://youtu.be/x"]) {
            CliIntent::Video(Some(url)) => assert!(url.ends_with("/x")),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(parse(&["--video"]), CliIntent::Video(None)));
    }

    #[test]
    fn bare_url_is_a_download() {
        assert!(matches!(parse(&["https://example.com/a.iso"]), CliIntent::AddUrl(_)));
    }

    #[test]
    fn background_flags_are_not_urls() {
        assert!(matches!(parse(&["--background", "--minimized"]), CliIntent::None));
    }

    /// A double-clicked .torrent arrives as a bare path in argv.
    #[test]
    fn a_torrent_path_is_recognised() {
        match parse(&["C:\\Users\\me\\a.torrent"]) {
            CliIntent::AddTorrent(path) => assert!(path.ends_with("a.torrent")),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            parse(&["/home/me/a.torrent"]),
            CliIntent::AddTorrent(_)
        ));
    }

    /// Linux file managers hand us a `file://` URI when the association fires.
    #[test]
    fn a_file_uri_torrent_is_recognised() {
        assert!(matches!(
            parse(&["file:///tmp/a.torrent"]),
            CliIntent::AddTorrent(_)
        ));
        // Windows hands us the same shape with a drive letter.
        assert!(matches!(
            parse(&["file:///C:/Users/me/a.torrent"]),
            CliIntent::AddTorrent(_)
        ));
    }

    #[test]
    fn the_torrent_flag_without_a_path_asks_for_a_file() {
        assert!(matches!(parse(&["--add-torrent"]), CliIntent::PickTorrent));
    }

    #[test]
    fn the_torrent_flag_with_a_path_adds_it() {
        assert!(matches!(
            parse(&["--add-torrent", "/tmp/x.torrent"]),
            CliIntent::AddTorrent(_)
        ));
    }

    /// A magnet is a URL, never a path — it must not be swallowed by the
    /// torrent-path branch.
    #[test]
    fn a_magnet_is_not_a_path() {
        assert!(!super::looks_like_torrent_argument(
            "magnet:?xt=urn:btih:abc"
        ));
    }

    /// A magnet on a cold start used to be ignored: the parser only recognised
    /// `http(s)` and `.torrent` paths, so `zdm "magnet:?xt=…"` did nothing while
    /// the same link worked from the single-instance callback. The two entry
    /// points must agree.
    #[test]
    fn a_magnet_on_the_command_line_is_a_torrent() {
        let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=x";

        match parse(&[magnet]) {
            CliIntent::AddTorrent(path) => assert_eq!(path, magnet),
            other => panic!("a magnet must be an AddTorrent intent, got {other:?}"),
        }

        // It must stay a magnet: `looks_like_torrent_argument` is about paths, and
        // reporting `true` for a magnet would send it to the disk reader again.
        assert!(!super::looks_like_torrent_argument(magnet));

        // Upper case is the same link.
        assert!(matches!(
            parse(&["MAGNET:?xt=urn:btih:abc"]),
            CliIntent::AddTorrent(_)
        ));
    }

    /// The reported bug: a double-clicked `.torrent` did nothing while ZDM was
    /// already running in the tray. On a cold start it worked, because that path
    /// goes through `CliIntent::parse`; the single-instance callback forwarded
    /// `http(s)` URLs only. Both paths must now recognise the same shapes.
    #[test]
    fn every_torrent_shape_is_recognised_by_both_entry_points() {
        let shapes = [
            r"C:\Users\me\a.torrent",
            "/home/me/a.torrent",
            "file:///tmp/a.torrent",
            "file:///C:/Users/me/a.torrent",
        ];

        for shape in shapes {
            assert!(
                super::looks_like_torrent_argument(shape),
                "the single-instance path must forward {shape}"
            );
            assert!(
                matches!(parse(&[shape]), CliIntent::AddTorrent(_)),
                "the cold-start path must add {shape}"
            );
        }

        // A magnet is the torrent engine's job too, but it is a URL: it must not
        // be mistaken for a path (that would try to read it from disk).
        assert!(!super::looks_like_torrent_argument("magnet:?xt=urn:btih:abc"));
    }

    /// An `http(s)` link to a `.torrent` file must reach the torrent engine.
    /// Sending it to `add_download` produced "This is a torrent — use Add
    /// torrent", so pasting such a link never downloaded anything.
    #[test]
    fn an_http_torrent_link_is_a_torrent_not_a_file() {
        for url in [
            "https://tracker.example/files/movie.torrent",
            "http://tracker.example/files/movie.torrent",
            "https://tracker.example/files/MOVIE.TORRENT",
        ] {
            assert!(
                super::looks_like_torrent_argument(url),
                "{url} must be recognised as a torrent"
            );
            match parse(&[url]) {
                CliIntent::AddTorrent(kept) => assert_eq!(kept, url),
                other => panic!("{url} must not go through add_download, got {other:?}"),
            }
        }

        // An ordinary file URL must stay a plain download.
        assert!(matches!(
            parse(&["https://example.com/file.zip"]),
            CliIntent::AddUrl(_)
        ));
    }

    #[test]
    fn normalises_schemeless_urls() {
        assert_eq!(normalise_url("example.com/x"), "https://example.com/x");
        assert_eq!(normalise_url("https://a.b"), "https://a.b");
    }
}
