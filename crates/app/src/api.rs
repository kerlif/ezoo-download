//! Tiny local HTTP API on 127.0.0.1 used by the browser extension.
//!
//! Naming note (P8-bis): the `X-SwiftGet` header, the old
//! `X-SwiftGet-Token` bearer, and the `/swiftget-*` API paths are the
//! frozen wire-protocol name — they are NOT renamed, because the
//! released 1.1.1 extension speaks them and the 1.1.2 app must keep
//! answering them. The product name is "Ezoo Download Manager
//! (EDM)"; the internal crate names (`swiftget`, `swiftget-engine`)
//! and the gettext domain (`swiftget.po`) are also kept — the binary
//! is EzooDownload, so no user ever sees them.

//! Only the extension can talk to it: requests must carry the `X-SwiftGet`
//! header (a web page cannot add custom headers to a cross-origin request
//! without a CORS preflight, which we never approve) and any Origin header
//! must belong to a browser extension. POSTs must also carry an HMAC
//! signature (`X-Ezoo-Time` / `X-Ezoo-Nonce` / `X-Ezoo-Sig`, see
//! api_sign.rs) over the method, path, time, nonce and body, keyed by
//! the per-install token: the app hands the token to the extension
//! through GET /config, and a web page cannot read that answer. The
//! old `X-SwiftGet-Token` bearer header is gone (it was replayable
//! forever): a POST without a valid signature is refused, with no
//! fallback. GET /ping and /config stay open (liveness, and the
//! token bootstrap). /quit and /show need a valid signature too.
//!
//! POST /check asks whether the program can fetch a URL the browser
//! started downloading (see `check_url`): a HEAD-only look with the
//! browser's sign-in, so a link that works once only is never spent by
//! asking. It answers what the program knows — direct file, HLS/DASH
//! stream, or a sign-in page — and whether the browser should keep it.

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

/// Addresses the user chose to download with the browser itself (the server
/// refused Ezoo Download Manager): the extension leaves them to the browser once.
static FOR_BROWSER: std::sync::Mutex<Vec<(String, std::time::Instant)>> =
    std::sync::Mutex::new(Vec::new());

/// Version of the browser add-in that ships with this program.
pub fn bundled_extension_version() -> String {
    serde_json::from_str::<serde_json::Value>(include_str!("../../../extension/manifest.json"))
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string))
        .unwrap_or_default()
}

pub fn leave_to_browser(url: &str) {
    let mut l = FOR_BROWSER.lock().unwrap();
    l.retain(|(_, at)| at.elapsed() < Duration::from_secs(300));
    l.push((url.to_string(), std::time::Instant::now()));
}

fn take_for_browser(urls: &[String]) -> bool {
    let mut l = FOR_BROWSER.lock().unwrap();
    l.retain(|(_, at)| at.elapsed() < Duration::from_secs(300));
    if let Some(i) = l.iter().position(|(u, _)| urls.iter().any(|x| x == u)) {
        l.remove(i);
        return true;
    }
    false
}

#[derive(Deserialize)]
struct BrowserQuery {
    urls: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    /// YouTube style: separate audio stream to merge with `url`
    #[serde(default)]
    pub audio_url: Option<String>,
    /// extra headers the links need (from yt-dlp)
    #[serde(default)]
    pub headers: Option<Vec<(String, String)>>,
    /// YouTube page + quality key, so expired links can be renewed
    #[serde(default)]
    pub source_page: Option<String>,
    #[serde(default)]
    pub source_key: Option<String>,
    /// the quality the overlay offered for this download (e.g. an HLS
    /// variant like "1080p"), stored on the record for a later quality
    /// window (Q26 plumbing)
    #[serde(default)]
    pub quality_key: Option<String>,
    /// cut only this part of the video (seconds; exact = re-encode).
    /// An invalid part is a 400 — it never falls back to a silent
    /// whole-file download.
    #[serde(default)]
    pub clip: Option<swiftget_engine::Clip>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRequest {
    url: String,
    /// the site's cookies, used only if it asks for a login
    #[serde(default)]
    cookie_list: Vec<swiftget_engine::ytdlp::BrowserCookie>,
    /// a stream found on a page: that page
    #[serde(default)]
    referer: Option<String>,
}

/// "Download all links with Ezoo Download Manager" from the browser's right click menu.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinksRequest {
    #[serde(default)]
    pub page: String,
    pub links: Vec<PageLink>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageLink {
    pub url: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug)]
pub enum ApiEvent {
    Add(AddRequest),
    Links(LinksRequest),
    Show,
    /// "Download all" on a playlist page: open the playlist window with it
    Playlist(String),
    /// a .torrent file / magnet link opened from Windows (double click):
    /// the torrent window with it
    OpenTorrent(String),
    /// the setup of a new version wants the program closed (it saves
    /// everything first; the downloads go on after the update)
    Quit,
}

