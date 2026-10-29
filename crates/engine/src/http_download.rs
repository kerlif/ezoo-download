//! Multi-connection download of a normal HTTP file.
//!
//! Every connection ("worker") asks the segment table for work, downloads its
//! byte range with an HTTP Range request and writes straight into the right
//! offset of the `.sgpart` file. When a worker finishes it takes the
//! biggest remaining segment and cuts it in half, so all connections stay
//! busy until the end.
//!
//! Adaptive behaviour (see `adaptive.rs` for the decisions): the download
//! starts with a few connections and a supervisor measures aggregate
//! throughput over short windows, adding a connection only while
//! throughput clearly rises and shedding one when it falls. Rate-limit
//! answers are honoured with halved concurrency, and a segment that
//! stalls is handed to a fresh connection from the last byte written.

use crate::adaptive::{jittered, AdaptiveConfig, AdaptiveReport, Scaler, SegmentMonitor};
use crate::fileio::{make_sparse, write_at};
use crate::http::{
    http_status_of, is_network_error, reconnect_delay, request, ConnGuard, HttpStatus, NetDrop,
    Progress, RateLimited, Shared,
};
use crate::model::RequestHeaders;
use crate::segments::{self, Segment};
use anyhow::{anyhow, bail};
use reqwest::header::RANGE;
use reqwest::StatusCode;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// How long we wait for the next chunk before treating the connection as dead.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// How often a waiting connection is polled, so a stall rescue can close
/// it promptly instead of waiting out the whole stall timeout.
const CHUNK_POLL: Duration = Duration::from_secs(1);

/// A segmented HTTP file download: URL, request data, part file, and limits.
#[derive(Clone)]
pub struct HttpJob {
    pub url: String,
    pub headers: RequestHeaders,
    pub user_agent: String,
    pub total_size: Option<u64>,
    pub resumable: bool,
    pub part_path: PathBuf,
    pub connections: u32,
    pub min_split: u64,
    pub retries: u32,
    /// Bytes from earlier stages of the same download (Dash: the video part
    /// when this job is the audio part), added to the progress counter.
    pub progress_base: u64,
    /// Ask for at most this many bytes per HTTP request. YouTube slows a
    /// connection down to playback speed when one request asks for too much,
    /// so for its servers we fetch each part in 10 MB requests.
    pub max_request: Option<u64>,
    /// this download's own speed limit (shared with its other parts)
    pub limiter: Arc<crate::limiter::SpeedLimiter>,
    /// Adaptive tuning for this download (all defaults in one struct).
    pub adaptive: AdaptiveConfig,
    /// Starting connection count learned from this host before, if any.
    pub learned_start: Option<u32>,
    /// Filled in when the download ends (also on failure), when present.
    /// The caller records the per-host learning from it.
    pub report: Option<Arc<Mutex<AdaptiveReport>>>,
}

/// Request size limit for servers known to throttle big range requests.
pub fn max_request_for(url: &str) -> Option<u64> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
    if host.ends_with("googlevideo.com") {
        Some(10 * 1024 * 1024)
    } else {
        None
    }
}

/// The shared segment table: byte ranges with per-segment progress.
pub type SegTable = Arc<Mutex<Vec<Segment>>>;

/// Error returned when the user cancels a download.
#[derive(Debug)]
pub struct Cancelled;
impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}
impl std::error::Error for Cancelled {}

/// Error returned when a segment was flagged for rescue: the connection
/// is closed and the segment is re-requested from the last byte written.
/// Not a failure: the download loop treats it as "this segment is free
/// for the taking again".
#[derive(Debug)]
pub struct Takeover;
impl std::fmt::Display for Takeover {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("segment handed to a fresh connection")
    }
}
impl std::error::Error for Takeover {}

