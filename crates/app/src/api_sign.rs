//! HMAC signing for the extension channel (P2, Q53 plan default (a)).
//!
//! Every POST to the local API must carry `X-Ezoo-Time` / `X-Ezoo-Nonce` /
//! `X-Ezoo-Sig`, where the signature is HMAC-SHA256 over
//!
//! ```text
//! METHOD\npath\ntime\nnonce\n<body bytes>
//! ```
//!
//! with the per-install token (the 0600 `local-api-token` file, see
//! `local_token.rs`) as the key. The token itself is never sent —
//! unlike the old `X-SwiftGet-Token` bearer header, which was
//! replayable forever. The signature covers the body too (upstream
//! 1.0.9's scheme did not), the timestamp must be within ±5 minutes
//! of the server clock, and each nonce is accepted only once, so a
//! captured request cannot be replayed.
//!
//! Design notes (deliberately NOT upstream's scheme):
//! - single port 9614 only, no second port, no `/pair`, no `token.json`;
//! - the key comes from the OS random source via `local_token.rs`
//!   (`getrandom`); there is no weak `timestamp ^ PID` fallback —
//!   token generation fails loudly instead of minting a weak key;
//! - HMAC-SHA256 is implemented here from `sha2` (RFC 2104) rather
//!   than adding a new dependency for one call.

use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// Seconds a signed request stays valid around the server clock.
pub const SIGN_WINDOW_SECS: u64 = 300;

/// How long a used nonce is remembered (longer than the window, so a
/// replay that arrives just after the window still finds its nonce).
const NONCE_REMEMBER_SECS: u64 = 2 * SIGN_WINDOW_SECS;

/// Nonces already accepted, with the server time they arrived at.
/// A Vec, not a set: only a handful are alive at once (pruned to the
/// remember window), and `Vec::new` is usable in a static.
static SEEN_NONCES: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// HMAC-SHA256 (RFC 2104) of `msg` under `key`.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    // keys longer than the block size are hashed first
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hashed = Sha256::digest(key);
        k[..32].copy_from_slice(&hashed);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// Lowercase hex of bytes.
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The canonical signed bytes: header lines plus the raw body.
pub fn canonical(method: &str, path: &str, time: u64, nonce: &str, body: &[u8]) -> Vec<u8> {
    let mut v = format!("{method}\n{path}\n{time}\n{nonce}\n").into_bytes();
    v.extend_from_slice(body);
    v
}

/// Sign a request; returns the hex signature.
pub fn sign(token: &str, method: &str, path: &str, time: u64, nonce: &str, body: &[u8]) -> String {
    hex(&hmac_sha256(
        token.as_bytes(),
        &canonical(method, path, time, nonce, body),
    ))
}

/// A fresh nonce: 16 random bytes as 32 lowercase hex chars.
pub fn new_nonce() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source is required");
    hex(&bytes)
}

/// Constant-time comparison: no early exit on the first differing byte.
fn equal_const_time(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Is `time` within the window around `now` (both unix seconds)?
pub fn time_in_window(time: u64, now: u64) -> bool {
    now.abs_diff(time) <= SIGN_WINDOW_SECS
}

/// Accept `nonce` once: false if it was already seen inside the
/// remember window. Prunes entries older than the window first.
fn accept_nonce_once(nonce: &str, now: u64) -> bool {
    let mut seen = SEEN_NONCES.lock().unwrap();
    seen.retain(|(_, at)| now.saturating_sub(*at) <= NONCE_REMEMBER_SECS);
    if seen.iter().any(|(n, _)| n == nonce) {
        return false;
    }
    seen.push((nonce.to_string(), now));
    true
}

/// Everything a signed POST carries, for verification.
pub struct Signed<'a> {
    pub token: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub time: &'a str,
    pub nonce: &'a str,
    pub sig: &'a str,
    pub body: &'a [u8],
    pub now: u64,
}

