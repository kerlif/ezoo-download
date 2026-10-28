//! Phase 5 integration tests: the adaptive engine against a local
//! throttled server (127.0.0.1, ephemeral port). Every case compares
//! the SHA-256 of the downloaded file with the served bytes.

use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use swiftget_engine::adaptive::{AdaptiveConfig, AdaptiveReport};
use swiftget_engine::http::{build_client, Progress, Shared};
use swiftget_engine::http_download::{self, HttpJob};
use swiftget_engine::limiter::SpeedLimiter;
use swiftget_engine::segments;
use swiftget_engine::RequestHeaders;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

fn test_data(len: usize) -> Arc<Vec<u8>> {
    Arc::new((0..len).map(|i| (i * 31 % 251) as u8).collect())
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn test_cfg() -> AdaptiveConfig {
    AdaptiveConfig {
        measure_window: Duration::from_millis(200),
        stall_grace: Duration::from_secs(1),
        stall_seconds: Duration::from_secs(2),
        ..AdaptiveConfig::default()
    }
}

fn short_cfg() -> AdaptiveConfig {
    AdaptiveConfig {
        measure_window: Duration::from_millis(100),
        stall_grace: Duration::from_millis(500),
        stall_seconds: Duration::from_secs(1),
        ..AdaptiveConfig::default()
    }
}

struct ServerCfg {
    data: Arc<Vec<u8>>,
    support_range: bool,
    /// bytes/second per connection; None = as fast as the loopback allows
    per_conn_bps: Option<u64>,
    /// first N requests get 429 + Retry-After
    fail_429_first: AtomicU32,
    retry_after_secs: u64,
    /// first N connections are closed after half their body
    drop_first: AtomicU32,
    /// connections asking from this start, first one only: send
    /// `stall_after` bytes, then stop sending (hold the socket open)
    stall_start: Option<u64>,
    stall_after: usize,
    stall_used: AtomicBool,
    /// connections asking from below this start get this delay per chunk
    slow_below: Option<(u64, Duration)>,
    peak_concurrent: AtomicU32,
    concurrent: AtomicU32,
    requests: AtomicU32,
}

impl ServerCfg {
    fn fast(data: Arc<Vec<u8>>) -> Self {
        ServerCfg {
            data,
            support_range: true,
            per_conn_bps: None,
            fail_429_first: AtomicU32::new(0),
            retry_after_secs: 1,
            drop_first: AtomicU32::new(0),
            stall_start: None,
            stall_after: 0,
            stall_used: AtomicBool::new(false),
            slow_below: None,
            peak_concurrent: AtomicU32::new(0),
            concurrent: AtomicU32::new(0),
            requests: AtomicU32::new(0),
        }
    }
}

async fn serve(cfg: Arc<ServerCfg>) -> (u16, Arc<ServerCfg>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let cfg2 = cfg.clone();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let cfg = cfg.clone();
            tokio::spawn(async move {
                let cur = cfg.concurrent.fetch_add(1, Ordering::Relaxed) + 1;
                cfg.peak_concurrent.fetch_max(cur, Ordering::Relaxed);
                let _guard = ConcurrentGuard(cfg.clone());
                let (r, mut w) = sock.into_split();
                let mut r = BufReader::new(r);
                let mut range: Option<(usize, usize)> = None;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).await.unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    let l = line.to_ascii_lowercase();
                    if let Some(v) = l.strip_prefix("range: bytes=") {
                        if let Some((a, b)) = v.trim().split_once('-') {
                            let a: usize = a.parse().unwrap_or(0);
                            let b: usize = if b.is_empty() {
                                cfg.data.len() - 1
                            } else {
                                b.parse().unwrap_or(cfg.data.len() - 1)
                            };
                            range = Some((a, b.min(cfg.data.len() - 1)));
                        }
                    }
                }
                let req_no = cfg.requests.fetch_add(1, Ordering::Relaxed);
                // rate-limit injection
                if req_no < cfg.fail_429_first.load(Ordering::Relaxed) {
                    let head = format!(
                        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        cfg.retry_after_secs
                    );
                    let _ = w.write_all(head.as_bytes()).await;
                    return;
                }
                let (a, b) = match (cfg.support_range, range) {
                    (true, Some((a, b))) => (a, b),
                    _ => (0, cfg.data.len() - 1),
                };
                let head = if cfg.support_range && range.is_some() {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {a}-{b}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        cfg.data.len(),
                        b - a + 1
                    )
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        cfg.data.len()
                    )
                };
                if w.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                let body = &cfg.data[a..=b];
                // stall injection: first connection from the stall start
                let stalling = cfg.stall_start == Some(a as u64)
                    && !cfg.stall_used.swap(true, Ordering::Relaxed);
                let send_len = if stalling {
                    cfg.stall_after.min(body.len())
                } else if req_no < cfg.drop_first.load(Ordering::Relaxed) {
                    body.len() / 2
                } else {
                    body.len()
                };
                let slow_delay = cfg
                    .slow_below
                    .filter(|(below, _)| (a as u64) < *below)
                    .map(|(_, d)| d);
                for chunk in body[..send_len].chunks(8192) {
                    if w.write_all(chunk).await.is_err() {
                        return;
                    }
                    if let Some(bps) = cfg.per_conn_bps {
                        let secs = chunk.len() as f64 / bps as f64;
                        tokio::time::sleep(Duration::from_secs_f64(secs)).await;
                    }
                    if let Some(d) = slow_delay {
                        tokio::time::sleep(d).await;
                    }
                }
                if stalling {
                    // stop sending, hold the socket: the client must
                    // rescue this segment from another connection
                    tokio::time::sleep(Duration::from_secs(120)).await;
                }
                // dropping the connection: the client retries from `pos`
            });
        }
    });
    (port, cfg2)
}

