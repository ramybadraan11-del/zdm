//! ZDM Automatic Browser Extension & Host Installer
//! Developed by Ramy Badraan
//!
//! Automatically installs Native Messaging Hosts and registers External Extensions
//! for Chrome, Chromium, Brave, Edge, Vivaldi, and Firefox on system launch,
//! and provides instant one-click browser launch with extension loaded.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct BrowserStatus {
    pub id: String,
    pub name: String,
    pub is_installed: bool,
    pub config_dir: String,
}

pub fn get_installed_browsers_list() -> Vec<BrowserStatus> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let home_path = PathBuf::from(home);

    let candidates = vec![
        ("google-chrome", "Google Chrome", home_path.join(".config/google-chrome")),
        ("brave", "Brave Browser", home_path.join(".config/BraveSoftware/Brave-Browser")),
        ("edge", "Microsoft Edge", home_path.join(".config/microsoft-edge")),
        ("chromium", "Chromium", home_path.join(".config/chromium")),
        ("vivaldi", "Vivaldi", home_path.join(".config/vivaldi")),
        ("firefox", "Mozilla Firefox", home_path.join(".mozilla")),
    ];

    candidates
        .into_iter()
        .map(|(id, name, path)| {
            let is_installed = path.exists() || check_cmd_exists(id);
            BrowserStatus {
                id: id.to_string(),
                name: name.to_string(),
                is_installed,
                config_dir: path.to_string_lossy().to_string(),
            }
        })
        .collect()
}

fn check_cmd_exists(cmd: &str) -> bool {
    let check_binary = match cmd {
        "google-chrome" => "google-chrome-stable",
        "brave" => "brave",
        "edge" => "microsoft-edge",
        "chromium" => "chromium",
        "vivaldi" => "vivaldi",
        "firefox" => "firefox",
        _ => cmd,
    };
    Command::new("which").arg(check_binary).output().map(|o| o.status.success()).unwrap_or(false)
}

pub fn install_browser_integrations() {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => PathBuf::from(h),
        Err(_) => return,
    };

    let host_path = find_host_binary(&home);
    let extension_path = find_extension_dir(&home);

    // 1. Install for Chromium-based browsers
    let chromium_browser_dirs = vec![
        ("google-chrome", home.join(".config/google-chrome")),
        ("chromium", home.join(".config/chromium")),
        ("brave", home.join(".config/BraveSoftware/Brave-Browser")),
        ("edge", home.join(".config/microsoft-edge")),
        ("vivaldi", home.join(".config/vivaldi")),
    ];

    let chrome_manifest_json = serde_json::json!({
        "name": "com.zyzoom.zdm",
        "description": "Zyzoom Download Manager (ZDM) Native Messaging Host - Developed by Ramy Badraan",
        "path": host_path.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            "chrome-extension://*/",
            "chrome-extension://com.zyzoom.zdm/"
        ]
    });

    for (name, dir) in &chromium_browser_dirs {
        if dir.exists() || dir.parent().map(|p| p.exists()).unwrap_or(false) {
            // Write Native Messaging Host
            let host_dir = dir.join("NativeMessagingHosts");
            let _ = fs::create_dir_all(&host_dir);
            let manifest_file = host_dir.join("com.zyzoom.zdm.json");
            let _ = fs::write(&manifest_file, serde_json::to_string_pretty(&chrome_manifest_json).unwrap_or_default());

            // Write External Extension registration (Auto-installs extension in browser)
            if let Some(ext_path) = &extension_path {
                let ext_dir = dir.join("External Extensions");
                let _ = fs::create_dir_all(&ext_dir);
                let ext_manifest = serde_json::json!({
                    "external_path": ext_path.to_string_lossy()
                });
                let _ = fs::write(ext_dir.join("zdm_extension.json"), serde_json::to_string_pretty(&ext_manifest).unwrap_or_default());
            }

            println!("[ZDM] Registered browser integration for {}", name);
        }
    }

    // 2. Install for Firefox
    let firefox_dir = home.join(".mozilla");
    if firefox_dir.exists() {
        let ff_host_dir = firefox_dir.join("native-messaging-hosts");
        let _ = fs::create_dir_all(&ff_host_dir);

        let firefox_manifest_json = serde_json::json!({
            "name": "com.zyzoom.zdm",
            "description": "Zyzoom Download Manager (ZDM) Native Messaging Host - Developed by Ramy Badraan",
            "path": host_path.to_string_lossy(),
            "type": "stdio",
            "allowed_extensions": [
                "zdm@zyzoom.com"
            ]
        });

        let ff_manifest_file = ff_host_dir.join("com.zyzoom.zdm.json");
        let _ = fs::write(&ff_manifest_file, serde_json::to_string_pretty(&firefox_manifest_json).unwrap_or_default());
        println!("[ZDM] Registered browser integration for Firefox");
    }
}

pub fn launch_browser(browser_id: &str) -> Result<(), String> {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => PathBuf::from(h),
        Err(_) => return Err("Could not find user home directory".into()),
    };

    let ext_dir = find_extension_dir(&home)
        .ok_or_else(|| "Extension directory not found on system".to_string())?;

    let binary = match browser_id {
        "google-chrome" => {
            if check_cmd_exists("google-chrome-stable") { "google-chrome-stable" } else { "google-chrome" }
        }
        "brave" => {
            if check_cmd_exists("brave-browser") { "brave-browser" } else { "brave" }
        }
        "edge" => {
            if check_cmd_exists("microsoft-edge-stable") { "microsoft-edge-stable" } else { "microsoft-edge" }
        }
        "chromium" => "chromium",
        "vivaldi" => "vivaldi",
        "firefox" => "firefox",
        _ => "google-chrome",
    };

    let ext_arg = format!("--load-extension={}", ext_dir.to_string_lossy());

    if browser_id == "firefox" {
        Command::new(binary)
            .arg("about:debugging#/runtime/this-firefox")
            .spawn()
            .map_err(|e| format!("Failed to launch {}: {}", binary, e))?;
    } else {
        Command::new(binary)
            .arg(&ext_arg)
            .arg("chrome://extensions")
            .spawn()
            .map_err(|e| format!("Failed to launch {}: {}", binary, e))?;
    }

    Ok(())
}

fn find_host_binary(home: &Path) -> PathBuf {
    let candidates = vec![
        PathBuf::from("/usr/local/bin/zdm-host"),
        PathBuf::from("/usr/bin/zdm-host"),
        home.join(".local/bin/zdm-host"),
        PathBuf::from("target/release/zdm-host"),
        PathBuf::from("target/debug/zdm-host"),
    ];

    for c in candidates {
        if c.exists() {
            return c;
        }
    }

    PathBuf::from("/usr/local/bin/zdm-host")
}

pub fn find_extension_dir(home: &Path) -> Option<PathBuf> {
    let candidates = vec![
        PathBuf::from("/usr/local/share/zdm/extension"),
        PathBuf::from("/usr/share/zdm/extension"),
        home.join(".local/share/zdm/extension"),
        PathBuf::from("apps/extension"),
    ];

    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }

    None
}
