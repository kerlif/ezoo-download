//! HTTP client, request building and probing a URL.

use crate::filename;
use crate::limiter::SpeedLimiter;
use crate::model::{DownloadKind, ProbeResult, RequestHeaders};
use anyhow::{bail, Context};
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, ACCEPT_RANGES, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH,
    CONTENT_RANGE, CONTENT_TYPE, COOKIE, HOST, LOCATION, PROXY_AUTHORIZATION, RANGE, REFERER,
    USER_AGENT, WWW_AUTHENTICATE,
};
use reqwest::{Client, RequestBuilder, StatusCode};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Live counters of a running download, read by the UI.
#[derive(Default)]
pub struct Progress {
    pub bytes: AtomicU64,
    pub connections: AtomicU32,
    pub parts_total: AtomicU32,
    pub parts_done: AtomicU32,
    pub assembling: AtomicBool,
    /// after the data: what it does now (Step code, 0 = nothing) and how
    /// far (per mille) — the second progress bar
    pub step: AtomicU8,
    pub step_done: AtomicU32,
    /// the connection dropped: waiting for the internet to come back
    pub waiting: AtomicBool,
    /// woken when data flows again, so every waiting part retries at once
    pub back_online: tokio::sync::Notify,
    /// connections the user wants now (0 = as many as the download began
    /// with): changed while it runs, it gets more / fewer at once
    pub want_connections: AtomicU32,
    /// people sharing this torrent (torrents only)
    pub seeders: AtomicU32,
}

impl Progress {
    /// Now doing `step` (None: nothing more).
    pub fn set_step(&self, step: Option<crate::model::Step>) {
        self.step
            .store(step.map(|s| s.code()).unwrap_or(0), Ordering::Relaxed);
        self.step_done.store(0, Ordering::Relaxed);
    }
    /// How far the step is (0..1).
    pub fn step_at(&self, f: f64) {
        self.step_done
            .store((f.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
    }

    /// Data arrived: the connection works (again).
    pub fn online(&self) {
        if self.waiting.swap(false, Ordering::Relaxed) {
            self.back_online.notify_waiters();
        }
    }
}

/// The connection broke while receiving (stall, early close).
#[derive(Debug)]
pub struct NetDrop(pub String);
impl std::fmt::Display for NetDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for NetDrop {}

/// The server answered with an error status.
#[derive(Debug)]
pub struct HttpStatus(pub u16);
impl std::fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "server answered HTTP {}", self.0)
    }
}
impl std::error::Error for HttpStatus {}

/// The server is rate-limiting us (429 Too Many Requests or 503
/// Service Unavailable). Carries the wait the server asked for, if it
/// sent a `Retry-After` header, so the download loop can honour it
/// instead of guessing.
#[derive(Debug)]
pub struct RateLimited {
    pub status: u16,
    pub retry_after: Option<Duration>,
}

impl RateLimited {
    pub fn from_headers(status: u16, headers: &reqwest::header::HeaderMap) -> Self {
        let retry_after = headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| crate::adaptive::parse_retry_after(v, Duration::from_secs(600)));
        RateLimited {
            status,
            retry_after,
        }
    }
}

impl std::fmt::Display for RateLimited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.retry_after {
            Some(d) => write!(
                f,
                "server answered HTTP {} (rate limited, retry after {}s)",
                self.status,
                d.as_secs()
            ),
            None => write!(f, "server answered HTTP {} (rate limited)", self.status),
        }
    }
}
impl std::error::Error for RateLimited {}

/// A problem of the connection (no internet, DNS, reset, timeout), not of
/// the file or the link: worth waiting for and trying again without limit.
pub fn is_network_error(e: &anyhow::Error) -> bool {
    use std::io::ErrorKind as K;
    e.chain().any(|c| {
        if c.is::<NetDrop>() {
            return true;
        }
        if let Some(r) = c.downcast_ref::<reqwest::Error>() {
            return r.status().is_none()
                && !r.is_builder()
                && !r.is_redirect()
                && (r.is_connect()
                    || r.is_timeout()
                    || r.is_request()
                    || r.is_body()
                    || r.is_decode());
        }
        if let Some(io) = c.downcast_ref::<std::io::Error>() {
            return matches!(
                io.kind(),
                K::ConnectionReset
                    | K::ConnectionAborted
                    | K::ConnectionRefused
                    | K::NotConnected
                    | K::TimedOut
                    | K::BrokenPipe
                    | K::UnexpectedEof
                    | K::HostUnreachable
                    | K::NetworkUnreachable
                    | K::NetworkDown
            );
        }
        false
    })
}

/// The server's status code, if the error is one.
pub fn http_status_of(e: &anyhow::Error) -> Option<u16> {
    e.chain()
        .find_map(|c| c.downcast_ref::<HttpStatus>().map(|s| s.0))
}

/// How long to wait before trying again after `n` failures in a row while
/// the connection is down: quick at first, then every 10 seconds.
pub fn reconnect_delay(n: u32) -> Duration {
    Duration::from_secs(match n {
        0 | 1 => 2,
        2 => 3,
        3 => 5,
        4 => 8,
        _ => 10,
    })
}

