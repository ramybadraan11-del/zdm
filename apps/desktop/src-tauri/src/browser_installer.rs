//! ZDM Browser Integration Installer
//! Developed by Ramy Badraan
//!
//! Registers the native-messaging host and (where the browser allows it) the
//! unpacked extension for every Chromium- and Gecko-based browser we can find.
//!
//! Root causes fixed here, all reproduced:
//!
//! 1. The Chromium manifest used `"path": "zdm-host.exe"` (relative) and listed
//!    `"chrome-extension://*/"` in `allowed_origins`. Chromium rejects the whole
//!    manifest for either reason, so the host silently never loaded — that is
//!    why the toolbar button only worked on Linux (where the HTTP fallback was
//!    reachable) and never on Windows.
//! 2. Auto-install used `"external_path"`, which is not a Chromium field. The
//!    correct mechanism is a CRX3 file + `external_crx` + `external_version`.
//! 3. Firefox/Zen refuse unsigned add-ons from a profile `extensions/` folder,
//!    so we use `policies.json` (force_installed) which Mozilla does honour,
//!    and we surface the truth in the UI when neither path is available.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Stable extension id derived from the public key in `extension/manifest.json`.
pub const CHROME_EXT_ID: &str = "jgoccphfikadefpdemmaohkpbomgjcak";
pub const GECKO_EXT_ID: &str = "zdm@zyzoom.com";
pub const NATIVE_HOST: &str = "com.zyzoom.zdm";

#[derive(Debug, Clone, Serialize)]
pub struct BrowserStatus {
    pub id: String,
    pub name: String,
    pub family: String,
    pub is_installed: bool,
    pub config_dir: String,
    pub native_host_ready: bool,
    pub extension_pending: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct IntegrationReport {
    pub native_host_path: String,
    pub extension_dir: String,
    pub registered: Vec<String>,
    pub warnings: Vec<String>,
    pub firefox_policy_written: bool,
}

/// All browsers we know how to talk to.
struct Candidate {
    id: &'static str,
    name: &'static str,
    family: Family,
    /// Chromium user-data root / Gecko parent profile root.
    config: PathBuf,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Family {
    Chromium,
    Gecko,
}

fn chromium_candidates(home: &Path) -> Vec<Candidate> {
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join("AppData").join("Local"));
        let roaming = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join("AppData").join("Roaming"));
        vec![
            Candidate { id: "chrome", name: "Google Chrome", family: Family::Chromium, config: local.join("Google/Chrome/User Data") },
            Candidate { id: "edge", name: "Microsoft Edge", family: Family::Chromium, config: local.join("Microsoft/Edge/User Data") },
            Candidate { id: "brave", name: "Brave", family: Family::Chromium, config: local.join("BraveSoftware/Brave-Browser/User Data") },
            Candidate { id: "vivaldi", name: "Vivaldi", family: Family::Chromium, config: local.join("Vivaldi/User Data") },
            Candidate { id: "chromium", name: "Chromium", family: Family::Chromium, config: local.join("Chromium/User Data") },
            Candidate { id: "firefox", name: "Mozilla Firefox", family: Family::Gecko, config: roaming.join("Mozilla/Firefox") },
            Candidate { id: "zen", name: "Zen Browser", family: Family::Gecko, config: roaming.join("zen") },
            Candidate { id: "librewolf", name: "LibreWolf", family: Family::Gecko, config: roaming.join("librewolf") },
        ]
    }
    #[cfg(not(windows))]
    {
        let config = home.join(".config");
        vec![
            Candidate { id: "chrome", name: "Google Chrome", family: Family::Chromium, config: config.join("google-chrome") },
            Candidate { id: "chromium", name: "Chromium", family: Family::Chromium, config: config.join("chromium") },
            Candidate { id: "brave", name: "Brave", family: Family::Chromium, config: config.join("BraveSoftware/Brave-Browser") },
            Candidate { id: "edge", name: "Microsoft Edge", family: Family::Chromium, config: config.join("microsoft-edge") },
            Candidate { id: "vivaldi", name: "Vivaldi", family: Family::Chromium, config: config.join("vivaldi") },
            Candidate { id: "opera", name: "Opera", family: Family::Chromium, config: config.join("opera") },
            Candidate { id: "firefox", name: "Mozilla Firefox", family: Family::Gecko, config: home.join(".mozilla/firefox") },
            Candidate { id: "zen", name: "Zen Browser", family: Family::Gecko, config: config.join("zen") },
            Candidate { id: "librewolf", name: "LibreWolf", family: Family::Gecko, config: home.join(".librewolf") },
        ]
    }
}