struct ConcurrentGuard(Arc<ServerCfg>);
impl Drop for ConcurrentGuard {
    fn drop(&mut self) {
        self.0.concurrent.fetch_sub(1, Ordering::Relaxed);
    }
}

fn make_job(
    url: String,
    total: u64,
    resumable: bool,
    connections: u32,
    cfg: AdaptiveConfig,
    report: Option<Arc<Mutex<AdaptiveReport>>>,
    part: std::path::PathBuf,
) -> HttpJob {
    HttpJob {
        url,
        headers: RequestHeaders::default(),
        user_agent: "test".into(),
        total_size: Some(total),
        resumable,
        part_path: part,
        connections,
        min_split: 64 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: cfg,
        learned_start: None,
        report,
    }
}

async fn run_job(shared: Arc<Shared>, job: HttpJob, segs: Arc<Mutex<Vec<segments::Segment>>>) {
    let progress = Arc::new(Progress::default());
    http_download::run(shared, job, segs, progress, CancellationToken::new())
        .await
        .unwrap();
}

fn tmpdir(tag: &str, port: u16) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("swiftget-adaptive-{tag}-{port}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (a) mid-download restart: pause, resume from the saved segment table,
/// checksum equality.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resume_after_restart_matches_source() {
    let data = test_data(4 * 1024 * 1024 + 123);
    let (port, _) = serve(Arc::new(ServerCfg::fast(data.clone()))).await;
    let url = format!("http://127.0.0.1:{port}/a.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("resume", port);
    let part = dir.join("out.sgpart");
    let job = make_job(
        url,
        data.len() as u64,
        true,
        8,
        test_cfg(),
        None,
        part.clone(),
    );
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());
    let cancel = CancellationToken::new();
    let (c2, p2) = (cancel.clone(), progress.clone());
    let quarter = data.len() as u64 / 4;
    tokio::spawn(async move {
        while p2.bytes.load(Ordering::Relaxed) < quarter {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        c2.cancel();
    });
    let first = http_download::run(
        shared.clone(),
        job.clone(),
        segs.clone(),
        progress.clone(),
        cancel,
    )
    .await;
    assert!(first.is_err(), "the first run should have been cancelled");
    let saved = segs.lock().unwrap().clone();
    assert!(
        segments::validate_segment_map(&saved, data.len() as u64),
        "the saved segment table must cover the file exactly once"
    );
    let segs = Arc::new(Mutex::new(saved));
    http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let _ = std::fs::remove_dir_all(&dir);
}

/// (b) 429 with Retry-After is honoured: the wait is observed,
/// concurrency is halved, the download still completes byte-exact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rate_limit_retry_after_honoured() {
    let data = test_data(2 * 1024 * 1024 + 7);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.fail_429_first = AtomicU32::new(3);
    cfg.retry_after_secs = 1;
    let (port, _) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/b.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("ratelimit", port);
    let part = dir.join("out.sgpart");
    let report = Arc::new(Mutex::new(AdaptiveReport::default()));
    let job = make_job(
        url,
        data.len() as u64,
        true,
        4,
        test_cfg(),
        Some(report.clone()),
        part.clone(),
    );
    let started = Instant::now();
    run_job(shared, job, Arc::new(Mutex::new(Vec::new()))).await;
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(950),
        "Retry-After: 1 was not honoured (elapsed {elapsed:?})"
    );
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let r = report.lock().unwrap();
    assert!(
        r.rate_limit_hits >= 1,
        "no rate-limit answer was recorded: {r:?}"
    );
    assert!(
        r.min_target < 4,
        "concurrency was not halved after 429: {r:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (c) dropped connections are retried from the last byte written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dropped_connections_retry_to_completion() {
    let data = test_data(2 * 1024 * 1024 + 99);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.drop_first = AtomicU32::new(4);
    let (port, _) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/c.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("dropped", port);
    let part = dir.join("out.sgpart");
    let job = make_job(
        url,
        data.len() as u64,
        true,
        4,
        test_cfg(),
        None,
        part.clone(),
    );
    run_job(shared, job, Arc::new(Mutex::new(Vec::new()))).await;
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let _ = std::fs::remove_dir_all(&dir);
}