/// Ask the copy of the program that is open to close (for the setup).
/// true = it said it will. The request is HMAC-signed with the token
/// (Q53, no legacy fallback): the caller reads the token from the
/// token file, like the program itself does at startup.
pub fn notify_quit(port: u16, token: &str) -> bool {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let signed = signed_headers(token, "POST", "/quit", b"");
    let req = format!("POST /quit HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{signed}Content-Length: 0\r\nConnection: close\r\n\r\n");
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.contains("Ezoo")
}

/// What Windows may hand over to open: a magnet link or a .torrent file
/// that is there. Anything else is refused.
pub fn torrent_source_ok(s: &str) -> bool {
    let s = s.trim();
    swiftget_engine::torrent::is_magnet(s)
        || (s.to_ascii_lowercase().ends_with(".torrent") && std::path::Path::new(s).is_file())
}

/// What the extension needs to know from Options (file types to take,
/// sites to leave alone, keys, the video panel) plus the per-install
/// token it must show on every POST (Q29).
pub fn config_json(s: &swiftget_engine::Settings, token: &str) -> String {
    let split = |t: &str| {
        t.split_whitespace()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
    };
    serde_json::json!({
        "ok": true,
        "localToken": token,
        "minExt": bundled_extension_version(),
        "captureExts": split(&s.capture_exts.to_ascii_lowercase()),
        "skipSites": split(&s.skip_sites),
        "skipAddresses": s.skip_addresses,
        "skipKey": s.skip_key,
        "forceKey": s.force_key,
        "mp3Kbps": s.mp3_kbps,
        "panel": {
            "enabled": s.panel_enabled,
            "skipSites": split(&s.panel_skip_sites),
            "minVideoKb": s.panel_min_video_kb,
            "minAudioKb": s.panel_min_audio_kb,
        },
    })
    .to_string()
}

pub fn bind(port: u16) -> Result<Server, String> {
    Server::http(("127.0.0.1", port)).map_err(|e| e.to_string())
}

/// Another Ezoo Download Manager is already running on this port: ask it to show itself.
/// The request is HMAC-signed with the per-install token (Q53),
/// like every other POST.
pub fn notify_existing(port: u16, token: &str) -> Existing {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
    else {
        return Existing::None;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let signed = signed_headers(token, "POST", "/show", b"");
    let req = format!("POST /show HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{signed}Content-Length: 0\r\nConnection: close\r\n\r\n");
    if s.write_all(req.as_bytes()).is_err() {
        return Existing::None;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    if !(buf.contains("Ezoo") || buf.starts_with("HTTP/1.1 200")) {
        return Existing::None;
    }
    let body = buf.split("\r\n\r\n").nth(1).unwrap_or_default();
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) if v["hangs"].as_bool() == Some(true) => {
            Existing::Hangs(v["pid"].as_u64().unwrap_or(0) as u32)
        }
        _ => Existing::Shown,
    }
}

/// Another Ezoo Download Manager is already running: hand it the torrent to open.
/// The request is HMAC-signed with the token (Q53, no legacy
/// fallback): the caller reads it from the token file, like the
/// program itself does at startup.
pub fn notify_open_torrent(port: u16, token: &str, source: &str) -> bool {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let body = serde_json::json!({ "source": source }).to_string();
    let signed = signed_headers(token, "POST", "/open-torrent", body.as_bytes());
    let req = format!(
        "POST /open-torrent HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{signed}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.1 200")
}

/// Seconds (since 1970) when the window side of the program last moved; the
/// main timer sets it a few times a second. Far behind = the windows hang.
pub static UI_ALIVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// a message box / file picker of ours is open (the windows wait for it)
pub static UI_WAITS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ui_hangs() -> bool {
    use std::sync::atomic::Ordering;
    let last = UI_ALIVE.load(Ordering::Relaxed);
    last != 0 && !UI_WAITS.load(Ordering::Relaxed) && now_secs().saturating_sub(last) > 30
}

/// What the copy of the program that is open already said.
#[derive(Debug, PartialEq)]
pub enum Existing {
    /// nobody (or something else) answers on the port
    None,
    /// it brings its window to the front
    Shown,
    /// it answers, but its windows hang (this is its process id)
    Hangs(u32),
}

pub fn serve(
    server: Server,
    config: impl Fn() -> String + Send + 'static,
    on_event: impl Fn(ApiEvent) + Send + 'static,
    token: String,
    rt: tokio::runtime::Handle,
) {
    std::thread::Builder::new()
        .name("ezoo-api".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &config, &on_event, &token, &rt);
            }
        })
        .expect("failed to start API thread");
}

fn is_http(u: &str) -> bool {
    u.starts_with("http://") || u.starts_with("https://")
}

