//! ZDM Video & YouTube Sniffer Module
//! Developed by Ramy Badraan
//!
//! Automatically verifies and self-heals yt-dlp by downloading it
//! if missing from the user's system.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

/// Locate yt-dlp on the system or auto-download standalone binary if missing
pub async fn ensure_yt_dlp() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let home_path = PathBuf::from(home);

    let candidates = vec![
        home_path.join(".local/share/zdm/bin/yt-dlp"),
        home_path.join(".local/bin/yt-dlp"),
        PathBuf::from("/usr/local/bin/yt-dlp"),
        PathBuf::from("/usr/bin/yt-dlp"),
    ];

    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    // Check system PATH
    if let Ok(output) = Command::new("which").arg("yt-dlp").output().await {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Ok(PathBuf::from(path_str));
            }
        }
    }

    // Auto-download yt-dlp standalone binary
    println!("[ZDM Video] yt-dlp not found on system. Auto-downloading latest standalone binary...");
    let target_dir = home_path.join(".local/share/zdm/bin");
    let _ = fs::create_dir_all(&target_dir);
    let target_file = target_dir.join("yt-dlp");

    let download_url = if cfg!(windows) {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client for yt-dlp download: {}", e))?;

    let resp = client.get(download_url).send().await
        .map_err(|e| anyhow!("Failed to download yt-dlp from GitHub releases: {}", e))?;

    if !resp.status().is_success() {
        return Err(anyhow!("GitHub returned HTTP {} while downloading yt-dlp", resp.status()));
    }

    let bytes = resp.bytes().await
        .map_err(|e| anyhow!("Failed to read yt-dlp download stream: {}", e))?;

    fs::write(&target_file, bytes)
        .map_err(|e| anyhow!("Failed to write yt-dlp binary to disk: {}", e))?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&target_file)?.permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&target_file, perms);
    }

    println!("[ZDM Video] yt-dlp successfully downloaded to {:?}", target_file);
    Ok(target_file)
}

pub async fn query_video(url: &str) -> Result<VideoInfo> {
    let yt_dlp_path = ensure_yt_dlp().await?;

    let mut cmd = Command::new(&yt_dlp_path);
    cmd.arg("-J").arg("--no-playlist");

    // Pass node JS runtime if available for YouTube player deobfuscation
    if let Ok(output) = Command::new("which").arg("node").output().await {
        if output.status.success() {
            let node_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !node_path.is_empty() {
                cmd.arg("--js-runtimes").arg(format!("node:{}", node_path));
            }
        }
    }

    let output = cmd
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute {:?}: {}", yt_dlp_path, e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp extraction error: {}", err));
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

pub async fn download_with_yt_dlp(url: &str, format_id: &str, save_path: &str) -> Result<()> {
    let yt_dlp_path = ensure_yt_dlp().await?;

    let mut cmd = Command::new(&yt_dlp_path);
    cmd.arg("-f").arg(format_id)
       .arg("-o").arg(save_path);

    if let Ok(output) = Command::new("which").arg("node").output().await {
        if output.status.success() {
            let node_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !node_path.is_empty() {
                cmd.arg("--js-runtimes").arg(format!("node:{}", node_path));
            }
        }
    }

    let status = cmd.arg(url).status().await
        .map_err(|e| anyhow!("Failed to execute yt-dlp downloader: {}", e))?;

    if !status.success() {
        return Err(anyhow!("yt-dlp failed to complete download"));
    }
    Ok(())
}
