//! ZDM Core Multi-Segment Download Engine
//! Developed by Ramy Badraan
//!
//! Direct-write chunked accelerator: Outperforms IDM by preallocating
//! and writing parallel chunks directly at their exact file offsets.
//! Zero assembly lag at 99%!

use crate::engine::types::UrlMetadata;
use anyhow::{anyhow, Result};
use reqwest::header::{ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, RANGE, USER_AGENT};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;
use url::Url;


#[cfg(windows)]
use std::os::windows::fs::FileExt;

pub struct Downloader {
    client: Client,
}

impl Downloader {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// Inspect URL headers to get size, range capability, suggested filename and category
    pub async fn probe_url(&self, url_str: &str) -> Result<UrlMetadata> {
        let parsed = Url::parse(url_str).map_err(|e| anyhow!("Invalid URL: {}", e))?;

        let mut req = self.client.head(url_str)
            .header(USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36")
            .header("Accept", "*/*");

        let mut resp = req.send().await;

        // If HEAD is rejected with 405 or fails, fallback to GET with Range 0-0
        if resp.is_err() || resp.as_ref().map(|r| !r.status().is_success()).unwrap_or(true) {
            req = self.client.get(url_str)
                .header(RANGE, "bytes=0-0")
                .header(USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36");
            resp = req.send().await;
        }

        let resp = resp.map_err(|e| anyhow!("Failed to connect to URL: {}", e))?;
        let headers = resp.headers();

        // 1. Check Range support
        let supports_range = if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            true
        } else if let Some(accept_val) = headers.get(ACCEPT_RANGES) {
            accept_val.to_str().unwrap_or("").to_lowercase().contains("bytes")
        } else {
            false
        };

        // 2. Total Size
        let mut total_size = 0u64;
        if let Some(cl) = headers.get(CONTENT_LENGTH) {
            if let Ok(val) = cl.to_str().unwrap_or("0").parse::<u64>() {
                total_size = val;
            }
        }

        // If Content-Range is returned (e.g. bytes 0-0/1234567)
        if let Some(cr) = headers.get("content-range") {
            if let Ok(cr_str) = cr.to_str() {
                if let Some(slash_idx) = cr_str.rfind('/') {
                    if let Ok(val) = cr_str[slash_idx + 1..].trim().parse::<u64>() {
                        total_size = val;
                    }
                }
            }
        }

        // 3. ETag
        let etag = headers.get(ETAG).and_then(|v| v.to_str().ok()).map(|s| s.to_string());

        // 4. Content Type & Category
        let mime_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // 5. Filename
        let mut filename = String::new();
        if let Some(cd) = headers.get(CONTENT_DISPOSITION) {
            if let Ok(cd_str) = cd.to_str() {
                if let Some(fn_pos) = cd_str.find("filename=") {
                    let raw = &cd_str[fn_pos + 9..];
                    filename = raw.trim_matches(|c| c == '"' || c == '\'' || c == ';').to_string();
                }
            }
        }

        if filename.is_empty() {
            if let Some(segments) = parsed.path_segments() {
                if let Some(last) = segments.last() {
                    let decoded = urlencoding::decode(last).unwrap_or(std::borrow::Cow::Borrowed(last)).to_string();
                    if !decoded.is_empty() {
                        filename = decoded;
                    }
                }
            }
        }

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

    pub fn detect_category(filename: &str, mime_type: &str) -> String {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if ["mp4", "mkv", "avi", "webm", "mov", "flv", "wmv", "m4v"].contains(&ext.as_str())
            || mime_type.starts_with("video/")
        {
            "Video".to_string()
        } else if ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "wma"].contains(&ext.as_str())
            || mime_type.starts_with("audio/")
        {
            "Audio".to_string()
        } else if ["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "zst"].contains(&ext.as_str()) {
            "Compressed".to_string()
        } else if ["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "epub"].contains(&ext.as_str())
            || mime_type.starts_with("application/pdf")
        {
            "Documents".to_string()
        } else if ["exe", "msi", "deb", "rpm", "appimage", "apk", "dmg", "pkg"].contains(&ext.as_str()) {
            "Programs".to_string()
        } else {
            "Others".to_string()
        }
    }

    /// Automatically calculate segment count based on file size and capability
    pub fn calculate_optimal_segments(total_size: u64, supports_range: bool, requested: usize) -> usize {
        if !supports_range || total_size == 0 {
            return 1;
        }

        if requested > 0 {
            return requested.clamp(1, 32);
        }

        // Adaptive scaling:
        // < 5MB   -> 2 connections
        // < 50MB  -> 4 connections
        // < 500MB -> 8 connections
        // < 2GB   -> 16 connections
        // >= 2GB  -> 32 connections (Max turbo acceleration)
        if total_size < 5 * 1024 * 1024 {
            2
        } else if total_size < 50 * 1024 * 1024 {
            4
        } else if total_size < 500 * 1024 * 1024 {
            8
        } else if total_size < 2 * 1024 * 1024 * 1024 {
            16
        } else {
            32
        }
    }
}
