//! ZDM Video & Media Sniffer
//! Developed by Ramy Badraan
//!
//! yt-dlp is self-healed (downloaded once, then reused) and always launched
//! through `crate::process` so Windows never shows a console window.
//! Video transfers report progress through the same event channel as regular
//! downloads and can be paused/cancelled by killing the child process.

use crate::engine::downloader::{build_probe_client, sanitize_filename, BROWSER_UA};
use crate::engine::types::Settings;
use crate::process;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

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
    pub fps: Option<u64>,
    pub note: Option<String>,
    pub is_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration_seconds: Option<u64>,
    pub uploader: Option<String>,
    pub webpage_url: String,
    pub extractor: Option<String>,
    pub formats: Vec<VideoFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration_seconds: Option<u64>,
    pub thumbnail: Option<String>,
    pub index: u64,
    /// Estimated size (bytes) for each quality height offered by the playlist.
    pub sizes: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
    pub entries: Vec<PlaylistEntry>,
    /// Every distinct height found across the playlist, tallest first.
    pub heights: Vec<u64>,
    /// Total estimated size across all entries for each offered height.
    pub total_sizes: BTreeMap<u64, u64>,
    pub total_duration_seconds: u64,
}

/// Locate yt-dlp or fetch the standalone build once.
pub async fn ensure_yt_dlp() -> Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![
        crate::paths::bin_dir().join(exe("yt-dlp")),
        crate::paths::home_dir().join(".local").join("bin").join(exe("yt-dlp")),
        PathBuf::from("/usr/local/bin/yt-dlp"),
        PathBuf::from("/usr/bin/yt-dlp"),
    ];

    // Where the installer puts the bundled binary. Without this the app ignored
    // the copy shipped next to it, tried to reach GitHub, and (offline, or behind
    // a proxy) reported that video download was broken.
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            for relative in [
                "yt-dlp",
                "bin/yt-dlp",
                "resources/yt-dlp",
                "resources/bin/yt-dlp",
                "_up_/yt-dlp",
            ] {
                candidates.insert(0, dir.join(format!("{relative}{}", if cfg!(windows) { ".exe" } else { "" })));
            }
        }
    }


    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    if let Some(found) = process::which("yt-dlp") {
        return Ok(found);
    }

    crate::paths::ensure_dirs().ok();
    let target = crate::paths::bin_dir().join(exe("yt-dlp"));
    let url = if cfg!(windows) {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
    } else {
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    };

    let client = build_probe_client();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow!("Could not download yt-dlp: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("GitHub returned HTTP {} for yt-dlp", resp.status()));
    }

    // Stream to a temp file, then rename: a half-written binary must never be
    // left behind where it would shadow a working install.
    let staging = target.with_extension("part");
    {
        use futures_util::StreamExt;
        let mut file = tokio::fs::File::create(&staging).await?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| anyhow!("yt-dlp download interrupted: {e}"))?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await?;
    }
    tokio::fs::rename(&staging, &target).await?;

    #[cfg(unix)]
    {
        let mut perms = std::fs::metadata(&target)?.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&target, perms);
    }

    Ok(target)
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Query metadata and build a small, honest list of download options.
///
/// No `player_client` override: yt-dlp's default client rotation returns the
/// full quality ladder. Pinning clients (e.g. `android,web`) collapsed YouTube
/// to 360p, which is exactly the regression this avoids.
pub async fn query_video(url: &str) -> Result<VideoInfo> {
    let yt_dlp = ensure_yt_dlp().await?;

    let mut cmd = process::command(&yt_dlp);
    cmd.arg("-J")
        .arg("--skip-download")
        .arg("--no-playlist")
        .arg("--no-warnings")
        .arg("--no-call-home")
        .arg("--no-check-certificates")
        .arg("--extractor-args").arg("youtube:skip=translated_subs")
        .arg("--socket-timeout").arg("15");

    if let Some(node) = process::which("node") {
        cmd.arg("--js-runtimes").arg(format!("node:{}", node.to_string_lossy()));
    }

    cmd.arg(url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| anyhow!("Timed out while analysing the video"))?
        .map_err(|e| anyhow!("Failed to run yt-dlp: {e}"))?;

    if !output.status.success() {
        if let Ok(scraped) = try_scrape_media_from_url(url).await {
            return Ok(scraped);
        }
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp could not read this page: {}", tidy_stderr(&err)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| anyhow!("Unexpected yt-dlp output: {e}"))?;

    Ok(build_video_info(&json, url))
}

/// Analyse a playlist and return exactly the videos it contains.
///
/// Unlike a flat listing, this asks yt-dlp for the formats of every entry so the
/// dialog can show real sizes and offer one quality that applies to them all.
/// The call is allowed to run longer because a playlist may hold many videos.
pub async fn query_playlist(url: &str) -> Result<PlaylistInfo> {
    let yt_dlp = ensure_yt_dlp().await?;

    let mut cmd = process::command(&yt_dlp);
    cmd.arg("-J")
        .arg("--skip-download")
        .arg("--ignore-errors")
        .arg("--no-warnings")
        .arg("--no-call-home")
        .arg("--no-check-certificates")
        .arg("--extractor-args").arg("youtube:skip=translated_subs")
        .arg("--socket-timeout").arg("15");

    if let Some(node) = process::which("node") {
        cmd.arg("--js-runtimes").arg(format!("node:{}", node.to_string_lossy()));
    }

    cmd.arg(url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(std::time::Duration::from_secs(300), cmd.output())
        .await
        .map_err(|_| anyhow!("Timed out while analysing the playlist"))?
        .map_err(|e| anyhow!("Failed to run yt-dlp: {e}"))?;

    // `--ignore-errors` may make yt-dlp exit non-zero after skipping an
    // unavailable entry; the JSON we need is still on stdout.
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow!("yt-dlp could not read this playlist: {}", tidy_stderr(&err))
    })?;

    build_playlist_info(&json, url)
}

