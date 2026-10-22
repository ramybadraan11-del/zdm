//! ZDM Video & YouTube Sniffer Module
//! Developed by Ramy Badraan

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: Option<String>,
    pub filesize: Option<u64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration_seconds: Option<u64>,
    pub uploader: Option<String>,
    pub formats: Vec<VideoFormat>,
}

pub async fn query_video(url: &str) -> Result<VideoInfo> {
    // Check if yt-dlp exists
    let output = Command::new("yt-dlp")
        .arg("-J")
        .arg("--no-playlist")
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow!("yt-dlp execution error (is yt-dlp installed?): {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp failed: {}", err));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Failed to parse yt-dlp JSON: {}", e))?;

    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("Video").to_string();
    let thumbnail = json.get("thumbnail").and_then(|v| v.as_str()).map(|s| s.to_string());
    let duration_seconds = json.get("duration").and_then(|v| v.as_u64());
    let uploader = json.get("uploader").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut formats = Vec::new();
    if let Some(formats_arr) = json.get("formats").and_then(|f| f.as_array()) {
        for f in formats_arr {
            let format_id = f.get("format_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("mp4").to_string();
            let resolution = f.get("resolution").and_then(|v| v.as_str()).map(|s| s.to_string());
            let filesize = f.get("filesize").and_then(|v| v.as_u64());
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).map(|s| s.to_string());
            let acodec = f.get("acodec").and_then(|v| v.as_str()).map(|s| s.to_string());
            let direct_url = f.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());

            formats.push(VideoFormat {
                format_id,
                ext,
                resolution,
                filesize,
                vcodec,
                acodec,
                url: direct_url,
            });
        }
    }

    Ok(VideoInfo {
        id,
        title,
        thumbnail,
        duration_seconds,
        uploader,
        formats,
    })
}
