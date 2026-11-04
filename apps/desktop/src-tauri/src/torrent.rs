//! ZDM Torrent support
//! Developed by Ramy Badraan
//!
//! BitTorrent through `librqbit`, driven by the same engine contract as every
//! other transfer: one row in `downloads`, progress on `download_progress`, and
//! pause/resume/cancel through the existing commands.
//!
//! Why a real client instead of shelling out to a daemon: the app must work with
//! nothing installed, and a magnet link needs a live session (DHT, trackers,
//! peer connections) that has to be owned by this process.
//!
//! Design notes:
//!  * One session for the whole app, created lazily on the first torrent — a
//!    session opens sockets and joins the DHT, which is not worth doing at boot
//!    for someone who only downloads HTTP files.
//!  * A session is expensive to rebuild, so it lives for the process lifetime and
//!    is stopped on exit.
//!  * Progress is polled at 1 Hz rather than pushed by the library: the UI already
//!    throttles to ~10 Hz and a torrent's stats are cheap to read.

use std::path::PathBuf;
use std::sync::Arc;

use librqbit::{
    AddTorrent, AddTorrentOptions, ListenerMode, ListenerOptions, Magnet, Session,
    SessionOptions,
};
use tokio::sync::Mutex;

pub const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "http://tracker.openbittorrent.com:80/announce",
    "udp://opentracker.i2p.rocks:6969/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://tracker.moeking.me:6969/announce",
];

/// Append reliable tier-1 public trackers to a magnet URL so peers can be found rapidly.
pub fn enrich_magnet(url: &str) -> String {
    let trimmed = url.trim();
    if !trimmed.to_ascii_lowercase().starts_with("magnet:") {
        return trimmed.to_string();
    }
    let mut enriched = trimmed.to_string();
    for tracker in DEFAULT_TRACKERS {
        let enc = urlencoding::encode(tracker);
        if !enriched.contains(&format!("tr={enc}")) && !enriched.contains(tracker) {
            enriched.push_str("&tr=");
            enriched.push_str(&enc);
        }
    }
    enriched
}

/// Extract the 40-character hex info hash synchronously without network calls.
pub fn extract_info_hash(source: &str) -> Option<String> {
    let trimmed = source.trim();
    let lower = trimmed.to_ascii_lowercase();

    if lower.len() == 40 && lower.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(lower);
    }

    if lower.starts_with("magnet:") {
        if let Ok(m) = Magnet::parse(trimmed) {
            if let Some(id) = m.as_id20() {
                return Some(id.as_string());
            }
        }
        if let Some(pos) = lower.find("xt=urn:btih:") {
            let rest = &trimmed[pos + 12..];
            let raw_hash = rest.split('&').next().unwrap_or("").trim();
            if raw_hash.len() == 40 && raw_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(raw_hash.to_ascii_lowercase());
            }
        }
    }

    if looks_like_torrent_path(trimmed) {
        let path = normalize_torrent_path(trimmed);
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(meta) = librqbit::torrent_from_bytes(&bytes) {
                return Some(meta.info_hash.as_string());
            }
        }
    }

    None
}

/// Extract info hash from raw .torrent file bytes.
pub fn extract_info_hash_from_bytes(bytes: &[u8]) -> Option<String> {
    librqbit::torrent_from_bytes(bytes).ok().map(|m| m.info_hash.as_string())
}

/// Where a torrent comes from, and therefore how it must reach the engine.
///
/// The distinction matters because the two paths are not interchangeable: a
/// remote source is a URL the library fetches itself, while a local one must be
/// read by us and handed over as bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorrentSource {
    /// A magnet link, a bare info-hash, or an `http(s)` URL to a `.torrent` file.
    /// `librqbit` supports all three through `AddTorrent::from_url`.
    Remote(String),
    /// A `.torrent` file on this machine.
    Local(PathBuf),
}

/// Decide how a source should be given to the engine.
///
/// This used to be decided inline in `add` with a single `magnet:` check, so an
/// `https://tracker.example/file.torrent` link fell into the local-file branch
/// and failed with `could not read the torrent file: No such file or directory`
/// — a remote link was being opened as if it were a path on disk. Everything the
/// engine can fetch over the network is therefore classified first, and only a
/// real filesystem path is read locally.
pub fn resolve_source(source: &str) -> TorrentSource {
    let trimmed = source.trim();
    let lower = trimmed.to_ascii_lowercase();

    let is_hash = trimmed.len() == 40 && trimmed.chars().all(|c| c.is_ascii_hexdigit());
    let is_remote = lower.starts_with("magnet:")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || is_hash;

    if is_remote {
        return TorrentSource::Remote(trimmed.to_string());
    }

    TorrentSource::Local(normalize_torrent_path(trimmed))
}