fn tidy_stderr(raw: &str) -> String {
    let line = raw
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("unknown error");
    line.chars().take(300).collect()
}

fn build_video_info(json: &serde_json::Value, url: &str) -> VideoInfo {
    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("media").to_string();
    let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("Media").to_string();
    let thumbnail = json.get("thumbnail").and_then(|v| v.as_str()).map(str::to_string);
    let duration_seconds = json.get("duration").and_then(|v| v.as_f64()).map(|d| d as u64);
    let uploader = json.get("uploader").and_then(|v| v.as_str()).map(str::to_string);
    let webpage_url = json
        .get("webpage_url")
        .and_then(|v| v.as_str())
        .unwrap_or(url)
        .to_string();
    let extractor = json.get("extractor_key").and_then(|v| v.as_str()).map(str::to_string);

    // Best audio-only stream size, used to estimate merged muxed sizes.
    let mut best_audio = 0u64;
    let mut heights: BTreeMap<u64, (u64, Option<u64>)> = BTreeMap::new();

    if let Some(list) = json.get("formats").and_then(|f| f.as_array()) {
        for f in list {
            let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("");
            if ext == "mhtml" || ext == "none" {
                continue;
            }
            let vcodec = f.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            let acodec = f.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
            let size = f
                .get("filesize")
                .or_else(|| f.get("filesize_approx"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if vcodec == "none" && acodec != "none" {
                best_audio = best_audio.max(size);
                continue;
            }
            if vcodec == "none" {
                continue;
            }

            let Some(height) = f.get("height").and_then(|v| v.as_u64()) else { continue };
            if height == 0 {
                continue;
            }
            let fps = f.get("fps").and_then(|v| v.as_f64()).map(|v| v as u64);
            let entry = heights.entry(height).or_insert((0, fps));
            if size > entry.0 {
                entry.0 = size;
            }
            if entry.1.is_none() {
                entry.1 = fps;
            }
        }
    }

    let mut formats: Vec<VideoFormat> = heights
        .iter()
        .rev()
        .map(|(&height, &(size, fps))| {
            let label = match height {
                2160.. => format!("4K • {height}p"),
                1440..=2159 => format!("2K • {height}p"),
                1080..=1439 => format!("Full HD • {height}p"),
                720..=1079 => format!("HD • {height}p"),
                480..=719 => format!("SD • {height}p"),
                _ => format!("{height}p"),
            };
            let fps_suffix = match fps {
                Some(f) if f >= 50 => format!(" {f}fps"),
                _ => String::new(),
            };
            VideoFormat {
                format_id: format!("video_{height}"),
                ext: "mp4".to_string(),
                resolution: Some(format!("{label}{fps_suffix}")),
                filesize: if size > 0 { Some(size + best_audio) } else { None },
                vcodec: Some("h264".to_string()),
                acodec: Some("aac".to_string()),
                url: None,
                fps,
                note: None,
                is_audio: false,
            }
        })
        .collect();

    if formats.is_empty() {
        formats.push(VideoFormat {
            format_id: "best_mp4".to_string(),
            ext: "mp4".to_string(),
            resolution: Some("Best available".to_string()),
            filesize: None,
            vcodec: Some("mp4".to_string()),
            acodec: Some("aac".to_string()),
            url: None,
            fps: None,
            note: None,
            is_audio: false,
        });
    }

    formats.push(VideoFormat {
        format_id: "bestaudio_mp3".to_string(),
        ext: "mp3".to_string(),
        resolution: Some("Audio only".to_string()),
        filesize: if best_audio > 0 { Some(best_audio) } else { None },
        vcodec: Some("none".to_string()),
        acodec: Some("mp3".to_string()),
        url: None,
        fps: None,
        note: None,
        is_audio: true,
    });

    VideoInfo {
        id,
        title,
        thumbnail,
        duration_seconds,
        uploader,
        webpage_url,
        extractor,
        formats,
    }
}

fn format_size(format: &serde_json::Value) -> u64 {
    format
        .get("filesize")
        .and_then(|v| v.as_u64())
        .or_else(|| format.get("filesize_approx").and_then(|v| v.as_u64()))
        .unwrap_or(0)
}

/// Size of each video height, already including the best audio when the video
/// stream is not muxed. Heights with no known size are omitted.
fn sizes_from_formats(value: Option<&serde_json::Value>) -> BTreeMap<u64, u64> {
    let mut best_audio = 0u64;
    // height -> (largest video stream, whether that stream already has audio)
    let mut heights: BTreeMap<u64, (u64, bool)> = BTreeMap::new();

    if let Some(list) = value.and_then(|formats| formats.as_array()) {
        for format in list {
            let ext = format.get("ext").and_then(|v| v.as_str()).unwrap_or("");
            if ext == "mhtml" || ext == "none" {
                continue;
            }
            let vcodec = format.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            let acodec = format.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
            let size = format_size(format);

            if vcodec == "none" {
                if acodec != "none" {
                    best_audio = best_audio.max(size);
                }
                continue;
            }

            let Some(height) = format.get("height").and_then(|v| v.as_u64()) else { continue };
            if height == 0 {
                continue;
            }
            let muxed = acodec != "none";
            let entry = heights.entry(height).or_insert((size, muxed));
            if size > entry.0 {
                entry.0 = size;
                entry.1 = muxed;
            }
        }
    }

    heights
        .into_iter()
        .filter_map(|(height, (size, muxed))| {
            let total = if muxed { size } else { size.saturating_add(best_audio) };
            (total > 0).then_some((height, total))
        })
        .collect()
}

/// Size of an entry at `height`: the best stream at or below it, mirroring the
/// `bestvideo[height<=h]` selector. Falls back to the entry's tallest stream
/// when it has nothing that low.
fn size_at(sizes: &BTreeMap<u64, u64>, height: u64) -> u64 {
    if let Some((_, &size)) = sizes.range(..=height).next_back() {
        return size;
    }
    sizes.values().next_back().copied().unwrap_or(0)
}

fn build_playlist_info(json: &serde_json::Value, url: &str) -> Result<PlaylistInfo> {
    if json.get("_type").and_then(|v| v.as_str()) != Some("playlist") {
        return Err(anyhow!("This link is not a playlist."));
    }

    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("playlist").to_string();
    let title = json.get("title").and_then(|v| v.as_str()).unwrap_or("Playlist").to_string();
    let uploader = json.get("uploader").and_then(|v| v.as_str()).map(str::to_string);
    let thumbnail = json.get("thumbnail").and_then(|v| v.as_str()).map(str::to_string);
    let webpage_url = json
        .get("webpage_url")
        .and_then(|v| v.as_str())
        .unwrap_or(url)
        .to_string();

    struct RawEntry {
        id: String,
        url: String,
        title: String,
        duration: Option<u64>,
        thumbnail: Option<String>,
        sizes: BTreeMap<u64, u64>,
    }

    let mut raws: Vec<RawEntry> = Vec::new();
    let mut all_heights: BTreeSet<u64> = BTreeSet::new();

    if let Some(list) = json.get("entries").and_then(|v| v.as_array()) {
        for entry in list {
            // Flat-only stubs and unavailable videos never make it in.
            if entry.get("_type").and_then(|v| v.as_str()) == Some("url") {
                continue;
            }
            let Some(id) = entry.get("id").and_then(|v| v.as_str()) else { continue };
            if id.is_empty() {
                continue;
            }

            let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or(id).to_string();
            let entry_url = entry
                .get("webpage_url")
                .and_then(|v| v.as_str())
                .filter(|u| u.starts_with("http"))
                .map(str::to_string)
                .or_else(|| {
                    entry
                        .get("url")
                        .and_then(|v| v.as_str())
                        .filter(|u| u.starts_with("http"))
                        .map(str::to_string)
                })
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
            let duration = entry.get("duration").and_then(|v| v.as_f64()).map(|d| d as u64);
            let thumbnail = entry.get("thumbnail").and_then(|v| v.as_str()).map(str::to_string);
            let sizes = sizes_from_formats(entry.get("formats"));
            for height in sizes.keys() {
                all_heights.insert(*height);
            }

            raws.push(RawEntry {
                id: id.to_string(),
                url: entry_url,
                title,
                duration,
                thumbnail,
                sizes,
            });
        }
    }

    if raws.is_empty() {
        return Err(anyhow!("No videos were found in this playlist."));
    }

    let heights: Vec<u64> = all_heights.iter().rev().copied().collect();
    let mut total_sizes: BTreeMap<u64, u64> = heights.iter().map(|height| (*height, 0)).collect();
    let mut total_duration_seconds = 0u64;
    let mut entries = Vec::with_capacity(raws.len());

    for (index, raw) in raws.into_iter().enumerate() {
        let mut sizes = BTreeMap::new();
        for &height in &heights {
            let size = size_at(&raw.sizes, height);
            sizes.insert(height, size);
            if let Some(total) = total_sizes.get_mut(&height) {
                *total += size;
            }
        }
        total_duration_seconds += raw.duration.unwrap_or(0);
        entries.push(PlaylistEntry {
            id: raw.id,
            url: raw.url,
            title: raw.title,
            duration_seconds: raw.duration,
            thumbnail: raw.thumbnail,
            index: index as u64,
            sizes,
        });
    }

    Ok(PlaylistInfo {
        id,
        title,
        uploader,
        thumbnail,
        webpage_url,
        entries,
        heights,
        total_sizes,
        total_duration_seconds,
    })
}

/// Build the yt-dlp argument list for a chosen format id.
fn format_args(format_id: &str, is_audio: bool) -> Vec<String> {
    if is_audio || format_id == "bestaudio_mp3" {
        return vec![
            "-x".into(),
            "--audio-format".into(),
            "mp3".into(),
            "--audio-quality".into(),
            "0".into(),
            "-f".into(),
            "bestaudio/best".into(),
        ];
    }

    if let Some(height) = format_id.strip_prefix("video_") {
        if let Ok(h) = height.parse::<u64>() {
            return vec![
                "-f".into(),
                format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]/best"),
                "--merge-output-format".into(),
                "mp4".into(),
            ];
        }
    }

    vec![
        "-f".into(),
        "bestvideo+bestaudio/best".into(),
        "--merge-output-format".into(),
        "mp4".into(),
    ]
}

pub struct VideoJob {
    pub filename: String,
    pub save_path: String,
    pub total_size: u64,
}

/// Spawn yt-dlp and stream its progress. Returns a handle that can cancel it.
#[allow(clippy::too_many_arguments)]
pub fn spawn_yt_dlp(
    job: VideoJob,
    url: String,
    format_id: String,
    is_audio: bool,
    force_overwrite: bool,
    settings: Settings,
    cancel: Arc<AtomicU64>,
    on_progress: impl Fn(u64, u64, u64, u64, f32) + Send + 'static,
) -> tokio::task::JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let yt_dlp = ensure_yt_dlp().await?;

        let target_dir = Path::new(&job.save_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| crate::paths::expand(&settings.download_dir));
        tokio::fs::create_dir_all(&target_dir).await.ok();

        let template = target_dir.join(&job.filename);

        let mut cmd = process::command(&yt_dlp);
        let conn_count = settings.default_connections.clamp(1, 32);
        cmd.arg("--newline")
            .arg("--no-playlist")
            .arg("--no-warnings")
            .arg("--no-call-home")
            .arg("--no-check-certificates")
            .arg("--extractor-args").arg("youtube:skip=translated_subs")
            .arg("--retries").arg("10")
            .arg("--fragment-retries").arg("10")
            .arg("--concurrent-fragments").arg(conn_count.to_string())
            .arg("--buffer-size").arg("1024k")
            .arg("--progress")
            .arg("--progress-template")
            .arg("download:ZDM_PROG:%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s")
            .arg("--print")
            .arg("after_move:ZDM_DONE:%(filepath)s");

        // Use aria2c if available for external TCP multi-connection acceleration
        if let Some(aria2) = process::which("aria2c") {
            cmd.arg("--downloader").arg(aria2.to_string_lossy().to_string());
            cmd.arg("--downloader-args").arg(format!("aria2c:-c -x {conn_count} -s {conn_count} -k 1M -j {conn_count}"));
        }

        if force_overwrite {
            cmd.arg("--force-overwrites");
        } else {
            cmd.arg("--no-overwrites");
        }

        for arg in format_args(&format_id, is_audio) {
            cmd.arg(arg);
        }

        // ffmpeg is not optional for a merged download: YouTube serves video and
        // audio separately above 720p, and without ffmpeg yt-dlp writes
        // "<title>.f137.mp4" + "<title>.f140.m4a" instead of one file. Fetch it if
        // this machine has none (Windows always lacks it).
        match crate::ffmpeg::ensure().await {
            Ok(dir) => {
                cmd.arg("--ffmpeg-location").arg(dir);
            }
            Err(err) => {
                if !is_audio {
                    // A video request cannot be honoured properly without it, so
                    // fail with an actionable message instead of silently
                    // leaving two half-files behind.
                    return Err(anyhow!(
                        "ffmpeg is required to merge video and audio into one file. {err}"
                    ));
                }
                eprintln!("[ZDM video] ffmpeg unavailable: {err}");
            }
        }
        if let Some(node) = process::which("node") {
            cmd.arg("--js-runtimes").arg(format!("node:{}", node.to_string_lossy()));
        }

        cmd.arg("--user-agent").arg(BROWSER_UA);
        cmd.arg("-o").arg(&template);
        cmd.arg(&url);

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null());

        let mut child = cmd.spawn().map_err(|e| anyhow!("Failed to start yt-dlp: {e}"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("yt-dlp stdout unavailable"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("yt-dlp stderr unavailable"))?;

        // Drain stderr so a chatty run can never block on a full pipe.
        let stderr_tail = Arc::new(parking_lot::Mutex::new(String::new()));
        let tail_clone = Arc::clone(&stderr_tail);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut tail = tail_clone.lock();
                tail.push_str(&line);
                tail.push('\n');
                if tail.len() > 4000 {
                    let keep = tail.len() - 2000;
                    *tail = tail[keep..].to_string();
                }
            }
        });

        let mut lines = BufReader::new(stdout).lines();
        let mut last_total = job.total_size;
        let mut final_path: Option<String> = None;

        loop {
            let next = tokio::select! {
                line = lines.next_line() => line,
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    if cancel.load(Ordering::Relaxed) == 1 {
                        let _ = child.kill().await;
                        // yt-dlp leaves .part files behind; clean them so a
                        // cancelled video never shows up as a broken file.
                        cleanup_partials(&target_dir, &job.filename);
                        return Ok(());
                    }
                    continue;
                }
            };

            let Ok(Some(raw)) = next else { break };
            let line = raw.trim();

            if let Some(rest) = line.strip_prefix("ZDM_PROG:") {
                let parts: Vec<&str> = rest.split('|').collect();
                if parts.len() >= 5 {
                    let downloaded = parts[0].trim().parse::<u64>().unwrap_or(0);
                    let total = parts[1].trim().parse::<u64>().unwrap_or(0);
                    let estimate = parts[2].trim().parse::<u64>().unwrap_or(0);
                    let speed = parts[3]
                        .trim()
                        .parse::<f64>()
                        .map(|v| v as u64)
                        .unwrap_or(0);
                    let eta = parts[4].trim().parse::<u64>().unwrap_or(0);

                    let known_total = if total > 0 {
                        total
                    } else if estimate > 0 {
                        estimate
                    } else {
                        last_total
                    };
                    if known_total > last_total {
                        last_total = known_total;
                    }
                    let percent = if known_total > 0 {
                        ((downloaded as f64 / known_total as f64) * 100.0).min(99.5) as f32
                    } else {
                        0.0
                    };
                    on_progress(downloaded, known_total, speed, eta, percent);
                }
            } else if let Some(path) = line.strip_prefix("ZDM_DONE:") {
                final_path = Some(path.trim().to_string());
            }
        }

        let status = child.wait().await.map_err(|e| anyhow!("yt-dlp wait failed: {e}"))?;
        if cancel.load(Ordering::Relaxed) == 1 {
            cleanup_partials(&target_dir, &job.filename);
            return Ok(());
        }
        if !status.success() {
            let tail = stderr_tail.lock().clone();
            return Err(anyhow!("yt-dlp failed: {}", tidy_stderr(&tail)));
        }

        // yt-dlp may report the merged file, or nothing at all. Resolve the real
        // output deliberately so the row in the database always points at a file
        // that exists — that is what "open file" uses.
        finalise_output(&job, final_path.as_deref())?;

        Ok(())
    })
}


