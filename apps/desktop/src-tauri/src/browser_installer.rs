//! ZDM Automatic Browser Extension & Host Installer
//! Developed by Ramy Badraan
//!
//! Automatically installs Native Messaging Hosts and installs extensions
//! for Chrome, Chromium, Brave, Edge, Vivaldi, Firefox, and Zen Browser.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CHROME_EXT_ID: &str = "cjdipciekhdnndlcpcgjocjbdijdlhnn";

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
    let home_path = PathBuf::from(&home);

    #[cfg(windows)]
    let local_appdata = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| home_path.join("AppData\\Local"));
    #[cfg(windows)]
    let appdata = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| home_path.join("AppData\\Roaming"));

    let candidates = vec![
        #[cfg(not(windows))]
        ("zen", "Zen Browser", home_path.join(".config/zen")),
        #[cfg(not(windows))]
        ("firefox", "Mozilla Firefox", home_path.join(".mozilla")),
        #[cfg(not(windows))]
        ("google-chrome", "Google Chrome", home_path.join(".config/google-chrome")),
        #[cfg(not(windows))]
        ("brave", "Brave Browser", home_path.join(".config/BraveSoftware/Brave-Browser")),
        #[cfg(not(windows))]
        ("edge", "Microsoft Edge", home_path.join(".config/microsoft-edge")),
        #[cfg(not(windows))]
        ("chromium", "Chromium", home_path.join(".config/chromium")),
        #[cfg(not(windows))]
        ("vivaldi", "Vivaldi", home_path.join(".config/vivaldi")),

        #[cfg(windows)]
        ("edge", "Microsoft Edge", local_appdata.join("Microsoft\\Edge\\User Data")),
        #[cfg(windows)]
        ("google-chrome", "Google Chrome", local_appdata.join("Google\\Chrome\\User Data")),
        #[cfg(windows)]
        ("brave", "Brave Browser", local_appdata.join("BraveSoftware\\Brave-Browser\\User Data")),
        #[cfg(windows)]
        ("firefox", "Mozilla Firefox", appdata.join("Mozilla\\Firefox")),
        #[cfg(windows)]
        ("vivaldi", "Vivaldi", local_appdata.join("Vivaldi\\User Data")),
        #[cfg(windows)]
        ("chromium", "Chromium", local_appdata.join("Chromium\\User Data")),
        #[cfg(windows)]
        ("zen", "Zen Browser", appdata.join("zen")),
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
    #[cfg(windows)]
    {
        let exe_name = match cmd {
            "google-chrome" => "chrome.exe",
            "edge" => "msedge.exe",
            "brave" => "brave.exe",
            "firefox" => "firefox.exe",
            "vivaldi" => "vivaldi.exe",
            "chromium" => "chromium.exe",
            "zen" => "zen.exe",
            _ => cmd,
        };
        Command::new("where.exe")
            .arg(exe_name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let check_binary = match cmd {
            "google-chrome" => "google-chrome-stable",
            "brave" => "brave",
            "edge" => "microsoft-edge",
            "chromium" => "chromium",
            "vivaldi" => "vivaldi",
            "firefox" => "firefox",
            "zen" => "zen-browser",
            _ => cmd,
        };
        Command::new("which").arg(check_binary).output().map(|o| o.status.success()).unwrap_or(false)
    }
}

pub fn install_browser_integrations() {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => PathBuf::from(h),
        Err(_) => return,
    };

    let host_path = find_host_binary(&home);
    let extension_path = find_extension_dir(&home);
    let xpi_path = find_xpi_file(&home);

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
            format!("chrome-extension://{}/", CHROME_EXT_ID),
            "chrome-extension://*/"
        ]
    });

    for (name, dir) in &chromium_browser_dirs {
        if dir.exists() || dir.parent().map(|p| p.exists()).unwrap_or(false) {
            // Write Native Messaging Host
            let host_dir = dir.join("NativeMessagingHosts");
            let _ = fs::create_dir_all(&host_dir);
            let manifest_file = host_dir.join("com.zyzoom.zdm.json");
            let _ = fs::write(&manifest_file, serde_json::to_string_pretty(&chrome_manifest_json).unwrap_or_default());

            // Write External Extension registration with exact 32-character ID
            if let Some(ext_path) = &extension_path {
                let ext_dir = dir.join("External Extensions");
                let _ = fs::create_dir_all(&ext_dir);
                let ext_manifest = serde_json::json!({
                    "external_path": ext_path.to_string_lossy()
                });
                let ext_file = ext_dir.join(format!("{}.json", CHROME_EXT_ID));
                let _ = fs::write(ext_file, serde_json::to_string_pretty(&ext_manifest).unwrap_or_default());
            }

            println!("[ZDM] Registered browser integration for {}", name);
        }
    }

    // 2. Install for Firefox and Zen Browser (Direct Profile Extensions Injection)
    let gecko_parents = vec![
        ("zen", home.join(".config/zen")),
        ("firefox", home.join(".mozilla/firefox")),
    ];

    let gecko_manifest_json = serde_json::json!({
        "name": "com.zyzoom.zdm",
        "description": "Zyzoom Download Manager (ZDM) Native Messaging Host - Developed by Ramy Badraan",
        "path": host_path.to_string_lossy(),
        "type": "stdio",
        "allowed_extensions": [
            "zdm@zyzoom.com"
        ]
    });

    // Native messaging host for Firefox and Zen
    let ff_native_dir = home.join(".mozilla/native-messaging-hosts");
    let _ = fs::create_dir_all(&ff_native_dir);
    let _ = fs::write(ff_native_dir.join("com.zyzoom.zdm.json"), serde_json::to_string_pretty(&gecko_manifest_json).unwrap_or_default());

    let zen_native_dir = home.join(".config/zen/native-messaging-hosts");
    let _ = fs::create_dir_all(&zen_native_dir);
    let _ = fs::write(zen_native_dir.join("com.zyzoom.zdm.json"), serde_json::to_string_pretty(&gecko_manifest_json).unwrap_or_default());

    // Inject XPI directly into every profile directory
    if let Some(xpi_src) = &xpi_path {
        for (g_name, g_dir) in &gecko_parents {
            if g_dir.exists() {
                if let Ok(entries) = fs::read_dir(g_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            // Check if this is a profile directory
                            let has_prefs = path.join("prefs.js").exists() || path.join("times.json").exists() || path.join("extensions").exists();
                            if has_prefs {
                                let target_ext_dir = path.join("extensions");
                                let _ = fs::create_dir_all(&target_ext_dir);
                                let target_xpi = target_ext_dir.join("zdm@zyzoom.com.xpi");
                                let _ = fs::copy(xpi_src, &target_xpi);
                                println!("[ZDM] Auto-installed extension for {} profile at {:?}", g_name, target_xpi);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn get_browser_executable(browser_id: &str) -> String {
    #[cfg(windows)]
    {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
        let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Local".into());

        let candidates: Vec<PathBuf> = match browser_id {
            "edge" => vec![
                PathBuf::from(&pf86).join("Microsoft\\Edge\\Application\\msedge.exe"),
                PathBuf::from(&pf).join("Microsoft\\Edge\\Application\\msedge.exe"),
            ],
            "google-chrome" => vec![
                PathBuf::from(&pf).join("Google\\Chrome\\Application\\chrome.exe"),
                PathBuf::from(&pf86).join("Google\\Chrome\\Application\\chrome.exe"),
                PathBuf::from(&local_appdata).join("Google\\Chrome\\Application\\chrome.exe"),
            ],
            "brave" => vec![
                PathBuf::from(&pf).join("BraveSoftware\\Brave-Browser\\Application\\brave.exe"),
                PathBuf::from(&pf86).join("BraveSoftware\\Brave-Browser\\Application\\brave.exe"),
                PathBuf::from(&local_appdata).join("BraveSoftware\\Brave-Browser\\Application\\brave.exe"),
            ],
            "firefox" => vec![
                PathBuf::from(&pf).join("Mozilla Firefox\\firefox.exe"),
                PathBuf::from(&pf86).join("Mozilla Firefox\\firefox.exe"),
            ],
            "vivaldi" => vec![
                PathBuf::from(&local_appdata).join("Vivaldi\\Application\\vivaldi.exe"),
                PathBuf::from(&pf).join("Vivaldi\\Application\\vivaldi.exe"),
            ],
            _ => vec![],
        };

        for p in candidates {
            if p.exists() {
                return p.to_string_lossy().to_string();
            }
        }

        match browser_id {
            "edge" => "msedge".to_string(),
            "google-chrome" => "chrome".to_string(),
            "brave" => "brave".to_string(),
            "firefox" => "firefox".to_string(),
            "vivaldi" => "vivaldi".to_string(),
            _ => "chrome".to_string(),
        }
    }
    #[cfg(not(windows))]
    {
        match browser_id {
            "google-chrome" => if check_cmd_exists("google-chrome-stable") { "google-chrome-stable".into() } else { "google-chrome".into() },
            "brave" => if check_cmd_exists("brave-browser") { "brave-browser".into() } else { "brave".into() },
            "edge" => if check_cmd_exists("microsoft-edge-stable") { "microsoft-edge-stable".into() } else { "microsoft-edge".into() },
            "chromium" => "chromium".into(),
            "vivaldi" => "vivaldi".into(),
            "zen" => "zen-browser".into(),
            "firefox" => "firefox".into(),
            _ => "google-chrome".into(),
        }
    }
}

pub fn launch_browser(browser_id: &str) -> Result<(), String> {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => PathBuf::from(h),
        Err(_) => return Err("Could not find user home directory".into()),
    };

    let ext_dir = find_extension_dir(&home)
        .ok_or_else(|| "Extension directory not found on system".to_string())?;

    let binary = get_browser_executable(browser_id);

    if browser_id == "firefox" || browser_id == "zen" {
        if let Some(xpi) = find_xpi_file(&home) {
            Command::new(&binary)
                .arg(format!("file://{}", xpi.to_string_lossy()))
                .spawn()
                .map_err(|e| format!("Failed to launch {}: {}", binary, e))?;
        } else {
            Command::new(&binary)
                .arg("about:addons")
                .spawn()
                .map_err(|e| format!("Failed to launch {}: {}", binary, e))?;
        }
    } else {
        let ext_arg = format!("--load-extension={}", ext_dir.to_string_lossy());
        Command::new(&binary)
            .arg(&ext_arg)
            .spawn()
            .map_err(|e| format!("Failed to launch {}: {}", binary, e))?;
    }

    Ok(())
}

fn find_host_binary(home: &Path) -> PathBuf {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("zdm-host.exe"));
            candidates.push(dir.join("zdm-host"));
        }
    }

    #[cfg(windows)]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Zyzoom Download Manager\\zdm-host.exe"));
        }
    }

    candidates.push(PathBuf::from("/usr/local/bin/zdm-host"));
    candidates.push(PathBuf::from("/usr/bin/zdm-host"));
    candidates.push(home.join(".local/bin/zdm-host"));
    candidates.push(PathBuf::from("target/release/zdm-host"));
    candidates.push(PathBuf::from("target/debug/zdm-host"));

    for c in candidates {
        if c.exists() {
            return c;
        }
    }

    #[cfg(windows)]
    return PathBuf::from("zdm-host.exe");
    #[cfg(not(windows))]
    PathBuf::from("/usr/local/bin/zdm-host")
}