/// A failure of the server or the transfer (busy server, "HTTP 503", a
/// connection that broke off early) that may well go away by itself: the
/// download tries again later on its own. Not: a link that expired or a
/// file that was removed (401/403/404/410), a login, a full disk.
///
/// Narrow on purpose: only the transient gateway statuses (502/503/504)
/// count as "the server is busy", not every 5xx — a 500/501 is usually the
/// server saying no for good, and re-downloading a huge non-resumable
/// file from byte 0 on it would burn the user's bandwidth for nothing.
/// The text fallback is kept small for the same reason: a bare
/// "connection" matches almost any network sentence.
pub fn is_passing_error(e: &anyhow::Error) -> bool {
    if is_network_error(e) {
        return true;
    }
    if let Some(s) = http_status_of(e) {
        return s == 408 || s == 425 || s == 429 || matches!(s, 502..=504);
    }
    let text = format!("{e:#}").to_ascii_lowercase();
    let lasting = [
        "expired",
        "removed",
        "login",
        "age check",
        "not enough",
        "no room",
        "disk",
        "space",
        "access is denied",
        "permission",
        "bitlocker",
        "not found",
        "did not accept the byte range",
        "web page",
        "missing",
        "cannot",
        "can't",
        "invalid",
    ];
    if lasting.iter().any(|w| text.contains(w)) {
        return false;
    }
    [
        "closed early",
        "ended before",
        "timed out",
        "reset",
        "broken pipe",
        "unexpected eof",
    ]
    .iter()
    .any(|w| text.contains(w))
}

/// The server stopped taking the link (401 / 403 / 410, an expired link):
/// a new link is fetched the way the first one was (the address the
/// download came from, or the video's page), checked, and the download goes
/// on with it from where it was.
pub fn is_link_gone(e: &anyhow::Error) -> bool {
    if matches!(http_status_of(e), Some(401 | 403 | 410)) {
        return true;
    }
    let text = format!("{e:#}").to_ascii_lowercase();
    text.contains("link has expired") || text.contains("not the same file")
}

/// How long to wait before the next try to get a working link: 3 s, 10 s,
/// 30 s, 1 min, 2 min, 5 min.
pub fn relink_delay(n: u32) -> Duration {
    Duration::from_secs(match n {
        0 | 1 => 3,
        2 => 10,
        3 => 30,
        4 => 60,
        5 => 120,
        _ => 300,
    })
}

/// How long to wait before the next try after a passing failure:
/// 15 s, 30 s, 1 min, 2 min, 5 min, then every 10 minutes.
pub fn retry_delay(n: u32) -> Duration {
    Duration::from_secs(match n {
        0 | 1 => 15,
        2 => 30,
        3 => 60,
        4 => 120,
        5 => 300,
        _ => 600,
    })
}

/// Counts an open connection for as long as it lives.
pub struct ConnGuard(Arc<Progress>);
impl ConnGuard {
    pub fn new(p: &Arc<Progress>) -> Self {
        p.connections.fetch_add(1, Ordering::Relaxed);
        ConnGuard(p.clone())
    }
}
impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Things every transfer needs.
pub struct Shared {
    /// replaced when the proxy settings change
    client: RwLock<Client>,
    pub limiter: SpeedLimiter,
}

impl Shared {
    pub fn new(client: Client, limiter: SpeedLimiter) -> Shared {
        Shared {
            client: RwLock::new(client),
            limiter,
        }
    }
    /// The HTTP client (cheap to clone: it is shared inside).
    pub fn client(&self) -> Client {
        self.client.read().unwrap().clone()
    }
    pub fn set_client(&self, c: Client) {
        *self.client.write().unwrap() = c;
    }
}

/// Proxy part of the settings (Options > Proxy / Socks).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProxyConfig {
    /// 0 none, 1 system, 2 manual
    pub mode: u32,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub bypass: String,
}

impl ProxyConfig {
    pub fn from_settings(s: &crate::config::Settings) -> ProxyConfig {
        ProxyConfig {
            mode: s.proxy_mode,
            host: s.proxy_host.trim().to_string(),
            port: s.proxy_port,
            user: s.proxy_user.clone(),
            password: s.proxy_password.clone(),
            bypass: s.proxy_bypass.clone(),
        }
    }
}

/// How long a server may take to answer a request (or to send the next
/// piece of data) before the request is given up and made again.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

pub fn build_client() -> anyhow::Result<Client> {
    build_client_with(&ProxyConfig {
        mode: 1,
        ..Default::default()
    })
}

