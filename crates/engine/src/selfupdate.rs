//! New versions of Ezoo Download itself.
//!
//! Every release on GitHub carries (among other files):
//!
//! * `EzooDownload-Setup.exe` – the Windows installer
//! * `version.json`           – `{"version":"1.0.1","url":"…/EzooDownload-Setup.exe","sha256":"…","sig":"…"}`
//! * `SHA256SUMS.txt` / `SHA256SUMS-windows.txt` – SHA-256 of every release file
//!
//! The checker asks the GitHub releases API for the latest release of the
//! fork repository (the single `REPO` constant below — the only update
//! source), finds the signed `version.json` asset in that release, verifies
//! its Ed25519 signature against the public key built into the program,
//! and only then offers anything. A release whose manifest is missing or
//! unsigned is never offered. The downloaded artifact is then checked
//! against BOTH the signed manifest's SHA-256 and the release's
//! SHA256SUMS file before it is installed or, on Linux, put in place.
//!
//! `version.json` also carries `sig`: an Ed25519 signature of the version
//! and the SHA-256, made by the release build with a private key that only
//! lives in the repository's secrets. The public key is built into the
//! program, so a release (or a file) that was not made with that key is
//! never installed, even if someone could publish files on the repository.
//!
//! Failures (offline, rate-limited, malformed answers) are silent: the
//! caller gets `CheckOutcome::SilentFail` and retries later with backoff
//! (see `next_retry_delay`). Nothing here shows a dialog or nags.
//!
//! Privacy: the only headers sent are the app User-Agent
//! (`EzooDownload/<version>`), the GitHub API headers, and — when a cached
//! answer exists — `If-None-Match` carrying the cached ETag. No cookies, no
//! credentials, nothing identifying the user. See `docs/PRIVACY.md` and
//! `docs/UPDATE-CHECKER.md`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The one repository updates come from. Every update URL in this module is
/// derived from this constant; there is no second update source.
pub const REPO: &str = "kerlif/ezoo-download";
pub const SETUP_NAME: &str = "EzooDownload-Setup.exe";

/// Direct download of the latest release's `version.json`. Kept for
/// compatibility; the checker itself uses the releases API
/// (`api_latest_url`) instead, so the release's assets can be listed and
/// the signed manifest fetched from the release that was just announced.
pub fn version_url() -> String {
    format!("https://github.com/{REPO}/releases/latest/download/version.json")
}

/// The releases API endpoint for the latest release, derived from `REPO`.
pub fn api_latest_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

/// The web page a notify-only install points at, derived from `REPO`.
pub fn releases_url() -> String {
    format!("https://github.com/{REPO}/releases")
}

/// The running version.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The only User-Agent the update checker sends: app name + version, nothing
/// identifying the user or the machine.
pub fn update_user_agent() -> String {
    format!("EzooDownload/{}", current())
}

/// Public half of the fork's release signing key (Ed25519, 32 bytes, hex).
/// Generated for the fork in Phase 7; the private half lives only in the
/// repository's secrets as `UPDATE_SIGNING_KEY`. See
/// `docs/UPDATE-CHECKER.md` for the fingerprint and generation method.
pub const UPDATE_PUBLIC_KEY: &str =
    "a495ab88edacdf8d333ccef4e45cd8c4fc142fc3fb98792f16914098b9b640fc";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub url: String,
    pub sha256: String,
    /// Ed25519 signature of `signed_message(version, sha256)`, hex
    #[serde(default)]
    pub sig: String,
}

/// What the release signature covers.
pub fn signed_message(version: &str, sha256: &str) -> String {
    format!(
        "ezoo-download-update\n{}\n{}\n",
        version.trim(),
        sha256.trim().to_ascii_lowercase()
    )
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The signature of `r` was made with the key whose public half is `public_hex`.
pub fn signature_ok(r: &Release, public_hex: &str) -> bool {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let (Some(key), Some(sig)) = (from_hex(public_hex), from_hex(&r.sig)) else {
        return false;
    };
    if key.len() != 32 || sig.len() != 64 {
        return false;
    }
    UnparsedPublicKey::new(&ED25519, key)
        .verify(signed_message(&r.version, &r.sha256).as_bytes(), &sig)
        .is_ok()
}

impl Release {
    /// Where the installer is downloaded from. Only addresses of our own
    /// repository are accepted.
    pub fn setup_url(&self) -> String {
        let own = format!("https://github.com/{REPO}/");
        if self.url.starts_with(&own) {
            self.url.clone()
        } else {
            format!(
                "https://github.com/{REPO}/releases/download/v{}/{SETUP_NAME}",
                self.version
            )
        }
    }
}

fn numbers(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect()
}

/// `remote` is a higher version than `local` ("1.0.10" > "1.0.9").
/// Kept for compatibility; the checker compares release tags with
/// `is_newer_semver`, which also orders pre-releases.
pub fn is_newer(remote: &str, local: &str) -> bool {
    let (mut a, mut b) = (numbers(remote), numbers(local));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

// ---------------------------------------------------------------------------
// Semantic versions (release tags). No new dependency: parsed by hand.

/// One pre-release identifier: numeric ones compare by value, the rest by
/// text, and a numeric identifier is always lower than an alphanumeric one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreId {
    numeric: Option<u64>,
    text: String,
}

/// A parsed `major.minor.patch[-pre][+build]` version. Build metadata is
/// ignored, as semver orders it. Missing minor/patch read as 0 ("1.2" is
/// "1.2.0"); anything else malformed is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreId>,
}

