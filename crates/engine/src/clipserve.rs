//! Serve a video to ffmpeg over loopback, so only the bytes ffmpeg
//! seeks to are fetched from the site.
//!
//! ffmpeg reads the video from `http://127.0.0.1:{port}/c/{token}`:
//! each range it asks for is fetched from the site with the
//! download's own headers and sign-in, then streamed to ffmpeg.
//! The token is 64 random hex characters (OS randomness, no weak
//! fallback) and dies with its [`Ticket`]; the server itself only
//! ever listens on loopback and only answers loopback peers.
//!
//! Two races from the upstream design are fixed here, not carried
//! over: the server is started exactly once (starting it is held
//! under a lock, so two threads racing cannot each bind a listener
//! and leak one), and the token never travels on ffmpeg's command
//! line (see `clip::CutInput::Served`).

use crate::http::{request, Progress, SendOrdered, Shared};
use crate::model::RequestHeaders;
use anyhow::anyhow;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What one of our addresses stands for.
#[derive(Clone)]
pub struct Source {
    pub url: String,
    pub headers: RequestHeaders,
    pub user_agent: String,
}

struct Entry {
    src: Source,
    shared: Arc<Shared>,
    /// what comes down counts for this download's speed and size
    progress: Arc<Progress>,
}

type Table = Arc<Mutex<HashMap<String, Arc<Entry>>>>;

static SERVER: std::sync::OnceLock<(u16, Table)> = std::sync::OnceLock::new();
/// Starting the server is held under this lock: two threads racing
/// to start it get the same listener instead of each binding one
/// and leaking the loser's thread and port.
static START_LOCK: Mutex<()> = Mutex::new(());

/// Our address for one stream; it stops working when this is dropped.
pub struct Ticket {
    pub url: String,
    token: String,
    table: Table,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let mut t = self.table.lock().unwrap_or_else(|e| e.into_inner());
        t.remove(&self.token);
    }
}

/// The server, started once for as long as the program runs (not
/// tied to whoever asked first).
fn server() -> anyhow::Result<&'static (u16, Table)> {
    if let Some(s) = SERVER.get() {
        return Ok(s);
    }
    let _guard = START_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = SERVER.get() {
        return Ok(s);
    }
    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    std_listener.set_nonblocking(true)?;
    let port = std_listener.local_addr()?.port();
    let table: Table = Arc::new(Mutex::new(HashMap::new()));
    let t = table.clone();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ezoo-clip")
        .enable_all()
        .build()?;
    std::thread::Builder::new()
        .name("ezoo-clip-server".into())
        .spawn(move || {
            rt.block_on(async move {
                let Ok(listener) = tokio::net::TcpListener::from_std(std_listener) else {
                    return;
                };
                loop {
                    let Ok((sock, peer)) = listener.accept().await else {
                        continue;
                    };
                    // this computer only
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    let t = t.clone();
                    tokio::spawn(async move {
                        let _ = serve(sock, t).await;
                    });
                }
            });
        })?;
    let _ = SERVER.set((port, table));
    Ok(SERVER.get().expect("set above"))
}

fn random_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no OS randomness for the clip token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Our address for `src` (as long as the ticket lives).
pub async fn open(
    shared: Arc<Shared>,
    src: Source,
    progress: Arc<Progress>,
) -> anyhow::Result<Ticket> {
    let (port, table) = server()?;
    let token = random_token()?;
    {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        t.insert(
            token.clone(),
            Arc::new(Entry {
                src,
                shared,
                progress,
            }),
        );
    }
    Ok(Ticket {
        url: format!("http://127.0.0.1:{port}/c/{token}"),
        token,
        table: table.clone(),
    })
}

/// "bytes=100-" -> (100, None), "bytes=100-199" -> (100, Some(199)).
fn parse_range(v: &str) -> Option<(u64, Option<u64>)> {
    let r = v.trim().strip_prefix("bytes=")?;
    if r.contains(',') {
        return None;
    }
    let (start, end) = r.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end = if end.trim().is_empty() {
        None
    } else {
        Some(end.trim().parse::<u64>().ok()?)
    };
    Some((start, end))
}

