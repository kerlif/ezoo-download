//! Benchmark: adaptive connection scaling vs fixed connection counts,
//! on the same local throttled server. Run with:
//!   cargo run --release -p swiftget-engine --example adaptive_bench
//!
//! Two server profiles, three runs per configuration. Prints one line
//! per run plus a summary table. The numbers are written up honestly
//! in docs/BENCHMARKS.md, including cases where adaptive is not faster.

use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use swiftget_engine::adaptive::{AdaptiveConfig, AdaptiveReport};
use swiftget_engine::http::{build_client, Progress, Shared};
use swiftget_engine::http_download::{self, HttpJob};
use swiftget_engine::limiter::SpeedLimiter;
use swiftget_engine::RequestHeaders;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

fn test_data(len: usize) -> Arc<Vec<u8>> {
    Arc::new((0..len).map(|i| (i * 31 % 251) as u8).collect())
}

struct Profile {
    name: &'static str,
    /// file size for this profile: the fast profile needs a bigger
    /// file, or loopback finishes it in milliseconds and nothing is
    /// measurable
    file_bytes: usize,
    per_conn_bps: Option<u64>,
    chunk_delay: Duration,
}

async fn serve(data: Arc<Vec<u8>>, profile: &Profile) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let per_conn_bps = profile.per_conn_bps;
    let chunk_delay = profile.chunk_delay;
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let data = data.clone();
            tokio::spawn(async move {
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
                                data.len() - 1
                            } else {
                                b.parse().unwrap_or(data.len() - 1)
                            };
                            range = Some((a, b.min(data.len() - 1)));
                        }
                    }
                }
                let (a, b) = range.unwrap_or((0, data.len() - 1));
                let head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {a}-{b}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    data.len(),
                    b - a + 1
                );
                if w.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                for chunk in data[a..=b].chunks(8192) {
                    if w.write_all(chunk).await.is_err() {
                        return;
                    }
                    if let Some(bps) = per_conn_bps {
                        tokio::time::sleep(Duration::from_secs_f64(
                            chunk.len() as f64 / bps as f64,
                        ))
                        .await;
                    }
                    if !chunk_delay.is_zero() {
                        tokio::time::sleep(chunk_delay).await;
                    }
                }
            });
        }
    });
    port
}

struct RunResult {
    secs: f64,
    peak_target: u32,
    best_mbps: f64,
}

async fn one_run(
    shared: &Arc<Shared>,
    url: &str,
    total: u64,
    expected_sha256: &str,
    adaptive: bool,
    fixed: u32,
    dir: &std::path::Path,
) -> anyhow::Result<RunResult> {
    let part = dir.join("bench.sgpart");
    let _ = std::fs::remove_file(&part);
    let report = Arc::new(Mutex::new(AdaptiveReport::default()));
    let cfg = if adaptive {
        AdaptiveConfig {
            measure_window: Duration::from_millis(500),
            ..AdaptiveConfig::default()
        }
    } else {
        AdaptiveConfig {
            enabled: false,
            ..AdaptiveConfig::default()
        }
    };
    let job = HttpJob {
        url: url.to_string(),
        headers: RequestHeaders::default(),
        user_agent: "bench".into(),
        total_size: Some(total),
        resumable: true,
        part_path: part.clone(),
        connections: if adaptive { 16 } else { fixed },
        min_split: 64 * 1024,
        retries: 3,
        progress_base: 0,
        max_request: None,
        limiter: Arc::new(SpeedLimiter::new(0)),
        adaptive: cfg,
        learned_start: None,
        report: Some(report.clone()),
    };
    let started = Instant::now();
    http_download::run(
        shared.clone(),
        job,
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Progress::default()),
        CancellationToken::new(),
    )
    .await?;
    let secs = started.elapsed().as_secs_f64();
    let out = std::fs::read(&part)?;
    let mut h = Sha256::new();
    h.update(&out);
    let got = format!("{:x}", h.finalize());
    assert_eq!(got, expected_sha256, "checksum mismatch");
    let r = report.lock().unwrap();
    Ok(RunResult {
        secs,
        peak_target: r.peak_target,
        best_mbps: r.best_throughput_bps / 1_048_576.0,
    })
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> anyhow::Result<()> {
    let shared = Arc::new(Shared::new(build_client()?, SpeedLimiter::new(0)));
    let profiles = [
        Profile {
            name: "limited",
            file_bytes: 8 * 1024 * 1024,
            per_conn_bps: Some(512 * 1024),
            chunk_delay: Duration::from_millis(5),
        },
        Profile {
            name: "fast",
            file_bytes: 32 * 1024 * 1024,
            per_conn_bps: None,
            chunk_delay: Duration::from_millis(1),
        },
    ];
    let configs: [(&str, bool, u32); 5] = [
        ("adaptive", true, 0),
        ("fixed-1", false, 1),
        ("fixed-4", false, 4),
        ("fixed-8", false, 8),
        ("fixed-16", false, 16),
    ];
    let dir = std::env::temp_dir().join("swiftget-bench");
    std::fs::create_dir_all(&dir)?;
    for profile in &profiles {
        let total = profile.file_bytes;
        let data = test_data(total);
        let expected = {
            let mut h = Sha256::new();
            h.update(data.as_slice());
            format!("{:x}", h.finalize())
        };
        println!(
            "profile={} file: {total} bytes sha256={expected}",
            profile.name
        );
        let port = serve(data.clone(), profile).await;
        let url = format!("http://127.0.0.1:{port}/bench.bin");
        for (name, adaptive, fixed) in &configs {
            let mut times = Vec::new();
            let mut peak = 0u32;
            let mut best = 0.0f64;
            for run in 1..=3 {
                let r = one_run(
                    &shared,
                    &url,
                    total as u64,
                    &expected,
                    *adaptive,
                    *fixed,
                    &dir,
                )
                .await?;
                let mbps = total as f64 / 1_048_576.0 / r.secs;
                println!(
                    "RUN profile={} config={} run={} secs={:.3} mib_per_s={:.2} peak_target={} best_mib_per_s={:.2}",
                    profile.name, name, run, r.secs, mbps, r.peak_target, r.best_mbps
                );
                times.push(r.secs);
                peak = peak.max(r.peak_target);
                best = best.max(r.best_mbps);
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mean = times.iter().sum::<f64>() / times.len() as f64;
            let mean_mbps = total as f64 / 1_048_576.0 / mean;
            println!(
                "SUMMARY profile={} config={} mean_secs={:.3} mean_mib_per_s={:.2} peak_target={} best_mib_per_s={:.2}",
                profile.name, name, mean, mean_mbps, peak, best
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