pub fn build_client_with(proxy: &ProxyConfig) -> anyhow::Result<Client> {
    Ok(base_builder(proxy)?
        // redirects are followed by `SendOrdered::send_ordered`, which puts
        // "Host" first again on every step (see there)
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

/// Client for the update checker: the same proxy handling as
/// `build_client_with`, but redirects are NOT followed automatically —
/// the checker follows them itself in `selfupdate::open_guarded` and
/// refuses cross-host hops. No total timeout: artifact downloads stream
/// through this client too, and a slow one must not be cut off.
pub fn checker_client(proxy: &ProxyConfig) -> anyhow::Result<Client> {
    Ok(base_builder(proxy)?
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

fn base_builder(proxy: &ProxyConfig) -> anyhow::Result<reqwest::ClientBuilder> {
    let mut b = Client::builder()
        // Real separate TCP connections. With HTTP/2 all our
        // "connections" would share one TCP connection and be as slow as one.
        .http1_only()
        // "User-Agent:", not "user-agent:": the names written the way browsers
        // and other download programs write them. Cloudflare's protection
        // refuses lowercase names (HTTP 403, "challenge") even with the
        // browser's own cookies and name.
        .http1_title_case_headers()
        .tcp_nodelay(true)
        .connect_timeout(Duration::from_secs(20))
        // a server that takes the connection and then says nothing (no
        // answer, or the data stops): the request ends instead of waiting
        // for ever, and the download tries again by itself. Per read, so a
        // big file is not cut off: each chunk gets the full wait.
        .read_timeout(ANSWER_TIMEOUT)
        .pool_max_idle_per_host(64);
    match proxy.mode {
        // 1 = system settings: reqwest reads them by itself
        0 => b = b.no_proxy(),
        2 if !proxy.host.is_empty() => {
            let host = proxy
                .host
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            let url = format!("http://{host}:{}", proxy.port);
            let mut p = reqwest::Proxy::all(&url).context("the proxy address is not valid")?;
            if !proxy.user.is_empty() {
                p = p.basic_auth(&proxy.user, &proxy.password);
            }
            let bypass = proxy
                .bypass
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(",");
            p = p.no_proxy(reqwest::NoProxy::from_string(&bypass));
            b = b.proxy(p);
        }
        _ => {}
    }
    Ok(b)
}

/// How many redirects one request follows.
const MAX_REDIRECTS: usize = 15;

/// Sending a request the way browsers and other download programs send it:
/// "Host" is the first line after the request line. The library puts it
/// last, and Cloudflare's protection refuses such a request (HTTP 403,
/// "challenge"), whatever else it carries. The client follows no
/// redirects by itself, so every step of a redirect is sent here too,
/// with the "Host" of its own address.
pub trait SendOrdered {
    fn send_ordered(
        self,
    ) -> impl std::future::Future<Output = reqwest::Result<reqwest::Response>> + Send;
}

impl SendOrdered for RequestBuilder {
    async fn send_ordered(self) -> reqwest::Result<reqwest::Response> {
        let (client, req) = self.build_split();
        let mut req = req?;
        let mut steps = 0;
        loop {
            host_first(&mut req);
            let again = req.try_clone();
            let resp = client.execute(req).await?;
            let status = resp.status();
            if !status.is_redirection() || steps >= MAX_REDIRECTS {
                return Ok(resp);
            }
            let next = resp
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .and_then(|l| resp.url().join(l).ok())
                .filter(|u| matches!(u.scheme(), "http" | "https"));
            let (Some(next), Some(mut n)) = (next, again) else {
                return Ok(resp);
            };
            // 303, and 301 / 302 after a POST: the next step is a plain GET
            if status == StatusCode::SEE_OTHER
                || (matches!(status, StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND)
                    && n.method() == reqwest::Method::POST)
            {
                *n.method_mut() = reqwest::Method::GET;
                *n.body_mut() = None;
                n.headers_mut().remove(CONTENT_TYPE);
                n.headers_mut().remove(CONTENT_LENGTH);
            }
            // another site never gets this site's sign-in — and neither
            // does plain http after an https hop on the same host
            let prev = resp.url();
            if strip_credentials_on_hop(prev, &next) {
                for h in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, WWW_AUTHENTICATE] {
                    n.headers_mut().remove(h);
                }
            }
            // the page that sent us here is the referrer of the next step,
            // the way a browser reports it
            if let Ok(referer) = reqwest::header::HeaderValue::from_str(prev.as_str()) {
                n.headers_mut().insert(REFERER, referer);
            }
            *n.url_mut() = next;
            req = n;
            steps += 1;
        }
    }
}

/// Does a redirect hop from `prev` to `next` need the sign-in headers
/// stripped? Yes on a host or port change, and yes on an https-to-http
/// downgrade even on the same host (the cookie would travel in the clear).
/// An http-to-https upgrade keeps them: the next hop is safer, not worse.
pub fn strip_credentials_on_hop(prev: &reqwest::Url, next: &reqwest::Url) -> bool {
    if next.host_str() != prev.host_str() {
        return true;
    }
    // a downgrade strips whatever the port is (the cookie would go in clear)
    if prev.scheme() == "https" && next.scheme() == "http" {
        return true;
    }
    // same scheme, another port: strip; an upgrade keeps the port change
    // that comes with the scheme (80 -> 443) and keeps the sign-in
    prev.scheme() == next.scheme() && next.port_or_known_default() != prev.port_or_known_default()
}

/// "Host" (with the port when it is not the usual one) as the first header.
/// An IPv6 literal goes in brackets (`[::1]:8080`), or the header is
/// malformed and the server reads the wrong host.
pub fn host_first(req: &mut reqwest::Request) {
    let url = req.url();
    let Some(host) = url.host_str() else {
        return;
    };
    let host = if host.starts_with('[') {
        // `host_str` already brackets an IPv6 literal ("[::1]")
        host.to_string()
    } else if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let value = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };
    let Ok(value) = reqwest::header::HeaderValue::from_str(&value) else {
        return;
    };
    let old = std::mem::take(req.headers_mut());
    let mut h = reqwest::header::HeaderMap::with_capacity(old.len() + 1);
    h.insert(HOST, value);
    for (k, v) in old.iter() {
        if k != HOST {
            h.append(k.clone(), v.clone());
        }
    }
    *req.headers_mut() = h;
}

/// GET request carrying the browser's headers.
pub fn request(client: &Client, url: &str, h: &RequestHeaders, default_ua: &str) -> RequestBuilder {
    request_method(client, url, h, default_ua, reqwest::Method::GET)
}

/// The same request with another method (HEAD for probing).
pub(crate) fn request_method(
    client: &Client,
    url: &str,
    h: &RequestHeaders,
    default_ua: &str,
    method: reqwest::Method,
) -> RequestBuilder {
    let ua = h
        .user_agent
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(default_ua);
    // identity: ranges must refer to the raw bytes, never a compressed body
    let mut rb = client
        .request(method, url)
        .header(USER_AGENT, ua)
        .header(ACCEPT, "*/*")
        .header(ACCEPT_ENCODING, "identity");
    if let Some(r) = h.referer.as_deref().filter(|s| !s.is_empty()) {
        rb = rb.header(REFERER, r);
    }
    if let Some(c) = h.cookies.as_deref().filter(|s| !s.is_empty()) {
        rb = rb.header(COOKIE, c);
    }
    for (k, v) in &h.extra {
        rb = rb.header(k.as_str(), v.as_str());
    }
    if let Some((user, pass)) = h.auth.as_ref().filter(|(u, _)| !u.is_empty()) {
        rb = rb.basic_auth(user, Some(pass));
    }
    rb
}

fn looks_like_hls(url: &str, content_type: Option<&str>) -> bool {
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    if ct.contains("mpegurl") {
        return true;
    }
    url::Url::parse(url)
        .map(|u| u.path().to_ascii_lowercase().ends_with(".m3u8"))
        .unwrap_or(false)
}

/// DASH manifests: a .mpd URL, or the MIME type every DASH player sends.
fn looks_like_dash(url: &str, content_type: Option<&str>) -> bool {
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    if ct.contains("dash+xml") {
        return true;
    }
    url::Url::parse(url)
        .map(|u| u.path().to_ascii_lowercase().ends_with(".mpd"))
        .unwrap_or(false)
}

/// Ways to ask again when a server refuses the request as the browser sent
/// it: some sites refuse a byte range, or cookies / a referrer that belong to
/// the browser's own session. (with range?, headers)
fn fallbacks(h: &RequestHeaders) -> Vec<(bool, RequestHeaders)> {
    let mut out = vec![(true, h.clone()), (false, h.clone())];
    let has = |s: &Option<String>| s.as_deref().map(|v| !v.is_empty()).unwrap_or(false);
    if has(&h.cookies) {
        let mut n = h.clone();
        n.cookies = None;
        out.push((true, n.clone()));
        out.push((false, n));
    }
    if has(&h.cookies) || has(&h.referer) || has(&h.user_agent) {
        let mut n = h.clone();
        n.cookies = None;
        n.referer = None;
        n.user_agent = None;
        out.push((true, n.clone()));
        out.push((false, n));
    }
    out
}

/// Headers of a probe answer, shared by the HEAD and ranged paths.
struct AnswerHeaders {
    content_type: Option<String>,
    last_modified: Option<chrono::DateTime<chrono::Utc>>,
    etag: Option<String>,
    disposition: Option<String>,
    accepts_ranges: bool,
}

impl AnswerHeaders {
    /// Combine two answers for the same file: `other` (the ranged
    /// answer) fills what this one lacks, but never erases a value
    /// this one already has (e.g. a Content-Disposition only the
    /// HEAD answer carried).
    fn merged(self, other: AnswerHeaders) -> Self {
        AnswerHeaders {
            content_type: self.content_type.or(other.content_type),
            last_modified: self.last_modified.or(other.last_modified),
            etag: self.etag.or(other.etag),
            disposition: self.disposition.or(other.disposition),
            accepts_ranges: self.accepts_ranges || other.accepts_ranges,
        }
    }

    fn of(resp: &reqwest::Response) -> Self {
        let h = resp.headers();
        AnswerHeaders {
            content_type: h
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
            last_modified: h
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| chrono::DateTime::parse_from_rfc2822(s.trim()).ok())
                .map(|d| d.with_timezone(&chrono::Utc)),
            // a strong ETag only ("W/..." may stay the same for another
            // file, so it never proves the same version)
            etag: h
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.starts_with("W/")),
            disposition: h
                .get(CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string()),
            accepts_ranges: h
                .get(ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.eq_ignore_ascii_case("bytes"))
                .unwrap_or(false),
        }
    }
}

