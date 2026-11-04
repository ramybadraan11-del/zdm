//! ZDM Floating prompt windows.
//! Developed by Ramy Badraan
//!
//! Two independent windows, because a file transfer and a video download are two
//! different decisions:
//!
//!   * `prompt-file`  — a direct file: size, type, resume, folder, connections
//!   * `prompt-video` — a page: title, quality list, folder
//!
//! They used to share one window that was reloaded with `navigate()`. That is
//! what broke on Windows: the reload pointed at `tauri://localhost/...`, but the
//! Windows webview serves the app from `http://tauri.localhost/...`, so the
//! navigation silently failed and the window kept showing the previous content —
//! a file capture displayed the last video's quality list.
//!
//! Each prompt is now created fresh with its own size and its own payload, and
//! closed after use. No shared state, no cross-platform URL assumptions.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Prefix for prompt window labels. Each prompt gets a unique suffix.
pub const LABEL_PREFIX: &str = "prompt-";

/// Kind of prompt, used for the label and the payload.
pub const FILE_KIND: &str = "file";
pub const VIDEO_KIND: &str = "video";

/// Counter for unique labels.
///
/// Window creation is not synchronous with window destruction: `close()` returns
/// before the platform has torn the window down, so reusing a fixed label raised
/// "a webview with label `prompt-video` already exists". Unique labels remove the
/// race entirely — the old window finishes closing on its own time.
static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What the prompt should offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    File,
    Video,
    /// A magnet link or .torrent file: no size probe, no connections.
    Torrent,
}

impl PromptMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Video => "video",
            Self::Torrent => "torrent",
        }
    }

    /// Window label prefix for this mode, so a file prompt and a video prompt are
    /// always separate windows.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::File => FILE_KIND,
            Self::Video => VIDEO_KIND,
            Self::Torrent => "torrent",
        }
    }

    /// A fresh, unique window label for this mode.
    pub fn next_label(&self) -> String {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("{LABEL_PREFIX}{}-{n}", self.kind())
    }

    /// Width and height tuned to the measured content of each prompt.
    ///
    /// Measured in the preview harness (header + body children + footer):
    ///   file   → 267 CSS px
    ///   video  → 324 CSS px   (4 quality rows fill the grid)
    /// A few pixels of headroom absorbs font-metric differences between WebKitGTK
    /// and WebView2, which is where the two platforms drifted apart.
    pub fn size(&self) -> (f64, f64) {
        match self {
            Self::File => (400.0, 276.0),
            // Adds a 16:9 still above the two-column quality grid.
            Self::Video => (444.0, 470.0),
            // No size or connection controls: a magnet needs one button.
            Self::Torrent => (420.0, 300.0),
        }
    }
}

/// Payload handed to the prompt window through the URL.
///
/// The window is created before its webview can receive an event, so the URL is
/// the only channel guaranteed to be there on first paint.
pub fn prompt_url(url: &str, referer: Option<&str>, mode: PromptMode) -> String {
    let mut target = format!(
        "index.html?window=prompt&mode={}&url={}",
        mode.as_str(),
        urlencode(url)
    );
    if let Some(referer) = referer.filter(|r| !r.is_empty()) {
        target.push_str(&format!("&referer={}", urlencode(referer)));
    }
    target
}

/// Hosts whose URL should be treated as a *page* to analyse with yt-dlp rather
/// than a direct file to fetch.
const VIDEO_HOSTS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "m.youtube.com",
    "music.youtube.com",
    "vimeo.com",
    "dailymotion.com",
    "facebook.com",
    "fb.watch",
    "instagram.com",
    "tiktok.com",
    "twitter.com",
    "x.com",
    "twitch.tv",
    "reddit.com",
    "rumble.com",
    "bitchute.com",
    "streamable.com",
    "bilibili.com",
    "pinterest.com",
    "linkedin.com",
    "threads.net",
    "netflix.com",
    "primevideo.com",
];