pub fn find_extension_dir(home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    // 1. First priority: look directly in the executable directory (e.g. C:\Program Files\Zyzoom Download Manager\extension)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("extension"));
            candidates.push(dir.join("../extension"));
            candidates.push(dir.join("../../extension"));
            candidates.push(dir.join("apps/extension"));
        }
    }

    #[cfg(windows)]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Zyzoom Download Manager\\extension"));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(pf86).join("Zyzoom Download Manager\\extension"));
        }
    }

    candidates.push(PathBuf::from("/usr/local/share/zdm/extension"));
    candidates.push(PathBuf::from("/usr/share/zdm/extension"));
    candidates.push(home.join(".local/share/zdm/extension"));
    candidates.push(PathBuf::from("apps/extension"));

    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }

    None
}

pub fn find_xpi_file(home: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("zdm@zyzoom.com.xpi"));
            candidates.push(dir.join("extension/zdm@zyzoom.com.xpi"));
            candidates.push(dir.join("assets/zdm@zyzoom.com.xpi"));
        }
    }

    #[cfg(windows)]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Zyzoom Download Manager\\zdm@zyzoom.com.xpi"));
        }
    }

    candidates.push(PathBuf::from("/usr/local/share/zdm/zdm@zyzoom.com.xpi"));
    candidates.push(PathBuf::from("/usr/share/zdm/zdm@zyzoom.com.xpi"));
    candidates.push(home.join(".local/share/zdm/zdm@zyzoom.com.xpi"));
    candidates.push(PathBuf::from("assets/zdm@zyzoom.com.xpi"));

    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }

    None
}
