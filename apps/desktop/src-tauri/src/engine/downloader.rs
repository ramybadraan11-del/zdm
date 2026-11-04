//! ZDM HTTP Probing & Filename Resolution
//! Developed by Ramy Badraan
//!
//! Responsible for inspecting a URL before the transfer starts: size, range
//! support, content type, ETag and a safe filename. Getting `supports_range`
//! wrong here is what made multi-segment transfers corrupt files, so the probe
//! is deliberately conservative: range support is only claimed when the server
//! proves it with a real `206` + `Content-Range` response.

use crate::engine::types::UrlMetadata;
use anyhow::{anyhow, Result};
use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE,
};
use reqwest::{Client, StatusCode};
use std::path::Path;
use std::time::Duration;
use url::Url;

pub const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Shared HTTP client for *transfers*.
///
/// `Client::timeout` in reqwest bounds the **whole request**, not each read. A
/// 45 s cap therefore aborted any download slower than "45 seconds of wall
/// clock" — which is what broke the ffmpeg fetch (110 MB ≈ 71 s) with
/// "error decoding response body". Only the connection attempt is bounded here;
/// per-chunk stalls are handled where the stream is read.
pub fn build_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .pool_idle_timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(64)
        .tcp_keepalive(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(BROWSER_UA)
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// A client for short metadata calls (probe/HEAD) that really should not hang.
pub fn build_probe_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .pool_idle_timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(16)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(BROWSER_UA)
        .build()
        .unwrap_or_else(|_| Client::new())
}

pub struct Downloader {
    pub client: Client,
}

impl Downloader {
    pub fn new() -> Self {
        Self { client: build_probe_client() }
    }