/// Decide how to present a captured URL.
pub fn classify(url: &str) -> PromptMode {
    if crate::torrent::looks_like_torrent(url) {
        return PromptMode::Torrent;
    }

    let lower = url.to_lowercase();

    // A direct media file is a file transfer even on a video host.
    for extension in [
        ".mp4", ".m4v", ".mkv", ".webm", ".mov", ".avi", ".flv", ".mp3", ".m4a", ".flac", ".wav",
        ".ogg", ".opus",
    ] {
        if let Some(index) = lower.find(extension) {
            let after = &lower[index + extension.len()..];
            if after.is_empty() || after.starts_with(['?', '#', '&', '/']) {
                return PromptMode::File;
            }
        }
    }

    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let host = host.trim_start_matches("www.");
            if VIDEO_HOSTS
                .iter()
                .any(|known| host == *known || host.ends_with(&format!(".{known}")))
            {
                return PromptMode::Video;
            }
        }
        if parsed.path().contains("/watch") || parsed.path().contains("/video") {
            return PromptMode::Video;
        }
    }

    PromptMode::File
}

/// Percent-encode everything that would break a query string.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Show the prompt for one URL, in the window that matches its mode.
///
/// Any prompt already open is closed first: two prompts on screen at once is
/// confusing, and reusing a window by navigating it is exactly what broke on
/// Windows.
pub fn show(
    app: &AppHandle,
    url: &str,
    referer: Option<&str>,
    mode: PromptMode,
) -> Result<(), String> {
    let app_handle = app.clone();
    let url = url.to_string();
    let referer = referer.map(|r| r.to_string());

    // Already on the main thread (a tray click, a Tauri command): build inline so
    // callers keep the old synchronous contract.
    if is_main_thread() {
        return show_inner(&app_handle, &url, referer.as_deref(), mode);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let worker = app_handle.clone();
    app_handle
        .run_on_main_thread(move || {
            let result = show_inner(&worker, &url, referer.as_deref(), mode);
            let _ = tx.send(result);
        })
        .map_err(|e| format!("Could not reach the UI thread: {e}"))?;

    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|_| "The download prompt did not open in time.".to_string())?
}

/// Whether the caller already runs on the main (UI) thread.
///
/// Tauri has no portable "am I on the main thread" check, and the prompt is
/// reached from two places: Tauri commands (main thread) and the IPC server
/// (a tokio worker created by `tokio::spawn`). The absence of a tokio runtime on
/// the current thread identifies the UI thread reliably enough.
fn is_main_thread() -> bool {
    tokio::runtime::Handle::try_current().is_err()
}

/// The actual window construction; must run on the UI thread.
fn show_inner(
    app: &AppHandle,
    url: &str,
    referer: Option<&str>,
    mode: PromptMode,
) -> Result<(), String> {
    // Only one decision on screen at a time. Closing is deferred by the platform,
    // so the new window gets a label of its own instead of waiting for the old one
    // to disappear.
    close_all(app);

    let target = prompt_url(url, referer, mode);
    let (width, height) = mode.size();
    let label = mode.next_label();

    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(target.into()))
        .title("ZDM")
        .inner_size(width, height)
        .min_inner_size(width, height)
        .max_inner_size(width, height)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .focused(true)
        .visible(false)
        .build()
        .map_err(|e| format!("Could not open the download prompt: {e}"))?;

    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// Close every prompt window.
