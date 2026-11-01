//! Saving the download list so everything survives a restart or a crash.
//!
//! Both files written here hold secrets — `settings.json` the proxy
//! password, `downloads.json` the per-download cookies and auth headers
//! (sealed with DPAPI on Windows only; plain on other systems) — so the
//! temporary file is restricted to the owner (mode 0600 on Unix) before
//! the rename, and the finished file keeps that mode. A file left behind
//! by an older version heals itself on the next save, because the rename
//! replaces it with the restricted one.

use crate::model::DownloadRecord;
use serde::Serialize;
use std::path::Path;

/// Owner-only on Unix (mode 0600); a no-op elsewhere. Applied to the
/// temporary file before the rename, so the finished file inherits it.
/// Kept even though [`write_tmp`] creates the file restricted already:
/// a temporary left behind by a crashed earlier run may carry wider
/// permissions, and `OpenOptions::mode` only applies at creation.
fn restrict_tmp(tmp: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = tmp;
    }
    Ok(())
}

/// Write `bytes` to `tmp`, owner-only from creation on Unix (mode 0600),
/// so a secret never sits world-readable even briefly.
fn write_tmp(tmp: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(tmp)?;
        file.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(tmp, bytes)?;
    }
    Ok(())
}

/// Write JSON to `path` atomically (temporary file + rename), pretty-printed.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    write_tmp(&tmp, &serde_json::to_vec_pretty(value)?)?;
    restrict_tmp(&tmp)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load the download list; a missing or corrupt file yields an empty list.
pub fn load_downloads(path: &Path) -> Vec<DownloadRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str(&text) {
        Ok(list) => list,
        Err(_) => {
            // a broken list (the computer went off while it was written...):
            // kept aside, never overwritten by the empty one
            let _ = std::fs::copy(path, path.with_extension("broken.json"));
            Vec::new()
        }
    }
}

/// The download list: saved often, so without spaces (smaller, faster).
pub fn save_downloads(path: &Path, list: &[DownloadRecord]) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    write_tmp(&tmp, &serde_json::to_vec(&list)?)?;
    restrict_tmp(&tmp)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn written_files_are_owner_only() {
        let dir = std::env::temp_dir().join(format!("ezoo-store-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // settings.json via write_json_atomic
        let settings = dir.join("settings.json");
        write_json_atomic(&settings, &serde_json::json!({"a": 1})).unwrap();
        assert_eq!(mode_of(&settings), 0o600);

        // downloads.json via save_downloads
        let downloads = dir.join("downloads.json");
        save_downloads(&downloads, &[]).unwrap();
        assert_eq!(mode_of(&downloads), 0o600);

        // an older, world-readable file heals itself on the next save
        std::fs::set_permissions(&downloads, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(mode_of(&downloads), 0o644);
        save_downloads(&downloads, &[]).unwrap();
        assert_eq!(mode_of(&downloads), 0o600);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