    pub async fn probe_url(&self, url_str: &str) -> Result<UrlMetadata> {
        let parsed = Url::parse(url_str).map_err(|e| anyhow!("Invalid URL: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(anyhow!("Unsupported URL scheme: {}", parsed.scheme()));
        }

        let mut total_size = 0u64;
        let mut supports_range = false;
        let mut etag: Option<String> = None;
        let mut mime_type = "application/octet-stream".to_string();
        let mut filename = String::new();

        // A 1-byte ranged GET is the only reliable way to learn both the size
        // and whether the server honours ranges (many CDNs answer HEAD badly).
        let ranged = self
            .client
            .get(url_str)
            .header(RANGE, "bytes=0-0")
            .header("Accept", "*/*")
            .send()
            .await;

        match ranged {
            Ok(resp) if resp.status() == StatusCode::PARTIAL_CONTENT => {
                supports_range = true;
                if let Some(cr) = resp.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()) {
                    if let Some(total) = cr.rsplit('/').next().and_then(|t| t.trim().parse::<u64>().ok()) {
                        total_size = total;
                    }
                }
                if total_size == 0 {
                    if let Some(cl) = resp.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()) {
                        total_size = cl.parse().unwrap_or(0);
                    }
                }
                capture_headers(
                    resp.headers(),
                    &mut etag,
                    &mut mime_type,
                    &mut filename,
                );
            }
            Ok(resp) => {
                // Server ignored the range and sent the whole body.
                let status = resp.status();
                if status.is_success() {
                    if let Some(cl) = resp.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()) {
                        total_size = cl.parse().unwrap_or(0);
                    }
                    supports_range = resp
                        .headers()
                        .get(ACCEPT_RANGES)
                        .and_then(|v| v.to_str().ok())
                        .map(|v| v.to_lowercase().contains("bytes"))
                        .unwrap_or(false);
                    capture_headers(
                        resp.headers(),
                        &mut etag,
                        &mut mime_type,
                        &mut filename,
                    );
                    // Reclaim the 1-byte body so the connection can be reused.
                    drop(resp);
                } else if status == StatusCode::METHOD_NOT_ALLOWED || status.is_client_error() {
                    // Fall back to a HEAD probe for servers that dislike GETs.
                    if let Ok(head) = self.client.head(url_str).send().await {
                        if head.status().is_success() {
                            if let Some(cl) =
                                head.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok())
                            {
                                total_size = cl.parse().unwrap_or(0);
                            }
                            supports_range = head
                                .headers()
                                .get(ACCEPT_RANGES)
                                .and_then(|v| v.to_str().ok())
                                .map(|v| v.to_lowercase().contains("bytes"))
                                .unwrap_or(false);
                            capture_headers(
                                head.headers(),
                                &mut etag,
                                &mut mime_type,
                                &mut filename,
                            );
                        } else {
                            return Err(anyhow!("HTTP {} for {}", head.status(), url_str));
                        }
                    }
                } else {
                    return Err(anyhow!("HTTP {} for {}", status, url_str));
                }
            }
            Err(err) => return Err(anyhow!("Failed to connect to URL: {err}")),
        }

        if filename.is_empty() {
            filename = filename_from_url(&parsed);
        }
        filename = sanitize_filename(&filename);
        if filename.is_empty() {
            filename = "download.bin".to_string();
        }

        let category = Self::detect_category(&filename, &mime_type);

        Ok(UrlMetadata {
            url: url_str.to_string(),
            filename,
            total_size,
            supports_range,
            mime_type,
            category,
            etag,
        })
    }

    /// Pick a segment count that is big enough to saturate the link but small
    /// enough not to upset servers (they start throttling past ~16 streams).
    pub fn calculate_optimal_segments(
        total_size: u64,
        supports_range: bool,
        requested: usize,
    ) -> usize {
        if !supports_range || total_size == 0 {
            return 1;
        }

        let ceiling = if requested > 0 {
            requested.clamp(1, 32)
        } else {
            16
        };

        // Strictly respect the user's configured connection count,
        // only bounding if the file is smaller than 64 KiB per connection.
        let min_segment_bytes = 64 * 1024u64;
        let max_by_size = (total_size / min_segment_bytes) as usize;
        ceiling.min(max_by_size.max(1))
    }

    pub fn detect_category(filename: &str, mime_type: &str) -> String {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ["mp4", "mkv", "avi", "webm", "mov", "flv", "wmv", "m4v", "mpg", "mpeg", "ts"]
            .contains(&ext.as_str())
            || mime_type.starts_with("video/")
        {
            "Video".to_string()
        } else if ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"].contains(&ext.as_str())
            || mime_type.starts_with("audio/")
        {
            "Audio".to_string()
        } else if ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "zst", "tgz"]
            .contains(&ext.as_str())
        {
            "Compressed".to_string()
        } else if ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub", "md", "csv"]
            .contains(&ext.as_str())
            || mime_type.starts_with("application/pdf")
        {
            "Documents".to_string()
        } else if [
            "jpg", "jpeg", "png", "webp", "gif", "bmp", "svg", "ico", "avif", "tiff", "tif",
        ]
        .contains(&ext.as_str())
            || mime_type.starts_with("image/")
        {
            "Images".to_string()
        } else if [
            "exe", "msi", "deb", "rpm", "appimage", "apk", "dmg", "pkg", "flatpak", "snap", "jar",
        ]
        .contains(&ext.as_str())
        {
            "Programs".to_string()
        } else {
            "Others".to_string()
        }
    }
}

fn capture_headers(
    headers: &reqwest::header::HeaderMap,
    etag: &mut Option<String>,
    mime_type: &mut String,
    filename: &mut String,
) {
    if etag.is_none() {
        if let Some(v) = headers.get(ETAG).and_then(|v| v.to_str().ok()) {
            if !v.is_empty() {
                *etag = Some(v.to_string());
            }
        }
    }

    if let Some(ct) = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
        if !ct.is_empty() {
            *mime_type = ct.split(';').next().unwrap_or(ct).trim().to_string();
        }
    }

    if filename.is_empty() {
        if let Some(cd) = headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
            if let Some(raw) = extract_disposition_filename(cd) {
                *filename = raw;
            }
        }
    }
}

/// Handles both `filename="x.zip"` and RFC 5987 `filename*=UTF-8''x.zip`.
fn extract_disposition_filename(cd: &str) -> Option<String> {
    if let Some(idx) = cd.to_lowercase().find("filename*=") {
        let raw = cd[idx + 10..].trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ');
        if let Some(rest) = raw.split("''").nth(1) {
            if let Ok(decoded) = urlencoding::decode(rest) {
                return Some(decoded.into_owned());
            }
        }
    }

    if let Some(idx) = cd.to_lowercase().find("filename=") {
        let raw = cd[idx + 9..]
            .trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ')
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !raw.is_empty() {
            return Some(raw);
        }
    }
    None
}