/// Resolve yt-dlp's output into exactly one file at `job.save_path`.
///
/// Three cases have to be handled, all of them observed in the wild:
///
///  1. yt-dlp printed `after_move:` and the file is there — rename it.
///  2. it printed the path but the merge produced `<name>.mkv`/`.webm` — move
///     that instead, keeping the requested extension only if it matches.
///  3. ffmpeg was missing, so it wrote `<name>.f137.mp4` + `<name>.f140.m4a`.
///     Those per-format fragments are useless on their own, so report a clear
///     error and remove them rather than leaving debris next to the download.
fn finalise_output(job: &VideoJob, reported: Option<&str>) -> Result<()> {
    let target = Path::new(&job.save_path);
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");

    // 1. The path yt-dlp told us about.
    if let Some(path) = reported {
        let reported = Path::new(path);
        if reported.is_file() {
            if reported != target {
                let _ = std::fs::remove_file(target);
                std::fs::rename(reported, target)
                    .map_err(|e| anyhow!("Could not move the finished video: {e}"))?;
            }
            cleanup_fragments(dir, stem);
            return Ok(());
        }
    }

    // 2. Already at the target path.
    if target.is_file() {
        cleanup_fragments(dir, stem);
        return Ok(());
    }

    // 3. A container ffmpeg chose instead of the requested extension.
    for extension in ["mp4", "mkv", "webm", "m4a", "mp3"] {
        let candidate = dir.join(format!("{stem}.{extension}"));
        if candidate.is_file() {
            if candidate != target {
                let _ = std::fs::remove_file(target);
                std::fs::rename(&candidate, target)
                    .map_err(|e| anyhow!("Could not move the finished video: {e}"))?;
            }
            cleanup_fragments(dir, stem);
            return Ok(());
        }
    }

    // 4. Only per-format fragments exist: the merge never happened.
    let fragments = find_fragments(dir, stem);
    if !fragments.is_empty() {
        for fragment in &fragments {
            let _ = std::fs::remove_file(fragment);
        }
        return Err(anyhow!(
            "The video and audio streams could not be merged into one file. \
             This happens when ffmpeg is missing; ZDM now installs it automatically, \
             so retrying should work."
        ));
    }

    Err(anyhow!(
        "The download finished but produced no file at {}",
        target.display()
    ))
}