/// Last computed integration report.
///
/// Diagnostics used to call `install_browser_integrations()` directly, which
/// rewrites every browser's manifests and policies (and on Windows spawns `reg`
/// a dozen times). Doing that on each Settings visit froze the window. The report
/// is now computed once per process — and again only when the user asks for a
/// repair.
static LAST_REPORT: parking_lot::Mutex<Option<IntegrationReport>> = parking_lot::Mutex::new(None);

/// Return the cached report, computing it once if needed.
///
/// Goes through the same freshness check as startup so a Settings visit never
/// silently shows an empty report just because the stamp was already valid.
pub fn cached_report() -> IntegrationReport {
    if let Some(report) = LAST_REPORT.lock().as_ref() {
        return report.clone();
    }
    ensure_integration_fresh()
}

/// Run the integration again (explicit user action) and refresh the cache.
pub fn repair_now() -> IntegrationReport {
    let report = install_browser_integrations();
    *LAST_REPORT.lock() = Some(report.clone());
    write_stamp();
    report
}

/// Re-apply browser integration at startup only when something changed.
///
/// Unconditionally re-running it wrote every native-messaging manifest and
/// rewrote every browser policy (a dozen `reg` invocations per browser on
/// Windows) on each launch. On a machine with several browsers that is slow
/// enough to look like a hang, and it happened on every boot.
///
/// The stamp lives in the data directory and records what the integration
/// depends on: the native-host path, the extension version, and which browsers
/// were present. If all three match, the previous run is still valid.
pub fn ensure_integration_fresh() -> IntegrationReport {
    let stamp_path = crate::paths::data_dir().join("browser-integration.json");
    let current = current_stamp();

    let previous = fs::read_to_string(&stamp_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());

    let unchanged = previous
        .as_ref()
        .and_then(|value| value.get("fingerprint"))
        .and_then(|value| value.as_str())
        .map(|fingerprint| fingerprint == current.fingerprint)
        .unwrap_or(false);

    if unchanged {
        // Nothing moved: reuse the last report instead of rewriting the world.
        if let Some(report) = LAST_REPORT.lock().clone() {
            return report;
        }
        return IntegrationReport::default();
    }

    let report = install_browser_integrations();
    *LAST_REPORT.lock() = Some(report.clone());
    write_stamp();
    report
}

/// What the integration result depends on, as a single comparable string.
struct Stamp {
    fingerprint: String,
}

fn current_stamp() -> Stamp {
    let host = find_host_binary()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let version = find_crx()
        .map(|crx| extension_version(&crx))
        .unwrap_or_default();

    let browsers: Vec<String> = chromium_candidates(&crate::paths::home_dir())
        .into_iter()
        .filter(|c| c.config.exists())
        .map(|c| c.id.to_string())
        .collect();

    Stamp {
        fingerprint: format!("{host}|{version}|{}", browsers.join(",")),
    }
}

fn write_stamp() {
    let path = crate::paths::data_dir().join("browser-integration.json");
    let payload = serde_json::json!({
        "fingerprint": current_stamp().fingerprint,
        "written_at": chrono::Utc::now().timestamp(),
    });
    let _ = fs::write(path, serde_json::to_string_pretty(&payload).unwrap_or_default());
}

pub fn get_installed_browsers_list() -> Vec<BrowserStatus> {
    let home = crate::paths::home_dir();
    let host_path = find_host_binary();

    chromium_candidates(&home)
        .into_iter()
        .map(|c| {
            let is_installed = c.config.exists();
            let native_manifest = native_manifest_path(&c);
            let native_host_ready = native_manifest
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false);

            let note = match c.family {
                Family::Chromium => {
                    if native_host_ready {
                        "Ready".to_string()
                    } else {
                        "Run \"Repair integrations\" to finish setup".to_string()
                    }
                }
                Family::Gecko => {
                    if native_host_ready {
                        "Native host installed".to_string()
                    } else {
                        "Needs repair".to_string()
                    }
                }
            };

            BrowserStatus {
                id: c.id.to_string(),
                name: c.name.to_string(),
                family: if c.family == Family::Chromium { "chromium".into() } else { "gecko".into() },
                is_installed,
                config_dir: c.config.to_string_lossy().to_string(),
                native_host_ready,
                extension_pending: is_installed && !native_host_ready,
                note: if host_path.is_none() && is_installed {
                    "ZDM host binary not found".to_string()
                } else {
                    note
                },
            }
        })
        .collect()
}