fn header(req: &Request, name: &'static str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

fn reply(req: Request, code: u16, body: &str) {
    let resp = Response::from_string(body)
        .with_status_code(code)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    let _ = req.respond(resp);
}

/// The request was made to this computer by its own name. A web page that
/// points its own domain at 127.0.0.1 (DNS rebinding) sends its domain here.
fn host_allowed(host: Option<&str>) -> bool {
    let Some(host) = host else { return true }; // HTTP/1.0: no Host at all
    let h = host.trim().to_ascii_lowercase();
    let name = if let Some(rest) = h.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        h.split(':').next().unwrap_or("")
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// Only these may call the API (a web page has an http(s) origin).
fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(o) => {
            o.starts_with("chrome-extension://")
                || o.starts_with("moz-extension://")
                || o.starts_with("extension://")
        }
    }
}

/// The HMAC headers for an internal POST, signed with the token
/// (Q53): `X-Ezoo-Time` / `X-Ezoo-Nonce` / `X-Ezoo-Sig` over
/// `METHOD\npath\ntime\nnonce\n<body>`.
fn signed_headers(token: &str, method: &str, path: &str, body: &[u8]) -> String {
    let time = now_secs();
    let nonce = crate::api_sign::new_nonce();
    let sig = crate::api_sign::sign(token, method, path, time, &nonce, body);
    format!("X-Ezoo-Time: {time}\r\nX-Ezoo-Nonce: {nonce}\r\nX-Ezoo-Sig: {sig}\r\n")
}

/// The signature, and nothing else: every POST to this API requires a
/// valid HMAC over the method, path, time, nonce and body, keyed by
/// the per-install token. No origin is consulted here and no
/// unsigned caller passes — the 1.1.1 bearer header
/// (`X-SwiftGet-Token`) is gone: it was replayable forever, and a
/// request carrying only it is refused like any unsigned one.
fn signature_ok(req: &Request, path: &str, body: &[u8], token: &str) -> bool {
    let time = header(req, "X-Ezoo-Time").unwrap_or_default();
    let nonce = header(req, "X-Ezoo-Nonce").unwrap_or_default();
    let sig = header(req, "X-Ezoo-Sig").unwrap_or_default();
    crate::api_sign::verify(crate::api_sign::Signed {
        token,
        method: "POST",
        path,
        time: &time,
        nonce: &nonce,
        sig: &sig,
        body,
        now: now_secs(),
    })
}

/// Video look-ups running now (each one is a yt-dlp process).
static RESOLVING: AtomicUsize = AtomicUsize::new(0);
const MAX_RESOLVING: usize = 3;

/// What the program knows about a URL, from a HEAD-only look.
/// `ok` is false only when the program is sure it cannot fetch it: the
/// browser then keeps the download. Anything uncertain (a timeout, a
/// network error, the checker being busy) fails open with `ok: true`,
/// so a slow server never loses a download to the browser.
struct CheckVerdict {
    ok: bool,
    kind: &'static str,
    file_name: Option<String>,
    size: Option<u64>,
    resumable: bool,
    needs_browser: bool,
    reason: String,
}

/// The verdict from a HEAD-only probe (`probe_head` never fetches a
/// body, so a one-time link is not spent by asking):
/// - a file / stream the probe reached: take it, with what it knows
///   (kind, name, claimed size, whether it resumes)
/// - a sign-in page instead of a file: the browser keeps it
/// - a definite refusal (400/401/403/404/405/410): the browser keeps it
/// - anything else (timeout, network error): fail open, take it
fn check_verdict(found: anyhow::Result<swiftget_engine::ProbeResult>) -> CheckVerdict {
    match found {
        Ok(p) => {
            let kind = match p.kind {
                swiftget_engine::DownloadKind::Hls => "hls",
                swiftget_engine::DownloadKind::Dash => "dash",
                _ => "file",
            };
            let webpage = p.is_web_page();
            CheckVerdict {
                ok: !webpage,
                kind,
                file_name: Some(p.file_name),
                size: p.total_size,
                resumable: p.resumable,
                needs_browser: webpage,
                reason: if webpage {
                    "webpage".into()
                } else {
                    "ok".into()
                },
            }
        }
        Err(e) => match swiftget_engine::http::http_status_of(&e) {
            Some(code @ (400 | 401 | 403 | 404 | 405 | 410)) => CheckVerdict {
                ok: false,
                kind: "unknown",
                file_name: None,
                size: None,
                resumable: false,
                needs_browser: true,
                reason: format!("http{code}"),
            },
            _ => CheckVerdict {
                ok: true,
                kind: "unknown",
                file_name: None,
                size: None,
                resumable: false,
                needs_browser: false,
                reason: "unknown".into(),
            },
        },
    }
}