pub fn parse_semver(s: &str) -> Option<SemVer> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    if s.is_empty() {
        return None;
    }
    let s = s.split('+').next().unwrap_or("");
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (s, None),
    };
    let mut core_parts = core.split('.');
    let mut nums = Vec::new();
    for part in core_parts.by_ref() {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        nums.push(part.parse::<u64>().ok()?);
    }
    if nums.is_empty() || nums.len() > 3 {
        return None;
    }
    while nums.len() < 3 {
        nums.push(0);
    }
    let mut pre_ids = Vec::new();
    if let Some(pre) = pre {
        if pre.is_empty() {
            return None;
        }
        for id in pre.split('.') {
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                return None;
            }
            let numeric = if id.bytes().all(|b| b.is_ascii_digit()) {
                Some(id.parse::<u64>().ok()?)
            } else {
                None
            };
            pre_ids.push(PreId {
                numeric,
                text: id.to_string(),
            });
        }
    }
    Some(SemVer {
        major: nums[0],
        minor: nums[1],
        patch: nums[2],
        pre: pre_ids,
    })
}

fn compare_pre(a: &[PreId], b: &[PreId]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = match (x.numeric, y.numeric) {
            (Some(nx), Some(ny)) => nx.cmp(&ny),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => x.text.cmp(&y.text),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

/// Ordering of two version strings, or `None` when either is malformed.
/// A release (no pre-release) is higher than any of its pre-releases.
pub fn compare_semver(a: &str, b: &str) -> Option<Ordering> {
    let (a, b) = (parse_semver(a)?, parse_semver(b)?);
    let mut ord = a.major.cmp(&b.major);
    if ord == Ordering::Equal {
        ord = a.minor.cmp(&b.minor);
    }
    if ord == Ordering::Equal {
        ord = a.patch.cmp(&b.patch);
    }
    if ord != Ordering::Equal {
        return Some(ord);
    }
    Some(match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => compare_pre(&a.pre, &b.pre),
    })
}

/// `remote` is a higher semantic version than `local`. Malformed strings
/// are never "newer": the checker fails silently instead of offering them.
pub fn is_newer_semver(remote: &str, local: &str) -> bool {
    compare_semver(remote, local) == Some(Ordering::Greater)
}

pub fn parse(text: &str) -> anyhow::Result<Release> {
    let r: Release = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if r.sha256.len() != 64 || !r.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("version.json has no valid sha256");
    }
    if numbers(&r.version).iter().all(|n| *n == 0) {
        anyhow::bail!("version.json has no valid version");
    }
    Ok(r)
}

/// `parse`, and the release must be signed with our key.
pub fn parse_signed(text: &str) -> anyhow::Result<Release> {
    let r = parse(text)?;
    if !signature_ok(&r, UPDATE_PUBLIC_KEY) {
        anyhow::bail!("the update is not signed by Ezoo Download, so it is not installed");
    }
    Ok(r)
}

// ---------------------------------------------------------------------------
// Guarded fetching: the checker never lets the HTTP client follow a
// redirect on its own. Redirects are followed here, at most 3 hops, and
// only to the same origin or to one of the hosts GitHub serves release
// files from. A redirect anywhere else is refused outright.