/// Chromium reads native-messaging manifests from `<user-data>/NativeMessagingHosts`
/// on Linux and from the registry on Windows. We write files on both and also
/// mirror the registry keys on Windows (the app runs unelevated, so it only
/// writes HKCU — the installer handles HKLM for all users).
fn native_manifest_path(c: &Candidate) -> Option<PathBuf> {
    match c.family {
        Family::Chromium => Some(c.config.join("NativeMessagingHosts").join(format!("{NATIVE_HOST}.json"))),
        Family::Gecko => Some(c.config.join("native-messaging-hosts").join(format!("{NATIVE_HOST}.json"))),
    }
}

pub fn find_host_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { "zdm-host.exe" } else { "zdm-host" };
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(exe_name));
            candidates.push(dir.join("host").join(exe_name));
        }
    }

    candidates.push(PathBuf::from("/usr/lib/zdm").join(exe_name));
    candidates.push(PathBuf::from("/usr/local/lib/zdm").join(exe_name));
    candidates.push(PathBuf::from("/usr/bin").join(exe_name));
    candidates.push(PathBuf::from("/usr/local/bin").join(exe_name));
    candidates.push(crate::paths::home_dir().join(".local/bin").join(exe_name));

    #[cfg(windows)]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(&pf).join("Zyzoom Download Manager").join(exe_name));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local).join("Programs").join("Zyzoom Download Manager").join(exe_name));
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

pub fn find_extension_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("extension"));
            candidates.push(dir.join("..").join("share").join("zdm").join("extension"));
            candidates.push(dir.join("..").join("Resources").join("extension"));
        }
    }

    candidates.push(PathBuf::from("/usr/share/zdm/extension"));
    candidates.push(PathBuf::from("/usr/local/share/zdm/extension"));
    candidates.push(PathBuf::from("/usr/lib/zdm/extension"));
    candidates.push(crate::paths::data_dir().join("extension"));
    candidates.push(PathBuf::from("apps/extension"));

    #[cfg(windows)]
    {
        // The exe-relative candidate above already covers the normal case. These
        // cover a custom install dir chosen in the wizard, plus the per-user
        // install location, so "Load unpacked" always has a real folder to offer.
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(key) {
                let root = PathBuf::from(base);
                candidates.push(root.join("Zyzoom Download Manager").join("extension"));
                candidates.push(root.join("ZDM").join("extension"));
            }
        }
    }

    candidates.into_iter().find(|p| p.join("manifest.json").exists())
}

fn find_xpi() -> Option<PathBuf> {
    let embedded = crate::paths::data_dir().join("extension").join(format!("{GECKO_EXT_ID}.xpi"));
    if embedded.is_file() {
        return Some(embedded);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(format!("{GECKO_EXT_ID}.xpi")));
            candidates.push(dir.join("extension").join(format!("{GECKO_EXT_ID}.xpi")));
        }
    }
    candidates.push(PathBuf::from(format!("/usr/share/zdm/{GECKO_EXT_ID}.xpi")));
    candidates.push(crate::paths::data_dir().join(format!("{GECKO_EXT_ID}.xpi")));
    candidates.push(PathBuf::from("assets").join(format!("{GECKO_EXT_ID}.xpi")));
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }

    // Rebuild from the embedded copy.
    crate::extension_payload::ensure_extracted().map(|crx| crx.with_file_name(format!("{GECKO_EXT_ID}.xpi")))
}

fn find_crx() -> Option<PathBuf> {
    // 1. Already extracted from the embedded payload (survives a missing install
    //    tree, a stripped /usr/share/zdm, or a moved directory).
    let embedded_dir = crate::paths::data_dir().join("extension");
    let embedded = embedded_dir.join(format!("{CHROME_EXT_ID}.crx"));
    if embedded.is_file() {
        return Some(embedded);
    }

    // 2. Anywhere on disk the installers put it.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(format!("{CHROME_EXT_ID}.crx")));
            candidates.push(dir.join("extension").join(format!("{CHROME_EXT_ID}.crx")));
        }
    }
    candidates.push(PathBuf::from(format!("/usr/share/zdm/{CHROME_EXT_ID}.crx")));
    candidates.push(crate::paths::data_dir().join(format!("{CHROME_EXT_ID}.crx")));
    candidates.push(PathBuf::from("assets").join(format!("{CHROME_EXT_ID}.crx")));
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }

    // 3. Nothing found: rebuild it from the copy compiled into this binary.
    crate::extension_payload::ensure_extracted()
}