/// Verify a signed POST. The nonce is recorded only after the
/// signature checks out, so a failed attempt cannot burn a nonce.
pub fn verify(req: Signed<'_>) -> bool {
    let Ok(time) = req.time.parse::<u64>() else {
        return false;
    };
    if !time_in_window(time, req.now) {
        return false;
    }
    // a nonce is 16 random bytes as hex; anything else is not ours
    if req.nonce.len() != 32 || !req.nonce.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let expected = sign(req.token, req.method, req.path, time, req.nonce, req.body);
    if !equal_const_time(expected.as_bytes(), req.sig.as_bytes()) {
        return false;
    }
    accept_nonce_once(req.nonce, req.now)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 1: key 0x0b * 20, data "Hi There".
    #[test]
    fn hmac_matches_rfc4231_case_1() {
        let sig = hex(&hmac_sha256(&[0x0b; 20], b"Hi There"));
        assert_eq!(
            sig,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    /// RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
    #[test]
    fn hmac_matches_rfc4231_case_2() {
        let sig = hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?"));
        assert_eq!(
            sig,
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_covers_a_long_key() {
        // keys longer than the 64-byte block are hashed first (RFC 2104)
        let key = vec![0xaa; 131];
        let sig = hex(&hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        ));
        assert_eq!(
            sig,
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// Verify a POST signed under the test token (method is always POST).
    fn check(path: &str, time: &str, nonce: &str, sig: &str, body: &[u8], now: u64) -> bool {
        verify(Signed {
            token: TOKEN,
            method: "POST",
            path,
            time,
            nonce,
            sig,
            body,
            now,
        })
    }

    #[test]
    fn round_trip_and_tamper() {
        let now = 1_800_000_000u64;
        let body = br#"{"url":"https://example.com/file.bin"}"#;
        let nonce = "0123456789abcdef0123456789abcdef";
        let sig = sign(TOKEN, "POST", "/add", now, nonce, body);
        assert!(check("/add", &now.to_string(), nonce, &sig, body, now));
        // a tampered body fails (the signature covers the body) — same
        // nonce, same time, only the body differs
        let mut other = body.to_vec();
        other[10] = b'X';
        assert!(!check("/add", &now.to_string(), nonce, &sig, &other, now));
        // a tampered path fails — same nonce, only the path differs
        assert!(!check("/quit", &now.to_string(), nonce, &sig, body, now));
        // a wrong token fails — same nonce, only the key differs
        assert!(!verify(Signed {
            token: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            method: "POST",
            path: "/add",
            time: &now.to_string(),
            nonce,
            sig: &sig,
            body,
            now,
        }));
    }

    #[test]
    fn replay_is_rejected() {
        let now = 1_800_000_000u64;
        let body = b"";
        let nonce = new_nonce();
        let sig = sign(TOKEN, "POST", "/quit", now, &nonce, body);
        assert!(check("/quit", &now.to_string(), &nonce, &sig, body, now));
        // the exact same signed request again: the nonce is spent
        assert!(!check("/quit", &now.to_string(), &nonce, &sig, body, now));
    }

    #[test]
    fn stale_and_future_timestamps_are_rejected() {
        let now = 1_800_000_000u64;
        let body = b"";
        // 301 s old: just outside the 300 s window
        let old = now - SIGN_WINDOW_SECS - 1;
        let nonce = new_nonce();
        let sig = sign(TOKEN, "POST", "/quit", old, &nonce, body);
        assert!(!check("/quit", &old.to_string(), &nonce, &sig, body, now));
        // 301 s in the future: also outside
        let future = now + SIGN_WINDOW_SECS + 1;
        let nonce = new_nonce();
        let sig = sign(TOKEN, "POST", "/quit", future, &nonce, body);
        assert!(!check(
            "/quit",
            &future.to_string(),
            &nonce,
            &sig,
            body,
            now
        ));
        // 299 s old: inside the window, accepted
        let edge = now - SIGN_WINDOW_SECS + 1;
        let nonce = new_nonce();
        let sig = sign(TOKEN, "POST", "/quit", edge, &nonce, body);
        assert!(check("/quit", &edge.to_string(), &nonce, &sig, body, now));
    }

    #[test]
    fn missing_or_malformed_fields_are_rejected() {
        let now = 1_800_000_000u64;
        let body = b"";
        let nonce = new_nonce();
        let sig = sign(TOKEN, "POST", "/quit", now, &nonce, body);
        assert!(!check("/quit", "", &nonce, &sig, body, now));
        assert!(!check("/quit", "not-a-time", &nonce, &sig, body, now));
        assert!(!check("/quit", &now.to_string(), "", &sig, body, now));
        assert!(!check("/quit", &now.to_string(), "short", &sig, body, now));
        assert!(!check("/quit", &now.to_string(), &nonce, "", body, now));
        assert!(!check("/quit", &now.to_string(), &nonce, "zz", body, now));
    }
}
