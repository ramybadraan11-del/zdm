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
        if let Ok(scraped) = try_scrape_media_from_url(url).await {
            return Ok(scraped);
        }
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
        // 1. Calculate best audio size
        let mut best_audio_size: u64 = 0;
        for f in formats_arr {
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
            if vcodec == "none" && acodec != "none" {
                let size = f.get("filesize").or_else(|| f.get("filesize_approx")).and_then(|v| v.as_u64()).unwrap_or(0);
                if size > best_audio_size {
                    best_audio_size = size;
                }
            }
        }

        // 2. Collect unique video heights (excluding mhtml storyboards and audio-only)
        let mut heights_map: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new();
        for f in formats_arr {
            let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("");
            if ext == "mhtml" {
                continue;
            }
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            if vcodec == "none" {
                continue;
            }
            if let Some(h) = f.get("height").and_then(|v| v.as_u64()) {
                if h > 0 {
                    let size = f.get("filesize").or_else(|| f.get("filesize_approx")).and_then(|v| v.as_u64()).unwrap_or(0);
                    let entry = heights_map.entry(h).or_insert(0);
                    if size > *entry {
                        *entry = size;
                    }
                }
            }
        }

        // 3. Add clean MP4 options for each resolution from highest to lowest
        for (&height, &v_size) in heights_map.iter().rev() {
            let label = match height {
                2160.. => format!("4K Ultra HD ({}p)", height),
                1440..=2159 => format!("2K Quad HD ({}p)", height),
                1080..=1439 => format!("Full HD ({}p)", height),
                720..=1079 => format!("HD ({}p)", height),
                480..=719 => format!("SD ({}p)", height),
                _ => format!("{}p", height),
            };

            let total_size = if v_size > 0 {
                Some(v_size + best_audio_size)
            } else {
                None
            };

            formats.push(VideoFormat {
                format_id: format!("video_{}", height),
                ext: "mp4".to_string(),
                resolution: Some(label),
                filesize: total_size,
                vcodec: Some("mp4/h264".to_string()),
                acodec: Some("aac".to_string()),
                url: None,
            });
        }

        // Fallback for direct streams with no distinct heights
        if formats.is_empty() {
            formats.push(VideoFormat {
                format_id: "best_mp4".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("أعلى جودة متاحة (MP4)".to_string()),
                filesize: None,
                vcodec: Some("mp4".to_string()),
                acodec: Some("aac".to_string()),
                url: None,
            });
        }

        // 4. Add exactly ONE clean Audio option (MP3 / High Quality)
        formats.push(VideoFormat {
            format_id: "bestaudio_mp3".to_string(),
            ext: "mp3".to_string(),
            resolution: Some("تنزيل الصوت فقط (MP3 / أعلى جودة)".to_string()),
            filesize: if best_audio_size > 0 { Some(best_audio_size) } else { None },
            vcodec: Some("none".to_string()),
            acodec: Some("mp3".to_string()),
            url: None,
        });
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

    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
    let home_path = PathBuf::from(home);

    let is_audio = format_id == "bestaudio_mp3" || format_id.contains("audio");
    let category = if is_audio { "Audio" } else { "Video" };
    let default_dir = home_path.join("Downloads").join("ZDM").join(category);
    let _ = fs::create_dir_all(&default_dir);

    let final_save_path = if std::path::Path::new(save_path).is_absolute() {
        PathBuf::from(save_path)
    } else {
        default_dir.join(save_path)
    };

    let mut cmd = Command::new(&yt_dlp_path);

    if is_audio {
        cmd.arg("-x")
           .arg("--audio-format").arg("mp3")
           .arg("-f").arg("bestaudio/best");
    } else if format_id.starts_with("video_") {
        if let Ok(h) = format_id.trim_start_matches("video_").parse::<u64>() {
            cmd.arg("-f").arg(format!("bestvideo[height<={}]+bestaudio/best[height<={}]/best", h, h));
        } else {
            cmd.arg("-f").arg("bestvideo+bestaudio/best");
        }
        cmd.arg("--merge-output-format").arg("mp4");
    } else {
        cmd.arg("-f").arg("bestvideo+bestaudio/best")
           .arg("--merge-output-format").arg("mp4");
    }

    // Locate ffmpeg if available
    if let Ok(ff_out) = Command::new("which").arg("ffmpeg").output().await {
        if ff_out.status.success() {
            let ff_path = String::from_utf8_lossy(&ff_out.stdout).trim().to_string();
            if !ff_path.is_empty() {
                cmd.arg("--ffmpeg-location").arg(ff_path);
            }
        }
    }

    // Pass node JS runtime if available
    if let Ok(node_out) = Command::new("which").arg("node").output().await {
        if node_out.status.success() {
            let node_path = String::from_utf8_lossy(&node_out.stdout).trim().to_string();
            if !node_path.is_empty() {
                cmd.arg("--js-runtimes").arg(format!("node:{}", node_path));
            }
        }
    }

    cmd.arg("-o").arg(final_save_path.to_string_lossy().to_string());
    cmd.arg(url);

    let output = cmd.output().await.map_err(|e| anyhow!("Failed to execute yt-dlp downloader: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp download failed: {}", err));
    }

    Ok(())
}