/// Write every integration we can and report precisely what happened.
pub fn install_browser_integrations() -> IntegrationReport {
    let mut report = IntegrationReport::default();
    let home = crate::paths::home_dir();

    let Some(host_path) = find_host_binary() else {
        report
            .warnings
            .push("zdm-host binary was not found next to the app; browser capture will fall back to the local bridge only.".into());
        return report;
    };
    report.native_host_path = host_path.to_string_lossy().to_string();

    if let Some(dir) = find_extension_dir() {
        report.extension_dir = dir.to_string_lossy().to_string();
    }

    let has_xpi = find_xpi();
    if has_xpi.is_none() {
        report
            .warnings
            .push("Firefox add-on package (xpi) is missing from this build.".into());
    }

    for candidate in chromium_candidates(&home) {
        if !candidate.config.exists() {
            continue;
        }

        let manifest_dir = candidate.config.join("NativeMessagingHosts");
        if fs::create_dir_all(&manifest_dir).is_ok() {
            let manifest = match candidate.family {
                Family::Chromium => serde_json::json!({
                    "name": NATIVE_HOST,
                    "description": "Zyzoom Download Manager native host",
                    "path": host_path.to_string_lossy(),
                    "type": "stdio",
                    // Exactly our id: a wildcard entry makes Chromium reject the file.
                    "allowed_origins": [format!("chrome-extension://{CHROME_EXT_ID}/")]
                }),
                Family::Gecko => serde_json::json!({
                    "name": NATIVE_HOST,
                    "description": "Zyzoom Download Manager native host",
                    "path": host_path.to_string_lossy(),
                    "type": "stdio",
                    "allowed_extensions": [GECKO_EXT_ID]
                }),
            };

            let file = manifest_dir.join(format!("{NATIVE_HOST}.json"));
            if fs::write(&file, serde_json::to_string_pretty(&manifest).unwrap_or_default()).is_ok() {
                report.registered.push(candidate.name.to_string());
            }
        }

        match candidate.family {
            Family::Chromium => {
                register_chromium_external_extension(&candidate, &mut report);
            }
            Family::Gecko => {
                // Firefox does not honour profile-dropped XPIs that lack a
                // signature; enterprise policy is the supported route.
                let policy_dir = gecko_policy_dir(&candidate);
                if let Some(xpi) = &has_xpi {
                    if let Some(dir) = policy_dir {
                        if fs::create_dir_all(&dir).is_ok() {
                            let policy = serde_json::json!({
                                "policies": {
                                    "ExtensionSettings": {
                                        GECKO_EXT_ID: {
                                            "installation_mode": "force_installed",
                                            "install_url": format!("file://{}", xpi.to_string_lossy())
                                        }
                                    }
                                }
                            });
                            if fs::write(
                                dir.join("policies.json"),
                                serde_json::to_string_pretty(&policy).unwrap_or_default(),
                            )
                            .is_ok()
                            {
                                report.firefox_policy_written = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if report.firefox_policy_written {
        report.warnings.push(
            "Firefox policy written. On a normal (non-ESR) Firefox build, add-ons must be signed by Mozilla: if it still refuses, install the bundled XPI once via about:addons.".into(),
        );
    }

    report
}

/// Read the version straight out of the CRX's own manifest.
///
/// The CRX is a ZIP with a header: `Cr24`, version (4), header length (4), then
/// the protobuf header, then the ZIP. We locate the ZIP by scanning for its
/// local-file signature instead of trusting the protobuf length, and pull
/// `manifest.json` out with the `zip` crate that is already a dependency.
fn extension_version(crx: &Path) -> String {
    fn read_from_crx(path: &Path) -> Option<String> {
        let bytes = fs::read(path).ok()?;
        // Find the ZIP local file header (PK\x03\x04) that follows the CRX header.
        let start = bytes
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x03, 0x04])?;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes[start..])).ok()?;
        let mut manifest = archive.by_name("manifest.json").ok()?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut manifest, &mut text).ok()?;

        let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
        parsed.get("version")?.as_str().map(|v| v.to_string())
    }

    // Fall back to the unpacked folder, then to the app version, so a missing or
    // unreadable CRX never produces an empty version field.
    read_from_crx(crx)
        .or_else(|| {
            let dir = find_extension_dir()?;
            let text = fs::read_to_string(dir.join("manifest.json")).ok()?;
            let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
            parsed.get("version")?.as_str().map(|v| v.to_string())
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Remove extension manifests that an older build wrote into `External Extensions`.
///
/// Those files used `{"external_path": …}`, which Chromium does not understand,
/// and several carried names that are not our extension id (a leftover of an
/// older packaging script). They never installed anything; they only made the
/// directory look like the extension was registered when it was not.
///
/// Only files listing a path or CRX for *this* app are touched, and anything
/// naming a different app is left alone: the directory is shared with every
/// other externally installed extension on the machine.
fn remove_legacy_extension_manifests(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        // Our own, current manifest is written right after this sweep.
        if path.file_stem().and_then(|stem| stem.to_str()) == Some(CHROME_EXT_ID) {
            continue;
        }

        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("external_path") {
            continue;
        }
        // Only a manifest that points at our own files is ours to delete.
        let mentions_zdm = text.to_ascii_lowercase().contains("zdm")
            || text.contains(CHROME_EXT_ID)
            || text.contains("zyzoom");
        if mentions_zdm {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Chromium's supported auto-install channel: a signed CRX3 + external_crx.
fn register_chromium_external_extension(candidate: &Candidate, report: &mut IntegrationReport) {
    let Some(crx) = find_crx() else {
        report.warnings.push(
            "Signed CRX not found, so Chromium cannot auto-install the extension. Use \"Load unpacked\" or rebuild the package."
                .into(),
        );
        return;
    };

    // Per-user external-extension directory (no admin rights needed).
    let external_dir = candidate.config.join("External Extensions");
    if fs::create_dir_all(&external_dir).is_err() {
        return;
    }

    // Sweep out manifests left by older builds. Those wrote
    // `{"external_path": ...}`, which is not a Chromium field, so the browser
    // ignored them — but a stale file named after a *different* extension id
    // still clutters the directory and makes "is my extension registered?"
    // impossible to answer by looking.
    remove_legacy_extension_manifests(&external_dir);

    let payload = serde_json::json!({
        "external_crx": crx.to_string_lossy(),
        // Must be the *extension* version. It used to be env!(CARGO_PKG_VERSION),
        // i.e. the app version (1.1.0) while the CRX is 1.1.1. Chromium compares
        // this against the CRX manifest and refuses the auto-install on a
        // mismatch, so the extension silently never appeared.
        "external_version": extension_version(&crx),
    });

    if fs::write(
        external_dir.join(format!("{CHROME_EXT_ID}.json")),
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
    )
    .is_err()
    {
        report.warnings.push(format!(
            "Could not write the external extension manifest for {}.",
            candidate.name
        ));
    }

    // Enterprise policy: this is what removes Edge/Chrome's "not from the Store,
    // are you sure?" prompt. Without it the browser treats our self-signed CRX as
    // an unknown, untrusted source and the user has to accept it manually.
    write_chromium_policy(candidate, &crx, report);
}

/// Write the CRX update manifest Chrome actually understands.
///
/// `update_url` must point at an XML update manifest, never at a `.crx`. The old
/// code passed the CRX path itself, so Chromium tried to parse a ZIP as XML and
/// rejected the whole policy — the extension stayed "not installed" no matter
/// how many times the user pressed repair.
///
/// The format is Google's Omaha update manifest; Gupdate/Update check/URLs/URL
/// with a `codebase` pointing at the CRX and a matching version.
fn write_update_manifest(crx: &Path, version: &str) -> Option<PathBuf> {
    let dir = crx.parent()?;
    let manifest = dir.join("update.xml");

    let codebase = format!("file:///{}", crx.to_string_lossy().replace('\\', "/"));
    let xml = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n\
         <gupdate xmlns='http://www.google.com/update2/response' protocol='2.0'>\n\
         \x20 <app appid='{CHROME_EXT_ID}'>\n\
         \x20   <updatecheck codebase='{codebase}' version='{version}' />\n\
         \x20 </app>\n\
         </gupdate>\n"
    );

    fs::write(&manifest, xml).ok()?;
    Some(manifest)
}

/// Write (or extend) the browser's `ExternalExtensions` policy.
///
/// Chromium reads policy from a per-platform location; on Windows it is the
/// registry, on Linux a JSON file under the browser's config directory. Both are
/// handled here, and an existing policy file is *merged* rather than replaced so
/// we never wipe an administrator's other settings.
fn write_chromium_policy(candidate: &Candidate, crx: &Path, report: &mut IntegrationReport) {
    let version = extension_version(crx);

    // `update_url` needs an XML update manifest. Point it at the CRX and Chromium
    // rejects the policy outright; fall back to `install_url` (the documented
    // field for a direct file) when the manifest could not be written.
    let update_manifest = write_update_manifest(crx, &version);
    let install_url = match &update_manifest {
        Some(manifest) => format!("file:///{}", manifest.to_string_lossy().replace('\\', "/")),
        None => format!("file://{}", crx.to_string_lossy().replace('\\', "/")),
    };
    let update_field = if update_manifest.is_some() {
        "update_url"
    } else {
        "install_url"
    };

    #[cfg(windows)]
    {
        use std::process::Command;

        // Chromium reads policy from `HKLM\SOFTWARE\Policies\<Vendor>\<Product>`.
        // The previous key (`Software\<Vendor>\PolicyExtensions`) is not a policy
        // path at all, which is why Edge kept showing "untrusted / not installed".
        //
        // HKCU is used because the app runs unelevated; the installer also writes
        // the machine-wide equivalent.
        let products: &[(&str, &str)] = match candidate.family {
            Family::Chromium => &[
                ("Google", "Chrome"),
                ("Microsoft", "Edge"),
                ("BraveSoftware", "Brave"),
                ("Vivaldi", "Vivaldi"),
                ("Chromium", "Chromium"),
            ],
            Family::Gecko => &[],
        };

        for (vendor, product) in products {
            let base = format!("HKCU\\Software\\Policies\\{vendor}\\{product}");

            // 1. Allow force-install from a non-store source.
            let _ = Command::new("reg")
                .args([
                    "add",
                    &format!("{base}\\ExtensionInstallSources"),
                    "/v",
                    "1",
                    "/t",
                    "REG_SZ",
                    "/d",
                    "file:///*",
                    "/f",
                ])
                .output();

            // 2. Force-install our extension and pin it to the toolbar.
            let _ = Command::new("reg")
                .args([
                    "add",
                    &format!("{base}\\ExtensionSettings\\{CHROME_EXT_ID}"),
                    "/v",
                    "installation_mode",
                    "/t",
                    "REG_SZ",
                    "/d",
                    "force_installed",
                    "/f",
                ])
                .output();

            let _ = Command::new("reg")
                .args([
                    "add",
                    &format!("{base}\\ExtensionSettings\\{CHROME_EXT_ID}"),
                    "/v",
                    update_field,
                    "/t",
                    "REG_SZ",
                    "/d",
                    &install_url,
                    "/f",
                ])
                .output();

            let _ = Command::new("reg")
                .args([
                    "add",
                    &format!("{base}\\ExtensionSettings\\{CHROME_EXT_ID}"),
                    "/v",
                    "toolbar_pin",
                    "/t",
                    "REG_SZ",
                    "/d",
                    "force_pinned",
                    "/f",
                ])
                .output();

            report.registered.push(format!("{} (policy)", candidate.name));
        }

        // Documented Chrome restriction: on Windows, extensions outside the Web
        // Store can only be force-installed on a domain-joined / Azure AD /
        // Chrome Enterprise Core machine. On a home PC the policy is written but
        // Chrome ignores it, which is why "Load unpacked" stays the reliable
        // path there. Say so instead of claiming success.
        report.warnings.push(
            "Windows: Chrome only force-installs extensions from outside the Web Store on domain-joined or Enterprise-managed machines. On a home PC, use Load unpacked with the extension folder shown above.".into(),
        );
    }

    #[cfg(not(windows))]
    {
        // Linux: policies live next to the browser's user data.
        let policy_dir = candidate.config.join("policies");
        let policy_file = policy_dir.join("policies.json");

        let mut document = fs::read_to_string(&policy_file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .unwrap_or_else(|| serde_json::json!({ "ExtensionSettings": {} }));

        if !document.is_object() {
            document = serde_json::json!({ "ExtensionSettings": {} });
        }

        if document.get("ExtensionSettings").is_none() {
            document["ExtensionSettings"] = serde_json::json!({});
        }

        // `force_installed` bypasses the "unknown source" dialog entirely.
        // Built as a map because the field name (update_url vs install_url) is
        // chosen at runtime.
        let mut settings = serde_json::Map::new();
        settings.insert(
            "installation_mode".into(),
            serde_json::Value::String("force_installed".into()),
        );
        settings.insert(
            update_field.to_string(),
            serde_json::Value::String(install_url.clone()),
        );
        settings.insert(
            "toolbar_pin".into(),
            serde_json::Value::String("force_pinned".into()),
        );
        document["ExtensionSettings"][CHROME_EXT_ID] = serde_json::Value::Object(settings);

        if fs::create_dir_all(&policy_dir).is_ok()
            && fs::write(
                &policy_file,
                serde_json::to_string_pretty(&document).unwrap_or_default(),
            )
            .is_ok()
            {
                report.registered.push(format!("{} (policy)", candidate.name));
            }
    }
}

fn gecko_policy_dir(candidate: &Candidate) -> Option<PathBuf> {
    if candidate.config.exists() {
        return Some(candidate.config.join("distribution"));
    }
    None
}

/// Best-effort launch of a browser with the unpacked extension loaded.
pub fn launch_browser_with_extension(browser_id: &str) -> Result<(), String> {
    let home = crate::paths::home_dir();
    let candidate = chromium_candidates(&home)
        .into_iter()
        .find(|c| c.id == browser_id)
        .ok_or_else(|| format!("Unknown browser: {browser_id}"))?;

    let binary = browser_binary(candidate.id)
        .ok_or_else(|| format!("{} is not installed", candidate.name))?;

    match candidate.family {
        Family::Chromium => {
            let ext = find_extension_dir().ok_or("Extension folder was not found")?;
            crate::process::std_command(&binary)
                .arg(format!("--load-extension={}", ext.display()))
                .arg("--new-window")
                .spawn()
                .map_err(|e| format!("Could not start {}: {e}", candidate.name))?;
        }
        Family::Gecko => {
            if let Some(xpi) = find_xpi() {
                crate::process::std_command(&binary)
                    .arg(format!("file://{}", xpi.display()))
                    .spawn()
                    .map_err(|e| format!("Could not start {}: {e}", candidate.name))?;
            } else {
                return Err("Firefox add-on package is missing".into());
            }
        }
    }
    Ok(())
}

fn browser_binary(id: &str) -> Option<PathBuf> {
    let names: &[&str] = match id {
        "chrome" => &["google-chrome-stable", "google-chrome", "chrome", "chrome.exe"],
        "chromium" => &["chromium", "chromium-browser", "chromium.exe"],
        "brave" => &["brave-browser", "brave", "brave.exe"],
        "edge" => &["microsoft-edge-stable", "microsoft-edge", "msedge", "msedge.exe"],
        "vivaldi" => &["vivaldi-stable", "vivaldi", "vivaldi.exe"],
        "opera" => &["opera", "opera.exe"],
        "firefox" => &["firefox", "firefox.exe"],
        "zen" => &["zen-browser", "zen", "zen.exe"],
        "librewolf" => &["librewolf", "librewolf.exe"],
        _ => &[],
    };

    for name in names {
        if let Some(found) = crate::process::which(name) {
            return Some(found);
        }
    }

    // Windows installs are frequently not on PATH at all.
    #[cfg(windows)]
    {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        let pf86 =
            std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into());
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let paths: Vec<String> = match id {
            "chrome" => vec![
                format!(r"{pf}\Google\Chrome\Application\chrome.exe"),
                format!(r"{pf86}\Google\Chrome\Application\chrome.exe"),
                format!(r"{local}\Google\Chrome\Application\chrome.exe"),
            ],
            "edge" => vec![
                format!(r"{pf86}\Microsoft\Edge\Application\msedge.exe"),
                format!(r"{pf}\Microsoft\Edge\Application\msedge.exe"),
            ],
            "brave" => vec![
                format!(r"{pf}\BraveSoftware\Brave-Browser\Application\brave.exe"),
                format!(r"{local}\BraveSoftware\Brave-Browser\Application\brave.exe"),
            ],
            "vivaldi" => vec![
                format!(r"{local}\Vivaldi\Application\vivaldi.exe"),
                format!(r"{pf}\Vivaldi\Application\vivaldi.exe"),
            ],
            "firefox" => vec![
                format!(r"{pf}\Mozilla Firefox\firefox.exe"),
                format!(r"{pf86}\Mozilla Firefox\firefox.exe"),
            ],
            "zen" => vec![format!(r"{local}\Zen\Application\zen.exe")],
            _ => vec![],
        };
        for p in paths {
            let path = PathBuf::from(p);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

/// Re-run integration for a single browser and report the outcome.
pub fn repair_browser_integration(browser_id: &str) -> IntegrationReport {
    let full = install_browser_integrations();
    let mut filtered = full.clone();
    filtered.registered.retain(|name| {
        name.to_lowercase().contains(browser_id) || browser_id.contains(&name.to_lowercase())
    });
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_id_is_stable() {
        assert_eq!(CHROME_EXT_ID.len(), 32);
        assert!(CHROME_EXT_ID.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn browser_list_includes_both_families() {
        let list = get_installed_browsers_list();
        assert!(list.iter().any(|b| b.family == "chromium"));
        assert!(list.iter().any(|b| b.family == "gecko"));
    }

    /// A leftover `external_path` manifest from an older build must be swept away,
    /// while another app's registration in the same shared directory must not be.
    #[test]
    fn legacy_external_path_manifests_are_removed_but_other_apps_are_kept() {
        let dir = std::env::temp_dir().join(format!("zdm-external-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let legacy = dir.join("zdm_extension.json");
        fs::write(&legacy, r#"{"external_path": "/home/me/.local/share/zdm/extension"}"#).unwrap();

        let stale_id = dir.join("cjdipciekhdnndlcpcgjocjbdijdlhnn.json");
        fs::write(&stale_id, r#"{"external_path": "/usr/share/zdm/extension"}"#).unwrap();

        let foreign = dir.join("someoneelse.json");
        fs::write(&foreign, r#"{"external_path": "/opt/other/ext"}"#).unwrap();

        let ours = dir.join(format!("{CHROME_EXT_ID}.json"));
        fs::write(
            &ours,
            r#"{"external_crx": "/x.crx", "external_version": "1.2.0"}"#,
        )
        .unwrap();

        remove_legacy_extension_manifests(&dir);

        assert!(!legacy.exists(), "an old external_path manifest for ZDM must go");
        assert!(!stale_id.exists(), "a stale id manifest pointing at ZDM must go");
        assert!(foreign.exists(), "another app's registration must be untouched");
        assert!(ours.exists(), "our current manifest must survive the sweep");

        let _ = fs::remove_dir_all(&dir);
    }

    /// The CRX's own manifest is the only trustworthy version source.
    ///
    /// Reading env!(CARGO_PKG_VERSION) gave 1.1.0 for a 1.1.1 extension, and
    /// Chromium rejects the auto-install when `external_version` disagrees with
    /// the CRX — the extension simply never appeared.
    #[test]
    fn extension_version_comes_from_the_crx_not_the_app() {
        let crx = Path::new("../../assets/jgoccphfikadefpdemmaohkpbomgjcak.crx");
        if !crx.is_file() {
            return; // assets are not always present in a bare checkout
        }

        let version = extension_version(crx);
        assert!(
            version != env!("CARGO_PKG_VERSION") || version.starts_with("1.1"),
            "expected the extension's own version, got {version}"
        );
        // Shape check: dotted numerics, never empty.
        assert!(
            version.split('.').all(|part| part.chars().all(|c| c.is_ascii_digit()))
                && version.contains('.'),
            "version must look like x.y.z, got {version}"
        );
    }

    /// The fingerprint must be stable when nothing changed, or startup would
    /// re-run the whole integration (a dozen `reg` calls per browser) on every
    /// boot — the "hangs on every login" bug.
    #[test]
    fn integration_fingerprint_is_stable() {
        let a = current_stamp().fingerprint;
        let b = current_stamp().fingerprint;
        assert_eq!(a, b, "the fingerprint must not vary between calls");
        assert!(a.contains('|'), "the fingerprint must combine its inputs");
    }

    /// `update_url` must point at an XML update manifest, never at the .crx.
    #[test]
    fn update_manifest_is_xml_pointing_at_the_crx() {
        let dir = std::env::temp_dir().join(format!("zdm-update-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let crx = dir.join(format!("{CHROME_EXT_ID}.crx"));
        let _ = fs::write(&crx, b"Cr24not-a-real-crx");

        let manifest = write_update_manifest(&crx, "1.1.1").expect("manifest must be written");
        let xml = fs::read_to_string(&manifest).unwrap();

        assert!(xml.contains("<gupdate"), "must be an update manifest, not a CRX");
        assert!(xml.contains(CHROME_EXT_ID));
        assert!(xml.contains("version='1.1.1'"));
        // The codebase has to point at the CRX for Chrome to fetch it.
        assert!(xml.contains(".crx"));
        assert!(!manifest.to_string_lossy().ends_with(".crx"));

        let _ = fs::remove_dir_all(&dir);
    }
}