/// Shared adaptive state for one download run: the scaler, the stall
/// monitor, and the counters the supervisor and the workers share.
struct AdaptiveState {
    cfg: AdaptiveConfig,
    scaler: Mutex<Scaler>,
    monitor: Mutex<SegmentMonitor>,
    /// connections the download should be using right now
    target: AtomicU32,
    /// workers currently alive
    live: AtomicU32,
    /// rate-limit / reset errors since the last measurement window
    window_errors: AtomicU32,
    rescues: AtomicU32,
    first_byte_total_ms: AtomicU64,
    first_byte_count: AtomicU32,
}

impl AdaptiveState {
    fn new(cfg: AdaptiveConfig, initial: u32, cap: u32) -> Self {
        let target = initial.clamp(1, cap.max(1));
        AdaptiveState {
            scaler: Mutex::new(Scaler::new(cfg, target, cap.max(1))),
            monitor: Mutex::new(SegmentMonitor::new(cfg)),
            cfg,
            target: AtomicU32::new(target),
            live: AtomicU32::new(0),
            window_errors: AtomicU32::new(0),
            rescues: AtomicU32::new(0),
            first_byte_total_ms: AtomicU64::new(0),
            first_byte_count: AtomicU32::new(0),
        }
    }

    fn sync_target(&self) {
        let target = self.scaler.lock().unwrap().target();
        self.target.store(target, Ordering::Relaxed);
    }

    fn halve(&self) {
        self.scaler.lock().unwrap().halve();
        self.sync_target();
    }

    fn note_reset(&self) {
        self.scaler.lock().unwrap().note_reset();
        self.sync_target();
    }

    fn record_first_byte(&self, elapsed: Duration) {
        self.first_byte_total_ms
            .fetch_add(elapsed.as_millis() as u64, Ordering::Relaxed);
        self.first_byte_count.fetch_add(1, Ordering::Relaxed);
    }

    fn avg_first_byte_ms(&self) -> f64 {
        let count = self.first_byte_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            self.first_byte_total_ms.load(Ordering::Relaxed) as f64 / count as f64
        }
    }
}

/// Counts a live worker. Dropping it releases the slot, unless the
/// worker already released it by shrinking the pool (see `shrink`).
struct WorkerGuard {
    state: Arc<AdaptiveState>,
    released: bool,
}

impl WorkerGuard {
    fn new(state: &Arc<AdaptiveState>) -> Self {
        state.live.fetch_add(1, Ordering::Relaxed);
        WorkerGuard {
            state: state.clone(),
            released: false,
        }
    }