///
/// The app's own window map is the source of truth: labels are unique per prompt,
/// so scanning for the prefix closes whatever is open without assuming a name.
pub fn close_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = window.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_is_encoded_so_it_survives_the_query_string() {
        let encoded = prompt_url("https://a.com/x?y=1&z=2", None, PromptMode::File);
        assert!(encoded.starts_with("index.html?window=prompt&mode=file&url="));
        assert!(encoded.contains("&url=https%3A%2F%2Fa.com"));
        assert!(!encoded.contains("y=1&z=2"), "query must be encoded: {encoded}");
        assert!(encoded.contains("%3F"), "the ? must be encoded");
        assert!(encoded.contains("%26"), "the & must be encoded");
    }

    #[test]
    fn referer_is_appended_only_when_present() {
        let without = prompt_url("https://a.com/f.bin", None, PromptMode::File);
        assert!(!without.contains("referer="));

        let with = prompt_url(
            "https://a.com/f.bin",
            Some("https://a.com/page"),
            PromptMode::File,
        );
        assert!(with.contains("referer="));
        assert!(with.contains("https%3A%2F%2Fa.com%2Fpage"));
    }

    #[test]
    fn encoding_keeps_unreserved_characters_readable() {
        assert_eq!(urlencode("abc-._~123"), "abc-._~123");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    /// Each mode gets its own window: sharing one and reloading it is what made a
    /// file capture display the previous video's list on Windows.
    #[test]
    fn modes_use_distinct_window_labels() {
        assert_ne!(PromptMode::File.kind(), PromptMode::Video.kind());
        assert_eq!(PromptMode::File.kind(), FILE_KIND);
        assert_eq!(PromptMode::Video.kind(), VIDEO_KIND);
    }

    /// The reported bug: clicking a second link while a prompt was open failed
    /// with "a webview with label `prompt-video` already exists". Window teardown
    /// is asynchronous, so labels must be unique rather than reused.
    #[test]
    fn every_prompt_gets_a_unique_label() {
        let labels: Vec<String> = (0..5)
            .map(|_| PromptMode::Video.next_label())
            .collect();

        let unique: std::collections::HashSet<&String> = labels.iter().collect();
        assert_eq!(
            unique.len(),
            labels.len(),
            "labels must not repeat: {labels:?}"
        );

        for label in &labels {
            assert!(
                label.starts_with(LABEL_PREFIX),
                "close_all finds prompts by prefix, got {label}"
            );
            assert!(label.contains(VIDEO_KIND), "label must name its mode: {label}");
        }
    }

    /// A file prompt and a video prompt must never collide, even at the same index.
    #[test]
    fn file_and_video_labels_never_collide() {
        for _ in 0..3 {
            let file = PromptMode::File.next_label();
            let video = PromptMode::Video.next_label();
            assert_ne!(file, video);
        }
    }

    /// The video prompt carries a quality grid and needs more room.
    /// A magnet link must never be probed as an HTTP file.
    #[test]
    fn magnets_are_classified_as_torrents() {
        assert_eq!(
            classify("magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c"),
            PromptMode::Torrent
        );
        assert_eq!(
            classify("https://example.com/ubuntu.torrent"),
            PromptMode::Torrent
        );
        assert_eq!(
            classify("0123456789abcdef0123456789abcdef01234567"),
            PromptMode::Torrent
        );
    }

    #[test]
    fn torrent_mode_carries_its_own_label() {
        let url = prompt_url("magnet:?xt=x", None, PromptMode::Torrent);
        assert!(url.contains("mode=torrent"));
        assert_ne!(PromptMode::Torrent.kind(), PromptMode::File.kind());
    }

    #[test]
    fn video_window_is_taller_than_file_window() {
        let (_, file_height) = PromptMode::File.size();
        let (_, video_height) = PromptMode::Video.size();
        assert!(
            video_height > file_height,
            "the video prompt must have room for the quality list"
        );
    }

    #[test]
    fn video_pages_are_classified_as_video() {
        for url in [
            "https://www.youtube.com/watch?v=K9aZFpKYCPY",
            "https://youtu.be/abc123",
            "https://m.youtube.com/watch?v=x",
            "https://vimeo.com/123456",
            "https://www.tiktok.com/@user/video/123",
            "https://twitter.com/user/status/1",
        ] {
            assert_eq!(classify(url), PromptMode::Video, "{url} must be a video");
        }
    }

    #[test]
    fn direct_media_files_are_classified_as_files() {
        for url in [
            "https://cdn.example.com/movie.mp4",
            "https://example.com/song.mp3?sig=abc",
            "https://example.com/clip.webm#t=10",
            "https://example.com/archive.zip",
            "https://example.com/setup.exe",
        ] {
            assert_eq!(classify(url), PromptMode::File, "{url} must be a file");
        }
    }

    #[test]
    fn a_media_extension_wins_over_the_host() {
        assert_eq!(classify("https://www.youtube.com/file.mp4"), PromptMode::File);
    }

    #[test]
    fn mode_is_carried_in_the_url() {
        let video = prompt_url("https://youtu.be/x", None, PromptMode::Video);
        assert!(video.contains("mode=video"));
        let file = prompt_url("https://a.com/f.bin", None, PromptMode::File);
        assert!(file.contains("mode=file"));
    }

    #[test]
    fn spaces_and_unicode_in_a_path_are_encoded() {
        let encoded = prompt_url("https://a.com/my file.mp4", None, PromptMode::File);
        assert!(encoded.contains("my%20file.mp4"));
    }
}