/// Hosts a redirect may land on besides the origin it started from: the
/// hosts GitHub uses for the releases API, release pages, and release
/// asset downloads.
const REDIRECT_HOSTS: [&str; 4] = [
    "api.github.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

/// May a redirect hop from `from` to `to` be followed? Same origin (same
/// host and port; an http-to-https upgrade on the same host is allowed),
/// or an https hop to a host in `REDIRECT_HOSTS`. Anything else — a
/// different host, a downgrade to http on another host — is refused.
pub fn redirect_allowed(from: &reqwest::Url, to: &reqwest::Url) -> bool {
    let same_host = from
        .host_str()
        .zip(to.host_str())
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
    if !same_host {
        return to.scheme() == "https"
            && to
                .host_str()
                .is_some_and(|h| REDIRECT_HOSTS.iter().any(|a| h.eq_ignore_ascii_case(a)));
    }
    let same_port = from.port_or_known_default() == to.port_or_known_default();
    same_port && (from.scheme() == to.scheme() || to.scheme() == "https")
}

/// GET `url` with the checker's User-Agent plus `headers`, following at
/// most 3 allowed redirects by hand. The caller must pass a client built
/// with `redirect(Policy::none())` (see `http::checker_client`); with a
/// client that follows redirects itself this guard would never see the
/// hops. `If-None-Match` is sent only on the first request, never to a
/// redirected host.
pub async fn open_guarded(
    client: &reqwest::Client,
    url: &str,
    headers: &[(reqwest::header::HeaderName, String)],
) -> anyhow::Result<reqwest::Response> {
    let mut current = reqwest::Url::parse(url)?;
    for hop in 0..4 {
        let mut req = client
            .get(current.clone())
            .header(reqwest::header::USER_AGENT, update_user_agent());
        for (name, value) in headers {
            // the conditional header belongs to the API endpoint only
            if hop > 0 && *name == reqwest::header::IF_NONE_MATCH {
                continue;
            }
            req = req.header(name.clone(), value.clone());
        }
        let resp = req.send().await?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        let Some(location) = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
        else {
            return Ok(resp);
        };
        let next = current.join(&location)?;
        if !redirect_allowed(&current, &next) {
            anyhow::bail!("the update check was redirected to another host, so it stopped");
        }
        current = next;
    }
    anyhow::bail!("the update check was redirected too many times");
}

/// Read a small response body (API answers, manifests, checksum files)
/// with a size cap: nothing the checker reads this way is anywhere near
/// this big, and an unbounded body is not trusted.
async fn read_capped(resp: reqwest::Response, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
    let bytes = resp.bytes().await?;
    if bytes.len() > max_bytes {
        anyhow::bail!("the update answer is far bigger than expected");
    }
    Ok(bytes.to_vec())
}

const MAX_API_BYTES: usize = 1 << 20;

/// A `Retry-After` in seconds, or the seconds until `X-RateLimit-Reset`
/// (an epoch time), when the answer carries one.
fn retry_after_secs(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    if let Some(v) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
    {
        return Some(v);
    }
    let reset: u64 = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse().ok())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    reset.checked_sub(now)
}

// ---------------------------------------------------------------------------
// ETag cache: the last `releases/latest` answer, kept locally so repeat
// checks cost GitHub nothing when nothing changed.

/// `{"etag": "...", "body": "<the releases/latest JSON verbatim>"}`.
/// The body holds only public release data (tag, asset names and URLs);
/// no user data of any kind. Best effort throughout: a missing or corrupt
/// cache file simply means the next check fetches without conditions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckerCache {
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub body: String,
}