    /// The pool is over its target: release this worker's slot and
    /// report that it should exit. The compare-and-swap makes sure
    /// exactly the surplus workers exit, even when several check at
    /// the same moment.
    fn shrink(&mut self) -> bool {
        loop {
            let live = self.state.live.load(Ordering::Relaxed);
            let target = self.state.target.load(Ordering::Relaxed);
            if live <= target {
                return false;
            }
            if self
                .state
                .live
                .compare_exchange(live, live - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.released = true;
                return true;
            }
        }
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        if !self.released {
            self.state.live.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Everything a worker needs, bundled so spawning one is a single call.
#[derive(Clone)]
struct WorkerArgs {
    shared: Arc<Shared>,
    job: HttpJob,
    segs: SegTable,
    progress: Arc<Progress>,
    file: Arc<File>,
    cancel: CancellationToken,
    segmented: bool,
    adaptive: Arc<AdaptiveState>,
}

impl WorkerArgs {
    fn spawn(&self, set: &mut JoinSet<anyhow::Result<()>>) {
        let ctx = Worker {
            shared: self.shared.clone(),
            job: self.job.clone(),
            segs: self.segs.clone(),
            progress: self.progress.clone(),
            file: self.file.clone(),
            cancel: self.cancel.clone(),
            segmented: self.segmented,
            adaptive: self.adaptive.clone(),
        };
        set.spawn(ctx.run());
    }
}

pub async fn run(
    shared: Arc<Shared>,
    job: HttpJob,
    segs: SegTable,
    progress: Arc<Progress>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    if let Some(dir) = job.part_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let segmented = job.resumable && job.total_size.is_some();
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&job.part_path)?;

    {
        let mut table = segs.lock().unwrap();
        if segmented {
            let total = job.total_size.unwrap_or(0);
            // A segment table that does not faithfully cover the file
            // (gap, overlap, a position outside its segment) is never
            // resumed from: re-plan from scratch instead of trusting it.
            if !table.is_empty() && !segments::validate_segment_map(&table, total) {
                table.clear();
            }
            let fresh =
                table.is_empty() || file.metadata()?.len() == 0 && segments::downloaded(&table) > 0;
            if fresh {
                *table = segments::plan(total, job.connections, job.min_split);
                file.set_len(0)?;
            }
            if file.metadata()?.len() != total {
                make_sparse(&file);
                file.set_len(total)?;
            }
        } else {
            // no ranges: always start over with one connection
            file.set_len(0)?;
            *table = vec![Segment::new(0, job.total_size.unwrap_or(u64::MAX))];
        }
        for s in table.iter_mut() {
            s.active = false;
            s.takeover = false;
        }
        progress.bytes.store(
            job.progress_base + segments::downloaded(&table),
            Ordering::Relaxed,
        );
    }
    let bytes_at_start = progress.bytes.load(Ordering::Relaxed);

    let cfg = job.adaptive;
    let cap = if cfg.enabled {
        cfg.cap_for(job.connections)
    } else {
        job.connections.clamp(1, cfg.absolute_ceiling)
    };
    let initial = if segmented && cfg.enabled {
        cfg.starting_connections(job.learned_start, cap)
    } else if segmented {
        cap
    } else {
        1
    };
    let adaptive = Arc::new(AdaptiveState::new(cfg, initial, cap));

    let file = Arc::new(file);
    let child = cancel.child_token();
    let workers = if segmented { initial.max(1) } else { 1 };
    let worker_args = WorkerArgs {
        shared: shared.clone(),
        job: job.clone(),
        segs: segs.clone(),
        progress: progress.clone(),
        file: file.clone(),
        cancel: child.clone(),
        segmented,
        adaptive: adaptive.clone(),
    };
    let mut set: JoinSet<anyhow::Result<()>> = JoinSet::new();
    for _ in 0..workers {
        worker_args.spawn(&mut set);
    }

    // supervisor timers: throughput scaling and stall rescue. The first
    // tick of an interval fires at once, so both are consumed here and
    // the first real measurement covers a full window.
    let mut scale_tick = tokio::time::interval(cfg.measure_window.max(Duration::from_millis(50)));
    scale_tick.tick().await;
    let mut stall_tick = tokio::time::interval(Duration::from_secs(1));
    stall_tick.tick().await;
    let mut last_bytes = bytes_at_start;

    let mut first_error: Option<anyhow::Error> = None;
    loop {
        if set.is_empty() {
            break;
        }
        tokio::select! {
            res = set.join_next() => {
                let Some(res) = res else { break };
                let res = res
                    .map_err(|e| anyhow!("worker crashed: {e}"))
                    .and_then(|r| r);
                if let Err(e) = res {
                    if first_error.is_none() && !cancel.is_cancelled() {
                        first_error = Some(e);
                    }
                    child.cancel();
                }
            }
            _ = scale_tick.tick(), if segmented && cfg.enabled => {
                let now_bytes = progress.bytes.load(Ordering::Relaxed);
                let window_secs = cfg.measure_window.as_secs_f64().max(0.05);
                let bps = now_bytes.saturating_sub(last_bytes) as f64 / window_secs;
                last_bytes = now_bytes;
                let errors = adaptive.window_errors.swap(0, Ordering::Relaxed);
                let decision = adaptive.scaler.lock().unwrap().observe(bps, errors);
                adaptive.sync_target();
                if decision == crate::adaptive::ScaleDecision::Add {
                    worker_args.spawn(&mut set);
                }
            }
            _ = stall_tick.tick(), if segmented && cfg.enabled => {
                let snapshot: Vec<(usize, u64, bool)> = {
                    let table = segs.lock().unwrap();
                    table
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i, s.pos, s.active))
                        .collect()
                };
                let stalled = adaptive
                    .monitor
                    .lock()
                    .unwrap()
                    .evaluate(&snapshot, Instant::now());
                if !stalled.is_empty() {
                    let mut table = segs.lock().unwrap();
                    for idx in &stalled {
                        if let Some(s) = table.get_mut(*idx) {
                            s.takeover = true;
                        }
                    }
                    adaptive
                        .rescues
                        .fetch_add(stalled.len() as u32, Ordering::Relaxed);
                }
            }
        }
    }
    file.sync_all().ok();

