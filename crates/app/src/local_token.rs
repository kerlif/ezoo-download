//! The secret the browser add-in shows to prove it is ours (Q29, stage a).
//!
//! One random token per install, made once and kept in the data folder
//! (`local-api-token`, owner-only on Unix). The app hands it to the add-in
//! through GET /config; the add-in never sends it back — it uses it as
//! the HMAC key that signs every POST to the local API (see api_sign.rs).
//! A web page cannot read it: the /config answer is opaque to it (the
//! API never approves a CORS preflight), so the token keeps a malicious
//! page from driving the API even if it somehow got the browser to send
//! the X-SwiftGet header.
//!
//! What it does NOT do: a local process or another add-in can read the
//! token file or GET /config itself, so this is defence against web
//! pages only. The native-messaging host (stage b, a later release)
//! closes that too.

use std::fs;
use std::io::Write;
use std::path::Path;

/// The token file, inside the program's data folder.
const FILE_NAME: &str = "local-api-token";

/// A valid token: 32 random bytes as 64 lowercase hex chars.
pub fn is_valid_token(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Read the token made at install time, or make one now and keep it.
///
/// The owner-only permissions are re-applied on EVERY load, not just
/// at creation: `OpenOptions::mode` applies only when the file is
/// created, so a file an older build (or a user restore) left at
/// 0644 would stay readable by others unless tightened here (N3).
pub fn load_or_create(data_dir: &Path) -> String {
    let path = data_dir.join(FILE_NAME);
    if let Ok(existing) = fs::read_to_string(&path) {
        let token = existing.trim().to_string();
        if is_valid_token(&token) {
            restrict(&path);
            return token;
        }
    }
    let token = generate();
    if fs::create_dir_all(data_dir).is_ok() {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        // owner-only on Unix: the file must not be readable by anyone else
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        if let Ok(mut f) = opts.open(&path) {
            let _ = writeln!(f, "{token}");
        }
        restrict(&path);
    }
    token
}

/// Owner-only (0600) on Unix; a no-op elsewhere.
#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

/// 32 bytes from the operating system's random source, as hex.
fn generate() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random source is required");
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_stable_and_a_broken_file_is_replaced() {
        let dir = std::env::temp_dir().join(format!("ezoo-token-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let first = load_or_create(&dir);
        assert!(is_valid_token(&first));
        assert_eq!(load_or_create(&dir), first);
        // the file holds the token, owner-only on Unix
        let saved = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert_eq!(saved.trim(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.join(FILE_NAME))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        // a broken file is replaced, not trusted
        fs::write(dir.join(FILE_NAME), "not-a-token\n").unwrap();
        let second = load_or_create(&dir);
        assert!(is_valid_token(&second));
        assert_ne!(second, first);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A pre-existing file left at 0644 (older build, user restore) is
    /// tightened to 0600 on load, and the token it holds is kept.
    #[test]
    #[cfg(unix)]
    fn existing_0644_file_is_restricted_on_load() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ezoo-token-0644-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let token = generate();
        let path = dir.join(FILE_NAME);
        fs::write(&path, format!("{token}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_or_create(&dir), token);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