pub async fn try_scrape_media_from_url(url: &str) -> Result<VideoInfo> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")
        .build()?;

    let lower = url.to_lowercase();
    if lower.contains(".mp4") || lower.contains(".m3u8") || lower.contains(".webm") || lower.contains(".mkv") {
        let resp = client.head(url).send().await;
        let size = resp.ok().and_then(|r| r.content_length());
        let filename = url.split('?').next().unwrap_or("").split('/').last().unwrap_or("video.mp4");
        let clean_title = filename.replace(".mp4", "").replace(".m3u8", "").replace('_', " ").replace('-', " ");

        return Ok(VideoInfo {
            id: "direct_stream".to_string(),
            title: if clean_title.trim().is_empty() { "Direct Video Stream".to_string() } else { clean_title },
            thumbnail: None,
            duration_seconds: None,
            uploader: Some("Direct Web Stream".to_string()),
            formats: vec![VideoFormat {
                format_id: "direct_mp4".to_string(),
                ext: "mp4".to_string(),
                resolution: Some("أعلى دقة متاحة (Direct Media Stream)".to_string()),
                filesize: size,
                vcodec: Some("mp4".to_string()),
                acodec: Some("aac".to_string()),
                url: Some(url.to_string()),
            }],
        });
    }

    let resp = client.get(url).send().await
        .map_err(|e| anyhow!("Failed to fetch webpage: {}", e))?;

    let html = resp.text().await
        .map_err(|e| anyhow!("Failed to read webpage content: {}", e))?;

    let title = if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start + 7..].find("</title>") {
            html[start + 7..start + 7 + end].trim().to_string()
        } else {
            "Web Video".to_string()
        }
    } else {
        "Web Video".to_string()
    };

    let patterns = vec![
        r#"(?i)<source[^>]+src=["']([^"']+\.(?:mp4|m3u8|mpd|webm)[^"']*)["']"#,
        r#"(?i)<video[^>]+src=["']([^"']+\.(?:mp4|m3u8|mpd|webm)[^"']*)["']"#,
        r#"(?i)["'](https?://[^"']+\.(?:mp4|m3u8)[^"']*)["']"#,
        r#"(?i)(?:file|source|url|hls|src):\s*["'](https?://[^"']+)["']"#,
    ];

    for pat in patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            for cap in re.captures_iter(&html) {
                if let Some(m) = cap.get(1) {
                    let mut stream_url = m.as_str().to_string();
                    if stream_url.starts_with("//") {
                        stream_url = format!("https:{}", stream_url);
                    } else if stream_url.starts_with('/') {
                        if let Ok(base) = reqwest::Url::parse(url) {
                            if let Ok(joined) = base.join(&stream_url) {
                                stream_url = joined.to_string();
                            }
                        }
                    }

                    if stream_url.starts_with("http") {
                        let resp = client.head(&stream_url).send().await;
                        let size = resp.ok().and_then(|r| r.content_length());

                        return Ok(VideoInfo {
                            id: "extracted_stream".to_string(),
                            title: title.clone(),
                            thumbnail: None,
                            duration_seconds: None,
                            uploader: Some("Universal Web Extractor".to_string()),
                            formats: vec![VideoFormat {
                                format_id: "direct_mp4".to_string(),
                                ext: "mp4".to_string(),
                                resolution: Some("أعلى دقة متاحة (Universal Stream)".to_string()),
                                filesize: size,
                                vcodec: Some("mp4".to_string()),
                                acodec: Some("aac".to_string()),
                                url: Some(stream_url),
                            }],
                        });
                    }
                }
            }
        }
    }

    Err(anyhow!("No video stream found in webpage HTML"))
}