/// The total from a "Content-Range: bytes 0-0/<total>" answer, if any.
fn range_total(h: &reqwest::header::HeaderMap) -> Option<u64> {
    h.get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.rsplit('/').next())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn finish_probe(
    final_url: String,
    url: &str,
    info: AnswerHeaders,
    total_size: Option<u64>,
    unverified: bool,
    resumable: bool,
    headers_used: Option<RequestHeaders>,
) -> ProbeResult {
    let content_type = info.content_type;
    let name = info
        .disposition
        .as_deref()
        .and_then(filename::from_content_disposition)
        .or_else(|| filename::from_url(&final_url))
        .or_else(|| filename::from_url(url))
        .unwrap_or_else(|| "download".into());
    let kind = if looks_like_hls(&final_url, content_type.as_deref()) {
        DownloadKind::Hls
    } else if looks_like_dash(&final_url, content_type.as_deref()) {
        DownloadKind::Dash
    } else {
        DownloadKind::Http
    };
    let mut file_name = filename::sanitize(&name);
    if kind == DownloadKind::Hls {
        // the stream gets joined into one .ts video file
        let (stem, ext) = filename::split_ext(&file_name);
        if ext.eq_ignore_ascii_case("m3u8") || ext.is_empty() {
            file_name = format!("{stem}.ts");
        }
    } else {
        file_name = filename::ensure_extension(file_name, content_type.as_deref());
        if kind == DownloadKind::Dash && filename::extension(&file_name) == "mpd" {
            // the manifest becomes a video: the tracks are merged into one .mp4
            let (stem, _) = filename::split_ext(&file_name);
            file_name = format!("{stem}.mp4");
        }
    }
    ProbeResult {
        headers_used,
        final_url,
        file_name,
        total_size: if matches!(kind, DownloadKind::Hls | DownloadKind::Dash) {
            None
        } else {
            total_size
        },
        total_size_unverified: unverified
            && !matches!(kind, DownloadKind::Hls | DownloadKind::Dash),
        resumable: kind == DownloadKind::Http && resumable,
        content_type,
        kind,
        last_modified: info.last_modified,
        etag: info.etag,
    }
}

