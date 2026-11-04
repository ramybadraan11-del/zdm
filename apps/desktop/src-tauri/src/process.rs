//! ZDM Process Spawning Layer
//! Developed by Ramy Badraan
//!
//! Every child process ZDM launches on Windows used to flash a console window
//! ("DOS screen") because `CREATE_NO_WINDOW` was never set on the command.
//! All spawns must go through this module so the flag can never be forgotten.

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Windows: hide the console window for any child process.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Build a hidden `tokio::process::Command` for the given program.
pub fn command<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Same, but for a blocking `std::process::Command` (used for short-lived
/// fire-and-forget helpers such as opening a folder).
pub fn std_command<P: AsRef<std::ffi::OsStr>>(program: P) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, CREATE_NO_WINDOW);
    }
    cmd
}

/// Resolve an executable on `PATH` the portable way (`which` does not exist on
/// Windows, which is why the old code always failed there).
pub fn which(program: &str) -> Option<PathBuf> {
    let exe = Path::new(program);
    if exe.components().count() > 1 || exe.is_absolute() {
        return if exe.is_file() { Some(exe.to_path_buf()) } else { None };
    }

    let path_var = std::env::var_os("PATH")?;
    let suffixes: Vec<String> = if cfg!(windows) {
        let pathext = std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        let mut list: Vec<String> = pathext
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();
        list.insert(0, String::new());
        list
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path_var) {
        for suffix in &suffixes {
            let candidate = if suffix.is_empty() {
                dir.join(program)
            } else {
                dir.join(format!("{program}{suffix}"))
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_a_shell_on_unix() {
        if cfg!(windows) {
            assert!(which("cmd").is_some() || which("cmd.exe").is_some());
        } else {
            assert!(which("sh").is_some(), "expected to find sh on PATH");
            assert!(which("definitely-not-a-real-binary-xyz").is_none());
        }
    }

    #[test]
    fn absolute_path_is_returned_only_when_it_exists() {
        assert!(which("/definitely/not/here").is_none());
    }
}
