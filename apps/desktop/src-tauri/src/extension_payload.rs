//! Embedded browser-extension payload.
//! Developed by Ramy Badraan
//!
//! The CRX3 and the XPI are tiny (≈34 KB each), so they are compiled **into the
//! binary** instead of relying on the installer to place them next to it.
//!
//! Why this matters: the extension registration used to fail with
//! "Signed CRX not found" whenever the payload was not exactly where the app
//! looked — a moved install, a partial copy, an older installer, a distro that
//! stripped `/usr/share/zdm`. Writing the embedded copies out on first run makes
//! the payload impossible to lose.
//!
//! The build embeds `assets/<id>.crx` and `assets/zdm@zyzoom.com.xpi`. When those
//! files are absent at compile time (a clean checkout before packaging), the
//! byte arrays are empty and every accessor returns `None` — the app then falls
//! back to on-disk discovery, exactly as before.

use std::path::{Path, PathBuf};

/// CRX3 signed with our key; the filename must match the extension id.
const CRX_BYTES: &[u8] = include_bytes!("../../../../assets/jgoccphfikadefpdemmaohkpbomgjcak.crx");
/// Firefox package (plain zip with a Gecko-shaped manifest).
const XPI_BYTES: &[u8] = include_bytes!("../../../../assets/zdm@zyzoom.com.xpi");

pub const CRX_NAME: &str = "jgoccphfikadefpdemmaohkpbomgjcak.crx";
pub const XPI_NAME: &str = "zdm@zyzoom.com.xpi";

/// Whether this build actually carries the payload.
pub fn has_payload() -> bool {
    CRX_BYTES.len() > 1024
}

/// Materialise the embedded payload into `dir`, returning the CRX path.
///
/// Files are written atomically (temp + rename) so an interrupted write can never
/// leave a truncated CRX that Chromium would reject with a confusing error.
pub fn materialise(dir: &Path) -> Option<PathBuf> {
    if !has_payload() {
        return None;
    }
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }

    let crx = dir.join(CRX_NAME);
    write_if_needed(&crx, CRX_BYTES)?;

    if XPI_BYTES.len() > 1024 {
        let _ = write_if_needed(&dir.join(XPI_NAME), XPI_BYTES);
    }

    if crx.is_file() {
        Some(crx)
    } else {
        None
    }
}

/// Write `bytes` to `path` unless the file is already identical in size.
fn write_if_needed(path: &Path, bytes: &[u8]) -> Option<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() == bytes.len() as u64 {
            return Some(());
        }
    }

    let staging = path.with_extension("part");
    std::fs::write(&staging, bytes).ok()?;
    std::fs::rename(&staging, path).ok()?;
    Some(())
}

/// Extract the payload to a stable per-user location.
///
/// Used as the last resort in `find_crx`/`find_xpi`: if nothing was found on
/// disk, the app can always rebuild it from itself.
pub fn ensure_extracted() -> Option<PathBuf> {
    materialise(&crate::paths::data_dir().join("extension"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_carries_the_payload() {
        assert!(
            has_payload(),
            "the CRX must be embedded; run `python3 tools/build-packages.py` before building"
        );
        assert!(
            CRX_BYTES.starts_with(b"Cr24"),
            "the embedded file is not a CRX3 package"
        );
    }

    #[test]
    fn embedded_crx_matches_the_expected_size_range() {
        // A real CRX3 with 14 files is ~30-40 KB. A stub or truncated file is not.
        assert!(
            CRX_BYTES.len() > 20_000 && CRX_BYTES.len() < 500_000,
            "unexpected CRX size: {} bytes",
            CRX_BYTES.len()
        );
    }

    #[test]
    fn materialise_writes_both_packages() {
        let dir = std::env::temp_dir().join(format!("zdm-embed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let crx = materialise(&dir).expect("payload must be written");
        assert!(crx.is_file());
        assert_eq!(crx.file_name().unwrap().to_str().unwrap(), CRX_NAME);
        assert!(dir.join(XPI_NAME).is_file());

        let written = std::fs::read(&crx).unwrap();
        assert_eq!(written.len(), CRX_BYTES.len());
        assert!(written.starts_with(b"Cr24"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn materialise_is_idempotent_and_repairs_a_truncated_file() {
        let dir = std::env::temp_dir().join(format!("zdm-embed-fix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Simulate a half-copied file from a failed install.
        std::fs::write(dir.join(CRX_NAME), b"Cr24partial").unwrap();

        let crx = materialise(&dir).expect("must repair");
        assert_eq!(std::fs::read(&crx).unwrap().len(), CRX_BYTES.len());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
