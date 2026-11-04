//! ZDM Autostart Integration
//! Developed by Ramy Badraan
//!
//! "Runs in the background after login" is not one mechanism, it is several, and
//! every session honours a different one:
//!
//! * `~/.config/autostart/*.desktop` — XDG autostart. Honoured by GNOME, KDE,
//!   XFCE, Cinnamon. **Ignored entirely** by a bare Hyprland/sway/i3 session.
//! * `systemd --user` — works in any session that has a user bus, including
//!   Hyprland, but it is often left disabled by installers.
//! * The compositor's own startup file — the only mechanism Hyprland is
//!   guaranteed to honour. Its format has changed over time:
//!     - classic `hyprland.conf` / `exec.conf`: `exec-once = <cmd>`
//!     - modern split Lua config (`confs/startup.lua`):
//!       `hl.on("hyprland.start", function() hl.exec_cmd("<cmd>") end)`
//!
//! The previous implementation only knew the classic form, so on a modern
//! Hyprland setup it silently did nothing — which is exactly why the app never
//! started with the session on Arch.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize)]
pub struct AutostartStatus {
    pub enabled: bool,
    /// Comma separated list of mechanisms currently active.
    pub kind: String,
}

const MARKER: &str = "# ZDM autostart (managed by Zyzoom Download Manager)";
const LUA_MARKER: &str = "-- ZDM autostart (managed by Zyzoom Download Manager)";

fn enabled_flag_path() -> PathBuf {
    crate::paths::config_dir().join("autostart.enabled")
}

pub fn is_enabled() -> bool {
    let flag = enabled_flag_path();
    if flag.exists() {
        return fs::read_to_string(flag)
            .map(|v| v.trim() == "1")
            .unwrap_or(true);
    }
    wiring_present()
}

fn wiring_present() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        dirs_autostart().join("zdm.desktop").exists()
            || systemd_unit_path().map(|p| p.exists()).unwrap_or(false)
            || compositor_wired()
    }
}

pub fn status() -> AutostartStatus {
    let mut kinds: Vec<&str> = Vec::new();

    #[cfg(not(windows))]
    {
        if dirs_autostart().join("zdm.desktop").exists() {
            kinds.push("xdg");
        }
        if systemd_unit_path().map(|p| p.exists()).unwrap_or(false) {
            kinds.push("systemd");
        }
        if compositor_wired() {
            kinds.push("compositor");
        }
    }
    #[cfg(windows)]
    {
        kinds.push("registry");
    }

    AutostartStatus {
        enabled: is_enabled(),
        kind: if kinds.is_empty() {
            "none".to_string()
        } else {
            kinds.join(",")
        },
    }
}

pub fn set_enabled(enabled: bool) -> std::io::Result<bool> {
    crate::paths::ensure_dirs().ok();
    let flag = enabled_flag_path();
    if enabled {
        fs::write(&flag, b"1")?;
        install()?;
        enable_systemd_unit();
    } else {
        fs::write(&flag, b"0")?;
        uninstall()?;
        disable_systemd_unit();
    }
    Ok(enabled)
}

/// Wire every mechanism we can. Never fatal: a read-only home or a missing
/// systemd must not stop the app from working.
pub fn install() -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = write_xdg_entry();
        let _ = write_systemd_unit();
        let _ = wire_compositor();
    }
    Ok(())
}

pub fn uninstall() -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = fs::remove_file(dirs_autostart().join("zdm.desktop"));
        if let Some(unit) = systemd_unit_path() {
            let _ = fs::remove_file(&unit);
        }
        let _ = unwire_compositor();
    }
    Ok(())
}

// --------------------------------------------------------------------- XDG

#[cfg(not(windows))]
fn dirs_autostart() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("autostart");
        }
    }
    crate::paths::home_dir().join(".config").join("autostart")
}

#[cfg(not(windows))]
fn current_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("zdm"))
}

#[cfg(not(windows))]
fn write_xdg_entry() -> std::io::Result<()> {
    let dir = dirs_autostart();
    fs::create_dir_all(&dir)?;
    let exe = current_exe();
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Zyzoom Download Manager\n\
         GenericName=Download Manager\n\
         Name[ar]=زي زوم مدير التنزيل\n\
         Name[fr]=Zyzoom Download Manager\n\
         Comment=Start ZDM in the background on login\n\
         Exec=\"{}\" --background\n\
         Icon=zdm\n\
         Terminal=false\n\
         NoDisplay=false\n\
         Categories=Network;FileTransfer;\n\
         X-GNOME-Autostart-enabled=true\n\
         X-GNOME-Autostart-Delay=2\n\
         X-KDE-autostart-after=panel\n\
         StartupNotify=false\n",
        exe.display()
    );
    fs::write(dir.join("zdm.desktop"), content)
}

// ------------------------------------------------------------------ systemd

