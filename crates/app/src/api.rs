//! Tiny local HTTP API on 127.0.0.1 used by the browser extension.
//!
//! Only the extension can talk to it: requests must carry the `X-SwiftGet`
//! header (a web page cannot add custom headers to a cross-origin request
//! without a CORS preflight, which we never approve) and any Origin header
//! must belong to a browser extension. POSTs must also carry the
//! per-install token (`X-SwiftGet-Token`, see local_token.rs): the app
//! hands it to the extension through GET /config, and a web page cannot
//! read that answer. GET /ping and /config stay open (liveness, and the
//! token bootstrap). The token is required on every POST, with no
//! fallback: the 1.1.1 extension sends it on all of them (see
//! `token_strict`).

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

/// Addresses the user chose to download with the browser itself (the server
/// refused Ezoo Download): the extension leaves them to the browser once.
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

/// "Download all links with Ezoo Download" from the browser's right click menu.
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
/// true = it said it will. New in 1.1.1, so the token is required
/// from day one (Q29, no legacy fallback): the caller reads it from
/// the token file, like the program itself does at startup.
pub fn notify_quit(port: u16, token: &str) -> bool {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
    else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!("POST /quit HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\nX-SwiftGet-Token: {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
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

/// Another Ezoo Download is already running on this port: ask it to show itself.
/// The per-install token (Q29) goes on the POST, like every other POST.
pub fn notify_existing(port: u16, token: &str) -> Existing {
    let Ok(mut s) =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(800))
    else {
        return Existing::None;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!("POST /show HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\nX-SwiftGet-Token: {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
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

/// Another Ezoo Download is already running: hand it the torrent to open.
/// New in 1.1.1, so the token is required from day one (Q29, no legacy
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
    let req = format!(
        "POST /open-torrent HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\nX-SwiftGet-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
) {
    std::thread::Builder::new()
        .name("ezoo-api".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                handle(req, &config, &on_event, &token);
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

/// The token, and nothing else: every POST to this API requires the
/// per-install token. No origin is consulted here and no token-less
/// caller passes — the 1.1.0 extension's legacy fallback (a POST from a
/// browser-extension origin, or with no Origin, taken on the X-SwiftGet
/// header alone) was removed with the Phase 3 extension, which sends the
/// token on every POST.
fn token_strict(token_header: Option<&str>, token: &str) -> bool {
    token_header == Some(token)
}

fn token_ok(req: &Request, token: &str) -> bool {
    token_strict(header(req, "X-SwiftGet-Token").as_deref(), token)
}

/// Video look-ups running now (each one is a yt-dlp process).
static RESOLVING: AtomicUsize = AtomicUsize::new(0);
const MAX_RESOLVING: usize = 3;

fn handle(
    mut req: Request,
    config: &impl Fn() -> String,
    on_event: &impl Fn(ApiEvent),
    token: &str,
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
    // POSTs prove they are our add-in with the per-install token; GET
    // /ping and /config stay open (liveness, and the token bootstrap).
    // No fallback: a POST without the token is refused, whatever its
    // Origin (the 1.1.1 extension sends the token on every POST).
    if *req.method() == Method::Post && !token_ok(&req, token) {
        return reply(req, 403, r#"{"error":"forbidden"}"#);
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
        (Method::Post, "/links") => {
            let mut body = String::new();
            if req
                .as_reader()
                .take(16 << 20)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
            match serde_json::from_str::<LinksRequest>(&body) {
                Ok(mut l) => {
                    l.links.retain(|x| is_http(&x.url));
                    on_event(ApiEvent::Links(l));
                    reply(req, 200, r#"{"ok":true}"#)
                }
                Err(_) => reply(req, 400, r#"{"error":"invalid request"}"#),
            }
        }
        (Method::Post, "/browser") => {
            // should the browser keep this download? (see leave_to_browser)
            let mut body = String::new();
            if req
                .as_reader()
                .take(1 << 16)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
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
        (Method::Post, "/open-torrent") => {
            let mut body = String::new();
            if req
                .as_reader()
                .take(1 << 16)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
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
            let mut body = String::new();
            if req
                .as_reader()
                .take(1 << 16)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
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
        (Method::Post, "/add") => {
            let mut body = String::new();
            if req
                .as_reader()
                .take(1 << 20)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
            match serde_json::from_str::<AddRequest>(&body) {
                Ok(add)
                    if (is_http(&add.url) || swiftget_engine::torrent::is_magnet(&add.url))
                        && add.audio_url.as_deref().map(is_http).unwrap_or(true) =>
                {
                    on_event(ApiEvent::Add(add));
                    reply(req, 200, r#"{"ok":true}"#)
                }
                _ => reply(req, 400, r#"{"error":"invalid request"}"#),
            }
        }
        (Method::Post, "/resolve") | (Method::Post, "/youtube") => {
            // download choices for any page with a video (yt-dlp, a few
            // seconds), answered from its own thread so nothing else waits
            let mut body = String::new();
            if req
                .as_reader()
                .take(4 << 20)
                .read_to_string(&mut body)
                .is_err()
            {
                return reply(req, 400, r#"{"error":"bad body"}"#);
            }
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
        serve(
            server,
            || format!(r#"{{"ok":true,"localToken":"{TEST_TOKEN}"}}"#),
            move |ev| {
                let _ = tx.send(ev);
            },
            TEST_TOKEN.to_string(),
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

    fn post(path: &str, origin: Option<&str>, token: Option<&str>, body: &str) -> String {
        let origin = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        let token = token
            .map(|t| format!("X-SwiftGet-Token: {t}\r\n"))
            .unwrap_or_default();
        format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nX-SwiftGet: 1\r\n{origin}{token}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    fn post_without_token_is_refused() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(port, &post("/add", Some(EXT_ORIGIN), None, ADD_BODY));
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "refused POST got CORS: {resp}");
    }

    #[test]
    fn post_with_foreign_origin_is_refused_even_with_token() {
        let (port, _rx) = start_test_server();
        let resp = raw_request(
            port,
            &post("/add", Some(EVIL_ORIGIN), Some(TEST_TOKEN), ADD_BODY),
        );
        assert!(status_of(&resp).contains("403"), "{resp}");
        assert!(!has_cors(&resp), "refused POST got CORS: {resp}");
    }

    #[test]
    fn post_with_token_and_extension_origin_succeeds() {
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
    fn quit_and_open_torrent_need_the_token() {
        let t = "a".repeat(64);
        assert!(token_strict(Some(&t), &t));
        // no token, a wrong token, or an add-in origin without one:
        // all refused — there is no legacy fallback here
        assert!(!token_strict(None, &t));
        assert!(!token_strict(Some("wrong"), &t));
        assert!(!token_strict(None, ""));
    }

    #[test]
    fn every_post_needs_the_token() {
        let t = "0123456789abcdef".repeat(4);
        // the right token passes
        assert!(token_strict(Some(&t), &t));
        // a missing token is refused — the 1.1.0 fallback (no token from a
        // browser-extension origin, or with no Origin at all) is gone:
        // the token is the only thing that passes, on every POST
        assert!(!token_strict(None, &t));
        // a wrong token never passes
        assert!(!token_strict(Some("wrong"), &t));
        assert!(!token_strict(Some(""), &t));
    }
}
