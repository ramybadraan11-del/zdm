//! ZDM Automatic Browser Extension & Host Installer
//! Developed by Ramy Badraan
//!
//! Automatically installs Native Messaging Hosts and registers External Extensions
//! for Chrome, Chromium, Brave, Edge, Vivaldi, and Firefox on system launch.

use std::fs;
use std::path::{Path, PathBuf};

pub fn install_browser_integrations() {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => PathBuf::from(h),
        Err(_) => return,
    };

    // Determine host binary location
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
                // Extension ID file or generic loader
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

fn find_host_binary(home: &Path) -> PathBuf {
    // Check system-wide paths first, then local user paths
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

    // Default to /usr/local/bin/zdm-host
    PathBuf::from("/usr/local/bin/zdm-host")
}

fn find_extension_dir(home: &Path) -> Option<PathBuf> {
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