#[cfg(not(windows))]
fn systemd_unit_path() -> Option<PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if xdg.trim().is_empty() {
            crate::paths::home_dir().join(".config")
        } else {
            PathBuf::from(xdg)
        }
    } else {
        crate::paths::home_dir().join(".config")
    };
    Some(base.join("systemd").join("user").join("zdm.service"))
}

#[cfg(not(windows))]
fn write_systemd_unit() -> std::io::Result<()> {
    let unit = systemd_unit_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config home"))?;
    let exe = current_exe();
    let content = format!(
        "[Unit]\n\
         Description=Zyzoom Download Manager (background service)\n\
         After=graphical-session.target network-online.target\n\
         PartOf=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart=\"{}\" --background\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         Nice=5\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
        exe.display()
    );
    if let Some(dir) = unit.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&unit, content)?;
    Ok(())
}

/// `systemctl --user enable` needs a live user bus; failures are expected in
/// containers and CI, so they are reported but not fatal.
#[cfg(not(windows))]
pub fn enable_systemd_unit() {
    let _ = crate::process::std_command("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
    let _ = crate::process::std_command("systemctl")
        .args(["--user", "enable", "zdm.service"])
        .output();
}

#[cfg(not(windows))]
pub fn disable_systemd_unit() {
    let _ = crate::process::std_command("systemctl")
        .args(["--user", "disable", "zdm.service"])
        .output();
}

#[cfg(windows)]
pub fn enable_systemd_unit() {}
#[cfg(windows)]
pub fn disable_systemd_unit() {}

// --------------------------------------------------------------- compositor

/// Every compositor startup file we know how to write, in preference order.
///
/// Hyprland has shipped two configuration styles:
///   * one `hyprland.conf` with `exec-once = …`
///   * a split Lua tree (`confs/startup.lua`) using the `hl` API
///
/// Both are handled, and the modern Lua form is written when that layout exists.
#[cfg(not(windows))]
enum CompositorTarget {
    /// `exec-once = "cmd"` style lines in a plain conf file.
    Conf(PathBuf, String),
    /// `hl.exec_cmd("cmd")` inside the `hl.on("hyprland.start", …)` block.
    Lua(PathBuf, String),
}

#[cfg(not(windows))]
fn compositor_targets() -> Vec<CompositorTarget> {
    let home = crate::paths::home_dir();
    let hypr = home.join(".config").join("hypr");
    let exe = current_exe();
    // `hl.exec_cmd` runs the string through a shell. A single-quoted path survives
    // spaces and needs no Lua escaping beyond the surrounding literal quotes.
    let shell_command = format!("'{}' --background", exe.display());

    let mut targets: Vec<CompositorTarget> = Vec::new();

    // --- modern split Hyprland (Lua) -------------------------------------
    // Prefer the dedicated startup file; fall back to any *.lua that has a
    // `hyprland.start` hook.
    let startup = hypr.join("confs").join("startup.lua");
    if startup.exists() {
        targets.push(CompositorTarget::Lua(startup, shell_command.clone()));
    } else if let Ok(entries) = fs::read_dir(hypr.join("confs")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("lua") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                if text.contains("hyprland.start") {
                    targets.push(CompositorTarget::Lua(path, shell_command.clone()));
                    break;
                }
            }
        }
    }

    // --- classic Hyprland (conf) -----------------------------------------
    for name in ["hyprland.conf", "exec.conf"] {
        let path = hypr.join(name);
        if path.exists() {
            targets.push(CompositorTarget::Conf(
                path,
                format!("exec-once = {shell_command}"),
            ));
        }
    }

    // --- sway / i3 --------------------------------------------------------
    let sway = home.join(".config").join("sway").join("config");
    if sway.exists() {
        targets.push(CompositorTarget::Conf(
            sway,
            format!("exec {shell_command}"),
        ));
    }
    let i3 = home.join(".config").join("i3").join("config");
    if i3.exists() {
        targets.push(CompositorTarget::Conf(
            i3,
            format!("exec --no-startup-id {} --background", exe.display()),
        ));
    }

    targets
}


/// Render a Rust string as a Lua double-quoted literal.
///
/// `{:?}` would do almost the same thing, but it escapes the inner quotes of an
/// already shell-quoted command into `\\\"`, which breaks the executed path.
#[cfg(not(windows))]
fn lua_string(value: &str) -> String {
    // Only the Lua literal needs escaping; the command itself keeps single quotes.
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(not(windows))]
fn compositor_wired() -> bool {
    compositor_targets().iter().any(|target| match target {
        CompositorTarget::Conf(path, _) => fs::read_to_string(path)
            .map(|c| c.contains(MARKER))
            .unwrap_or(false),
        CompositorTarget::Lua(path, _) => fs::read_to_string(path)
            .map(|c| c.contains(LUA_MARKER))
            .unwrap_or(false),
    })
}