/// A magnet link or a `.torrent` file path.
pub fn looks_like_torrent(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("magnet:")
        || lower.ends_with(".torrent")
        || (lower.len() == 40 && lower.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Whether the input names a `.torrent` file on disk (as opposed to a magnet or
/// an http URL that happens to end in `.torrent`).
///
/// A plain path has no scheme and no `://`; that is what distinguishes
/// `C:\a\b.torrent` and `/home/x/b.torrent` from `https://host/x.torrent`.
pub fn looks_like_torrent_path(source: &str) -> bool {
    let trimmed = source.trim();
    if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("magnet:") {
        return false;
    }

    let is_uri = trimmed.to_ascii_lowercase().starts_with("http://")
        || trimmed.to_ascii_lowercase().starts_with("https://");
    if is_uri {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("file://") || lower.ends_with(".torrent")
}

/// The last path component of a torrent path, handling both separators.
///
/// `rsplit('/')` alone returned the entire `C:\dir\a.torrent` string on Windows.
pub fn file_name_of(source: &str) -> Option<String> {
    let trimmed = source.trim();
    trimmed
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .map(|part| part.to_string())
}

/// The display name of a `.torrent` source, for the row that exists before the
/// metadata resolves.
///
/// A `file://` URI must report the file name, not `file:` + the whole URI: the
/// raw string ends in `.torrent`, so the extension check passes while the last
/// slash-separated component is still correct — but the scheme and the
/// percent-encoding must not leak into the name.
pub fn display_name_of(source: &str) -> Option<String> {
    let path = normalize_torrent_path(source);

    // Split on both separators first: `Path::file_name` follows the host OS, so
    // on Linux `C:\Users\me\b.torrent` has no components at all and came back as
    // the entire string, scheme and drive letter included.
    if let Some(name) = file_name_of(&path.to_string_lossy()).filter(|name| !name.trim().is_empty()) {
        return Some(name);
    }

    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty())
}

/// The `dn` parameter of a magnet link, which is the torrent's display name.
///
/// Returns `None` when the link carries no usable name. The old code sliced the
/// text after the last `:`, which is the info hash — and indexing that to 16
/// bytes panicked on a short hash or on a v2-only (`btmh`) link.
pub fn magnet_display_name(source: &str) -> Option<String> {
    let query = source.trim().strip_prefix("magnet:?")?;

    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key != "dn" {
            continue;
        }

        // A magnet query is `application/x-www-form-urlencoded`, where a space is
        // `+`; `urlencoding::decode` only handles `%20`, so convert first. A real
        // plus sign in a name is sent as `%2B` and survives this step.
        let form_encoded = value.replace('+', " ");
        let decoded = urlencoding::decode(&form_encoded)
            .map(|name| name.into_owned())
            .unwrap_or(form_encoded);
        let name = decoded.trim();
        if name.is_empty() {
            return None;
        }

        // Keep the row named after a file, so the category icon and the
        // "open folder" action behave like every other download.
        return Some(if name.to_ascii_lowercase().ends_with(".torrent") {
            name.to_string()
        } else {
            format!("{name}.torrent")
        });
    }

    None
}

/// Turn whatever the UI or the OS handed us into a real filesystem path.
///
/// Handles the three shapes a `.torrent` path arrives in:
///   * `C:\Users\me\a.torrent`              — plain Windows path
///   * `file:///C:/Users/me/a.torrent`        — Chromium/Firefox URI on Windows
///   * `file:///home/me/a.torrent`            — the same URI on Linux
///   * `~/Downloads/a.torrent`                — shell-style home
///
/// A bare `trim_start_matches("file://")` produced `/C:/...` on Windows, which
/// fails with os error 123 before the file is ever opened.
fn normalize_torrent_path(source: &str) -> std::path::PathBuf {
    let mut value = source.trim().to_string();

    // Percent-decoding: a URI may carry `%20` for spaces.
    if value.contains('%') {
        value = urlencoding::decode(&value)
            .map(|decoded| decoded.into_owned())
            .unwrap_or(value);
    }

    // Strip the scheme only, not the third slash: `file:///home/x` must keep its
    // leading slash (`/home/x`), while `file:///C:/x` becomes `/C:/x` and is
    // fixed up below. Stripping `file:///` outright turned a POSIX path into a
    // relative one.
    if let Some(rest) = value.strip_prefix("file://") {
        value = rest.to_string();
    }

    // `/C:/dir` is not a valid Windows path: drop the leading slash in front of
    // a drive letter. Guarded so a real POSIX path such as `/home/x` is kept.
    let bytes = value.as_bytes();
    if bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes[2] == b':' || bytes[2] == b'|')
    {
        value = value[1..].to_string();
    }

    // `~` and environment variables, via the shared path helper.
    crate::paths::expand(&value)
}

/// Live torrent session, created on first use.
pub struct TorrentEngine {
    session: Mutex<Option<Arc<Session>>>,
}

impl Default for TorrentEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TorrentEngine {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }

    /// Get or create the session, downloading into `download_dir`.
    async fn session(&self, download_dir: &str) -> Result<Arc<Session>, String> {
        let mut guard = self.session.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Ok(Arc::clone(existing));
        }

        let folder = PathBuf::from(download_dir);
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("could not create {download_dir}: {error}"))?;

        let _ = librqbit::try_increase_nofile_limit();

        // UPnP port forwarding and uTP punching are unreliable on Windows (the
        // firewall often blocks the bind, which made the whole session fail to
        // start). Build the options once, then fall back to a bare session if
        // the listening socket cannot be opened, so a magnet still downloads
        // over outbound connections.
        let with_listen = SessionOptions {
            listen: Some(ListenerOptions {
                // uTP and UPnP are the parts that hung the Windows build; TCP-only
                // listening plus outbound connections is stable on every OS.
                mode: if cfg!(windows) { ListenerMode::TcpOnly } else { ListenerMode::TcpAndUtp },
                enable_upnp_port_forwarding: cfg!(not(windows)),
                ..Default::default()
            }),
            ..Default::default()
        };
        let session = match Session::new_with_opts(folder.clone(), with_listen).await {
            Ok(s) => s,
            Err(_) => {
                Session::new_with_opts(
                    folder,
                    SessionOptions {
                        listen: None,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| format!("could not start the torrent session: {error}"))?
            }
        };

        *guard = Some(Arc::clone(&session));
        Ok(session)
    }

    /// Add a magnet link or a .torrent file and start downloading.
    ///
    /// Returns the info-hash so the caller can map engine events back to a row.
    pub async fn add(
        &self,
        source: &str,
        download_dir: &str,
        paused: bool,
    ) -> Result<String, String> {
        let session = self.session(download_dir).await?;

        let add = match resolve_source(source) {
            TorrentSource::Remote(url) => {
                let enriched = enrich_magnet(&url);
                AddTorrent::from_url(enriched)
            }
            TorrentSource::Local(path) => {
                let bytes = std::fs::read(&path)
                    .map_err(|error| format!("could not read the torrent file: {error}"))?;
                AddTorrent::from_bytes(bytes)
            }
        };

        let options = AddTorrentOptions {
            paused,
            overwrite: true,
            output_folder: Some(download_dir.to_string()),
            ..Default::default()
        };

        let response = session
            .add_torrent(add, Some(options))
            .await
            .map_err(|error| format!("could not add the torrent: {error}"))?;

        let handle = match response {
            librqbit::AddTorrentResponse::Added(_, handle)
            | librqbit::AddTorrentResponse::AlreadyManaged(_, handle) => handle,
            librqbit::AddTorrentResponse::ListOnly(_) => {
                return Err("the torrent was opened in list-only mode".to_string())
            }
        };

        Ok(handle.info_hash().as_string())
    }

    /// Add raw .torrent file bytes directly.
    pub async fn add_bytes(
        &self,
        bytes: Vec<u8>,
        download_dir: &str,
        paused: bool,
    ) -> Result<String, String> {
        let session = self.session(download_dir).await?;
        let add = AddTorrent::from_bytes(bytes);
        let options = AddTorrentOptions {
            paused,
            overwrite: true,
            output_folder: Some(download_dir.to_string()),
            ..Default::default()
        };

        let response = session
            .add_torrent(add, Some(options))
            .await
            .map_err(|error| format!("could not add the torrent: {error}"))?;

        let handle = match response {
            librqbit::AddTorrentResponse::Added(_, handle)
            | librqbit::AddTorrentResponse::AlreadyManaged(_, handle) => handle,
            librqbit::AddTorrentResponse::ListOnly(_) => {
                return Err("the torrent was opened in list-only mode".to_string())
            }
        };

        Ok(handle.info_hash().as_string())
    }

    /// Current transfer stats for one torrent, if it is still known.
    pub async fn stats(&self, info_hash: &str) -> Option<TorrentStatsSnapshot> {
        let guard = self.session.lock().await;
        let session = guard.as_ref()?;

        let handle = session.get(librqbit::api::TorrentIdOrHash::parse(info_hash).ok()?)?;
        let stats = handle.stats();

        let peers = stats
            .live
            .as_ref()
            .map(|live| live.snapshot.peer_stats.live as usize)
            .unwrap_or(0);

        // `Speed` is in mebibytes per second, and the engine speaks bytes per
        // second. The old conversion multiplied by 1_000_000/8, which understated
        // the speed by about 5%: 1 MiB is 1_048_576 bytes.
        let down_bps = stats
            .live
            .as_ref()
            .map(|live| live.download_speed.as_bytes())
            .unwrap_or(0);

        Some(TorrentStatsSnapshot {
            downloaded: stats.progress_bytes,
            total: stats.total_bytes,
            finished: stats.finished,
            peers,
            down_bps,
            // A torrent that dies (bad magnet, no metadata, disk full) reports the
            // reason here. Without it the card sat at 0% forever with no message.
            error: stats.error.clone(),
        })
    }

    /// Pause or resume one torrent.
    pub async fn set_paused(&self, info_hash: &str, paused: bool) -> Result<(), String> {
        let parsed = librqbit::api::TorrentIdOrHash::parse(info_hash).map_err(|e| e.to_string())?;

        // The torrent may still be resolving in the background (magnet/.torrent added
        // via the fast path). Retry briefly instead of failing with "no longer known",
        // so a pause issued during metadata resolution actually takes effect.
        for _ in 0..40 {
            let session_opt = {
                let guard = self.session.lock().await;
                guard.as_ref().map(Arc::clone)
            };
            if let Some(session) = session_opt {
                if let Some(handle) = session.get(parsed) {
                    let result = if paused {
                        session.pause(&handle).await
                    } else {
                        session.unpause(&handle).await
                    };
                    if let Err(error) = result {
                        let text = error.to_string();
                        let already = text.contains("already paused") || text.contains("already live");
                        if !already {
                            return Err(text);
                        }
                    }
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        Err("that torrent is no longer known".to_string())
    }

    /// Forget a torrent, optionally deleting the data it wrote.
    ///
    /// `delete_files` is passed through to the session: the library knows which
    /// files belong to the torrent (it can be a folder of many files), whereas
    /// the row only stores one path. Removing the row without telling the session
    /// left the transfer writing to disk in the background.
    pub async fn remove(&self, info_hash: &str, delete_files: bool) -> Result<(), String> {
        let mut guard = self.session.lock().await;
        let Some(session) = guard.as_mut() else {
            return Ok(());
        };
        session
            .delete(
                librqbit::api::TorrentIdOrHash::parse(info_hash).map_err(|e| e.to_string())?,
                delete_files,
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Stop the session and free its sockets.
    pub async fn shutdown(&self) {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.take() {
            session.stop().await;
        }
    }
}

/// A plain snapshot, so the UI never depends on the library's types.
#[derive(Debug, Clone, Default)]
pub struct TorrentStatsSnapshot {
    pub downloaded: u64,
    pub total: u64,
    pub finished: bool,
    pub peers: usize,
    pub down_bps: u64,
    /// Why the torrent stopped, when the engine gave up on it.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnets_and_torrent_files_are_recognised() {
        assert!(looks_like_torrent("magnet:?xt=urn:btih:abc"));
        assert!(looks_like_torrent("https://example.com/file.torrent"));
        assert!(looks_like_torrent("/home/u/downloads/file.torrent"));
        // A bare 40-char info hash is also valid input.
        assert!(looks_like_torrent("0123456789abcdef0123456789abcdef01234567"));
    }

    #[test]
    fn ordinary_urls_are_not_torrents() {
        assert!(!looks_like_torrent("https://example.com/file.zip"));
        assert!(!looks_like_torrent("https://youtube.com/watch?v=x"));
        assert!(!looks_like_torrent(""));
        assert!(!looks_like_torrent("0123456789")); // too short to be a hash
    }

    #[test]
    fn a_torrent_extension_is_case_insensitive() {
        assert!(looks_like_torrent("https://example.com/FILE.TORRENT"));
        assert!(looks_like_torrent("MAGNET:?xt=urn:btih:abc"));
    }

    /// The Windows failure: stripping `file://` left `/C:/dir/f.torrent`, which
    /// the OS rejects with "os error 123" before the file is even opened.
    #[test]
    fn windows_file_uri_becomes_a_drive_path() {
        let path = normalize_torrent_path("file:///C:/Users/me/a.torrent");
        assert_eq!(
            path.to_string_lossy().replace('\\', "/"),
            "C:/Users/me/a.torrent"
        );
    }

    #[test]
    fn plain_windows_path_is_untouched() {
        let path = normalize_torrent_path(r"C:\Users\me\a.torrent");
        assert_eq!(path.to_string_lossy(), r"C:\Users\me\a.torrent");
    }

    /// A POSIX path must not lose its leading slash: `/home/x` is not a drive.
    #[test]
    fn posix_paths_keep_their_leading_slash() {
        assert_eq!(
            normalize_torrent_path("/home/me/a.torrent").to_string_lossy(),
            "/home/me/a.torrent"
        );
        assert_eq!(
            normalize_torrent_path("file:///home/me/a.torrent").to_string_lossy(),
            "/home/me/a.torrent"
        );
    }

    #[test]
    fn percent_encoded_spaces_are_decoded() {
        let path = normalize_torrent_path("file:///tmp/my%20file.torrent");
        assert_eq!(path.to_string_lossy(), "/tmp/my file.torrent");
    }

    /// The reported bug: an `http(s)` link to a `.torrent` file was opened as a
    /// local path and failed with "could not read the torrent file: No such file
    /// or directory", even though the engine can fetch such links itself.
    #[test]
    fn a_remote_torrent_link_is_fetched_not_opened_locally() {
        for url in [
            "https://tracker.example/files/movie.torrent",
            "http://tracker.example/files/movie.torrent",
            "HTTPS://Tracker.Example/files/movie.torrent",
        ] {
            match resolve_source(url) {
                TorrentSource::Remote(kept) => {
                    assert_eq!(kept, url.trim(), "the URL must reach the engine unchanged")
                }
                other => panic!("{url} must be a remote source, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_magnet_and_a_bare_hash_are_remote_sources() {
        assert!(matches!(
            resolve_source("magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c"),
            TorrentSource::Remote(_)
        ));
        assert!(matches!(
            resolve_source("dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c"),
            TorrentSource::Remote(_)
        ));
    }

    #[test]
    fn a_local_path_stays_local() {
        match resolve_source("/tmp/movie.torrent") {
            TorrentSource::Local(path) => {
                assert_eq!(path.to_string_lossy(), "/tmp/movie.torrent")
            }
            other => panic!("a plain path must stay local, got {other:?}"),
        }

        // The `file://` form is a path the OS handed us, not something to fetch.
        match resolve_source("file:///tmp/movie.torrent") {
            TorrentSource::Local(path) => {
                assert_eq!(path.to_string_lossy(), "/tmp/movie.torrent")
            }
            other => panic!("a file:// URI must stay local, got {other:?}"),
        }
    }

    /// The row's name must be the file name, never the `file://` URI.
    #[test]
    fn a_file_uri_reports_its_file_name() {
        assert_eq!(
            display_name_of("file:///tmp/a.torrent").as_deref(),
            Some("a.torrent")
        );
        assert_eq!(
            display_name_of("file:///tmp/my%20file.torrent").as_deref(),
            Some("my file.torrent")
        );
    }

    /// A Windows path keeps its own separators: `Path::file_name` follows the
    /// host OS, so on Linux a backslash path has no components at all.
    #[test]
    fn a_windows_path_reports_its_file_name_on_every_host() {
        assert_eq!(display_name_of(r"C:\Users\me\b.torrent").as_deref(), Some("b.torrent"));
    }

    /// A magnet is named after its `dn`, not after a slice of the info hash.
    /// Slicing the hash to 16 bytes panicked on a short or v2-only link.
    #[test]
    fn a_magnet_is_named_after_its_display_name() {
        let magnet = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny";
        assert_eq!(
            magnet_display_name(magnet).as_deref(),
            Some("Big Buck Bunny.torrent")
        );

        // No `dn`: the caller falls back to a fixed name instead of panicking.
        let bare = "magnet:?xt=urn:btih:abc";
        assert_eq!(magnet_display_name(bare), None);

        // A name that already ends in .torrent is not doubled.
        let named = "magnet:?xt=urn:btih:abc&dn=already.torrent";
        assert_eq!(magnet_display_name(named).as_deref(), Some("already.torrent"));
    }
}
