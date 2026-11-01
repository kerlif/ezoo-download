//! One reusable local HTTP server for the engine integration tests
//! (`download.rs`, `adaptive.rs`, `update_checker.rs`).
//!
//! Before this module, each of those files carried its own copy of the
//! same accept loop + request parser + response writer, with the faults
//! (dropped lines, 429s, stalls, truncated bodies, expiring links)
//! baked into the copy. That is the duplication the Zajil comparison
//! (Q34, `ZAJIL-COMPARISON.md` P14) calls out: a shared testserver
//! with canned behaviours, so a fix to the harness is made once.

//! The module is compiled into each test binary separately, and each
//! binary uses only a subset of it (for example `update_checker.rs`
//! never builds a `StreamResponse`); the unused pieces are dead code
//! in that binary only, not in the module, so dead-code lints are off
//! here on purpose.

#![allow(dead_code)]
//!
//! The shape, deliberately kept close to what `update_checker.rs`
//! already had:
//! * `spawn(handler)` binds an ephemeral 127.0.0.1 port, serves one
//!   request per connection (`Connection: close`), and returns the
//!   port plus shared `Stats` (hits per path, last headers per path,
//!   live/peak concurrent connections).
//! * A handler is any `Fn(&Request) -> R` where `R: Into<Action>`:
//!   a plain `Response` (whole body, one write), a `StreamResponse`
//!   (chunked, with per-chunk delays, holds, and a per-chunk stop
//!   hook for the flaky/busy/expiring servers), or `Action::Drop`
//!   (close without answering, as if the line went down).
//! * `Stats` keeps the field layout `update_checker.rs` used, so its
//!   assertions (`stats.lock().unwrap().hits.get(...)`) are unchanged.
//!
//! Behaviour notes, kept identical to the servers this replaces:
//! * `Content-Length` is the body length unless `content_length` is
//!   set: several fault servers deliberately advertise the full range
//!   length while sending only part of it, and the client must see
//!   that lie (a truncated body), not a short-but-complete response.
//! * A per-chunk hook runs after the chunk's delay; returning `false`
//!   stops the response there and closes the connection.
//! * `hold_before` sleeps before any byte is sent (a silent server);
//!   `hold_after` sleeps after the last byte, holding the socket.
//! * Only loopback, only for tests: no auth, no TLS, no keep-alive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// A parsed HTTP request: method, path, headers (keys as sent), body.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// First header with this name, case-insensitive, as sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Parses a `Range: bytes=a-b` header. The end may be open
    /// (`bytes=100-`), which yields `None`; a malformed header yields
    /// `None` instead of panicking, unlike the old per-file parsers.
    pub fn range(&self) -> Option<(usize, Option<usize>)> {
        let v = self.header("range")?;
        let v = v.trim().to_ascii_lowercase();
        let v = v.strip_prefix("bytes=")?;
        let (a, b) = v.split_once('-')?;
        let a: usize = a.trim().parse().ok()?;
        let b = if b.trim().is_empty() {
            None
        } else {
            Some(b.trim().parse().ok()?)
        };
        Some((a, b))
    }
}

/// A whole response, written in one go: headers, then the body.
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// A response written chunk by chunk, for slow / flaky / stalling
/// servers. All knobs default to "send it all at once".
pub struct StreamResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Bytes per write; default 8192, as the old servers used.
    pub chunk_size: usize,
    /// Delay after each chunk, given (offset in body, chunk length).
    pub chunk_delay: Arc<dyn Fn(usize, usize) -> Duration + Send + Sync>,
    /// Sleep before any byte is sent (a silent server).
    pub hold_before: Duration,
    /// Sleep after the last byte, holding the socket open.
    pub hold_after: Duration,
    /// Called after each chunk with the bytes just written; `false`
    /// stops the response there and closes the connection.
    pub on_chunk: Option<Arc<dyn Fn(usize) -> bool + Send + Sync>>,
    /// Advertised `Content-Length`, when it must differ from the body
    /// length (a fault server that cuts the body short).
    pub content_length: Option<usize>,
}

impl StreamResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            chunk_size: 8192,
            chunk_delay: Arc::new(|_, _| Duration::ZERO),
            hold_before: Duration::ZERO,
            hold_after: Duration::ZERO,
            on_chunk: None,
            content_length: None,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn chunk_size(mut self, n: usize) -> Self {
        self.chunk_size = n;
        self
    }

    /// The same delay after every chunk.
    pub fn chunk_delay(mut self, delay: Duration) -> Self {
        self.chunk_delay = Arc::new(move |_, _| delay);
        self
    }

    /// Delay after each chunk, from its offset and length.
    pub fn chunk_delay_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(usize, usize) -> Duration + Send + Sync + 'static,
    {
        self.chunk_delay = Arc::new(f);
        self
    }

    pub fn hold_before(mut self, delay: Duration) -> Self {
        self.hold_before = delay;
        self
    }

    pub fn hold_after(mut self, delay: Duration) -> Self {
        self.hold_after = delay;
        self
    }

    pub fn on_chunk<F>(mut self, f: F) -> Self
    where
        F: Fn(usize) -> bool + Send + Sync + 'static,
    {
        self.on_chunk = Some(Arc::new(f));
        self
    }

    pub fn content_length(mut self, n: usize) -> Self {
        self.content_length = Some(n);
        self
    }
}