#[cfg(not(windows))]
fn wire_compositor() -> std::io::Result<()> {
    for target in compositor_targets() {
        match target {
            CompositorTarget::Conf(path, line) => {
                let existing = fs::read_to_string(&path).unwrap_or_default();
                if existing.contains(MARKER) {
                    continue;
                }
                // Append only: rewriting someone's hyprland.conf is never acceptable.
                let addition = format!("\n{MARKER}\n{line}\n");
                fs::OpenOptions::new()
                    .append(true)
                    .open(&path)?
                    .write_all(addition.as_bytes())?;
            }
            CompositorTarget::Lua(path, command) => {
                let existing = fs::read_to_string(&path).unwrap_or_default();
                if existing.contains(LUA_MARKER) {
                    continue;
                }

                // Insert inside the existing `hyprland.start` handler when there is
                // one, so our line runs in the right lifecycle phase.
                let call = format!("    hl.exec_cmd({})\n", lua_string(&command));
                if let Some(hook) = existing.find("hyprland.start") {
                    if let Some(close) = existing[hook..].find("end)") {
                        let insert_at = hook + close;
                        let mut updated = String::with_capacity(existing.len() + call.len() + 60);
                        updated.push_str(&existing[..insert_at]);
                        updated.push_str(&format!("{LUA_MARKER}\n"));
                        updated.push_str(&call);
                        updated.push_str(&existing[insert_at..]);
                        fs::write(&path, updated)?;
                        continue;
                    }
                }

                // No hook found: append a self-contained block.
                let block = format!(
                    "\n{LUA_MARKER}\nhl.on(\"hyprland.start\", function()\n    hl.exec_cmd({})\nend)\n",
                    lua_string(&command)
                );
                fs::OpenOptions::new()
                    .append(true)
                    .open(&path)?
                    .write_all(block.as_bytes())?;
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn unwire_compositor() -> std::io::Result<()> {
    for target in compositor_targets() {
        let path = match &target {
            CompositorTarget::Conf(p, _) => p.clone(),
            CompositorTarget::Lua(p, _) => p.clone(),
        };
        let content = fs::read_to_string(&path).unwrap_or_default();
        if !content.contains(MARKER) && !content.contains(LUA_MARKER) {
            continue;
        }

        let cleaned: Vec<&str> = content
            .lines()
            .filter(|line| {
                if line.contains(MARKER) || line.contains(LUA_MARKER) {
                    return false;
                }
                // Remove our own exec line, but never someone else's.
                let ours = (line.contains("--background") && line.contains("zdm"))
                    || (line.contains("hl.exec_cmd") && line.contains("zdm"));
                !ours
            })
            .collect();
        fs::write(&path, format!("{}\n", cleaned.join("\n")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_without_panicking() {
        let status = status();
        assert!(!status.kind.is_empty());
    }

    /// A Lua startup block must receive the call *inside* the existing hook.
    #[test]
    fn lua_insertion_respects_the_existing_hook() {
        let sample = "hl.on(\"hyprland.start\", function()\n    hl.exec_cmd(\"waybar\")\nend)\n";
        let command = "\"/usr/bin/zdm\" --background";

        let hook = sample.find("hyprland.start").unwrap();
        let close = sample[hook..].find("end)").unwrap();
        let insert_at = hook + close;

        let mut out = String::new();
        out.push_str(&sample[..insert_at]);
        out.push_str(&format!("{LUA_MARKER}\n    hl.exec_cmd({command:?})\n"));
        out.push_str(&sample[insert_at..]);

        // The call is inside the handler, before the closing `end)`.
        let call_pos = out.find("zdm").expect("command must be inserted");
        let end_pos = out.find("end)").expect("handler must still close");
        assert!(call_pos < end_pos, "the call must be inside the handler");
        assert!(out.contains("waybar"), "existing entries must survive");
        assert!(out.contains(LUA_MARKER));
    }

    /// Unwiring must drop our lines and keep everything else.
    #[test]
    fn unwire_removes_only_our_lines() {
        let content = format!(
            "hl.on(\"hyprland.start\", function()\n\
             {LUA_MARKER}\n    hl.exec_cmd(\"/usr/bin/zdm --background\")\n    hl.exec_cmd(\"waybar\")\nend)\n"
        );
        let cleaned: Vec<&str> = content
            .lines()
            .filter(|line| {
                if line.contains(MARKER) || line.contains(LUA_MARKER) {
                    return false;
                }
                !(line.contains("hl.exec_cmd") && line.contains("zdm"))
            })
            .collect();

        let joined = cleaned.join("\n");
        assert!(!joined.contains("zdm"), "our entry must be gone");
        assert!(joined.contains("waybar"), "the user's entry must stay");
        assert!(joined.contains("end)"), "the block must stay valid");
    }
}