/// The /check look itself: the browser's sign-in (cookies, referrer,
/// user agent, the headers its own request carried) against the URL,
/// HEAD only, 2500 ms at most. Answered from the app's runtime, never
/// a fresh runtime per check.
async fn check_url(add: &AddRequest) -> String {
    let mut headers = swiftget_engine::RequestHeaders {
        referer: add.referrer.clone(),
        cookies: add.cookies.clone(),
        user_agent: add.user_agent.clone(),
        extra: Vec::new(),
        auth: None,
    };
    if let Some(list) = &add.headers {
        swiftget_engine::ytdlp::apply_headers(&mut headers, list);
    }
    let url = add.url.clone();
    let found = match swiftget_engine::http::build_client() {
        Ok(client) => {
            let look = swiftget_engine::http::probe_head(&client, &url, &headers, "Mozilla/5.0");
            match tokio::time::timeout(Duration::from_millis(2500), look).await {
                Ok(found) => found,
                Err(_) => {
                    return serde_json::json!({"ok": true, "reason": "unknown"}).to_string();
                }
            }
        }
        Err(_) => {
            return serde_json::json!({"ok": true, "reason": "unknown"}).to_string();
        }
    };
    let v = check_verdict(found);
    serde_json::json!({
        "ok": v.ok,
        "kind": v.kind,
        "fileName": v.file_name,
        "size": v.size,
        "resumable": v.resumable,
        "needsBrowser": v.needs_browser,
        "reason": v.reason,
    })
    .to_string()
}

/// /check looks running now (each one is a HEAD probe on the app runtime).
static CHECKING: AtomicUsize = AtomicUsize::new(0);
const MAX_CHECKING: usize = 2;