impl From<Response> for StreamResponse {
    fn from(resp: Response) -> Self {
        Self {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
            chunk_size: usize::MAX,
            chunk_delay: Arc::new(|_, _| Duration::ZERO),
            hold_before: Duration::ZERO,
            hold_after: Duration::ZERO,
            on_chunk: None,
            content_length: None,
        }
    }
}

/// What a handler answers with.
pub enum Action {
    /// A whole response, one write.
    Respond(Response),
    /// A chunked response with delays / holds / a stop hook.
    Stream(StreamResponse),
    /// Close the connection without answering.
    Drop,
}

impl From<Response> for Action {
    fn from(resp: Response) -> Self {
        Action::Respond(resp)
    }
}

impl From<StreamResponse> for Action {
    fn from(resp: StreamResponse) -> Self {
        Action::Stream(resp)
    }
}

/// Per-server counters, read by the tests. The `hits` /
/// `last_headers` layout is the one `update_checker.rs` used, so its
/// assertions are unchanged; the connection counts are what
/// `adaptive.rs` tracked itself (`peak_concurrent` / `concurrent`).
#[derive(Default)]
pub struct Stats {
    pub hits: HashMap<String, usize>,
    pub last_headers: HashMap<String, Vec<(String, String)>>,
    pub current_connections: usize,
    pub peak_connections: usize,
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        302 => "Found",
        304 => "Not Modified",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

struct ConnectionGuard(Arc<Mutex<Stats>>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.lock() {
            s.current_connections = s.current_connections.saturating_sub(1);
        }
    }
}

async fn handle_conn(
    sock: TcpStream,
    handler: &Arc<dyn Fn(&Request) -> Action + Send + Sync>,
    stats: &Arc<Mutex<Stats>>,
) {
    {
        let mut s = stats.lock().unwrap();
        s.current_connections += 1;
        s.peak_connections = s.peak_connections.max(s.current_connections);
    }
    let _guard = ConnectionGuard(stats.clone());
    let (r, mut w) = sock.into_split();
    let mut r = BufReader::new(r);
    let mut line = String::new();
    if r.read_line(&mut line).await.unwrap_or(0) == 0 || line.trim().is_empty() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = line.trim_end().split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut body = Vec::new();
    let content_length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if content_length > 0 {
        body.resize(content_length, 0);
        if r.read_exact(&mut body).await.is_err() {
            return;
        }
    }
    let req = Request {
        method,
        path,
        headers,
        body,
    };
    let action = handler(&req);
    {
        let mut s = stats.lock().unwrap();
        *s.hits.entry(req.path.clone()).or_insert(0) += 1;
        s.last_headers.insert(req.path.clone(), req.headers.clone());
    }
    let stream: StreamResponse = match action {
        Action::Drop => return,
        Action::Respond(resp) => resp.into(),
        Action::Stream(resp) => resp,
    };
    if !stream.hold_before.is_zero() {
        tokio::time::sleep(stream.hold_before).await;
    }
    let mut head = format!("HTTP/1.1 {} {}\r\n", stream.status, reason(stream.status));
    for (k, v) in &stream.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        stream.content_length.unwrap_or(stream.body.len())
    ));
    if w.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let mut sent = 0usize;
    for chunk in stream.body.chunks(stream.chunk_size) {
        if w.write_all(chunk).await.is_err() {
            return;
        }
        sent += chunk.len();
        let delay = (stream.chunk_delay)(sent - chunk.len(), chunk.len());
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        if let Some(check) = &stream.on_chunk {
            if !check(chunk.len()) {
                return;
            }
        }
    }
    if !stream.hold_after.is_zero() {
        tokio::time::sleep(stream.hold_after).await;
    }
}

/// Starts the server on an ephemeral 127.0.0.1 port and returns the
/// port plus shared stats. Must be called from inside a Tokio runtime
/// (every caller is a `#[tokio::test]`); the accept loop runs as a
/// background task until the test runtime shuts down.
pub fn spawn<F, R>(handler: F) -> (u16, Arc<Mutex<Stats>>)
where
    F: Fn(&Request) -> R + Send + Sync + 'static,
    R: Into<Action>,
{
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("testserver bind");
    std_listener
        .set_nonblocking(true)
        .expect("testserver nonblocking");
    let listener = TcpListener::from_std(std_listener).expect("testserver from_std");
    let port = listener.local_addr().expect("testserver port").port();
    let handler: Arc<dyn Fn(&Request) -> Action + Send + Sync> =
        Arc::new(move |req| handler(req).into());
    let stats = Arc::new(Mutex::new(Stats::default()));
    let (handler_task, stats_task) = (handler.clone(), stats.clone());
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let handler = handler_task.clone();
            let stats = stats_task.clone();
            tokio::spawn(async move {
                handle_conn(sock, &handler, &stats).await;
            });
        }
    });
    (port, stats)
}