impl CheckerCache {
    pub fn load(path: &Path) -> CheckerCache {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_string(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The check itself.

/// What the GitHub releases API answers for one release.
#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubPayload {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

/// A newer release whose signed manifest verified: ready to offer, once
/// the caller has downloaded and verified the artifact itself.
#[derive(Debug, Clone)]
pub struct CheckedRelease {
    /// Version from the signed manifest (what is offered).
    pub version: String,
    /// The release tag (e.g. "v1.1.0").
    pub tag: String,
    /// Web page of the release (notify-only paths link here).
    pub release_url: String,
    /// The signed manifest.
    pub manifest: Release,
    /// The release's SHA256SUMS text, for the artifact cross-check.
    pub sums_text: String,
    /// Every asset of the release: (file name, download URL).
    pub assets: Vec<(String, String)>,
}

impl CheckedRelease {
    /// Download URL of the Windows installer: the asset of that name when
    /// the release lists it, else the manifest's own URL (which the
    /// manifest restricts to our repository).
    pub fn setup_asset_url(&self) -> String {
        self.assets
            .iter()
            .find(|(n, _)| n == SETUP_NAME)
            .map(|(_, u)| u.clone())
            .unwrap_or_else(|| self.manifest.setup_url())
    }

    /// The AppImage asset of this release, if any: prefer the x86_64
    /// build, else the first `.AppImage` asset.
    pub fn appimage_asset(&self) -> Option<(String, String)> {
        self.assets
            .iter()
            .find(|(n, _)| n.ends_with(".AppImage") && n.contains("x86_64"))
            .or_else(|| self.assets.iter().find(|(n, _)| n.ends_with(".AppImage")))
            .cloned()
    }
}

/// The outcome of one check. `SilentFail` means: say nothing, retry later
/// with backoff. `rate_limited` + `retry_after_secs` carry what the server
/// asked for, when it asked.
#[derive(Debug)]
pub enum CheckOutcome {
    UpToDate,
    Available(CheckedRelease),
    SilentFail {
        rate_limited: bool,
        retry_after_secs: Option<u64>,
    },
}

fn silent() -> CheckOutcome {
    CheckOutcome::SilentFail {
        rate_limited: false,
        retry_after_secs: None,
    }
}

async fn fetch_text(
    client: &reqwest::Client,
    url: &str,
    headers: &[(reqwest::header::HeaderName, String)],
) -> anyhow::Result<(reqwest::StatusCode, reqwest::header::HeaderMap, String)> {
    let resp = open_guarded(client, url, headers).await?;
    let status = resp.status();
    let map = resp.headers().clone();
    let bytes = read_capped(resp, MAX_API_BYTES).await?;
    let text = String::from_utf8(bytes)?;
    Ok((status, map, text))
}

/// Check for a newer release, verifying the signed manifest before
/// anything is offered. `api_url` is `api_latest_url()` in production; it
/// is a parameter only so tests can point it at a local server.
/// `cache_path` holds the ETag cache (see `CheckerCache`).
pub async fn check_latest(
    client: &reqwest::Client,
    api_url: &str,
    cache_path: &Path,
    current_version: &str,
) -> CheckOutcome {
    check_latest_with_key(
        client,
        api_url,
        cache_path,
        current_version,
        UPDATE_PUBLIC_KEY,
    )
    .await
}

/// `check_latest`, verifying against `public_key` instead of the built-in
/// one, so tests can sign fixtures with throwaway keys.
pub async fn check_latest_with_key(
    client: &reqwest::Client,
    api_url: &str,
    cache_path: &Path,
    current_version: &str,
    public_key: &str,
) -> CheckOutcome {
    let mut cache = CheckerCache::load(cache_path);
    let mut headers: Vec<(reqwest::header::HeaderName, String)> = vec![
        (
            reqwest::header::ACCEPT,
            "application/vnd.github+json".to_string(),
        ),
        (
            reqwest::header::HeaderName::from_static("x-github-api-version"),
            "2022-11-28".to_string(),
        ),
    ];
    if let Some(etag) = &cache.etag {
        headers.push((reqwest::header::IF_NONE_MATCH, etag.clone()));
    }
    let (status, resp_headers, body) = match fetch_text(client, api_url, &headers).await {
        Ok(v) => v,
        Err(_) => return silent(),
    };
    let payload_text = if status == reqwest::StatusCode::NOT_MODIFIED {
        if cache.body.is_empty() {
            return silent();
        }
        cache.body.clone()
    } else if status.is_success() {
        // cache the fresh answer only once it parses: a malformed body
        // must not overwrite a good cached one
        let payload: GithubPayload = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(_) => return silent(),
        };
        cache.etag = resp_headers
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string());
        cache.body = body.clone();
        let _ = cache.save(cache_path);
        drop(payload);
        body
    } else if status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return CheckOutcome::SilentFail {
            rate_limited: true,
            retry_after_secs: retry_after_secs(&resp_headers),
        };
    } else {
        return silent();
    };
    let payload: GithubPayload = match serde_json::from_str(&payload_text) {
        Ok(p) => p,
        Err(_) => return silent(),
    };
    // a malformed tag is never an offer
    if parse_semver(&payload.tag_name).is_none() {
        return silent();
    }
    if !is_newer_semver(&payload.tag_name, current_version) {
        return CheckOutcome::UpToDate;
    }
    // the signed manifest of this release: missing or unsigned means
    // no offer, whatever the tag says
    let Some(manifest_url) = payload
        .assets
        .iter()
        .find(|a| a.name == "version.json")
        .map(|a| a.browser_download_url.clone())
    else {
        return silent();
    };
    let Ok((mstatus, _, mtext)) = fetch_text(client, &manifest_url, &[]).await else {
        return silent();
    };
    if !mstatus.is_success() {
        return silent();
    }
    let manifest = match parse(&mtext) {
        Ok(m) => m,
        Err(_) => return silent(),
    };
    if !signature_ok(&manifest, public_key) {
        return silent();
    }
    if parse_semver(&manifest.version).is_none()
        || !is_newer_semver(&manifest.version, current_version)
    {
        return CheckOutcome::UpToDate;
    }
    // the checksum file the artifact is cross-checked against: also part
    // of the same release, also required
    let mut sums_text = None;
    for name in ["SHA256SUMS.txt", "SHA256SUMS-windows.txt"] {
        let Some(sums_url) = payload
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
        else {
            continue;
        };
        match fetch_text(client, &sums_url, &[]).await {
            Ok((sstatus, _, stext)) if sstatus.is_success() => {
                sums_text = Some(stext);
                break;
            }
            Ok((sstatus, _, _)) if sstatus == reqwest::StatusCode::NOT_FOUND => continue,
            _ => return silent(),
        }
    }
    let Some(sums_text) = sums_text else {
        return silent();
    };
    CheckOutcome::Available(CheckedRelease {
        version: manifest.version.clone(),
        tag: payload.tag_name.clone(),
        release_url: if payload.html_url.is_empty() {
            releases_url()
        } else {
            payload.html_url.clone()
        },
        manifest,
        sums_text,
        assets: payload
            .assets
            .into_iter()
            .map(|a| (a.name, a.browser_download_url))
            .collect(),
    })
}

/// How long to wait after a failed check before trying again.
/// Ladder: 5 min, 15 min, 1 h, 6 h, then 24 h (the regular interval).
/// A server-asked wait (`Retry-After` / rate-limit reset) wins, clamped
/// to between 1 minute and 24 hours. The checker itself never retries
/// inside one check, so a rate limit can never turn into a retry storm.
pub fn next_retry_delay(consecutive_failures: u32, retry_after_secs: Option<u64>) -> Duration {
    if let Some(s) = retry_after_secs {
        return Duration::from_secs(s.clamp(60, 24 * 3600));
    }
    const LADDER_SECS: [u64; 5] = [300, 900, 3600, 21_600, 86_400];
    let idx = consecutive_failures.saturating_sub(1) as usize;
    Duration::from_secs(LADDER_SECS[idx.min(LADDER_SECS.len() - 1)])
}

/// The regular interval between successful checks.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

// ---------------------------------------------------------------------------
// Checksum files.