fn handle(
    mut req: Request,
    config: &impl Fn() -> String,
    on_event: &impl Fn(ApiEvent),
    token: &str,
    rt: &tokio::runtime::Handle,
) {
    // never answer CORS preflights: web pages must not reach this API
    if *req.method() == Method::Options {
        return reply(req, 403, r#"{"error":"forbidden"}"#);
    }
    if !host_allowed(header(&req, "Host").as_deref()) {
        return reply(req, 403, r#"{"error":"forbidden"}"#);
    }
    if !origin_allowed(header(&req, "Origin").as_deref()) {
        return reply(req, 403, r#"{"error":"forbidden"}"#);
    }
    if header(&req, "X-SwiftGet").is_none() {
        return reply(req, 403, r#"{"error":"missing header"}"#);
    }
    let path = req.url().split('?').next().unwrap_or("").to_string();
    // POSTs prove they are our add-in with an HMAC signature over the
    // method, path, time, nonce and body (Q53); GET /ping and /config
    // stay open (liveness, and the token bootstrap). No fallback: a
    // POST without a valid signature is refused, whatever its Origin
    // (the 1.1.2 extension signs every POST; the 1.1.1 bearer header
    // alone no longer passes). The body is read before the check
    // because the signature covers it.
    let mut body = String::new();
    if *req.method() == Method::Post {
        if req
            .as_reader()
            .take(16 << 20)
            .read_to_string(&mut body)
            .is_err()
        {
            return reply(req, 400, r#"{"error":"bad body"}"#);
        }
        if !signature_ok(&req, &path, body.as_bytes(), token) {
            return reply(req, 403, r#"{"error":"forbidden"}"#);
        }
    }
    match (req.method().clone(), path.as_str()) {
        (Method::Get, "/ping") => {
            // "ext": the add-in version that came with this program; an older
            // add-in loaded from the program's folder reloads itself
            let body = format!(
                r#"{{"app":"EzooDownload","version":"{}","ext":"{}"}}"#,
                env!("CARGO_PKG_VERSION"),
                bundled_extension_version()
            );
            reply(req, 200, &body)
        }
        (Method::Get, "/config") => reply(req, 200, &config()),
        (Method::Post, "/links") => match serde_json::from_str::<LinksRequest>(&body) {
            Ok(mut l) => {
                l.links.retain(|x| is_http(&x.url));
                on_event(ApiEvent::Links(l));
                reply(req, 200, r#"{"ok":true}"#)
            }
            Err(_) => reply(req, 400, r#"{"error":"invalid request"}"#),
        },
        (Method::Post, "/browser") => {
            // should the browser keep this download? (see leave_to_browser)
            let keep = serde_json::from_str::<BrowserQuery>(&body)
                .map(|q| take_for_browser(&q.urls))
                .unwrap_or(false);
            reply(
                req,
                200,
                if keep {
                    r#"{"browser":true}"#
                } else {
                    r#"{"browser":false}"#
                },
            )
        }
        (Method::Post, "/check") => {
            // the browser started a download: can the program get it too?
            // Links that work once only (the browser just used them) or
            // only with the page's own sign-in stay in the browser
            // instead of failing here. Answered from the app's runtime
            // (no fresh runtime per check); at most MAX_CHECKING looks
            // run at once, and a busy checker fails open — the extension
            // treats any unclear answer as "take it".
            let Ok(add) = serde_json::from_str::<AddRequest>(&body) else {
                return reply(req, 400, r#"{"error":"invalid request"}"#);
            };
            if !is_http(&add.url) {
                return reply(req, 400, r#"{"error":"not a web address"}"#);
            }
            if CHECKING.fetch_add(1, Ordering::SeqCst) >= MAX_CHECKING {
                CHECKING.fetch_sub(1, Ordering::SeqCst);
                return reply(req, 200, r#"{"ok":true,"reason":"busy"}"#);
            }
            let rt = rt.clone();
            rt.spawn(async move {
                struct Done;
                impl Drop for Done {
                    fn drop(&mut self) {
                        CHECKING.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                let _done = Done;
                let body = check_url(&add).await;
                reply(req, 200, &body);
            });
        }
        (Method::Post, "/open-torrent") => {
            match serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["source"].as_str().map(String::from))
            {
                Some(s) if torrent_source_ok(&s) => {
                    on_event(ApiEvent::OpenTorrent(s.trim().to_string()));
                    reply(req, 200, r#"{"ok":true}"#)
                }
                _ => reply(req, 400, r#"{"error":"invalid request"}"#),
            }
        }
        (Method::Post, "/quit") => {
            on_event(ApiEvent::Quit);
            reply(req, 200, r#"{"ok":true,"app":"Ezoo"}"#)
        }
        (Method::Post, "/show") => {
            if ui_hangs() {
                let body = format!(
                    r#"{{"ok":true,"app":"Ezoo","hangs":true,"pid":{}}}"#,
                    std::process::id()
                );
                return reply(req, 200, &body);
            }
            on_event(ApiEvent::Show);
            reply(req, 200, r#"{"ok":true,"app":"Ezoo"}"#)
        }
        (Method::Post, "/playlist") => {
            match serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["url"].as_str().map(String::from))
            {
                Some(url) if is_http(&url) => {
                    on_event(ApiEvent::Playlist(url));
                    reply(req, 200, r#"{"ok":true}"#)
                }
                _ => reply(req, 400, r#"{"error":"invalid request"}"#),
            }
        }
        (Method::Post, "/add") => match serde_json::from_str::<AddRequest>(&body) {
            Ok(add)
                if (is_http(&add.url) || swiftget_engine::torrent::is_magnet(&add.url))
                    && add.audio_url.as_deref().map(is_http).unwrap_or(true)
                    && add
                        .clip
                        .as_ref()
                        .map(|c| c.validate().is_ok())
                        .unwrap_or(true) =>
            {
                on_event(ApiEvent::Add(add));
                reply(req, 200, r#"{"ok":true}"#)
            }
            _ => reply(req, 400, r#"{"error":"invalid request"}"#),
        },
        (Method::Post, "/resolve") | (Method::Post, "/youtube") => {
            // download choices for any page with a video (yt-dlp, a few
            // seconds), answered from its own thread so nothing else waits
            let Ok(r) = serde_json::from_str::<ResolveRequest>(&body) else {
                return reply(req, 400, r#"{"error":"invalid request"}"#);
            };
            if !is_http(&r.url) {
                return reply(req, 400, r#"{"error":"not a web address"}"#);
            }
            if RESOLVING.fetch_add(1, Ordering::SeqCst) >= MAX_RESOLVING {
                RESOLVING.fetch_sub(1, Ordering::SeqCst);
                return reply(
                    req,
                    429,
                    r#"{"ok":false,"error":"busy, try again in a moment"}"#,
                );
            }
            std::thread::spawn(move || {
                struct Done;
                impl Drop for Done {
                    fn drop(&mut self) {
                        RESOLVING.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                let _done = Done;
                let cookies = (!r.cookie_list.is_empty()).then_some(r.cookie_list.as_slice());
                let referer = r.referer.as_deref().filter(|u| is_http(u));
                let body = match swiftget_engine::ytdlp::resolve_for(&r.url, cookies, referer) {
                    Ok(info) => serde_json::json!({
                        "ok": true, "title": info.title, "site": info.site, "options": info.options
                    }),
                    Err(e) => {
                        serde_json::json!({ "ok": false, "error": crate::err_text(&format!("{e:#}")) })
                    }
                };
                reply(req, 200, &body.to_string());
            });
        }
        _ => reply(req, 404, r#"{"error":"not found"}"#),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::sync::mpsc;
    use std::time::Duration;

    /// The token the test server enforces (64 hex chars, like a real
    /// per-install token).
    const TEST_TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// Start a real server on an ephemeral loopback port; returns the
    /// port and a receiver for the events the handlers emit.
    fn start_test_server() -> (u16, mpsc::Receiver<ApiEvent>) {
        let server = bind(0).expect("bind test server");
        let port = match server.server_addr() {
            tiny_http::ListenAddr::IP(addr) => addr.port(),
            #[cfg(unix)]
            tiny_http::ListenAddr::Unix(_) => panic!("expected an IP socket"),
        };
        let (tx, rx) = mpsc::channel();
        // the /check arm answers from this runtime; it is leaked on
        // purpose: the tests end with the process
        let rt = tokio::runtime::Runtime::new().expect("test runtime");
        let handle = rt.handle().clone();
        std::mem::forget(rt);
        serve(
            server,
            // the real /config body, so the test server advertises what
            // the program does (minExt included)
            || config_json(&swiftget_engine::Settings::default(), TEST_TOKEN),
            move |ev| {
                let _ = tx.send(ev);
            },
            TEST_TOKEN.to_string(),
            handle,
        );
        (port, rx)
    }

    /// One raw HTTP request over a fresh connection; returns the whole
    /// response (status line + headers + body) as text.
    fn raw_request(port: u16, req: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(req.as_bytes()).expect("write");
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match s.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn status_of(resp: &str) -> &str {
        resp.lines().next().unwrap_or("")
    }

    fn has_cors(resp: &str) -> bool {
        resp.to_ascii_lowercase()
            .contains("access-control-allow-origin")
    }

    fn get(path: &str, origin: Option<&str>) -> String {
        let origin = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{origin}Connection: close\r\n\r\n")
    }

    /// A POST signed the way the extension signs it: HMAC over the
    /// method, path, time, nonce and body, keyed by `key`. `None`
    /// sends no signature headers at all.
    fn post(path: &str, origin: Option<&str>, key: Option<&str>, body: &str) -> String {
        let origin = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        let sig = key
            .map(|k| {
                let time = now_secs();
                let nonce = crate::api_sign::new_nonce();
                let sig = crate::api_sign::sign(k, "POST", path, time, &nonce, body.as_bytes());
                format!("X-Ezoo-Time: {time}\r\nX-Ezoo-Nonce: {nonce}\r\nX-Ezoo-Sig: {sig}\r\n")
            })
            .unwrap_or_default();
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{origin}{sig}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// A POST whose signature was computed over `signed_body` but
    /// which actually sends `sent_body` (a tampered body).
    fn post_tampered(path: &str, signed_body: &str, sent_body: &str) -> String {
        let time = now_secs();
        let nonce = crate::api_sign::new_nonce();
        let sig = crate::api_sign::sign(
            TEST_TOKEN,
            "POST",
            path,
            time,
            &nonce,
            signed_body.as_bytes(),
        );
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\nOrigin: {EXT_ORIGIN}\r\nX-Ezoo-Time: {time}\r\nX-Ezoo-Nonce: {nonce}\r\nX-Ezoo-Sig: {sig}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sent_body}",
            sent_body.len()
        )
    }

    /// A POST carrying only the old 1.1.1 bearer header, no signature.
    fn post_bearer_only(path: &str, body: &str) -> String {
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\nOrigin: {EXT_ORIGIN}\r\nX-SwiftGet-Token: {TEST_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    const ADD_BODY: &str = r#"{"url":"https://example.com/file.bin"}"#;
    const EXT_ORIGIN: &str = "chrome-extension://abcdef";
    const EVIL_ORIGIN: &str = "https://evil.example";

    #[test]
    fn foreign_origin_cannot_read_config() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(port, &get("/config", Some(EVIL_ORIGIN)));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "foreign origin got CORS: {resp}");
        assert!(!resp.contains("localToken"), "token leaked: {resp}");
    }

    #[test]
    fn foreign_origin_gets_no_cors_on_ping() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(port, &get("/ping", Some(EVIL_ORIGIN)));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "foreign origin got CORS: {resp}");
    }

    #[test]
    fn preflight_is_never_answered() {
        let (port, _rx) = start_test_server();
        let req = format!(
            "OPTIONS /add HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: {EVIL_ORIGIN}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: x-swiftget-token\r\nConnection: close\r\n\r\n"
        );
        let resp = raw_request(port, &req);
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "preflight got CORS: {resp}");
    }

    #[test]
    fn post_without_signature_is_refused() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(port, &post("/add", Some(EXT_ORIGIN), None, ADD_BODY));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "refused POST got CORS: {resp}");
    }

    #[test]
    fn post_with_foreign_origin_is_refused_even_when_signed() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(
            port,
            &post("/add", Some(EVIL_ORIGIN), Some(TEST_TOKEN), ADD_BODY),
        );
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "refused POST got CORS: {resp}");
    }

    #[test]
    fn post_with_valid_signature_succeeds() {
        let (port, rx) = start_test_server();
        let resp = raw_request(
            port,
            &post("/add", Some(EXT_ORIGIN), Some(TEST_TOKEN), ADD_BODY),
        );
        assert!(status_of(&resp).contains("200"), "{resp}");
        assert!(!has_cors(&resp), "even a 200 carries no CORS: {resp}");
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ApiEvent::Add(add)) => assert_eq!(add.url, "https://example.com/file.bin"),
            other => panic!("expected an Add event, got {other:?}"),
        }
    }

    #[test]
    fn add_with_an_invalid_clip_is_refused_not_a_full_download() {
        let (port, rx) = start_test_server();
        let body = r#"{"url":"https://example.com/video.mp4","clip":{"start":10.0,"end":5.0,"exact":false}}"#;
        let resp = raw_request(
            port,
            &post("/add", Some(EXT_ORIGIN), Some(TEST_TOKEN), body),
        );
        assert!(status_of(&resp).contains("400"), "{resp}");
        assert!(
            rx.recv_timeout(Duration::from_millis(500)).is_err(),
            "an invalid clip must never reach the download list"
        );
    }

    #[test]
    fn add_with_a_valid_clip_reaches_the_download() {
        let (port, rx) = start_test_server();
        let body = r#"{"url":"https://example.com/video.mp4","clip":{"start":10.0,"end":20.0,"exact":true}}"#;
        let resp = raw_request(
            port,
            &post("/add", Some(EXT_ORIGIN), Some(TEST_TOKEN), body),
        );
        assert!(status_of(&resp).contains("200"), "{resp}");
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ApiEvent::Add(add)) => {
                assert_eq!(
                    add.clip,
                    Some(swiftget_engine::Clip {
                        start: 10.0,
                        end: 20.0,
                        exact: true
                    })
                );
            }
            other => panic!("expected an Add event with a clip, got {other:?}"),
        }
    }

    #[test]
    fn config_open_to_extension_origin_and_local_clients() {
        let (port, _rx) = start_test_server();
        // the extension's own origin (its privileged background context)
        let resp = raw_request(port, &get("/config", Some(EXT_ORIGIN)));
        assert!(status_of(&resp).contains("200"), "{resp}");
        assert!(resp.contains("localToken"), "{resp}");
        assert!(!has_cors(&resp), "even a 200 carries no CORS: {resp}");
        // no Origin at all (curl / a local process)
        let resp = raw_request(port, &get("/config", None));
        assert!(status_of(&resp).contains("200"), "{resp}");
        assert!(resp.contains("localToken"), "{resp}");
    }

    #[test]
    fn only_this_computer_by_its_own_name() {
        for ok in [
            None,
            Some("127.0.0.1:9614"),
            Some("localhost:9614"),
            Some("127.0.0.1"),
            Some("[::1]:9614"),
        ] {
            assert!(host_allowed(ok), "{ok:?}");
        }
        // DNS rebinding: the attacker's own name pointed at 127.0.0.1
        for bad in [
            Some("evil.example:9614"),
            Some("127.0.0.1.evil.example"),
            Some("localhost.evil:9614"),
            Some(""),
        ] {
            assert!(!host_allowed(bad), "{bad:?}");
        }
    }

    #[test]
    fn only_browser_add_ins() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("chrome-extension://abcdef")));
        assert!(origin_allowed(Some("moz-extension://1234-5678")));
        assert!(!origin_allowed(Some("https://evil.example")));
        assert!(!origin_allowed(Some("null")));
    }

    #[test]
    fn old_bearer_header_alone_is_refused() {
        // the 1.1.1 extension's bearer header, without a signature,
        // no longer passes: it was replayable forever
        let (port, rx) = start_test_server();
        let resp = raw_request(port, &post_bearer_only("/add", ADD_BODY));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "bearer-only POST reached the handler"
        );
    }

    #[test]
    fn tampered_body_is_refused() {
        // signed over one body, sent with another: the signature
        // covers the body, so the swap fails
        let (port, rx) = start_test_server();
        let tampered = r#"{"url":"https://evil.example/pwn.bin"}"#;
        let resp = raw_request(port, &post_tampered("/add", ADD_BODY, tampered));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "tampered POST reached the handler"
        );
    }

    #[test]
    fn replayed_request_is_refused() {
        // the exact same signed bytes twice: the first passes, the
        // second finds its nonce already spent
        let (port, rx) = start_test_server();
        let req = post("/add", Some(EXT_ORIGIN), Some(TEST_TOKEN), ADD_BODY);
        let first = raw_request(port, &req);
        assert!(status_of(&first).contains("200"), "{first}");
        let second = raw_request(port, &req);
        assert!(status_of(&second).contains("403"), "{second}");
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ApiEvent::Add(add)) => assert_eq!(add.url, "https://example.com/file.bin"),
            other => panic!("expected one Add event, got {other:?}"),
        }
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "the replay reached the handler too"
        );
    }

    #[test]
    fn quit_and_show_without_signature_are_refused() {
        let (port, rx) = start_test_server();
        for path in ["/quit", "/show"] {
            let resp = raw_request(port, &post(path, Some(EXT_ORIGIN), None, ""));
            assert!(status_of(&resp).contains("403"), "{path}: {resp}");
        }
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "an unsigned /quit or /show reached the handler"
        );
    }

    #[test]
    fn signed_quit_and_show_are_accepted() {
        let (port, rx) = start_test_server();
        let resp = raw_request(port, &post("/show", Some(EXT_ORIGIN), Some(TEST_TOKEN), ""));
        assert!(status_of(&resp).contains("200"), "{resp}");
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ApiEvent::Show) => (),
            other => panic!("expected a Show event, got {other:?}"),
        }
        let resp = raw_request(port, &post("/quit", Some(EXT_ORIGIN), Some(TEST_TOKEN), ""));
        assert!(status_of(&resp).contains("200"), "{resp}");
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(ApiEvent::Quit) => (),
            other => panic!("expected a Quit event, got {other:?}"),
        }
    }

    /// A server that answers HEAD requests (the /check look is HEAD
    /// only): a file, a gone link, a sign-in page, a web page, a slow
    /// server, and a redirect.
    fn head_server() -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut s = s;
                    let mut buf = [0u8; 4096];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = head.split_whitespace().nth(1).unwrap_or("");
                    let answer = match path {
                        "/file.zip" => "HTTP/1.1 200 OK\r\nContent-Length: 12345\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n".to_string(),
                        "/gone" => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                        "/signin" => "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                        "/page" => "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n".to_string(),
                        "/hop" => format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/file.zip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                        "/slow" => {
                            std::thread::sleep(Duration::from_secs(30));
                            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n".to_string()
                        }
                        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    };
                    let _ = s.write_all(answer.as_bytes());
                });
            }
        });
        port
    }

    /// The /check cap is process-global, so the check tests run one at a
    /// time here: a slow check would otherwise make its neighbours
    /// answer "busy" and fail for no reason of their own.
    static CHECK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// POST /check for a path on the HEAD server; returns the JSON body.
    fn check(port: u16, head_port: u16, path: &str) -> serde_json::Value {
        let _guard = CHECK_LOCK.lock().unwrap();
        let body = format!(r#"{{"url":"http://127.0.0.1:{head_port}{path}"}}"#);
        let resp = raw_request(
            port,
            &post("/check", Some(EXT_ORIGIN), Some(TEST_TOKEN), &body),
        );
        assert!(status_of(&resp).contains("200"), "{path}: {resp}");
        let json = resp.split("\r\n\r\n").nth(1).unwrap_or("");
        serde_json::from_str(json).expect("check answer is JSON")
    }

    #[test]
    fn check_takes_a_plain_file_with_what_it_knows() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/file.zip");
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["kind"], "file", "{v}");
        assert_eq!(v["fileName"], "file.zip", "{v}");
        assert_eq!(v["size"], 12345, "{v}");
        assert_eq!(v["needsBrowser"], false, "{v}");
    }

    #[test]
    fn check_follows_a_redirect_to_the_file() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/hop");
        assert_eq!(v["ok"], true, "{v}");
        assert_eq!(v["size"], 12345, "{v}");
    }

    #[test]
    fn check_leaves_a_gone_link_in_the_browser() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/gone");
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["needsBrowser"], true, "{v}");
        assert_eq!(v["reason"], "http404", "{v}");
    }

    #[test]
    fn check_leaves_a_sign_in_page_in_the_browser() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/signin");
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["reason"], "http401", "{v}");
    }

    #[test]
    fn check_leaves_a_web_page_in_the_browser() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/page");
        assert_eq!(v["ok"], false, "{v}");
        assert_eq!(v["needsBrowser"], true, "{v}");
        assert_eq!(v["reason"], "webpage", "{v}");
    }

    #[test]
    fn check_fails_open_when_the_server_is_slow() {
        let (port, _rx) = start_test_server();
        let hp = head_server();
        let v = check(port, hp, "/slow");
        assert_eq!(v["ok"], true, "slow to answer: take it: {v}");
        assert_eq!(v["reason"], "unknown", "{v}");
    }

    #[test]
    fn check_without_signature_is_refused() {
        let (port, rx) = start_test_server();
        let body = r#"{"url":"https://example.com/file.zip"}"#;
        let resp = raw_request(port, &post("/check", Some(EXT_ORIGIN), None, body));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(
            rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "an unsigned /check reached the handler"
        );
    }

    #[test]
    fn check_rejects_a_bad_url() {
        let (port, _rx) = start_test_server();
        let body = r#"{"url":"ftp://example.com/file.zip"}"#;
        let resp = raw_request(
            port,
            &post("/check", Some(EXT_ORIGIN), Some(TEST_TOKEN), body),
        );
        assert!(status_of(&resp).contains("400"), "{resp}");
    }

    #[test]
    fn config_advertises_the_minimum_extension_version() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(port, &get("/config", Some(EXT_ORIGIN)));
        assert!(status_of(&resp).contains("200"), "{resp}");
        assert!(resp.contains("\"minExt\""), "no minExt in /config: {resp}");
    }
}