/// Ask the server about a file: real size, final URL after redirects,
/// file name and whether it supports ranges (= resume + many connections).
///
/// The size is treated as UNTRUSTED until a byte-range request confirms
/// it: HEAD is asked first for Content-Length, then a ranged GET
/// (bytes=0-) must agree via its Content-Range total. A server that
/// reports a false Content-Length is corrected by the ranged total; a
/// server that ignores ranges keeps the HEAD length, flagged
/// `total_size_unverified`, so the UI shows it as an estimate.
/// HEAD answers 403/405 (or no Content-Length) fall back to the ranged
/// GET, exactly as before.
///
/// The ranged request is open-ended (`bytes=0-`, like a browser asking
/// to resume): some servers refuse a one-byte range (`bytes=0-0`) but
/// take this. Only the answer's headers are read — the response is
/// dropped at once, without reading the body, so a huge file costs the
/// probe nothing (see the `probe_drops_a_huge_body_at_once` test).
pub async fn probe(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
) -> anyhow::Result<ProbeResult> {
    let mut first_refusal: Option<u16> = None;
    for (with_range, h) in fallbacks(headers) {
        let headers_used = if h != *headers { Some(h.clone()) } else { None };
        // 1. HEAD: cheap, and the only request some servers answer at all.
        let head = request_method(client, url, &h, default_ua, reqwest::Method::HEAD)
            .timeout(Duration::from_secs(5))
            .send_ordered()
            .await;
        if let Ok(resp) = head {
            let status = resp.status();
            if status == StatusCode::UNAUTHORIZED {
                bail!("the server asks for a user name and password (HTTP 401) - use \"Use authorization\" or Options > Site Logins");
            }
            if status.is_success() {
                let info = AnswerHeaders::of(&resp);
                let final_url = resp.url().to_string();
                // reqwest reports a HEAD body as length 0, so read the
                // header itself: it is the size the server claims.
                let claimed = resp
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok());
                drop(resp);
                if let Some(claimed) = claimed {
                    // Verify the claim with one open-ended ranged request:
                    // a 206 total wins over the HEAD length; anything else
                    // leaves the HEAD length standing, flagged untrusted.
                    // The ranged answer's headers win over the HEAD ones:
                    // some servers only send Content-Disposition (or a
                    // real Content-Type) with a ranged answer. The body
                    // is never read: the response is dropped at once.
                    let rb = request(client, url, &h, default_ua)
                        .header(RANGE, "bytes=0-")
                        .timeout(Duration::from_secs(5));
                    let (verified, ranged_info) = match rb.send_ordered().await {
                        Ok(r) => {
                            let total = if r.status() == StatusCode::PARTIAL_CONTENT {
                                range_total(r.headers())
                            } else {
                                None
                            };
                            let info = if r.status().is_success() {
                                Some(AnswerHeaders::of(&r))
                            } else {
                                None
                            };
                            drop(r);
                            (total, info)
                        }
                        Err(_) => (None, None),
                    };
                    let info = match ranged_info {
                        Some(ranged) => info.merged(ranged),
                        None => info,
                    };
                    // Only a 206 proves the server takes byte ranges: a
                    // 200 answer ignored the range, whatever
                    // "Accept-Ranges" claims, so this file gets one
                    // plain connection and no resume.
                    let (total, unverified, resumable) = match verified {
                        Some(total) => (Some(total), false, true),
                        None => (Some(claimed), true, false),
                    };
                    return Ok(finish_probe(
                        final_url,
                        url,
                        info,
                        total,
                        unverified,
                        resumable,
                        headers_used,
                    ));
                }
                // HEAD with no length: the ranged request below decides.
            } else if matches!(status.as_u16(), 403 | 405 | 406 | 416 | 429) {
                first_refusal.get_or_insert(status.as_u16());
            }
        }
        // 2. Ranged GET (the old path): open-ended, headers only — the
        // body is dropped at once, never read (see the doc comment).
        let mut rb = request(client, url, &h, default_ua).timeout(Duration::from_secs(5));
        if with_range {
            rb = rb.header(RANGE, "bytes=0-");
        }
        let resp = rb
            .send_ordered()
            .await
            .context("could not connect to server")?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            bail!("the server asks for a user name and password (HTTP 401) - use \"Use authorization\" or Options > Site Logins");
        }
        // refused (forbidden, range not allowed, too many requests...): try
        // the next way; anything else is the server's real answer
        if matches!(status.as_u16(), 403 | 406 | 416 | 429) {
            first_refusal.get_or_insert(status.as_u16());
            continue;
        }
        if !status.is_success() {
            bail!("server answered HTTP {}", status.as_u16());
        }
        let final_url = resp.url().to_string();
        let info = AnswerHeaders::of(&resp);
        let (total_size, resumable, unverified) = if status == StatusCode::PARTIAL_CONTENT {
            // "bytes 0-123455/123456": the total is confirmed by the range itself
            let total = range_total(resp.headers());
            (total, total.is_some(), false)
        } else {
            // the whole file came, not a part: whatever "Accept-Ranges"
            // says, this server did not take a byte range, so one
            // connection and no resume (parts it would not honour were
            // made again and again before)
            let len = resp.content_length();
            (len, false, len.is_some())
        };
        drop(resp);
        // a server that refused a byte range gets one plain connection
        let resumable = resumable && with_range;
        return Ok(finish_probe(
            final_url,
            url,
            info,
            total_size,
            unverified,
            resumable,
            headers_used,
        ));
    }
    bail!("server answered HTTP {}", first_refusal.unwrap_or(403))
}