/// Parse `sha256sum`-style lines (`"<hash>  <name>"`, `"*"` for binary
/// mode allowed) into (file name, lowercase hash). Lines that do not
/// match are ignored.
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\\');
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        out.push((
            name.trim_start_matches('*').to_string(),
            hash.to_ascii_lowercase(),
        ));
    }
    out
}

/// The checksum the sums file lists for `filename`, if any.
pub fn sums_hash(sums_text: &str, filename: &str) -> Option<String> {
    parse_sums(sums_text)
        .into_iter()
        .find(|(n, _)| n == filename)
        .map(|(_, h)| h)
}

/// The sums file agrees with `sha256` for `filename`.
pub fn sums_match(sums_text: &str, filename: &str, sha256: &str) -> bool {
    sums_hash(sums_text, filename).is_some_and(|h| h.eq_ignore_ascii_case(sha256.trim()))
}

// ---------------------------------------------------------------------------
// Artifact download + verification, and package detection.

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn file_sha256(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// No update artifact of ours is anywhere near this big.
const MAX_ARTIFACT_BYTES: u64 = 500 << 20;

/// Why an artifact download failed. `Verification` means the bytes that
/// arrived did not match what the signed release promised: the release
/// is not retried eagerly, and nothing is ever offered from those bytes.
/// `Transport` means the network failed and a later retry may succeed.
#[derive(Debug)]
pub enum DownloadError {
    Transport(anyhow::Error),
    Verification(anyhow::Error),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Transport(e) => write!(f, "{e:#}"),
            DownloadError::Verification(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for DownloadError {}

impl DownloadError {
    pub fn is_verification(&self) -> bool {
        matches!(self, DownloadError::Verification(_))
    }
}

/// Download the artifact at `url` into `dir` under `filename`, checking
/// its SHA-256 against BOTH `sha256` (from the signed manifest) and the
/// release's sums file (`sums_text`). A copy that was already downloaded
/// and checks out is reused. Anything that fails a check is deleted and
/// the download is refused — never offered, never installed.
pub async fn download_artifact(
    client: &reqwest::Client,
    url: &str,
    filename: &str,
    sha256: &str,
    sums_text: &str,
    dir: &Path,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<PathBuf, DownloadError> {
    let filename = Path::new(filename)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| {
            DownloadError::Verification(anyhow::anyhow!("the update file name is not usable"))
        })?;
    if !sums_match(sums_text, &filename, sha256) {
        return Err(DownloadError::Verification(anyhow::anyhow!(
            "the release checksum file does not list this update file"
        )));
    }
    std::fs::create_dir_all(dir).map_err(|e| DownloadError::Transport(e.into()))?;
    let dest = dir.join(&filename);
    if dest.exists() {
        match file_sha256(&dest) {
            Ok(got) if got.eq_ignore_ascii_case(sha256.trim()) => return Ok(dest),
            _ => {
                let _ = std::fs::remove_file(&dest);
            }
        }
    }
    // older artifacts are not needed any more
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let resp = open_guarded(client, url, &[])
        .await
        .map_err(DownloadError::Transport)?;
    let resp = resp
        .error_for_status()
        .map_err(|e| DownloadError::Transport(e.into()))?;
    let tmp = dest.with_extension("download");
    let got = match stream_artifact(resp, &tmp, progress).await {
        Ok(hash) => hash,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if !got.eq_ignore_ascii_case(sha256.trim()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(DownloadError::Verification(anyhow::anyhow!(
            "the downloaded update does not match version.json (sha256 {got})"
        )));
    }
    std::fs::rename(&tmp, &dest).map_err(|e| DownloadError::Transport(e.into()))?;
    Ok(dest)
}

/// Stream one artifact response into `tmp`, hashing as it goes. The file
/// handle lives in this function, so a failed stream drops it before the
/// caller deletes the partial file.
async fn stream_artifact(
    mut resp: reqwest::Response,
    tmp: &Path,
    progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
) -> Result<String, DownloadError> {
    use std::io::Write;
    let mut file = std::fs::File::create(tmp).map_err(|e| DownloadError::Transport(e.into()))?;
    let mut h = Sha256::new();
    let mut total: u64 = 0;
    let size = resp.content_length();
    progress(0, size);
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(e) => return Err(DownloadError::Transport(e.into())),
        };
        total += chunk.len() as u64;
        progress(total, size);
        if total > MAX_ARTIFACT_BYTES {
            return Err(DownloadError::Verification(anyhow::anyhow!(
                "the update is far bigger than expected"
            )));
        }
        h.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| DownloadError::Transport(e.into()))?;
    }
    file.flush()
        .map_err(|e| DownloadError::Transport(e.into()))?;
    drop(file);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Run the installer without any window. It closes this program, installs
/// the new version over it and starts it again.
/// `visible`: the setup shows its progress (Help > Check for updates);
/// otherwise nothing is shown (the automatic update).
pub fn run_installer(setup: &Path, sha256: &str, visible: bool) -> anyhow::Result<()> {
    // checked again right before it runs: the file sat on the disk meanwhile
    if !file_sha256(setup)?.eq_ignore_ascii_case(sha256) {
        let _ = std::fs::remove_file(setup);
        anyhow::bail!("the downloaded update was changed on the disk, so it is not installed");
    }
    let mut cmd = std::process::Command::new(setup);
    cmd.arg(if visible { "/SILENT" } else { "/VERYSILENT" });
    cmd.args([
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
        "/AUTOUPDATE=1",
    ]);
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        cmd.arg(format!("/DIR={}", dir.display()));
    }
    cmd.spawn()?;
    Ok(())
}

/// Installed with the installer (not a portable copy): only then the
/// installer may replace the program.
pub fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("unins000.exe").exists()))
        .unwrap_or(false)
}

