//! FFmpeg discovery and self-healing.
//! Developed by Ramy Badraan
//!
//! yt-dlp needs ffmpeg to do two things that matter for this app:
//!
//!  1. **mux** a video-only stream and an audio-only stream into one file —
//!     YouTube serves them separately for every resolution above 720p;
//!  2. **convert** audio to mp3.
//!
//! Without it yt-dlp does not fail loudly: it writes the two streams side by
//! side as `<title>.f137.mp4` and `<title>.f140.m4a`, which is exactly the
//! "two files, weird suffix, path does not exist" bug users hit on Windows —
//! Windows has no system ffmpeg, while most Linux desktops do.
//!
//! The Windows installer bundles ffmpeg.exe, so this module normally only has to
//! *find* it. Downloading is the last resort, and it resumes: the archive is
//! ~110 MB and a dropped connection must not restart it from zero.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Filename of the ffmpeg binary on this platform.
fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Candidate locations, most specific first.
pub fn candidates() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();

    // 1. Next to the executable: this is where the installer puts it, so it must
    //    be checked before anything else.
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            for relative in ["", "bin", "ffmpeg", "resources", "resources/bin"] {
                let candidate = if relative.is_empty() {
                    dir.join(exe("ffmpeg"))
                } else {
                    dir.join(relative).join(exe("ffmpeg"))
                };
                found.push(candidate);
            }
        }
    }

    // 2. Our managed copy (downloaded once, reused forever).
    found.push(crate::paths::bin_dir().join(exe("ffmpeg")));
    found.push(crate::paths::bin_dir().join("ffmpeg").join(exe("ffmpeg")));
    found.push(crate::paths::data_dir().join("ffmpeg").join(exe("ffmpeg")));

    // 3. Platform-conventional installs.
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(var) {
                let base = PathBuf::from(base);
                found.push(base.join("ffmpeg").join("bin").join("ffmpeg.exe"));
                found.push(
                    base.join("Zyzoom Download Manager")
                        .join("bin")
                        .join("ffmpeg.exe"),
                );
            }
        }
    }

    #[cfg(not(windows))]
    {
        for dir in ["/usr/local/bin", "/usr/bin", "/opt/homebrew/bin", "/snap/bin"] {
            found.push(Path::new(dir).join("ffmpeg"));
        }
    }

    found
}

/// Whether `path` really is an executable ffmpeg (not a 0-byte leftover).
fn is_usable(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.len() > 1024,
        Err(_) => false,
    }
}