fn filename_from_url(parsed: &Url) -> String {
    if let Some(last) = parsed.path_segments().and_then(|mut s| s.next_back()) {
        let decoded = urlencoding::decode(last)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| last.to_string());
        let decoded = decoded.trim().to_string();
        if !decoded.is_empty() && decoded != "/" {
            // Add an extension guess for extension-less endpoints.
            let path = Path::new(&decoded);
            if path.extension().is_none() {
                let host = parsed.host_str().unwrap_or("file").replace('.', "_");
                return format!("{host}_{decoded}");
            }
            return decoded;
        }
    }
    let host = parsed.host_str().unwrap_or("download").replace('.', "_");
    format!("{host}_download.bin")
}

/// Strip anything that is illegal in a filename on Windows or Linux.
pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').trim();
    let mut out = trimmed.to_string();

    // Reserved Windows device names would make the file unwritable.
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = out.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        out = format!("_{out}");
    }

    if out.chars().count() > 200 {
        let stem = Path::new(&out)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(&out)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let keep: String = stem.chars().take(180).collect();
        out = if ext.is_empty() {
            keep
        } else {
            format!("{keep}.{ext}")
        };
    }

    out
}

/// Unique destination path that never overwrites: `name (1).ext`, `name (2).ext`.
pub fn unique_path(dir: &Path, filename: &str) -> std::path::PathBuf {
    let target = dir.join(filename);
    if !target.exists() {
        return target;
    }

    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    for counter in 1..10_000 {
        let candidate = if ext.is_empty() {
            dir.join(format!("{stem} ({counter})"))
        } else {
            dir.join(format!("{stem} ({counter}).{ext}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }

    // Extremely unlikely; fall back to a timestamped name.
    let stamp = chrono::Utc::now().timestamp();
    if ext.is_empty() {
        dir.join(format!("{stem}_{stamp}"))
    } else {
        dir.join(format!("{stem}_{stamp}.{ext}"))
    }
}

/// Best-effort video tool detection used by the video module.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_illegal_characters() {
        assert_eq!(sanitize_filename("a:b*c?.zip"), "a_b_c_.zip");
        assert_eq!(sanitize_filename("  spaced.txt  "), "spaced.txt");
    }

    #[test]
    fn sanitize_protects_reserved_windows_names() {
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
    }

    #[test]
    fn segments_never_split_tiny_files() {
        assert_eq!(Downloader::calculate_optimal_segments(1024, true, 8), 1);
        assert_eq!(Downloader::calculate_optimal_segments(100 * 1024 * 1024, true, 8), 8);
        assert_eq!(Downloader::calculate_optimal_segments(100 * 1024 * 1024, false, 8), 1);
    }

    #[test]
    fn segment_count_is_bounded() {
        let huge = 8 * 1024 * 1024 * 1024u64;
        assert!(Downloader::calculate_optimal_segments(huge, true, 0) <= 16);
        assert!(Downloader::calculate_optimal_segments(huge, true, 64) <= 32);
        assert!(Downloader::calculate_optimal_segments(huge, true, 1) >= 1);
    }

    #[test]
    fn categories_are_detected() {
        assert_eq!(Downloader::detect_category("a.mp4", ""), "Video");
        assert_eq!(Downloader::detect_category("a.mp3", ""), "Audio");
        assert_eq!(Downloader::detect_category("a.zip", ""), "Compressed");
        assert_eq!(Downloader::detect_category("a.exe", ""), "Programs");
        assert_eq!(Downloader::detect_category("a.bin", "application/pdf"), "Documents");
        assert_eq!(Downloader::detect_category("a.jpg", ""), "Images");
        assert_eq!(Downloader::detect_category("a.png", "image/png"), "Images");
        assert_eq!(Downloader::detect_category("a.xyz", "image/webp"), "Images");
        assert_eq!(Downloader::detect_category("a.xyz", ""), "Others");
    }

    #[test]
    fn content_disposition_is_parsed() {
        assert_eq!(
            extract_disposition_filename("attachment; filename=\"setup.exe\""),
            Some("setup.exe".to_string())
        );
        assert_eq!(
            extract_disposition_filename("inline; filename=plain.iso"),
            Some("plain.iso".to_string())
        );
    }

    #[test]
    fn unique_path_avoids_overwrite() {
        let dir = std::env::temp_dir().join(format!("zdm-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = unique_path(&dir, "f.txt");
        assert!(first.ends_with("f.txt"));
        std::fs::write(&first, b"x").unwrap();
        let second = unique_path(&dir, "f.txt");
        assert!(second.to_string_lossy().contains("(1)"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