/// How this copy was packaged on Linux. Only the distinction that changes
/// update behaviour is made: an AppImage can be replaced by a downloaded
/// file, everything else is notify-only. Ambiguous installs fall back to
/// notify-only, which is always safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    AppImage,
    Flatpak,
    /// Installed by the system package manager (.deb/.rpm and the like).
    Managed,
    Unknown,
}

/// Detect the package kind from faked inputs (the environment values and
/// the executable path), so tests can cover every shape without faking
/// the process environment. Order: Flatpak first (its sandbox sets
/// `FLATPAK_ID`), then the `APPIMAGE` path the AppImage runtime sets
/// (only when the file exists), then the runtime mount point, then
/// system install locations. Anything else is `Unknown`.
pub fn detect_package(
    flatpak_id: Option<&str>,
    appimage: Option<&Path>,
    exe_path: &Path,
) -> PackageKind {
    if flatpak_id.is_some_and(|s| !s.trim().is_empty()) {
        return PackageKind::Flatpak;
    }
    if let Some(p) = appimage {
        if !p.as_os_str().is_empty() && p.exists() {
            return PackageKind::AppImage;
        }
    }
    let exe = exe_path.to_string_lossy();
    if exe.starts_with("/tmp/.mount_") {
        return PackageKind::AppImage;
    }
    if let Some(parent) = exe_path.parent() {
        let parent = parent.to_string_lossy();
        if parent.starts_with("/usr/")
            || parent.starts_with("/opt/")
            || parent.starts_with("/snap/")
        {
            return PackageKind::Managed;
        }
    }
    PackageKind::Unknown
}

/// `detect_package` on the real process environment and executable path.
pub fn detect_package_current() -> PackageKind {
    let flatpak = std::env::var("FLATPAK_ID").ok();
    let appimage = std::env::var("APPIMAGE").ok().map(PathBuf::from);
    let exe = std::env::current_exe().unwrap_or_default();
    detect_package(flatpak.as_deref(), appimage.as_deref(), &exe)
}