/// Ask the server about a file with HEAD requests only — no GET, no body
/// is ever fetched. That is the whole point: a link that works once only
/// (a one-time export link) is not spent by asking, and a huge file
/// costs the check nothing but its headers.
///
/// Redirects are followed by hand (at most MAX_REDIRECTS hops), staying
/// a HEAD on every hop — never turning into a GET the way a generic
/// redirect follower would on a 303. Credentials are stripped on a
/// host/port change and on an https-to-http downgrade, exactly like
/// `send_ordered`, and the referrer is updated per hop. A non-success
/// status is an error carrying `HttpStatus`, so the caller can tell "the
/// server said no" (400/401/403/404/405/410) from "the network failed"
/// (fail open). The size is the HEAD claim only, flagged unverified: no
/// ranged request confirms it here, by design.
pub async fn probe_head(
    client: &Client,
    url: &str,
    headers: &RequestHeaders,
    default_ua: &str,
) -> anyhow::Result<ProbeResult> {
    let mut req = request_method(client, url, headers, default_ua, reqwest::Method::HEAD)
        .timeout(Duration::from_secs(5))
        .build()
        .context("could not build the request")?;
    let mut steps = 0;
    loop {
        host_first(&mut req);
        let again = req.try_clone();
        let resp = client
            .execute(req)
            .await
            .context("could not connect to server")?;
        let status = resp.status();
        if status.is_redirection() && steps < MAX_REDIRECTS {
            let next = resp
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .and_then(|l| resp.url().join(l).ok())
                .filter(|u| matches!(u.scheme(), "http" | "https"));
            if let (Some(next), Some(mut n)) = (next, again) {
                // another site never gets this site's sign-in — and
                // neither does plain http after an https hop
                if strip_credentials_on_hop(resp.url(), &next) {
                    for h in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, WWW_AUTHENTICATE] {
                        n.headers_mut().remove(h);
                    }
                }
                if let Ok(referer) = reqwest::header::HeaderValue::from_str(resp.url().as_str()) {
                    n.headers_mut().insert(REFERER, referer);
                }
                *n.url_mut() = next;
                req = n;
                steps += 1;
                continue;
            }
        }
        if !status.is_success() {
            return Err(anyhow::Error::new(HttpStatus(status.as_u16())));
        }
        let info = AnswerHeaders::of(&resp);
        let final_url = resp.url().to_string();
        // reqwest reports a HEAD body as length 0, so read the header
        // itself: it is the size the server claims.
        let claimed = resp
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        drop(resp);
        return Ok(finish_probe(
            final_url,
            url,
            info,
            claimed,
            claimed.is_some(),
            false,
            None,
        ));
    }
}

#[cfg(test)]
mod refused_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A server that answers 403 to the browser's cookie and to byte ranges.
    async fn picky_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
                    let refused = req.contains("\r\ncookie:") || req.contains("\r\nrange:");
                    let resp = if refused {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\nhello".to_string()
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/tool.bin")
    }

    #[tokio::test]
    async fn probe_falls_back_when_refused() {
        let url = picky_server().await;
        let client = Client::builder().no_proxy().build().unwrap();
        let headers = RequestHeaders {
            cookies: Some("cf=1".into()),
            ..Default::default()
        };
        let p = probe(&client, &url, &headers, "UA").await.unwrap();
        assert_eq!(p.total_size, Some(5));
        assert!(
            !p.resumable,
            "the server refuses ranges: one plain connection"
        );
        let used = p.headers_used.expect("it worked only without the cookie");
        assert!(used.cookies.is_none());
    }
}

#[cfg(test)]
mod passing_tests {
    use super::*;
    #[test]
    fn which_failures_pass() {
        assert!(is_passing_error(&anyhow::Error::new(HttpStatus(503))));
        assert!(is_passing_error(&anyhow::Error::new(HttpStatus(502))));
        assert!(is_passing_error(&anyhow::Error::new(HttpStatus(504))));
        assert!(is_passing_error(&anyhow::Error::new(HttpStatus(429))));
        assert!(!is_passing_error(&anyhow::Error::new(HttpStatus(404))));
        assert!(!is_passing_error(&anyhow::Error::new(HttpStatus(403))));
        // narrowed: a 500/501 is the server saying no, not "busy" — it
        // must not re-download a huge non-resumable file from byte 0
        assert!(!is_passing_error(&anyhow::Error::new(HttpStatus(500))));
        assert!(!is_passing_error(&anyhow::Error::new(HttpStatus(501))));
        assert!(!is_passing_error(&anyhow::Error::new(HttpStatus(507))));
        assert!(is_passing_error(&anyhow::anyhow!(
            "download ended before all parts were received"
        )));
        assert!(!is_passing_error(&anyhow::anyhow!(
            "not enough room on the disk"
        )));
        assert!(!is_passing_error(
            &anyhow::Error::new(HttpStatus(410))
                .context("the link has expired or the file was removed")
        ));
        assert_eq!(retry_delay(1).as_secs(), 15);
        assert_eq!(retry_delay(20).as_secs(), 600);
    }
}

