//! Passwords and cookies on the disk: encrypted with the user's own key.
//! Windows: DPAPI (`CryptProtectData`). Linux: the Secret Service
//! (libsecret) — the value lives in the user's keyring under a random id,
//! and only the id is written to the disk file. Only the same user on the
//! same computer can read them back; a settings or list file copied
//! elsewhere (or read by another user) shows no password.
//!
//! On the disk a protected text looks like `"dpapi:<hex>"` (Windows) or
//! `"keyring:<id>"` (Linux). Files of older versions hold plain text: it
//! is read as it is and protected on the next save. Use the field
//! attributes below:
//!
//! ```ignore
//! #[serde(with = "crate::secret::string")]          pub password: String,
//! #[serde(default, with = "crate::secret::opt")]    pub cookies: Option<String>,
//! #[serde(default, with = "crate::secret::pair")]   pub auth: Option<(String, String)>,
//! ```
//!
//! When no keyring answers on Linux (headless, no D-Bus, locked
//! collection), the value falls back to plain text in the 0600 file and
//! a warning is printed once on stderr — the app keeps working. That
//! fallback is the old non-Windows behaviour; on Linux it is now the
//! exception, not the rule.
//!
//! Known wart: every seal stores a NEW keyring item under a fresh random
//! id, so each app run orphans the previous run's items (one per distinct
//! secret per run). They hold the same secrets the current items hold —
//! no extra exposure — but they accumulate in the keyring. A later pass
//! should store one master key and encrypt locally instead.

/// The Windows on-disk prefix.
const DPAPI_PREFIX: &str = "dpapi:";
/// The Linux on-disk prefix.
const KEYRING_PREFIX: &str = "keyring:";

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Only the Windows and other-Unix decoders read hex back; the Linux
/// decoder validates the 32-char id inline, so this is dead on Linux.
#[cfg(not(target_os = "linux"))]
fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(windows)]
mod os {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    /// The prefix this platform writes.
    pub const PREFIX: &str = super::DPAPI_PREFIX;

    /// How a protected blob is written after the prefix.
    pub fn encode(bytes: &[u8]) -> String {
        super::to_hex(bytes)
    }

