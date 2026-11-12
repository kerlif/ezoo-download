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
/// so a secret never sits world-readable even briefly. Flushed to the
/// disk before the rename, so a crash cannot leave a half-written file
/// behind (copy -> sync -> save).
fn write_tmp(tmp: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(tmp)?
        }
        #[cfg(not(unix))]
        {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(tmp)?
        }
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Flush a directory after a rename, so the rename itself survives a
/// crash (on Unix; Windows has no cheap equivalent and skips it).
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(f) = std::fs::File::open(dir) {
            let _ = f.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
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
    if let Some(dir) = path.parent() {
        sync_dir(dir);
    }
    Ok(())
}

/// Load the download list; a missing or corrupt file yields an empty list.
pub fn load_downloads(path: &Path) -> Vec<DownloadRecord> {
    load_downloads_status(path).0
}

/// Load the download list and say whether the file was corrupt: a
/// corrupt list is never silently overwritten by the empty one (see
/// `Manager::new`). A symbolic link or a hard link in its place counts
/// as corrupt and is never followed — the list holds logins, and a
/// link would hand them to wherever it points.
pub fn load_downloads_status(path: &Path) -> (Vec<DownloadRecord>, bool) {
    let md = match std::fs::symlink_metadata(path) {
        Ok(md) => md,
        Err(_) => return (Vec::new(), false),
    };
    if md.file_type().is_symlink() {
        return (Vec::new(), true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if md.nlink() > 1 {
            return (Vec::new(), true);
        }
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return (Vec::new(), false);
    };
    match serde_json::from_str(&text) {
        Ok(list) => (list, false),
        Err(_) => {
            // a broken list (the computer went off while it was written...):
            // kept aside, never overwritten by the empty one
            let _ = std::fs::copy(path, path.with_extension("broken.json"));
            (Vec::new(), true)
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
    if let Some(dir) = path.parent() {
        sync_dir(dir);
    }
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

    #[cfg(unix)]
    #[test]
    fn corrupt_list_is_reported_and_kept_aside() {
        let dir = std::env::temp_dir().join(format!("ezoo-store-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("downloads.json");

        // a good list loads clean
        save_downloads(&path, &[]).unwrap();
        let (list, corrupt) = load_downloads_status(&path);
        assert!(list.is_empty() && !corrupt);

        // a broken list is corrupt, and the broken bytes are kept aside
        std::fs::write(&path, b"{not json").unwrap();
        let (list, corrupt) = load_downloads_status(&path);
        assert!(list.is_empty() && corrupt);
        assert_eq!(
            std::fs::read(dir.join("downloads.broken.json")).unwrap(),
            b"{not json"
        );

        // a symlink in its place is corrupt and is never followed: the
        // target's bytes stay unread, and nothing is copied aside
        let target = dir.join("elsewhere.json");
        std::fs::write(&target, b"[]").unwrap();
        let link = dir.join("linked.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let (list, corrupt) = load_downloads_status(&link);
        assert!(list.is_empty() && corrupt);
        assert!(!dir.join("linked.broken.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