/// `<stem>.f137.mp4`, `<stem>.f140.m4a`, `<stem>.temp.mp4`, `.part` files.
fn find_fragments(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(stem) {
            continue;
        }

        // A per-format fragment carries ".f<number>." in the name.
        let rest = &name[stem.len()..];
        let is_fragment = rest
            .split('.')
            .filter(|part| !part.is_empty())
            .any(|part| {
                part.strip_prefix('f')
                    .map(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
            });
        let is_temp = name.ends_with(".part") || name.ends_with(".ytdl") || name.ends_with(".temp.mp4");

        if is_fragment || is_temp {
            found.push(path);
        }
    }
    found
}

fn cleanup_fragments(dir: &Path, stem: &str) {
    for fragment in find_fragments(dir, stem) {
        let _ = std::fs::remove_file(fragment);
    }
}

fn cleanup_partials(dir: &Path, stem: &str) {
    let base = Path::new(stem)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(stem)
        .to_string();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with(&base) && (name.ends_with(".part") || name.ends_with(".ytdl")) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Last-resort fallback for plain HTML pages with a direct media URL.
pub async fn try_scrape_media_from_url(url: &str) -> Result<VideoInfo> {
    let client = build_probe_client();
    let lower = url.to_lowercase();
    let looks_direct = [".mp4", ".m3u8", ".webm", ".mkv", ".mov"]
        .iter()
        .any(|ext| lower.contains(ext));

    if looks_direct {
        let size = client
            .head(url)
            .send()
            .await
            .ok()
            .and_then(|r| r.content_length());
        let name = url
            .split('?')
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("video.mp4");
        return Ok(VideoInfo {
            id: "direct".into(),
            title: name.to_string(),
            thumbnail: None,
            duration_seconds: None,
            uploader: Some("Direct stream".into()),
            webpage_url: url.to_string(),
            extractor: Some("direct".into()),
            formats: vec![VideoFormat {
                format_id: "best_mp4".into(),
                ext: "mp4".into(),
                resolution: Some("Direct stream".into()),
                filesize: size,
                vcodec: Some("mp4".into()),
                acodec: Some("aac".into()),
                url: Some(url.to_string()),
                fps: None,
                note: Some("direct".into()),
                is_audio: false,
            }],
        });
    }

    let html = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow!("Could not load the page: {e}"))?
        .text()
        .await
        .map_err(|e| anyhow!("Could not read the page: {e}"))?;

    let title = extract_title(&html).unwrap_or_else(|| "Web video".to_string());
    let patterns = [
        r#"(?i)<source[^>]+src=["']([^"']+\.(?:mp4|m3u8|webm)[^"']*)["']"#,
        r#"(?i)<video[^>]+src=["']([^"']+\.(?:mp4|m3u8|webm)[^"']*)["']"#,
        r#"(?i)["'](https?://[^"']+\.(?:mp4|m3u8)[^"']*)["']"#,
    ];

    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            for capture in re.captures_iter(&html) {
                let Some(found) = capture.get(1) else { continue };
                let mut stream_url = found.as_str().to_string();
                if stream_url.starts_with("//") {
                    stream_url = format!("https:{stream_url}");
                } else if stream_url.starts_with('/') {
                    if let Ok(base) = reqwest::Url::parse(url) {
                        if let Ok(joined) = base.join(&stream_url) {
                            stream_url = joined.to_string();
                        }
                    }
                }
                if !stream_url.starts_with("http") {
                    continue;
                }
                let size = client
                    .head(&stream_url)
                    .send()
                    .await
                    .ok()
                    .and_then(|r| r.content_length());
                return Ok(VideoInfo {
                    id: "extracted".into(),
                    title: title.clone(),
                    thumbnail: None,
                    duration_seconds: None,
                    uploader: None,
                    webpage_url: url.to_string(),
                    extractor: Some("html".into()),
                    formats: vec![VideoFormat {
                        format_id: "best_mp4".into(),
                        ext: "mp4".into(),
                        resolution: Some("Best available".into()),
                        filesize: size,
                        vcodec: Some("mp4".into()),
                        acodec: Some("aac".into()),
                        url: Some(stream_url),
                        fps: None,
                        note: Some("scraped".into()),
                        is_audio: false,
                    }],
                });
            }
        }
    }

    Err(anyhow!("No downloadable media was found on this page"))
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let open = lower.find("<title")?;
    let start = lower[open..].find('>')? + open + 1;
    let end = lower[start..].find("</title>")? + start;
    let title = html[start..end].trim();
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(120).collect())
    }
}