/// (d) a stalled connection is rescued from the last byte written:
/// no data is lost and the checksum matches.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stalled_connection_rescued_from_last_byte() {
    let data = test_data(3 * 1024 * 1024 + 55);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.stall_start = Some(0);
    cfg.stall_after = 64 * 1024;
    let (port, _) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/d.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("stall", port);
    let part = dir.join("out.sgpart");
    let report = Arc::new(Mutex::new(AdaptiveReport::default()));
    let job = make_job(
        url,
        data.len() as u64,
        true,
        4,
        short_cfg(),
        Some(report.clone()),
        part.clone(),
    );
    run_job(shared, job, Arc::new(Mutex::new(Vec::new()))).await;
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let r = report.lock().unwrap();
    assert!(
        r.rescues >= 1,
        "no stall rescue was recorded even though a connection stalled: {r:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (e) no Range support: one connection, completes, and a cancelled
/// run restarts cleanly from zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_range_support_uses_one_connection() {
    let data = test_data(1024 * 1024 + 33);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.support_range = false;
    // slow enough that the cancel below reliably lands mid-download
    cfg.per_conn_bps = Some(512 * 1024);
    let (port, server) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/e.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("norange", port);
    let part = dir.join("out.sgpart");
    let job = make_job(
        url.clone(),
        data.len() as u64,
        false,
        8,
        test_cfg(),
        None,
        part.clone(),
    );
    // cancel mid-way, then restart: without ranges the file restarts
    let segs = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(Progress::default());
    let cancel = CancellationToken::new();
    let (c2, p2) = (cancel.clone(), progress.clone());
    let half = data.len() as u64 / 2;
    tokio::spawn(async move {
        while p2.bytes.load(Ordering::Relaxed) < half {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        c2.cancel();
    });
    let first = http_download::run(
        shared.clone(),
        job.clone(),
        segs.clone(),
        progress.clone(),
        cancel,
    )
    .await;
    assert!(first.is_err());
    // wait for the cancelled connection to close server-side, so the
    // peak-concurrency check below measures only live connections
    for _ in 0..100 {
        if server.concurrent.load(Ordering::Relaxed) == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(server.concurrent.load(Ordering::Relaxed), 0);
    let segs = Arc::new(Mutex::new(Vec::new()));
    http_download::run(
        shared,
        job,
        segs,
        progress.clone(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    assert_eq!(
        server.peak_concurrent.load(Ordering::Relaxed),
        1,
        "without Range support exactly one connection may be used"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// (f) split while a segment is being written: the slow head of the
/// file is cut while its worker is still writing, checksum equality
/// and no overlap.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn split_while_writing_matches_source() {
    let data = test_data(6 * 1024 * 1024 + 11);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.slow_below = Some((2 * 1024 * 1024, Duration::from_millis(30)));
    let (port, _) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/f.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("split", port);
    let part = dir.join("out.sgpart");
    let report = Arc::new(Mutex::new(AdaptiveReport::default()));
    let job = make_job(
        url,
        data.len() as u64,
        true,
        4,
        test_cfg(),
        Some(report.clone()),
        part.clone(),
    );
    let segs = Arc::new(Mutex::new(Vec::new()));
    run_job(shared, job, segs.clone()).await;
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let table = segs.lock().unwrap();
    assert!(
        table.len() > 4,
        "the slow segment was never split while being written: {} segments",
        table.len()
    );
    assert!(segments::validate_segment_map(&table, data.len() as u64));
    drop(table);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Scaling proof: on a per-connection-limited server the adaptive
/// download grows past its starting connection count.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adaptive_scaling_grows_connections_on_limited_server() {
    let data = test_data(8 * 1024 * 1024);
    let mut cfg = ServerCfg::fast(data.clone());
    cfg.per_conn_bps = Some(256 * 1024);
    let (port, _) = serve(Arc::new(cfg)).await;
    let url = format!("http://127.0.0.1:{port}/g.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("scaling", port);
    let part = dir.join("out.sgpart");
    let report = Arc::new(Mutex::new(AdaptiveReport::default()));
    let job = make_job(
        url,
        data.len() as u64,
        true,
        16,
        test_cfg(),
        Some(report.clone()),
        part.clone(),
    );
    run_job(shared, job, Arc::new(Mutex::new(Vec::new()))).await;
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    let r = report.lock().unwrap();
    assert!(
        r.peak_target > 4,
        "the scaler never grew past its start on a limited server: {r:?}"
    );
    assert!(
        r.best_throughput_bps > 0.0,
        "no throughput was measured: {r:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A corrupt resumed segment table is re-planned, never trusted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn corrupt_segment_map_is_replanned() {
    let data = test_data(1024 * 1024 + 5);
    let (port, _) = serve(Arc::new(ServerCfg::fast(data.clone()))).await;
    let url = format!("http://127.0.0.1:{port}/h.bin");
    let shared = Arc::new(Shared::new(build_client().unwrap(), SpeedLimiter::new(0)));
    let dir = tmpdir("corrupt", port);
    let part = dir.join("out.sgpart");
    // a table with a gap: segment 1 starts past segment 0's end
    let mut corrupt = segments::plan(data.len() as u64, 4, 64 * 1024);
    corrupt[1].start += 1000;
    corrupt[1].pos = corrupt[1].start;
    assert!(!segments::validate_segment_map(&corrupt, data.len() as u64));
    let job = make_job(
        url,
        data.len() as u64,
        true,
        4,
        test_cfg(),
        None,
        part.clone(),
    );
    let segs = Arc::new(Mutex::new(corrupt));
    run_job(shared, job, segs.clone()).await;
    let out = std::fs::read(&part).unwrap();
    assert_eq!(sha256_hex(&out), sha256_hex(&data));
    assert!(segments::validate_segment_map(
        &segs.lock().unwrap(),
        data.len() as u64
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