    if cfg.enabled {
        if let Some(report) = &job.report {
            let scaler = adaptive.scaler.lock().unwrap();
            let mut r = report.lock().unwrap();
            r.best_target = scaler.best_target();
            r.final_target = scaler.target();
            r.min_target = scaler.min_target();
            r.peak_target = scaler.peak_target();
            r.best_throughput_bps = scaler.best_throughput();
            r.rate_limit_hits = scaler.rate_limit_hits();
            r.rescues = adaptive.rescues.load(Ordering::Relaxed);
            r.segments_final = segs.lock().unwrap().len();
            r.bytes_downloaded = progress
                .bytes
                .load(Ordering::Relaxed)
                .saturating_sub(bytes_at_start);
            r.avg_first_byte_ms = adaptive.avg_first_byte_ms();
        }
    }

    if cancel.is_cancelled() {
        return Err(Cancelled.into());
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    let table = segs.lock().unwrap();
    if segmented && !segments::all_finished(&table) {
        bail!("download ended before all parts were received");
    }
    Ok(())
}

struct Worker {
    shared: Arc<Shared>,
    job: HttpJob,
    segs: SegTable,
    progress: Arc<Progress>,
    file: Arc<File>,
    cancel: CancellationToken,
    segmented: bool,
    adaptive: Arc<AdaptiveState>,
}

impl Worker {
    async fn run(self) -> anyhow::Result<()> {
        let mut guard = WorkerGuard::new(&self.adaptive);
        loop {
            // the pool shrank below the number of live workers: exit
            // instead of taking more work
            if guard.shrink() {
                return Ok(());
            }
            let idx = {
                let mut table = self.segs.lock().unwrap();
                if self.segmented {
                    let idx = if self.adaptive.cfg.enabled {
                        segments::claim_work_split(
                            &mut table,
                            self.adaptive.cfg.dynamic_split_min_remaining,
                        )
                    } else {
                        segments::claim_work(&mut table, self.job.min_split)
                    };
                    if let Some(i) = idx {
                        table[i].takeover = false;
                    }
                    idx
                } else {
                    let s = &mut table[0];
                    if s.active || s.is_finished() {
                        None
                    } else {
                        s.active = true;
                        Some(0)
                    }
                }
            };
            let Some(idx) = idx else { return Ok(()) };
            if self.segmented && self.adaptive.cfg.enabled {
                let pos = self.segs.lock().unwrap()[idx].pos;
                self.adaptive
                    .monitor
                    .lock()
                    .unwrap()
                    .start(idx, pos, Instant::now());
            }
            let result = self.download_segment(idx).await;
            {
                let mut table = self.segs.lock().unwrap();
                let finished = table.get(idx).map(|s| s.is_finished()).unwrap_or(false);
                if let Some(s) = table.get_mut(idx) {
                    s.active = false;
                }
                if finished && self.segmented && self.adaptive.cfg.enabled {
                    self.adaptive.monitor.lock().unwrap().finish(idx);
                }
            }
            result?;
        }
    }