/// The AppImage file this copy is running from, when it is one.
pub fn current_appimage_path() -> Option<PathBuf> {
    let p = std::env::var("APPIMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())?;
    let p = PathBuf::from(p);
    p.exists().then_some(p)
}

/// Replace the running AppImage file at `current` with the downloaded,
/// verified `new_file`. The hash is checked again first (the file sat on
/// the disk since it was downloaded). The new file is written next to the
/// target and renamed over it, so the running copy keeps its mounted
/// inode and the new version starts on the next launch. A symlink target
/// is resolved first, so launching through a link still replaces the
/// real file. On failure the target is left untouched.
pub fn install_appimage(new_file: &Path, current: &Path, sha256: &str) -> anyhow::Result<()> {
    if !file_sha256(new_file)?.eq_ignore_ascii_case(sha256.trim()) {
        anyhow::bail!("the downloaded AppImage was changed on the disk, so it is not installed");
    }
    let target = std::fs::canonicalize(current).unwrap_or_else(|_| current.to_path_buf());
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("the AppImage has no folder to replace it in"))?
        .to_path_buf();
    let tmp = parent.join(format!(".ezoo-update-{}", std::process::id()));
    std::fs::copy(new_file, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("v1.1", "1.0.9"));
        assert!(is_newer("1.0.10", "1.0.9"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn semver_orders_pre_releases() {
        // the semver ordering chain, in full
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for w in chain.windows(2) {
            assert_eq!(
                compare_semver(w[0], w[1]),
                Some(Ordering::Less),
                "{} < {}",
                w[0],
                w[1]
            );
            assert!(is_newer_semver(w[1], w[0]));
            assert!(!is_newer_semver(w[0], w[1]));
        }
        assert_eq!(compare_semver("1.0.0", "1.0.0"), Some(Ordering::Equal));
        // numeric pre-release identifiers compare by value, not text
        assert_eq!(compare_semver("1.0.0-2", "1.0.0-10"), Some(Ordering::Less));
        // numeric is lower than alphanumeric; build metadata is ignored
        assert_eq!(compare_semver("1.0.0-1", "1.0.0-a"), Some(Ordering::Less));
        assert_eq!(
            compare_semver("1.0.0+build.5", "1.0.0+build.6"),
            Some(Ordering::Equal)
        );
        // leading v, missing parts, equal after padding
        assert_eq!(compare_semver("v1.2", "1.2.0"), Some(Ordering::Equal));
        assert_eq!(compare_semver("1.10.0", "1.9.9"), Some(Ordering::Greater));
        assert!(is_newer_semver("1.1.0-beta", "1.0.9"));
        assert!(!is_newer_semver("1.1.0-beta", "1.1.0"));
        assert!(is_newer_semver("1.1.0", "1.1.0-beta"));
    }

    #[test]
    fn semver_malformed_is_none_and_never_newer() {
        for bad in [
            "",
            "banana",
            "v",
            "1.2.3-",
            "1..2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-alpha..1",
        ] {
            assert_eq!(parse_semver(bad), None, "{bad:?}");
            assert_eq!(compare_semver(bad, "1.0.0"), None);
            assert!(!is_newer_semver(bad, "1.0.0"));
            assert!(!is_newer_semver("2.0.0", bad));
        }
    }

    #[test]
    fn parse_and_url() {
        let sha = sha256_hex(b"abc");
        assert_eq!(
            sha,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let r = parse(&format!(
            "\u{feff}{{\"version\":\"1.0.1\",\"sha256\":\"{sha}\"}}"
        ))
        .unwrap();
        assert_eq!(
            r.setup_url(),
            format!("https://github.com/{REPO}/releases/download/v1.0.1/{SETUP_NAME}")
        );
        let r2 = parse(&format!("{{\"version\":\"1.0.1\",\"url\":\"https://example.invalid/setup\",\"sha256\":\"{sha}\"}}")).unwrap();
        assert!(r2.setup_url().starts_with("https://github.com/"));
        assert!(parse("{\"version\":\"1.0.1\",\"sha256\":\"zz\"}").is_err());
        // the update source is the fork, derived from the one constant
        assert_eq!(REPO, "kerlif/ezoo-download");
        assert_eq!(
            api_latest_url(),
            "https://api.github.com/repos/kerlif/ezoo-download/releases/latest"
        );
        assert_eq!(
            releases_url(),
            "https://github.com/kerlif/ezoo-download/releases"
        );
        assert_eq!(update_user_agent(), format!("EzooDownload/{}", current()));
    }

    fn signed(version: &str, sha: &str) -> (String, String) {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let kp = Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap();
        let public: String = kp
            .public_key()
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let sig: String = kp
            .sign(signed_message(version, sha).as_bytes())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        (public, sig)
    }

    #[test]
    fn signatures() {
        let sha = sha256_hex(b"setup");
        let (public, sig) = signed("1.0.5", &sha);
        let good = Release {
            version: "1.0.5".into(),
            url: String::new(),
            sha256: sha.clone(),
            sig: sig.clone(),
        };
        assert!(signature_ok(&good, &public));
        // another version, another file, no signature, a broken one: refused
        assert!(!signature_ok(
            &Release {
                version: "1.0.6".into(),
                ..good.clone()
            },
            &public
        ));
        assert!(!signature_ok(
            &Release {
                sha256: sha256_hex(b"evil"),
                ..good.clone()
            },
            &public
        ));
        assert!(!signature_ok(
            &Release {
                sig: String::new(),
                ..good.clone()
            },
            &public
        ));
        assert!(!signature_ok(
            &Release {
                sig: "zz".repeat(64),
                ..good.clone()
            },
            &public
        ));
        // signed with another key: refused by the key in the program
        assert!(!signature_ok(&good, UPDATE_PUBLIC_KEY));
        let text = format!("{{\"version\":\"1.0.5\",\"sha256\":\"{sha}\",\"sig\":\"{sig}\"}}");
        assert!(parse_signed(&text).is_err());
        assert_eq!(UPDATE_PUBLIC_KEY.len(), 64);
    }

    #[test]
    fn installer_checked_again_before_it_runs() {
        let dir = std::env::temp_dir().join(format!("ezoo-su-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("setup.exe");
        std::fs::write(&f, b"changed").unwrap();
        let err = run_installer(&f, &sha256_hex(b"original"), false).unwrap_err();
        assert!(format!("{err}").contains("changed"));
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sums_parsing_and_matching() {
        let text = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  EzooDownload-Setup.exe\r\n\
                    0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef *EzooDownload-1.1.0-x86_64.AppImage\n\
                    not-a-hash  junk.txt\n";
        let sha = sha256_hex(b"abc");
        assert!(sums_match(text, "EzooDownload-Setup.exe", &sha));
        assert!(!sums_match(
            text,
            "EzooDownload-Setup.exe",
            &sha256_hex(b"other")
        ));
        assert!(sums_match(
            text,
            "EzooDownload-1.1.0-x86_64.AppImage",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert_eq!(sums_hash(text, "junk.txt"), None);
        assert_eq!(sums_hash(text, "missing.exe"), None);
        assert!(parse_sums("").is_empty());
    }

    #[test]
    fn redirect_policy() {
        let from = reqwest::Url::parse("https://api.github.com/repos/x/y/releases/latest").unwrap();
        let same =
            reqwest::Url::parse("https://api.github.com/repos/x/y/releases/123/assets").unwrap();
        assert!(redirect_allowed(&from, &same));
        let asset = reqwest::Url::parse(
            "https://objects.githubusercontent.com/github-production-release-asset/f",
        )
        .unwrap();
        assert!(redirect_allowed(&from, &asset));
        let other = reqwest::Url::parse("https://evil.example.invalid/f").unwrap();
        assert!(!redirect_allowed(&from, &other));
        let downgrade = reqwest::Url::parse("http://objects.githubusercontent.com/f").unwrap();
        assert!(!redirect_allowed(&from, &downgrade));
        // http origins: same host+port may upgrade, another port may not
        let h1 = reqwest::Url::parse("http://127.0.0.1:1234/a").unwrap();
        let h1s = reqwest::Url::parse("https://127.0.0.1:1234/a").unwrap();
        assert!(redirect_allowed(&h1, &h1s));
        let h2 = reqwest::Url::parse("http://127.0.0.1:5678/a").unwrap();
        assert!(!redirect_allowed(&h1, &h2));
    }

    #[test]
    fn backoff_ladder_and_rate_limit_wait() {
        assert_eq!(next_retry_delay(0, None), Duration::from_secs(300));
        assert_eq!(next_retry_delay(1, None), Duration::from_secs(300));
        assert_eq!(next_retry_delay(2, None), Duration::from_secs(900));
        assert_eq!(next_retry_delay(3, None), Duration::from_secs(3600));
        assert_eq!(next_retry_delay(4, None), Duration::from_secs(21_600));
        assert_eq!(next_retry_delay(5, None), Duration::from_secs(86_400));
        assert_eq!(next_retry_delay(99, None), Duration::from_secs(86_400));
        assert_eq!(next_retry_delay(1, Some(120)), Duration::from_secs(120));
        // a server-asked wait is clamped, never a storm and never forever
        assert_eq!(next_retry_delay(1, Some(5)), Duration::from_secs(60));
        assert_eq!(
            next_retry_delay(1, Some(999_999)),
            Duration::from_secs(86_400)
        );
        assert_eq!(CHECK_INTERVAL, Duration::from_secs(24 * 3600));
    }

    #[test]
    fn cache_round_trips_and_corrupt_means_empty() {
        let dir = std::env::temp_dir().join(format!("ezoo-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("checker-cache.json");
        let c = CheckerCache {
            etag: Some("\"abc123\"".into()),
            body: "{\"tag_name\":\"v9.9.9\"}".into(),
        };
        c.save(&path).unwrap();
        let back = CheckerCache::load(&path);
        assert_eq!(back.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(back.body, "{\"tag_name\":\"v9.9.9\"}");
        std::fs::write(&path, "{broken").unwrap();
        let empty = CheckerCache::load(&path);
        assert!(empty.etag.is_none() && empty.body.is_empty());
        assert!(CheckerCache::load(&dir.join("missing.json"))
            .body
            .is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_detection_from_faked_inputs() {
        let exe = Path::new("/home/user/EzooDownload");
        assert_eq!(
            detect_package(Some("io.github.kerlif.EzooDownload"), None, exe),
            PackageKind::Flatpak
        );
        assert_eq!(detect_package(Some("  "), None, exe), PackageKind::Unknown);
        // a real AppImage path that exists
        let dir = std::env::temp_dir().join(format!("ezoo-pkg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("EzooDownload-1.1.0-x86_64.AppImage");
        std::fs::write(&img, b"fake").unwrap();
        assert_eq!(detect_package(None, Some(&img), exe), PackageKind::AppImage);
        // APPIMAGE set but the file is gone: ambiguous, notify-only
        assert_eq!(
            detect_package(None, Some(&dir.join("gone.AppImage")), exe),
            PackageKind::Unknown
        );
        // the runtime mount point, and system locations
        assert_eq!(
            detect_package(None, None, Path::new("/tmp/.mount_EzooAb12/EzooDownload")),
            PackageKind::AppImage
        );
        assert_eq!(
            detect_package(None, None, Path::new("/usr/bin/EzooDownload")),
            PackageKind::Managed
        );
        assert_eq!(
            detect_package(None, None, Path::new("/opt/ezoo/EzooDownload")),
            PackageKind::Managed
        );
        assert_eq!(
            detect_package(None, None, Path::new("/snap/ezoo/current/EzooDownload")),
            PackageKind::Managed
        );
        assert_eq!(
            detect_package(None, None, Path::new("/home/user/Downloads/EzooDownload")),
            PackageKind::Unknown
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn appimage_replace_rechecks_the_hash_and_keeps_a_bad_file_out() {
        let dir = std::env::temp_dir().join(format!("ezoo-appimg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let current = dir.join("EzooDownload-1.0.0-x86_64.AppImage");
        std::fs::write(&current, b"old").unwrap();
        let new = dir.join("new.AppImage");
        std::fs::write(&new, b"new-bytes").unwrap();
        let sha = sha256_hex(b"new-bytes");
        install_appimage(&new, &current, &sha).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new-bytes");
        // a changed file is refused and the target stays as it was
        std::fs::write(&new, b"tampered").unwrap();
        assert!(install_appimage(&new, &current, &sha).is_err());
        assert_eq!(std::fs::read(&current).unwrap(), b"new-bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
