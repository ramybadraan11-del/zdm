//! ZDM Path Resolution
//! Developed by Ramy Badraan
//!
//! Single source of truth for every filesystem location ZDM touches.
//! The old code hard-coded `~/.local/share` on every platform, which silently
//! created broken directories on Windows. Everything is resolved per-OS here.

use std::path::PathBuf;

/// Windows: `%APPDATA%\ZDM` (roaming, survives reboots/profile moves).
/// Linux/macOS: `$XDG_DATA_HOME/zdm` or `~/.local/share/zdm`.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("ZDM");
        }
        home_dir().join("AppData").join("Roaming").join("ZDM")
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("zdm");
            }
        }
        home_dir().join(".local").join("share").join("zdm")
    }
}

pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        data_dir()
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("zdm");
            }
        }
        home_dir().join(".config").join("zdm")
    }
}

/// Where the self-healing yt-dlp binary lives.
pub fn bin_dir() -> PathBuf {
    data_dir().join("bin")
}

pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("ZDM").join("cache");
        }
        data_dir().join("cache")
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join("zdm");
            }
        }
        home_dir().join(".cache").join("zdm")
    }
}

pub fn db_path() -> PathBuf {
    data_dir().join("zdm.db")
}

pub fn log_path() -> PathBuf {
    data_dir().join("zdm.log")
}

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// `~/Downloads/ZDM` — the friendly default users expect.
pub fn default_download_dir() -> PathBuf {
    home_dir().join("Downloads").join("ZDM")
}

/// Resolve an arbitrary user-supplied path, expanding a leading `~`.
pub fn expand(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    if trimmed == "~" {
        return home_dir();
    }
    PathBuf::from(trimmed)
}

/// Map an internal category key to its on-disk folder name.
pub fn category_folder(category: &str) -> &'static str {
    match category {
        "Video" => "Video",
        "Audio" => "Audio",
        "Compressed" => "Compressed",
        "Documents" => "Documents",
        "Images" => "Images",
        "Programs" => "Programs",
        _ => "Others",
    }
}

/// Resolve the directory a download of `category` must land in, honouring the
/// user's per-category override. `overrides` maps category -> folder.
pub fn category_dir(
    download_dir: &std::path::Path,
    category: &str,
    overrides: &std::collections::BTreeMap<String, String>,
) -> PathBuf {
    if let Some(custom) = overrides.get(category) {
        return expand(custom);
    }
    download_dir.join(category_folder(category))
}

/// Ensure the core directories exist. Called once at startup.
pub fn ensure_dirs() -> std::io::Result<()> {
    for dir in [data_dir(), config_dir(), bin_dir(), cache_dir()] {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde() {
        let home = home_dir();
        assert_eq!(expand("~/Downloads"), home.join("Downloads"));
        assert_eq!(expand("~"), home);
        assert_eq!(expand("/tmp/x"), PathBuf::from("/tmp/x"));
    }

    #[test]
    fn category_folders_are_stable() {
        assert_eq!(category_folder("Programs"), "Programs");
        assert_eq!(category_folder("Images"), "Images");
        assert_eq!(category_folder("nonsense"), "Others");
    }

    #[test]
    fn data_dir_is_absolute_and_named() {
        let dir = data_dir();
        assert!(dir.is_absolute(), "data dir must be absolute: {dir:?}");
        let name = dir.file_name().unwrap().to_string_lossy().to_lowercase();
        assert!(name == "zdm", "unexpected data dir name: {name}");
    }
}