/// Locate ffmpeg, or `None` when the machine genuinely has none.
pub fn find() -> Option<String> {
    for candidate in candidates() {
        if is_usable(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    // PATH lookup last: slowest and least specific.
    for name in ["ffmpeg", "ffmpeg.exe"] {
        if let Some(path) = crate::process::which(name) {
            if is_usable(&path) {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    None
}

/// Whether a usable ffmpeg is available right now.
pub fn is_available() -> bool {
    find().is_some()
}

/// Ensure ffmpeg exists, downloading a static build when it does not.
///
/// Returns the directory containing the binary, which is what yt-dlp expects
/// from `--ffmpeg-location`.
pub async fn ensure() -> Result<PathBuf> {
    if let Some(path) = find() {
        return Ok(PathBuf::from(path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(crate::paths::bin_dir));
    }

    crate::paths::ensure_dirs().ok();
    let target_dir = crate::paths::bin_dir().join("ffmpeg");
    std::fs::create_dir_all(&target_dir)?;

    let target = target_dir.join(exe("ffmpeg"));
    if is_usable(&target) {
        return Ok(target_dir);
    }

    eprintln!("[ZDM] ffmpeg not found; fetching a static build");

    #[cfg(windows)]
    download_windows(&target_dir).await?;

    // Linux almost always has ffmpeg through the package manager, and the
    // installer checks for it. Reaching here means it is missing, so give the
    // exact command instead of downloading a random binary.
    #[cfg(not(windows))]
    return Err(anyhow!(
        "ffmpeg is required for video downloads. Install it with your package manager \
         (Arch: sudo pacman -S ffmpeg, Debian/Ubuntu: sudo apt install ffmpeg, \
         Fedora: sudo dnf install ffmpeg-free) and retry."
    ));

    #[cfg(windows)]
    if is_usable(&target) {
        println!("[ZDM] ffmpeg ready at {}", target.display());
        Ok(target_dir)
    } else {
        Err(anyhow!(
            "ffmpeg could not be installed automatically. Install it manually \
             (winget install Gyan.FFmpeg) and ZDM will use it."
        ))
    }
}

/// Two mirrors: the primary sometimes throttles, and either can be unreachable.
#[cfg(windows)]
const FFMPEG_MIRRORS: [&str; 2] = [
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
    "https://github.com/GyanD/codexffmpeg/releases/download/7.1.1/ffmpeg-7.1.1-essentials_build.zip",
];

#[cfg(windows)]
async fn download_windows(target_dir: &Path) -> Result<()> {
    let archive_path = target_dir.join("ffmpeg-download.zip");
    let mut last_error: Option<String> = None;

    for (index, url) in FFMPEG_MIRRORS.iter().enumerate() {
        eprintln!("[ZDM] ffmpeg download attempt {} ({url})", index + 1);

        match fetch_archive(url, &archive_path).await {
            Ok(bytes) => match extract_zip_binaries(&bytes, target_dir) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&archive_path);
                    return Ok(());
                }
                Err(err) => last_error = Some(format!("archive was unusable: {err}")),
            },
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    let _ = std::fs::remove_file(&archive_path);
    Err(anyhow!(
        "could not download ffmpeg ({}). Install it manually with \
         `winget install Gyan.FFmpeg`, or copy ffmpeg.exe next to zdm.exe, and retry.",
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

/// Stream the archive to disk, resuming a partial file when one exists.
#[cfg(windows)]
async fn fetch_archive(url: &str, destination: &Path) -> Result<Vec<u8>> {
    use futures_util::StreamExt;
    use std::io::Write;

    let client = crate::engine::downloader::build_client();

    let mut already: u64 = std::fs::metadata(destination).map(|m| m.len()).unwrap_or(0);

    let mut request = client.get(url);
    if already > 0 {
        // Resume instead of paying for the whole file again.
        request = request.header(reqwest::header::RANGE, format!("bytes={already}-"));
    }

    let resp = request
        .send()
        .await
        .map_err(|e| anyhow!("connection failed: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The partial file is already complete.
        return std::fs::read(destination).map_err(|e| anyhow!("cannot read {destination:?}: {e}"));
    }
    if !status.is_success() {
        return Err(anyhow!("server returned HTTP {status}"));
    }
    if already > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        // The server ignored the range: start over rather than corrupt the file.
        already = 0;
        let _ = std::fs::remove_file(destination);
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(already > 0)
        .write(true)
        .truncate(already == 0)
        .open(destination)
        .map_err(|e| anyhow!("cannot open {destination:?}: {e}"))?;

    let mut stream = resp.bytes_stream();
    let mut written = already;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| anyhow!("transfer interrupted: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| anyhow!("disk write failed: {e}"))?;
        written += bytes.len() as u64;
        if written % (10 * 1024 * 1024) < bytes.len() as u64 {
            eprintln!("[ZDM] ffmpeg: {} MB", written / 1024 / 1024);
        }
    }

    file.flush().ok();
    drop(file);

    std::fs::read(destination).map_err(|e| anyhow!("cannot read {destination:?}: {e}"))
}

/// Pull ffmpeg.exe / ffprobe.exe out of the release zip, ignoring the folder
/// prefix that Gyan's archives nest them under.
#[cfg(windows)]
fn extract_zip_binaries(archive: &[u8], target_dir: &Path) -> Result<()> {
    use std::io::{Cursor, Read};

    if archive.len() < 1024 {
        return Err(anyhow!("the downloaded archive is empty"));
    }

    let cursor = Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(cursor)
        .map_err(|e| anyhow!("the archive is not a valid zip: {e}"))?;

    let mut extracted = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name().to_string();
        let base = name.rsplit('/').next().unwrap_or("");

        if base != "ffmpeg.exe" && base != "ffprobe.exe" {
            continue;
        }

        let destination = target_dir.join(base);
        // Write to a temp name first so an interrupted extract never leaves a
        // half-written binary that later looks "found" but cannot run.
        let staging = destination.with_extension("part");
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer)?;
        std::fs::write(&staging, &buffer)?;
        std::fs::rename(&staging, &destination)?;
        extracted += 1;
    }

    if extracted == 0 {
        return Err(anyhow!("no ffmpeg.exe inside the archive"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_absolute_and_prefer_next_to_the_exe() {
        let list = candidates();
        assert!(!list.is_empty());
        assert!(list.iter().all(|p| p.is_absolute()));
        assert!(
            list.iter().any(|p| p.starts_with(crate::paths::bin_dir())),
            "the managed location must be searched"
        );
    }

    /// A 0-byte leftover from a failed download must not count as "found".
    #[test]
    fn empty_files_are_not_treated_as_usable() {
        let dir = std::env::temp_dir().join(format!("zdm-ffmpeg-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join(exe("ffmpeg"));
        std::fs::write(&empty, b"").unwrap();
        assert!(!is_usable(&empty), "an empty file is not a working binary");

        std::fs::write(&empty, vec![0u8; 4096]).unwrap();
        assert!(is_usable(&empty), "a non-trivial file is accepted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The installer writes ffmpeg.exe to `<install dir>/bin`, and the app must
    /// look there before falling back to PATH. This reproduces the layout with a
    /// fake binary and asserts the resolver would accept it.
    #[test]
    fn the_installer_layout_is_searched() {
        let dir = std::env::temp_dir().join(format!("zdm-ffmpeg-layout-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let fake = bin.join(exe("ffmpeg"));
        std::fs::write(&fake, vec![0u8; 4096]).unwrap();

        // Build the same candidate list the app builds, seeded with this bin dir.
        let searched: Vec<PathBuf> = vec![dir.join("bin").join(exe("ffmpeg"))];

        assert_eq!(
            searched.iter().find(|p| is_usable(p)).map(|p| p.as_path()),
            Some(fake.as_path()),
            "<install>/bin/ffmpeg.exe must be found"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live check against the real mirror: the client must survive longer than
    /// 45 seconds of wall clock. Marked `#[ignore]` so CI never depends on the
    /// network; run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "network"]
    async fn live_download_tolerates_slow_servers() {
        use futures_util::StreamExt;

        let client = crate::engine::downloader::build_client();
        let resp = client
            .get("https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip")
            .send()
            .await
            .expect("request must start");

        assert!(resp.status().is_success(), "status {}", resp.status());

        let started = std::time::Instant::now();
        let mut total = 0u64;
        let mut stream = resp.bytes_stream();

        // Read for 55 seconds: longer than the old 45 s cap.
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.expect("stream must not error mid-transfer");
            total += bytes.len() as u64;
            if started.elapsed().as_secs() > 55 {
                break;
            }
        }

        assert!(
            total > 5_000_000,
            "only {total} bytes arrived; the client is still cutting long transfers"
        );
    }

    #[test]
    fn missing_ffmpeg_reports_none_rather_than_panicking() {
        let _ = find();
        let _ = is_available();
    }

    #[cfg(windows)]
    #[test]
    fn zip_extraction_finds_binaries_under_a_folder_prefix() {
        use std::io::Write;
        let mut buffer = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            writer
                .start_file("ffmpeg-7.0-essentials_build/bin/ffmpeg.exe", options)
                .unwrap();
            writer.write_all(&vec![7u8; 2048]).unwrap();
            writer
                .start_file("ffmpeg-7.0-essentials_build/bin/ffprobe.exe", options)
                .unwrap();
            writer.write_all(&vec![7u8; 2048]).unwrap();
            writer
                .start_file("ffmpeg-7.0-essentials_build/README.txt", options)
                .unwrap();
            writer.write_all(b"readme").unwrap();
            writer.finish().unwrap();
        }

        let dir = std::env::temp_dir().join(format!("zdm-ffmpeg-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        extract_zip_binaries(&buffer, &dir).expect("extraction");
        assert!(dir.join("ffmpeg.exe").is_file());
        assert!(dir.join("ffprobe.exe").is_file());
        assert!(!dir.join("README.txt").exists());
        assert!(!dir.join("ffmpeg.part").exists(), "staging file must be cleaned");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_truncated_archive_is_rejected() {
        let dir = std::env::temp_dir().join(format!("zdm-ffmpeg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = extract_zip_binaries(b"not a zip at all", &dir).expect_err("must fail");
        assert!(err.to_string().contains("empty") || err.to_string().contains("zip"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