    /// Download one segment, retrying with back-off and continuing from where
    /// the previous attempt stopped.
    async fn download_segment(&self, idx: usize) -> anyhow::Result<()> {
        let mut attempt = 0u32;
        let mut net_tries = 0u32;
        loop {
            if self.cancel.is_cancelled() {
                return Err(Cancelled.into());
            }
            let before = self.segs.lock().unwrap()[idx].pos;
            let outcome = tokio::select! {
                r = self.fetch_once(idx) => r,
                _ = self.cancel.cancelled() => return Err(Cancelled.into()),
            };
            match outcome {
                Ok(()) => {
                    if self.segs.lock().unwrap()[idx].is_finished() {
                        return Ok(());
                    }
                    attempt = 0; // one request done, ask for the next piece
                }
                Err(e) => {
                    // rescue: the segment was handed to a fresh
                    // connection. Its progress stands; leave it idle
                    // for the next claim, from the last byte written.
                    if e.chain().any(|c| c.is::<Takeover>()) {
                        return Ok(());
                    }
                    // rate limiting: honour the asked wait (with a
                    // small spread), halve the connections, and retry
                    // from the same position without using up retries
                    let rate_limited = e
                        .chain()
                        .find_map(|c| c.downcast_ref::<RateLimited>())
                        .map(|rl| rl.retry_after);
                    if let Some(retry_after) = rate_limited {
                        let asked =
                            retry_after.unwrap_or(self.adaptive.cfg.rate_limit_default_backoff);
                        let wait = jittered(asked, self.adaptive.cfg.jitter_fraction);
                        self.adaptive.window_errors.fetch_add(1, Ordering::Relaxed);
                        self.adaptive.halve();
                        tokio::select! {
                            _ = tokio::time::sleep(wait) => {},
                            _ = self.cancel.cancelled() => return Err(Cancelled.into()),
                        }
                        continue;
                    }
                    let after = self.segs.lock().unwrap()[idx].pos;
                    if after > before {
                        attempt = 0; // it was making progress, don't give up yet
                        net_tries = 0;
                    }
                    let network = is_network_error(&e);
                    let status = http_status_of(&e);
                    if network {
                        // no internet / reset / stall: wait as long as it takes,
                        // the part keeps what it already has
                        net_tries += 1;
                        self.adaptive.window_errors.fetch_add(1, Ordering::Relaxed);
                        // repeated resets step the connection count down
                        self.adaptive.note_reset();
                        self.progress.waiting.store(true, Ordering::Relaxed);
                    } else {
                        attempt += 1;
                        // an expired or removed link won't come back by itself
                        let gone = matches!(status, Some(401 | 403 | 404 | 410));
                        let limit = if gone {
                            self.job.retries.min(2)
                        } else {
                            self.job.retries
                        };
                        if attempt > limit {
                            if gone {
                                return Err(e.context(
                                    "the link has expired or the file was removed - use \"Refresh download address\"",
                                ));
                            }
                            return Err(e);
                        }
                    }
                    if !self.segmented {
                        // can't resume without ranges: restart from zero
                        let mut table = self.segs.lock().unwrap();
                        let s = &mut table[idx];
                        self.progress.bytes.fetch_sub(s.done(), Ordering::Relaxed);
                        s.pos = s.start;
                        let _ = self.file.set_len(0);
                    }
                    let backoff = if network {
                        reconnect_delay(net_tries)
                    } else {
                        Duration::from_secs((1u64 << attempt.min(5)).min(30))
                    };
                    tokio::select! {
                        _ = tokio::time::sleep(backoff) => {},
                        // another part got through: the internet is back
                        _ = self.progress.back_online.notified() => {},
                        _ = self.cancel.cancelled() => return Err(Cancelled.into()),
                    }
                }
            }
        }
    }