#[cfg(test)]
mod ordered_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn host_first_brackets_ipv6() {
        let client = Client::builder().no_proxy().build().unwrap();
        let mut req = client
            .get("http://[::1]:8080/f.zip")
            .header(USER_AGENT, "t")
            .build()
            .unwrap();
        host_first(&mut req);
        let first = req.headers().keys().next().unwrap().as_str();
        assert_eq!(first, "host");
        assert_eq!(
            req.headers().get(HOST).unwrap().to_str().unwrap(),
            "[::1]:8080"
        );
        // no port: brackets still, no default port added
        let mut req = client
            .get("http://[::1]/f.zip")
            .header(USER_AGENT, "t")
            .build()
            .unwrap();
        host_first(&mut req);
        assert_eq!(req.headers().get(HOST).unwrap().to_str().unwrap(), "[::1]");
        // plain host with an explicit port is unchanged
        let mut req = client
            .get("http://example.com:8080/f.zip")
            .header(USER_AGENT, "t")
            .build()
            .unwrap();
        host_first(&mut req);
        assert_eq!(
            req.headers().get(HOST).unwrap().to_str().unwrap(),
            "example.com:8080"
        );
    }

    #[test]
    fn strip_credentials_on_hop_rules() {
        let https = reqwest::Url::parse("https://example.com/a").unwrap();
        let http_same = reqwest::Url::parse("http://example.com/b").unwrap();
        let https_other = reqwest::Url::parse("https://other.com/b").unwrap();
        let http_other = reqwest::Url::parse("http://other.com/b").unwrap();
        let https_port = reqwest::Url::parse("https://example.com:8443/b").unwrap();
        // downgrade on the same host: strip (the cookie would go in clear)
        assert!(strip_credentials_on_hop(&https, &http_same));
        // another host, either scheme: strip
        assert!(strip_credentials_on_hop(&https, &https_other));
        assert!(strip_credentials_on_hop(&https, &http_other));
        // another port on the same host: strip
        assert!(strip_credentials_on_hop(&https, &https_port));
        // same host, same port, same scheme: keep
        assert!(!strip_credentials_on_hop(
            &https,
            &reqwest::Url::parse("https://example.com/c").unwrap()
        ));
        // upgrade http -> https on the same host: keep
        assert!(!strip_credentials_on_hop(
            &reqwest::Url::parse("http://example.com/a").unwrap(),
            &https
        ));
    }

    /// A server like Cloudflare's protection: lowercase header names are a
    /// program, not a browser, and get HTTP 403; "Host" must be first.
    #[tokio::test]
    async fn header_names_are_written_like_a_browser() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let browser_like = [
                        "User-Agent: ",
                        "Accept: ",
                        "Range: ",
                        "Referer: ",
                        "Cookie: ",
                    ]
                    .iter()
                    .all(|h| req.contains(h))
                        && !req.contains("user-agent:")
                        && req
                            .lines()
                            .nth(1)
                            .is_some_and(|l| l.starts_with("Host: 127.0.0.1:"));
                    let resp = if browser_like {
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 0-4/5\r\nConnection: close\r\n\r\nhello"
                    } else {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let headers = RequestHeaders {
            cookies: Some("a=1".into()),
            referer: Some("https://example.com/".into()),
            ..Default::default()
        };
        let p = probe(
            &client,
            &format!("http://{addr}/f.zip"),
            &headers,
            "Mozilla/5.0 test",
        )
        .await
        .unwrap();
        assert_eq!(p.total_size, Some(5));
        assert!(
            p.headers_used.is_none(),
            "the first, full request was taken"
        );
    }

    /// A redirect to another site: "Host" first and right on every step,
    /// and the first site's cookie stays with the first site.
    #[tokio::test]
    async fn redirects_to_another_site_keep_host_first() {
        let second = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port2 = second.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = second.accept().await {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let ok = req.lines().nth(1) == Some(&format!("Host: localhost:{port2}"))
                    && !req.contains("Cookie:");
                let resp = if ok {
                    "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello"
                        .to_string()
                } else {
                    "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                };
                let _ = s.write_all(resp.as_bytes()).await;
            }
        });
        let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = first.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = first.accept().await {
                let mut buf = vec![0u8; 8192];
                let _ = s.read(&mut buf).await;
                let resp = format!("HTTP/1.1 302 Found\r\nLocation: http://localhost:{port2}/real.zip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = s.write_all(resp.as_bytes()).await;
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let headers = RequestHeaders {
            cookies: Some("only=first".into()),
            ..Default::default()
        };
        let p = probe(
            &client,
            &format!("http://{addr}/go"),
            &headers,
            "Mozilla/5.0 test",
        )
        .await
        .unwrap();
        assert_eq!(p.total_size, Some(5));
        assert!(p.final_url.ends_with("/real.zip"));
    }

    /// An empty Location wastes no hop: the redirect answer itself comes back.
    #[tokio::test]
    async fn empty_location_ends_the_redirect_chain() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let _ = s.read(&mut buf).await;
                    let _ = s
                        .write_all(b"HTTP/1.1 302 Found\r\nLocation: \r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                });
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let resp = client
            .get(format!("http://{addr}/go"))
            .send_ordered()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 302);
    }

    /// The probe's open-ended range must not drink a huge body: the server
    /// trickles a 5 MB file in 64 KB chunks, and the probe must be back
    /// with the size long before the body arrives.
    #[tokio::test]
    async fn probe_drops_a_huge_body_at_once() {
        use std::sync::Arc;
        let total: u64 = 5 * 1024 * 1024;
        let written = Arc::new(AtomicU64::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let w2 = written.clone();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let w3 = w2.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let head = req.starts_with("HEAD");
                    let ranged = req.contains("Range: bytes=0-");
                    if head {
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n"
                        );
                        let _ = s.write_all(resp.as_bytes()).await;
                        return;
                    }
                    if !ranged {
                        let _ = s
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .await;
                        return;
                    }
                    let resp = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{total}\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n",
                        total - 1
                    );
                    if s.write_all(resp.as_bytes()).await.is_err() {
                        return;
                    }
                    // trickle the body: if the probe read it, this takes ~16 s
                    let chunk = vec![b'x'; 64 * 1024];
                    let mut sent: u64 = 0;
                    while sent < total {
                        match s.write_all(&chunk).await {
                            Ok(()) => {
                                sent += chunk.len() as u64;
                                w3.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            }
                            Err(_) => break,
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                });
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let start = std::time::Instant::now();
        let p = probe(
            &client,
            &format!("http://{addr}/big.bin"),
            &RequestHeaders::default(),
            "test",
        )
        .await
        .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(p.total_size, Some(total));
        assert!(p.resumable, "206 proves ranges");
        assert!(
            elapsed < Duration::from_secs(5),
            "probe took {elapsed:?} for a 5 MB body it never reads"
        );
        // the server only got its first chunk(s) out before the probe hung up
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            written.load(Ordering::Relaxed) < 1024 * 1024,
            "probe drank {} bytes of a body it only needs headers of",
            written.load(Ordering::Relaxed)
        );
    }

    /// A weak ETag proves nothing: it is dropped, and a strong one from the
    /// ranged answer is what the probe keeps.
    #[tokio::test]
    async fn probe_weak_etag_is_ignored() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let resp = if req.starts_with("HEAD") {
                        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nETag: W/\"abc\"\r\nConnection: close\r\n\r\n"
                    } else {
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-4/5\r\nETag: \"strong1\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\n"
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let p = probe(
            &client,
            &format!("http://{addr}/f.bin"),
            &RequestHeaders::default(),
            "test",
        )
        .await
        .unwrap();
        assert_eq!(p.etag.as_deref(), Some("\"strong1\""));
    }

    /// A server that answers 200 to a range request did not take the range,
    /// whatever "Accept-Ranges" claims: one connection, no resume.
    #[tokio::test]
    async fn probe_whole_file_is_not_resumable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let resp = if req.starts_with("HEAD") {
                        "HTTP/1.1 200 OK\r\nContent-Length: 777\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    } else {
                        "HTTP/1.1 200 OK\r\nContent-Length: 777\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\nhello"
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        let client = build_client_with(&ProxyConfig {
            mode: 0,
            ..Default::default()
        })
        .unwrap();
        let p = probe(
            &client,
            &format!("http://{addr}/f.bin"),
            &RequestHeaders::default(),
            "test",
        )
        .await
        .unwrap();
        assert_eq!(p.total_size, Some(777));
        assert!(!p.resumable, "a 200 answer ignored the range");
        assert!(p.total_size_unverified);
    }
}