    /// How a stored suffix is read back into a blob. A suffix that is
    /// not this platform's format decodes to nothing.
    pub fn decode(s: &str) -> Option<Vec<u8>> {
        super::from_hex(s)
    }

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).ok()?,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: `input` points at `data`, which outlives the call and is only
        // read; on success Windows allocates `out` (LocalAlloc), which is
        // copied and then freed with LocalFree exactly once.
        unsafe {
            let ok = if protect {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut out,
                )
            };
            if ok == 0 || out.pbData.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as _);
            Some(bytes)
        }
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// The prefix this platform writes.
    pub const PREFIX: &str = super::KEYRING_PREFIX;

    /// How a protected blob is written after the prefix: the blob IS the
    /// keyring id (32 lowercase hex chars), written as it is.
    pub fn encode(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    /// How a stored suffix is read back into a blob: only a 32-char
    /// lowercase-hex id is a keyring id; anything else decodes to
    /// nothing (so a `dpapi:` suffix can never be mistaken for one).
    pub fn decode(s: &str) -> Option<Vec<u8>> {
        if s.len() == 32
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Some(s.as_bytes().to_vec())
        } else {
            None
        }
    }

    /// The schema every item is stored under: one attribute, the random
    /// id. The schema name is the app id, so the items are namespaced
    /// to this program in the keyring. Built fresh on each call:
    /// `Schema` is not Send/Sync, so it cannot live in a static.
    fn schema() -> libsecret::Schema {
        let mut attrs = HashMap::new();
        attrs.insert("id", libsecret::SchemaAttributeType::String);
        libsecret::Schema::new(
            "io.github.kerlif.EzooDownload",
            libsecret::SchemaFlags::NONE,
            attrs,
        )
    }

    fn attributes(id: &str) -> HashMap<&str, &str> {
        let mut attrs = HashMap::new();
        attrs.insert("id", id);
        attrs
    }

    /// Every Secret Service call goes through this lock: the glib type
    /// system is not safe to initialise from several threads at once
    /// (concurrent first use corrupts the schema's attribute types —
    /// seen live as GLib-CRITICAL `g_enum_get_value` failures, and a
    /// store made under a corrupted schema is never found again by
    /// lookup, so the password is silently lost). The lock also
    /// serialises the D-Bus round-trips themselves. The calls are rare
    /// (seals are cached per run), so the contention is negligible.
    static KEYRING_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Warn once per process when the keyring cannot be used, so the
    /// downloads list (saved every few seconds) does not spam stderr.
    static WARNED: AtomicBool = AtomicBool::new(false);

    fn warn_unavailable(err: &str) {
        if !WARNED.swap(true, Ordering::Relaxed) {
            eprintln!(
                "ezoo: Secret Service unavailable ({err}); site passwords and cookies are stored as plain text in the 0600 downloads file"
            );
        }
    }

    /// Store the value in the keyring under a fresh random id and return
    /// the id. `None` means no keyring answered (the caller falls back
    /// to plain text).
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        let _guard = KEYRING_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let plain = std::str::from_utf8(data).ok()?;
        let mut rand = [0u8; 16];
        getrandom::fill(&mut rand).ok()?;
        let id = super::to_hex(&rand);
        match libsecret::password_store_sync(
            Some(&schema()),
            attributes(&id),
            None,
            "Ezoo Download Manager secret",
            plain,
            None::<&gio::Cancellable>,
        ) {
            Ok(()) => Some(id.into_bytes()),
            Err(e) => {
                warn_unavailable(&e.to_string());
                None
            }
        }
    }

    /// Look the value up by id. `None` means no such item, or no
    /// keyring answered — the caller treats both as "no password".
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        let _guard = KEYRING_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let id = std::str::from_utf8(data).ok()?;
        match libsecret::password_lookup_sync(
            Some(&schema()),
            attributes(id),
            None::<&gio::Cancellable>,
        ) {
            Ok(Some(found)) => Some(found.as_bytes().to_vec()),
            _ => None,
        }
    }

    /// Delete the keyring item for an id. Best-effort: the result is
    /// ignored — no such item, or no keyring, is not an error here.
    /// (Test cleanup today; the shape a future "forget this password"
    /// action takes.)
    #[cfg(test)]
    pub fn clear(id: &str) {
        let _guard = KEYRING_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _ = libsecret::password_clear_sync(
            Some(&schema()),
            attributes(id),
            None::<&gio::Cancellable>,
        );
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod os {
    // other systems: no user key store, so nothing is encrypted
    /// The prefix this platform writes (never written: protect is None).
    pub const PREFIX: &str = super::DPAPI_PREFIX;

    /// How a protected blob is written after the prefix.
    pub fn encode(bytes: &[u8]) -> String {
        super::to_hex(bytes)
    }

    /// How a stored suffix is read back into a blob.
    pub fn decode(s: &str) -> Option<Vec<u8>> {
        super::from_hex(s)
    }

    pub fn protect(_data: &[u8]) -> Option<Vec<u8>> {
        None
    }
    pub fn unprotect(_data: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

/// Texts protected already in this run: the download list is saved every
/// few seconds and each protection is a Windows call, so the same cookie
/// text is protected once, not again and again. (Only in memory, where the
/// plain text is anyway.)
static SEALED: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
    std::sync::Mutex::new(None);

/// The text as it is kept on the disk.
pub fn seal(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    let mut cache = SEALED.lock().unwrap_or_else(|p| p.into_inner());
    let map = cache.get_or_insert_with(Default::default);
    if let Some(done) = map.get(plain) {
        return done.clone();
    }
    let sealed = match os::protect(plain.as_bytes()) {
        Some(enc) => format!("{}{}", os::PREFIX, os::encode(&enc)),
        // No key store answered (Linux without a keyring, or other
        // systems): the plain text goes to the 0600 file. On Linux the
        // warning is printed once, inside protect().
        None => plain.to_string(),
    };
    // a cookie text is a few KB: keep at most about a thousand of them
    if map.len() > 1000 {
        map.clear();
    }
    map.insert(plain.to_string(), sealed.clone());
    sealed
}

/// The text from the disk. A protected text that can't be opened (another
/// user or computer, a deleted keyring item, or the other platform's
/// format) becomes empty: the user types the password again. Plain text
/// from older versions reads as it is.
pub fn open(stored: &str) -> String {
    for prefix in [DPAPI_PREFIX, KEYRING_PREFIX] {
        if let Some(rest) = stored.strip_prefix(prefix) {
            return os::decode(rest)
                .and_then(|b| os::unprotect(&b))
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_default();
        }
    }
    stored.to_string()
}

pub mod string {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &str, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::seal(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        Ok(super::open(&String::deserialize(d)?))
    }
}

pub mod opt {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
        v.as_deref().map(super::seal).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        Ok(Option::<String>::deserialize(d)?
            .map(|x| super::open(&x))
            .filter(|x| !x.is_empty()))
    }
}

pub mod pair {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<(String, String)>, s: S) -> Result<S::Ok, S::Error> {
        v.as_ref()
            .map(|(u, p)| (u.clone(), super::seal(p)))
            .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<(String, String)>, D::Error> {
        Ok(Option::<(String, String)>::deserialize(d)?.map(|(u, p)| (u, super::open(&p))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct T {
        #[serde(with = "string")]
        p: String,
        #[serde(default, with = "opt")]
        c: Option<String>,
        #[serde(default, with = "pair")]
        a: Option<(String, String)>,
    }

    #[test]
    fn plain_text_of_old_files_still_reads() {
        let t: T = serde_json::from_str(r#"{"p":"pw","c":"k=v","a":["u","x"]}"#).unwrap();
        assert_eq!(
            t,
            T {
                p: "pw".into(),
                c: Some("k=v".into()),
                a: Some(("u".into(), "x".into()))
            }
        );
        let back: T = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn unreadable_protected_text_is_empty_not_garbage() {
        assert_eq!(open("dpapi:zz"), "");
        assert_eq!(open("dpapi:00ff"), ""); // not this user's key
        let t: T = serde_json::from_str(r#"{"p":"dpapi:1234","c":"dpapi:","a":null}"#).unwrap();
        assert_eq!(t.p, "");
        assert_eq!(t.c, None);
        assert_eq!(seal(""), "");
    }

    /// A sealed value opens back to the plain text — through the keyring
    /// when one answers, through the 0600-file fallback when none does.
    /// When the keyring was used, the item is deleted afterwards so the
    /// test leaves nothing behind.
    #[test]
    fn seal_round_trips() {
        let sealed = seal("s3cr3t-cookie");
        assert_eq!(open(&sealed), "s3cr3t-cookie");
        #[cfg(target_os = "linux")]
        if let Some(id) = sealed.strip_prefix(KEYRING_PREFIX) {
            // when a key store answered, the disk text is not the plain text
            assert_ne!(sealed, "s3cr3t-cookie");
            super::os::clear(id);
        }
    }

    /// A keyring id that was never stored opens to empty, and a malformed
    /// id never reaches the keyring at all.
    #[test]
    fn unknown_or_malformed_keyring_id_is_empty() {
        assert_eq!(open("keyring:00000000000000000000000000000000"), "");
        assert_eq!(open("keyring:xyz"), "");
        assert_eq!(open("keyring:"), "");
        assert_eq!(open("keyring:ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"), "");
    }

    /// The live keyring path, when a keyring answers: store under a
    /// random id, look it up, delete it. When no keyring answers (CI,
    /// headless, no D-Bus) the test says so and passes — the fallback
    /// path is covered by seal_round_trips above.
    #[test]
    #[cfg(target_os = "linux")]
    fn live_keyring_store_lookup_clear() {
        let Some(id_bytes) = super::os::protect(b"live-test-value") else {
            eprintln!("secret test: no keyring answered, live path skipped");
            return;
        };
        assert_eq!(
            super::os::unprotect(&id_bytes).as_deref(),
            Some(b"live-test-value".as_slice())
        );
        let id = std::str::from_utf8(&id_bytes).unwrap();
        super::os::clear(id);
        assert_eq!(super::os::unprotect(&id_bytes), None);
    }
}
