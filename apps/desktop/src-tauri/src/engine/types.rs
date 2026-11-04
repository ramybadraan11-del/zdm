//! ZDM Engine Types & Models
//! Developed by Ramy Badraan

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Categories that have a dedicated folder and can be re-mapped by the user.
pub const CATEGORY_KEYS: [&str; 7] = [
    "Video", "Audio", "Compressed", "Documents", "Images", "Programs", "Others",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Error,
    Cancelled,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "error" => Self::Error,
            "cancelled" => Self::Cancelled,
            _ => Self::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentProgress {
    pub index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub is_active: bool,
    pub is_finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub total_size: u64,
    pub downloaded_size: u64,
    pub status: DownloadStatus,
    pub category: String,
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub progress_percent: f32,
    pub connections: usize,
    pub segments: Vec<SegmentProgress>,
    pub supports_range: bool,
    pub etag: Option<String>,
    pub referer: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlMetadata {
    pub url: String,
    pub filename: String,
    pub total_size: u64,
    pub supports_range: bool,
    pub mime_type: String,
    pub category: String,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStats {
    pub total_speed_bps: u64,
    pub active_count: usize,
    pub queued_count: usize,
    pub completed_count: usize,
    pub disk_free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDownloadRequest {
    pub url: String,
    pub filename: Option<String>,
    pub save_path: Option<String>,
    pub connections: Option<usize>,
    pub category: Option<String>,
    pub start_now: Option<bool>,
    #[serde(default)]
    pub referer: Option<String>,
}

/// Persisted user configuration. Every field has a sane default so that
/// reading an older/partial row still yields a complete Settings value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub max_concurrent_downloads: usize,
    pub default_connections: usize,
    pub language: String,
    pub theme: String,
    pub auto_start: bool,
    pub capture_downloads: bool,
    pub notifications: bool,
    pub sniffer_enabled: bool,
    pub speed_limit_kbps: u64,
    pub keep_partial_on_cancel: bool,
    /// Missing field => default list; explicit empty array is preserved so the
    /// user can deliberately capture nothing.
    #[serde(default = "default_intercept_extensions")]
    /// File extensions the browser integration captures. Empty means "capture
    /// nothing" — the user can still send links manually via the context menu or
    /// the sniffer button. Image formats are deliberately absent by default:
    /// the browser already downloads those fine.
    pub intercept_extensions: Vec<String>,
    /// Optional per-category folder overrides (absolute or `~/`-prefixed).
    /// Missing categories fall back to `<download_dir>/<Category>`.
    #[serde(default)]
    pub category_dirs: BTreeMap<String, String>,
}

/// The out-of-the-box capture list. Images are excluded on purpose.
const DEFAULT_INTERCEPT_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "webm", "mov", "flv", "wmv", "m4v", "mpg", "mpeg", "ts",
    "mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma",
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "tgz", "iso",
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub", "md", "csv",
    "exe", "msi", "deb", "rpm", "appimage", "apk", "dmg", "pkg", "flatpak", "snap", "jar",
];

fn default_intercept_extensions() -> Vec<String> {
    DEFAULT_INTERCEPT_EXTENSIONS
        .iter()
        .map(|extension| extension.to_string())
        .collect()
}

/// Keep only known category keys and non-empty trimmed paths.
fn sanitize_category_dirs(map: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for key in CATEGORY_KEYS {
        if let Some(value) = map.get(key) {
            let cleaned = value.trim().to_string();
            if !cleaned.is_empty() {
                out.insert(key.to_string(), cleaned);
            }
        }
    }
    out
}

fn sanitize_extensions(list: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(list.len());
    for entry in list {
        let trimmed = entry.trim().trim_start_matches('.').to_ascii_lowercase();
        let cleaned: String = trimmed
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+'))
            .take(16)
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        if !out.iter().any(|seen| seen == &cleaned) {
            out.push(cleaned);
        }
    }
    out
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: crate::paths::default_download_dir()
                .to_string_lossy()
                .to_string(),
            max_concurrent_downloads: 3,
            default_connections: 8,
            language: String::new(), // empty => follow system locale
            theme: "moon".to_string(),
            auto_start: true,
            capture_downloads: true,
            notifications: true,
            sniffer_enabled: true,
            speed_limit_kbps: 0,
            keep_partial_on_cancel: false,
            intercept_extensions: default_intercept_extensions(),
            category_dirs: BTreeMap::new(),
        }
    }
}

impl Settings {
    pub fn sanitized(mut self) -> Self {
        self.max_concurrent_downloads = self.max_concurrent_downloads.clamp(1, 16);
        self.default_connections = self.default_connections.clamp(1, 32);
        if !matches!(self.language.as_str(), "ar" | "en" | "fr") {
            self.language = String::new();
        }
        if !matches!(self.theme.as_str(), "moon" | "dawn" | "system") {
            self.theme = "moon".to_string();
        }
        if self.download_dir.trim().is_empty() {
            self.download_dir = crate::paths::default_download_dir()
                .to_string_lossy()
                .to_string();
        }
        self.speed_limit_kbps = self.speed_limit_kbps.min(10_000_000);
        self.intercept_extensions = sanitize_extensions(std::mem::take(&mut self.intercept_extensions));
        self.category_dirs = sanitize_category_dirs(std::mem::take(&mut self.category_dirs));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitized_extensions_are_lowercase_deduped_and_dot_free() {
        let settings = Settings {
            intercept_extensions: vec![
                "MP4".into(),
                ".mkv".into(),
                " zip ".into(),
                "zip".into(),
                ".".into(),
                "exe!".into(),
                "trailing..".into(),
                "".into(),
            ],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(
            settings.intercept_extensions,
            vec!["mp4", "mkv", "zip", "exe", "trailing"]
        );
    }

    #[test]
    fn default_extensions_exclude_images() {
        let settings = Settings::default();
        for image in ["jpg", "jpeg", "png", "webp", "gif", "bmp", "svg", "ico", "avif"] {
            assert!(!settings.intercept_extensions.iter().any(|e| e == image));
        }
        for expected in ["mp4", "zip", "exe", "pdf"] {
            assert!(settings.intercept_extensions.iter().any(|e| e == expected));
        }
    }

    #[test]
    fn empty_intercept_list_is_preserved_for_explicit_opt_out() {
        let settings = Settings {
            intercept_extensions: vec![],
            ..Settings::default()
        }
        .sanitized();
        assert!(settings.intercept_extensions.is_empty());
    }
}