#[cfg(test)]
mod head_tests {
    use super::*;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A server that answers HEAD only, and records every method it saw:
    /// the check must never turn into a GET (that would spend a
    /// one-time link and fetch a body it never reads).
    async fn head_server() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let seen = seen2.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let method = head.split_whitespace().next().unwrap_or("").to_string();
                    let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                    seen.lock().unwrap().push(method.clone());
                    let resp = match (method.as_str(), path.as_str()) {
                        (_, "/file.zip") => "HTTP/1.1 200 OK\r\nContent-Length: 12345\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n".to_string(),
                        (_, "/gone") => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                        (_, "/signin") => "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                        (_, "/page") => "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n".to_string(),
                        (_, "/hop") => format!("HTTP/1.1 302 Found\r\nLocation: http://{addr}/file.zip\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
                        (_, "/slow") => {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                            "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n".to_string()
                        }
                        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    };
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        (format!("http://{addr}"), seen)
    }

    fn client() -> Client {
        Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn head_only_reports_a_file_without_fetching_it() {
        let (base, seen) = head_server().await;
        let p = probe_head(
            &client(),
            &format!("{base}/file.zip"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap();
        assert_eq!(p.total_size, Some(12345));
        assert!(
            p.total_size_unverified,
            "HEAD length is never range-confirmed"
        );
        assert!(!p.resumable, "ranges are not proven by a HEAD");
        assert!(!p.is_web_page());
        assert_eq!(p.file_name, "file.zip");
        assert!(
            seen.lock().unwrap().iter().all(|m| m == "HEAD"),
            "no GET: {:?}",
            seen.lock().unwrap()
        );
    }

    #[tokio::test]
    async fn head_only_reports_refusals_with_their_status() {
        let (base, _) = head_server().await;
        let e = probe_head(
            &client(),
            &format!("{base}/gone"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap_err();
        assert_eq!(http_status_of(&e), Some(404));
        let e = probe_head(
            &client(),
            &format!("{base}/signin"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap_err();
        assert_eq!(http_status_of(&e), Some(401));
    }

    #[tokio::test]
    async fn head_only_sees_a_web_page() {
        let (base, _) = head_server().await;
        let p = probe_head(
            &client(),
            &format!("{base}/page"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap();
        assert!(p.is_web_page(), "a sign-in page is not a file");
    }

    #[tokio::test]
    async fn head_only_redirects_stay_head() {
        let (base, seen) = head_server().await;
        let p = probe_head(
            &client(),
            &format!("{base}/hop"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap();
        assert_eq!(p.total_size, Some(12345), "followed to the file");
        assert_eq!(
            seen.lock().unwrap().len(),
            2,
            "one hop: {:?}",
            seen.lock().unwrap()
        );
        assert!(
            seen.lock().unwrap().iter().all(|m| m == "HEAD"),
            "no GET on the hop: {:?}",
            seen.lock().unwrap()
        );
    }

    #[tokio::test]
    async fn head_only_times_out_instead_of_hanging() {
        let (base, _) = head_server().await;
        let e = probe_head(
            &client(),
            &format!("{base}/slow"),
            &RequestHeaders::default(),
            "UA",
        )
        .await
        .unwrap_err();
        assert_eq!(
            http_status_of(&e),
            None,
            "a timeout is not a refusal: {e:#}"
        );
    }
}