/// "bytes 100-199/1000" -> the total (1000).
fn parse_total(v: &str) -> Option<u64> {
    v.trim()
        .strip_prefix("bytes ")?
        .split('/')
        .nth(1)?
        .trim()
        .parse()
        .ok()
}

async fn reply_empty(wr: &mut tokio::net::tcp::OwnedWriteHalf, status: &str) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = wr.write_all(head.as_bytes()).await;
}

async fn serve(sock: tokio::net::TcpStream, table: Table) -> anyhow::Result<()> {
    let (mut rd, mut wr) = sock.into_split();
    // one request per connection is all ffmpeg makes here; a short
    // head (8 KiB) is plenty for a request line + a few headers.
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = rd.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..n]);
        if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > 8192 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut range: Option<(u64, Option<u64>)> = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some(v) = line
            .strip_prefix("Range:")
            .or_else(|| line.strip_prefix("range:"))
        {
            range = parse_range(v);
        }
    }
    if method != "GET" && method != "HEAD" {
        reply_empty(&mut wr, "405 Method Not Allowed").await;
        return Ok(());
    }
    let Some(token) = path
        .strip_prefix("/c/")
        .filter(|t| !t.is_empty() && !t.contains('/'))
    else {
        reply_empty(&mut wr, "404 Not Found").await;
        return Ok(());
    };
    let entry = {
        let t = table.lock().unwrap_or_else(|e| e.into_inner());
        t.get(token).cloned()
    };
    let Some(entry) = entry else {
        reply_empty(&mut wr, "404 Not Found").await;
        return Ok(());
    };
    // ask the site for exactly the range ffmpeg wants (or the whole
    // file when it asks for no range); redirects and Host-first go
    // through the same ordered sender as every other request
    let client = entry.shared.client();
    let mut req = request(
        &client,
        &entry.src.url,
        &entry.src.headers,
        &entry.src.user_agent,
    );
    if let Some((start, end)) = range {
        let value = match end {
            Some(e) => format!("bytes={start}-{e}"),
            None => format!("bytes={start}-"),
        };
        req = req.header("Range", value);
    }
    let mut resp = match req.send_ordered().await {
        Ok(r) => r,
        Err(_) => {
            reply_empty(&mut wr, "502 Bad Gateway").await;
            return Ok(());
        }
    };
    let upstream_status = resp.status();
    if !upstream_status.is_success() && upstream_status.as_u16() != 206 {
        let status = format!(
            "{} {}",
            upstream_status.as_u16(),
            upstream_status.canonical_reason().unwrap_or("")
        );
        reply_empty(&mut wr, &status).await;
        return Ok(());
    }
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("video/mp4")
        .to_string();
    let total = resp
        .headers()
        .get("content-range")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_total)
        .or_else(|| {
            resp.headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|_| range.is_none())
        });
    let is_partial = upstream_status.as_u16() == 206;
    let (start, end) = match range {
        Some((s, e)) => (s, e),
        None => (0, None),
    };
    let length = match (end, total) {
        (Some(e), _) if e >= start => Some(e - start + 1),
        (None, Some(t)) if t > start => Some(t - start),
        _ => None,
    };
    let mut headers = format!("Content-Type: {content_type}\r\nAccept-Ranges: bytes\r\n");
    if is_partial {
        let end_out = match length {
            Some(l) => start + l - 1,
            None => start,
        };
        let total_out = total.map(|t| t.to_string()).unwrap_or_else(|| "*".into());
        headers.push_str(&format!(
            "Content-Range: bytes {start}-{end_out}/{total_out}\r\n"
        ));
    }
    if method == "HEAD" {
        let len = length.unwrap_or(0);
        let head = format!(
            "HTTP/1.1 {} {}\r\n{headers}Content-Length: {len}\r\nConnection: close\r\n\r\n",
            upstream_status.as_u16(),
            upstream_status.canonical_reason().unwrap_or("")
        );
        let _ = wr.write_all(head.as_bytes()).await;
        return Ok(());
    }
    let status_line = if is_partial {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let len_header = match length {
        Some(l) => format!("Content-Length: {l}\r\n"),
        None => String::new(),
    };
    let head = format!("HTTP/1.1 {status_line}\r\n{headers}{len_header}Connection: close\r\n\r\n");
    wr.write_all(head.as_bytes()).await?;
    let mut got: u64 = 0;
    loop {
        let chunk = match resp.chunk().await {
            Ok(Some(c)) => c,
            _ => break,
        };
        entry
            .progress
            .bytes
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        got += chunk.len() as u64;
        if wr.write_all(&chunk).await.is_err() {
            break;
        }
        if let Some(l) = length {
            if got >= l {
                break;
            }
        }
    }
    let _ = wr.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny origin server that serves fixed bytes and honours
    /// Range, so the clip server can be tested without the internet.
    async fn origin_server(bytes: Vec<u8>) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let bytes = bytes.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let Ok(n) = sock.read(&mut buf).await else {
                        return;
                    };
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let mut range: Option<(u64, Option<u64>)> = None;
                    for line in text.lines() {
                        let lower = line.to_ascii_lowercase();
                        if let Some(v) = lower.strip_prefix("range:") {
                            range = parse_range(v);
                        }
                    }
                    let (status, start, end) = match range {
                        Some((s, e)) => {
                            let e = e
                                .unwrap_or((bytes.len() as u64).saturating_sub(1))
                                .min(bytes.len() as u64 - 1);
                            ("206 Partial Content", s, e)
                        }
                        None => ("200 OK", 0, bytes.len() as u64 - 1),
                    };
                    let body = &bytes[start as usize..=end as usize];
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: video/mp4\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len(),
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body).await;
                });
            }
        });
        (port, handle)
    }

    fn test_shared() -> Arc<Shared> {
        let client = reqwest::Client::builder().build().unwrap();
        Arc::new(Shared::new(client, crate::limiter::SpeedLimiter::new(0)))
    }

    #[tokio::test]
    async fn range_is_served_and_token_required() {
        let bytes: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let (port, _srv) = origin_server(bytes.clone()).await;
        let shared = test_shared();
        let ticket = open(
            shared,
            Source {
                url: format!("http://127.0.0.1:{port}/video.mp4"),
                headers: RequestHeaders::default(),
                user_agent: "test".into(),
            },
            Arc::new(Progress::default()),
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();
        // a range ffmpeg would ask for comes back byte-exact
        let r = client
            .get(&ticket.url)
            .header("Range", "bytes=100-199")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 206);
        assert_eq!(r.bytes().await.unwrap().as_ref(), &bytes[100..200]);
        // no token, no video: same port, a made-up token
        let base = ticket.url.rsplit_once("/c/").unwrap().0;
        let r = client
            .get(format!(
                "{base}/c/0000000000000000000000000000000000000000000000000000000000000000"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
        // the ticket dies with its owner: the address stops working
        let url = ticket.url.clone();
        drop(ticket);
        let r = client
            .get(&url)
            .header("Range", "bytes=0-9")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
    }

    #[test]
    fn ranges_parse() {
        assert_eq!(parse_range("bytes=100-"), Some((100, None)));
        assert_eq!(parse_range("bytes=100-199"), Some((100, Some(199))));
        assert_eq!(parse_range("bytes=0-0, 10-20"), None);
        assert_eq!(parse_range("items=0-9"), None);
        assert_eq!(parse_total("bytes 100-199/1000"), Some(1000));
        assert_eq!(parse_total("bytes */1000"), Some(1000));
    }
}