    async fn fetch_once(&self, idx: usize) -> anyhow::Result<()> {
        let (pos, end) = {
            let table = self.segs.lock().unwrap();
            (table[idx].pos, table[idx].end)
        };
        if pos >= end {
            return Ok(());
        }
        // where this single request stops (the segment may need several)
        let req_end = match self.job.max_request {
            Some(limit) if self.segmented => end.min(pos + limit.max(1)),
            _ => end,
        };
        let mut rb = request(
            &self.shared.client(),
            &self.job.url,
            &self.job.headers,
            &self.job.user_agent,
        );
        if self.segmented {
            rb = rb.header(RANGE, format!("bytes={}-{}", pos, req_end - 1));
        }
        let request_started = Instant::now();
        let mut resp = rb.send().await?;
        let status = resp.status();
        if matches!(status.as_u16(), 429 | 503) {
            return Err(RateLimited::from_headers(status.as_u16(), resp.headers()).into());
        }
        if self.segmented {
            if status != StatusCode::PARTIAL_CONTENT {
                if status.is_client_error() || status.is_server_error() {
                    return Err(HttpStatus(status.as_u16()).into());
                }
                bail!(
                    "server did not accept the byte range (HTTP {})",
                    status.as_u16()
                );
            }
        } else if !status.is_success() {
            return Err(HttpStatus(status.as_u16()).into());
        }
        let _guard = ConnGuard::new(&self.progress);

        // Poll in short slices: a connection that stops sending must
        // be closable promptly when the stall monitor flags its
        // segment, while a merely slow connection still gets the full
        // stall timeout before it is declared dead.
        let mut idle = Duration::ZERO;
        let mut first_byte_recorded = false;
        loop {
            let chunk = match tokio::time::timeout(CHUNK_POLL, resp.chunk()).await {
                Ok(r) => r?,
                Err(_) => {
                    idle += CHUNK_POLL;
                    if self.cancel.is_cancelled() {
                        return Err(Cancelled.into());
                    }
                    let flagged = self.segs.lock().unwrap()[idx].takeover;
                    if flagged {
                        return Err(Takeover.into());
                    }
                    if idle >= STALL_TIMEOUT {
                        return Err(NetDrop("connection timed out").into());
                    }
                    continue;
                }
            };
            let Some(chunk) = chunk else { break };
            idle = Duration::ZERO;
            if !first_byte_recorded {
                first_byte_recorded = true;
                self.adaptive.record_first_byte(request_started.elapsed());
            }
            self.shared.limiter.acquire(chunk.len()).await;
            self.job.limiter.acquire(chunk.len()).await;
            let (finished, flagged, new_pos) = {
                let mut table = self.segs.lock().unwrap();
                let s = &mut table[idx];
                // `end` may have moved down because another worker split us
                let stop = s.end.min(req_end);
                let room = stop.saturating_sub(s.pos);
                let take = (chunk.len() as u64).min(room) as usize;
                if take > 0 {
                    write_at(&self.file, &chunk[..take], s.pos)?;
                    s.pos += take as u64;
                    self.progress
                        .bytes
                        .fetch_add(take as u64, Ordering::Relaxed);
                    self.progress.online();
                }
                (s.pos >= stop, s.takeover, s.pos)
            };
            if self.segmented && self.adaptive.cfg.enabled {
                self.adaptive
                    .monitor
                    .lock()
                    .unwrap()
                    .sample(idx, new_pos, Instant::now());
            }
            if finished {
                return Ok(()); // dropping `resp` closes the connection
            }
            if flagged {
                // the stall monitor flagged this segment while its
                // bytes were still flowing: close the connection
                // anyway, the segment resumes from `pos`
                return Err(Takeover.into());
            }
        }

        // the server closed the stream
        let mut table = self.segs.lock().unwrap();
        let s = &mut table[idx];
        if !self.segmented && (self.job.total_size.is_none() || s.end == u64::MAX) {
            s.end = s.pos; // unknown size: end of stream means done
            return Ok(());
        }
        if s.pos >= s.end.min(req_end) {
            Ok(())
        } else {
            Err(NetDrop("connection closed early").into())
        }
    }
}