/// Write a `.desktop`-friendly name for the given title.
pub fn safe_stem(title: &str) -> String {
    let cleaned = sanitize_filename(title);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "media".to_string()
    } else {
        trimmed.to_string()
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_args_picks_the_right_selector() {
        let args = format_args("video_1080", false);
        assert!(args.iter().any(|a| a.contains("height<=1080")));

        let audio = format_args("bestaudio_mp3", true);
        assert!(audio.iter().any(|a| a == "-x"));
        assert!(audio.iter().any(|a| a == "mp3"));
    }

    #[test]
    fn video_info_builds_mp4_options_and_audio() {
        let json = serde_json::json!({
            "id": "abc",
            "title": "Sample",
            "formats": [
                { "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 1080, "filesize": 1000 },
                { "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 720, "filesize": 500 },
                { "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 100 },
                { "ext": "mhtml", "vcodec": "none", "acodec": "none" }
            ]
        });
        let info = build_video_info(&json, "https://example.com/v");
        assert_eq!(info.title, "Sample");
        // 2 video heights + 1 audio option, highest first.
        assert_eq!(info.formats.len(), 3);
        assert!(info.formats[0].resolution.as_ref().unwrap().contains("1080"));
        assert!(info.formats[2].is_audio);
        assert_eq!(info.formats[0].filesize, Some(1100));
    }

    #[test]
    fn playlist_keeps_only_its_own_videos_and_sizes() {
        let json = serde_json::json!({
            "_type": "playlist",
            "id": "PL1",
            "title": "My list",
            "webpage_url": "https://www.youtube.com/playlist?list=PL1",
            "entries": [
                {
                    "id": "a",
                    "title": "First",
                    "duration": 60.0,
                    "webpage_url": "https://www.youtube.com/watch?v=a",
                    "formats": [
                        { "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 1080, "filesize": 1000 },
                        { "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 720, "filesize": 500 },
                        { "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 100 }
                    ]
                },
                {
                    "id": "b",
                    "title": "Second",
                    "duration": 30.0,
                    "formats": [
                        { "ext": "mp4", "vcodec": "avc1", "acodec": "none", "height": 720, "filesize": 400 },
                        { "ext": "m4a", "vcodec": "none", "acodec": "mp4a", "filesize": 50 }
                    ]
                },
                { "_type": "url", "id": "c", "title": "Unavailable" }
            ]
        });

        let info = build_playlist_info(&json, "https://www.youtube.com/playlist?list=PL1").unwrap();
        assert_eq!(info.title, "My list");
        assert_eq!(info.entries.len(), 2);
        assert_eq!(info.entries[1].url, "https://www.youtube.com/watch?v=b");
        // Tallest first: 1080 from the first video, then 720 shared by both.
        assert_eq!(info.heights, vec![1080, 720]);
        assert_eq!(info.total_sizes[&1080], 1100 + 450);
        assert_eq!(info.total_sizes[&720], 600 + 450);
        assert_eq!(info.total_duration_seconds, 90);
    }

    #[test]
    fn a_single_video_is_rejected_as_a_playlist() {
        let json = serde_json::json!({ "id": "a", "title": "One" });
        assert!(build_playlist_info(&json, "https://example.com/v").is_err());
    }

    /// Validates against a real yt-dlp dump when one is supplied:
    /// `ZDM_SAMPLE_PLAYLIST=/tmp/pl.json cargo test sample_playlist -- --nocapture`.
    #[test]
    fn sample_playlist_from_yt_dlp() {
        let Ok(path) = std::env::var("ZDM_SAMPLE_PLAYLIST") else { return };
        let bytes = std::fs::read(path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let info = build_playlist_info(&json, "sample").unwrap();
        assert!(!info.entries.is_empty());
        assert!(!info.heights.is_empty());
        assert!(info.entries.iter().all(|entry| entry.url.starts_with("http")));
        println!(
            "playlist {:?}: {} videos, heights {:?}",
            info.title,
            info.entries.len(),
            info.heights
        );
    }

    #[test]
    fn title_extraction_is_case_insensitive() {
        assert_eq!(
            extract_title("<html><TITLE>Hello</TITLE></html>"),
            Some("Hello".to_string())
        );
        assert_eq!(extract_title("<html></html>"), None);
    }

    /// The exact bug reported: without ffmpeg yt-dlp leaves `<name>.f137.mp4`
    /// and `<name>.f140.m4a`. Those must never be mistaken for a finished file.
    #[test]
    fn per_format_fragments_are_detected() {
        let dir = std::env::temp_dir().join(format!("zdm-frag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        for name in [
            "Title.f137.mp4",
            "Title.f140.m4a",
            "Title.f251.webm",
            "Title.part",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        std::fs::write(dir.join("Unrelated.mp4"), b"x").unwrap();

        let fragments = find_fragments(&dir, "Title");
        assert_eq!(fragments.len(), 4, "got {fragments:?}");

        // A plain file with the same stem is not a fragment.
        assert!(fragments
            .iter()
            .all(|p| !p.to_string_lossy().ends_with("Unrelated.mp4")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// finalise_output must move a container ffmpeg chose (e.g. .mkv) onto the
    /// requested path, so "open file" never points at a missing path.
    #[test]
    fn finalise_moves_a_different_container_onto_the_target() {
        let dir = std::env::temp_dir().join(format!("zdm-final-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let target = dir.join("Clip.mp4");
        // ffmpeg produced mkv instead of the requested mp4.
        std::fs::write(dir.join("Clip.mkv"), b"video").unwrap();

        let job = VideoJob {
            filename: "Clip.mp4".into(),
            save_path: target.to_string_lossy().to_string(),
            total_size: 0,
        };

        finalise_output(&job, None).expect("should resolve the mkv");
        assert!(target.is_file(), "the target must exist");
        assert_eq!(std::fs::read(&target).unwrap(), b"video");
        assert!(!dir.join("Clip.mkv").exists(), "the source must be moved");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only fragments present = the merge never happened: fail loudly and clean up.
    #[test]
    fn finalise_reports_a_clear_error_when_only_fragments_exist() {
        let dir = std::env::temp_dir().join(format!("zdm-onlyfrag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let target = dir.join("Movie.mp4");
        std::fs::write(dir.join("Movie.f137.mp4"), b"v").unwrap();
        std::fs::write(dir.join("Movie.f140.m4a"), b"a").unwrap();

        let job = VideoJob {
            filename: "Movie.mp4".into(),
            save_path: target.to_string_lossy().to_string(),
            total_size: 0,
        };

        let err = finalise_output(&job, None).expect_err("must fail");
        assert!(err.to_string().contains("could not be merged"), "got: {err}");
        assert!(!dir.join("Movie.f137.mp4").exists(), "debris must be removed");
        assert!(!dir.join("Movie.f140.m4a").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_stem_never_empty() {
        assert_eq!(safe_stem(""), "media");
        assert_eq!(safe_stem("a/b:c"), "a_b_c");
    }
}
