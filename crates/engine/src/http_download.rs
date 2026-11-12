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

use crate::adaptive::{AdaptiveConfig, AdaptiveReport, RetryPolicy, Scaler, SegmentMonitor};
use crate::fileio::{check_not_hardlinked, make_sparse, refuse_linked, write_at};
use crate::http::{
    http_status_of, is_network_error, reconnect_delay, request, ConnGuard, HttpStatus, NetDrop,
    Progress, RateLimited, SendOrdered, Shared,
};
use crate::model::RequestHeaders;
use crate::segments::{self, Segment};
use anyhow::{anyhow, bail};
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, IF_RANGE, RANGE};
use reqwest::StatusCode;
use std::collections::VecDeque;
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

/// How long a request waits for the server's answer before it is made again.
const HEADERS_TIMEOUT: Duration = Duration::from_secs(20);

/// A connection much slower than the others is dropped and made again (the
/// server often gives a new one full speed): checked over this long...
const SLOW_WINDOW: Duration = Duration::from_secs(8);
/// ...when it brings less than this...
const SLOW_BYTES_PER_SEC: u64 = 48 * 1024;
/// ...and less than this part of what one connection brings on average.
const SLOW_SHARE: u64 = 6;

/// A segmented HTTP file download: URL, request data, part file, and limits.
#[derive(Clone)]
pub struct HttpJob {
    pub url: String,
    pub headers: RequestHeaders,
    pub user_agent: String,
    pub total_size: Option<u64>,
    pub resumable: bool,
    /// The strong ETag the probe saw, when it saw one: sent back as
    /// `If-Range` when a segment resumes mid-way, so a changed file is
    /// never mixed with the old bytes. Weak ETags (`W/...`) are never
    /// sent: they prove nothing about the bytes.
    pub etag: Option<String>,
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

/// Error returned when the pool shrank below the live workers: this one
/// stops at once, mid-segment if needed; its part stays in the table for
/// the others. Not a failure: the download loop treats it as "this worker
/// is done".
#[derive(Debug)]
pub struct Leave;
impl std::fmt::Display for Leave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("fewer connections wanted")
    }
}
impl std::error::Error for Leave {}

/// Error returned when a connection is much slower than the others and
/// is dropped to be made again. Typed on purpose: the old code matched
/// the text "slow connection, made again", which is exactly the kind of
/// fragile string match a rename would silently break — and the
/// supervisor counts these drops to decide whether the site itself is
/// shedding connections (see `AdaptiveState::note_slow_drop`).
#[derive(Debug)]
pub struct SlowDrop;
impl std::fmt::Display for SlowDrop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("slow connection, made again")
    }
}
impl std::error::Error for SlowDrop {}

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
    /// when slow connections were dropped, newest last (for the
    /// "the site sheds connections" rule below)
    slow_drops: Mutex<VecDeque<Instant>>,
    /// bytes downloaded when the current slow-drop window started
    slow_drop_start_bytes: AtomicU64,
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
            slow_drops: Mutex::new(VecDeque::new()),
            slow_drop_start_bytes: AtomicU64::new(0),
        }
    }

    fn sync_target(&self) {
        let target = self.scaler.lock().unwrap().target();
        self.target.store(target, Ordering::Relaxed);
    }

    /// Enforce the connection count the user wants right now (0 = let the
    /// scaler decide): returns the enforced target, if any. While the user
    /// holds the count the scaler does not probe; the pool shrinks through
    /// the workers' own check and grows through the supervisor spawning.
    fn apply_want(&self, want: u32) -> Option<u32> {
        if want == 0 {
            return None;
        }
        let mut scaler = self.scaler.lock().unwrap();
        // the held count may run past the count the download started
        // with, up to the per-host and absolute ceilings
        let target = self.cfg.cap_for(want);
        scaler.set_target_external(target);
        drop(scaler);
        self.sync_target();
        Some(target)
    }

    fn live(&self) -> u32 {
        self.live.load(Ordering::Relaxed)
    }

    fn target(&self) -> u32 {
        self.target.load(Ordering::Relaxed)
    }

    /// The most connections a count the user holds may ask for: the
    /// per-host and absolute ceilings, not the count the download
    /// started with (that one only sets where the scaler begins).
    fn want_cap(&self) -> u32 {
        self.cfg.cap_for(u32::MAX)
    }

    fn halve(&self) {
        self.scaler.lock().unwrap().halve();
        self.sync_target();
    }

    /// One slow connection was dropped. Returns true when the site is
    /// shedding connections on purpose: at least 6 slow drops inside
    /// 60 seconds while the download still made at least 1 MiB of
    /// progress — then the pool halves, like after a 429. Counted,
    /// not matched on error text.
    fn note_slow_drop(&self, bytes_now: u64) -> bool {
        let mut drops = self.slow_drops.lock().unwrap();
        let now = Instant::now();
        while drops
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            drops.pop_front();
        }
        if drops.is_empty() {
            self.slow_drop_start_bytes
                .store(bytes_now, Ordering::Relaxed);
        }
        drops.push_back(now);
        let made = bytes_now.saturating_sub(self.slow_drop_start_bytes.load(Ordering::Relaxed));
        if drops.len() >= 6 && made >= 1024 * 1024 {
            drops.clear();
            true
        } else {
            false
        }
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

/// Is there work a new connection could take (an untouched part, or one
/// big enough to split)? Spawning without any only makes a connection
/// that exits at once.
fn has_work(segs: &SegTable, min_split: u64) -> bool {
    let map = segs.lock().unwrap();
    map.iter()
        .any(|s| !s.is_finished() && (!s.active || s.remaining() >= min_split * 2))
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
    fn shrink_to(&mut self, target: u32) -> bool {
        loop {
            let live = self.state.live.load(Ordering::Relaxed);
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

    /// This worker already gave its slot back (it left mid-segment):
    /// it must exit instead of taking new work.
    fn released(&self) -> bool {
        self.released
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
    // the state file (.sgpart) is ours alone: refuse a symlink or a
    // hardlink planted in its place before anything is written to it
    refuse_linked(&job.part_path)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&job.part_path)?;
    // after opening, refuse a hardlink too (a second name for the same
    // file would let another process read half-written bytes)
    check_not_hardlinked(&file)?;

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
    // the count the user wants is checked often (not once a measure
    // window): a change takes effect within a fraction of a second
    let mut want_tick = tokio::time::interval(Duration::from_millis(100));
    want_tick.tick().await;
    let mut last_bytes = bytes_at_start;
    let mut applied_want = 0u32;

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
            _ = want_tick.tick(), if segmented => {
                // the user changed the connection count while it runs:
                // hold that count instead of probing (also when the
                // adaptive engine is off: the count is then fixed at
                // it). Workers shrink on their own against the same
                // count; this arm grows the pool back up to it.
                let want = progress.want_connections.load(Ordering::Relaxed);
                if want != 0 {
                    if let Some(target) = adaptive.apply_want(want) {
                        for _ in adaptive.live()..target {
                            if !has_work(&segs, job.min_split) {
                                break;
                            }
                            worker_args.spawn(&mut set);
                        }
                        if applied_want != want {
                            applied_want = want;
                            last_bytes = progress.bytes.load(Ordering::Relaxed);
                        }
                    }
                } else {
                    applied_want = 0;
                }
            }
            _ = scale_tick.tick(), if segmented => {
                // while the user holds the count, the scaler does not
                // probe (observe would move the target it holds)
                if progress.want_connections.load(Ordering::Relaxed) != 0 {
                    continue;
                }
                if !cfg.enabled {
                    continue;
                }
                let now_bytes = progress.bytes.load(Ordering::Relaxed);
                let window_secs = cfg.measure_window.as_secs_f64().max(0.05);
                let bps = now_bytes.saturating_sub(last_bytes) as f64 / window_secs;
                last_bytes = now_bytes;
                let errors = adaptive.window_errors.swap(0, Ordering::Relaxed);
                // how many segments are actually being fetched: when it
                // is below the target, work finished — the aggregate fall
                // that follows is done work, not a useless connection
                let active = segs
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|s| s.active)
                    .count() as u32;
                let decision = adaptive
                    .scaler
                    .lock()
                    .unwrap()
                    .observe_work(bps, errors, active);
                adaptive.sync_target();
                if decision == crate::adaptive::ScaleDecision::Add
                    && has_work(&segs, job.min_split)
                {
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
    /// Connections the pool should hold now: the count the user wants,
    /// when they set one, else the scaler's target. Read straight from
    /// the request (not through the supervisor's tick), so a change
    /// takes effect at the next chunk, not a whole measure window later.
    fn effective_target(&self) -> u32 {
        let want = self.progress.want_connections.load(Ordering::Relaxed);
        if want == 0 {
            self.adaptive.target()
        } else {
            want.clamp(1, self.adaptive.want_cap())
        }
    }

    async fn run(self) -> anyhow::Result<()> {
        let mut guard = WorkerGuard::new(&self.adaptive);
        loop {
            // the pool shrank below the number of live workers: exit
            // instead of taking more work
            if guard.shrink_to(self.effective_target()) {
                return Ok(());
            }
            let idx = {
                let mut table = self.segs.lock().unwrap();
                if self.segmented {
                    let idx = if self.adaptive.cfg.enabled {
                        // work-stealing: the free connection takes half of
                        // the segment that will take longest to finish
                        // (remaining bytes over current speed), not the
                        // biggest one; a stalled connection is split first
                        let speeds: Vec<(usize, f64)> = {
                            let monitor = self.adaptive.monitor.lock().unwrap();
                            let now = Instant::now();
                            table
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| s.active && !s.is_finished())
                                .filter_map(|(i, s)| monitor.speed(i, s.pos, now).map(|sp| (i, sp)))
                                .collect()
                        };
                        // While the pool is still growing toward the count
                        // it should hold, splits may go down to the job's
                        // own minimum: the supervisor only spawns a worker
                        // when such a piece exists (see `has_work`), so a
                        // higher floor here would leave the new worker with
                        // nothing to take. At full strength the 1 MB floor
                        // stands: splitting a nearly-done piece is not
                        // worth a fresh connection.
                        let floor = if self.adaptive.live() < self.effective_target() {
                            self.job.min_split
                        } else {
                            self.adaptive.cfg.dynamic_split_min_remaining
                        };
                        segments::claim_work_eta(&mut table, floor, &speeds)
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
            let result = self.download_segment(idx, &mut guard).await;
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
            if guard.released() {
                // left mid-segment because fewer connections are wanted
                // now: exit instead of taking new work
                return Ok(());
            }
            result?;
        }
    }

    /// Download one segment, retrying with back-off and continuing from where
    /// the previous attempt stopped.
    async fn download_segment(&self, idx: usize, guard: &mut WorkerGuard) -> anyhow::Result<()> {
        let mut attempt = 0u32;
        let mut net_tries = 0u32;
        // 429 answers in a row for this part, with no progress between
        // them: a server that rate-limits every request will never let
        // this part finish, so the run gives up after
        // `rate_limit_give_up_after` instead of waiting forever.
        let mut rate_limited_in_a_row = 0u32;
        loop {
            if self.cancel.is_cancelled() {
                return Err(Cancelled.into());
            }
            let before = self.segs.lock().unwrap()[idx].pos;
            let outcome = tokio::select! {
                r = self.fetch_once(idx, guard) => r,
                _ = self.cancel.cancelled() => return Err(Cancelled.into()),
            };
            match outcome {
                Ok(()) => {
                    if self.segs.lock().unwrap()[idx].is_finished() {
                        return Ok(());
                    }
                    attempt = 0; // one request done, ask for the next piece
                    rate_limited_in_a_row = 0;
                }
                Err(e) => {
                    // fewer connections wanted: stop at once, the part
                    // stays in the table for the others
                    if e.chain().any(|c| c.is::<Leave>()) {
                        return Ok(());
                    }
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
                        rate_limited_in_a_row += 1;
                        if rate_limited_in_a_row
                            >= self.adaptive.cfg.rate_limit_give_up_after.max(1)
                        {
                            return Err(anyhow::anyhow!(
                                "the server kept answering 429 (rate limit) {} times in a row \
                                for the same part; the download was stopped instead of waiting \
                                forever - try again later",
                                rate_limited_in_a_row
                            ));
                        }
                        let wait = RetryPolicy::default().wait(
                            1,
                            Some(
                                retry_after.unwrap_or(self.adaptive.cfg.rate_limit_default_backoff),
                            ),
                        );
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
                        rate_limited_in_a_row = 0;
                    }
                    // a slow connection was dropped: if the site keeps
                    // shedding them, halve the pool like after a 429
                    let slow_drop = e.chain().any(|c| c.is::<SlowDrop>());
                    if slow_drop
                        && self
                            .adaptive
                            .note_slow_drop(self.progress.bytes.load(Ordering::Relaxed))
                    {
                        self.adaptive.halve();
                    }
                    let network = is_network_error(&e) || slow_drop;
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
                        RetryPolicy::default().wait(attempt, None)
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

    async fn fetch_once(&self, idx: usize, guard: &mut WorkerGuard) -> anyhow::Result<()> {
        let (pos, end, start) = {
            let table = self.segs.lock().unwrap();
            (table[idx].pos, table[idx].end, table[idx].start)
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
            // resuming mid-segment with a strong ETag: the server must
            // answer 206 for these exact bytes, or 412 for a changed
            // file — never a silent mix of old and new bytes
            if pos > start {
                if let Some(etag) = &self.job.etag {
                    if !etag.starts_with("W/") {
                        rb = rb.header(IF_RANGE, etag.as_str());
                    }
                }
            }
        }
        let request_started = Instant::now();
        let mut resp = match tokio::time::timeout(HEADERS_TIMEOUT, rb.send_ordered()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                // a single request that fails is a passing failure;
                // anything else is too
                return Err(NetDrop(format!("the server did not answer ({e})")).into());
            }
            Err(_) => {
                return Err(NetDrop("the server did not answer (timed out)".into()).into());
            }
        };
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
            // a ranged answer that is a web page is an error page, not
            // the file: never write HTML into the part file
            if resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| {
                    ct.split(';')
                        .next()
                        .is_some_and(|t| t.trim().eq_ignore_ascii_case("text/html"))
                })
            {
                bail!("the server sent a web page instead of the file");
            }
            // the part must be the one asked for (and of the same file):
            // anything else is never written
            if let Some((start, total)) = resp
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_content_range)
            {
                if start != pos {
                    return Err(NetDrop("the server sent another part of the file".into()).into());
                }
                if let (Some(total), Some(known)) = (total, self.job.total_size) {
                    if total != known {
                        bail!(
                            "the link now gives another file: not the same file ({total} bytes instead of {known})"
                        );
                    }
                }
            }
        } else if !status.is_success() {
            return Err(HttpStatus(status.as_u16()).into());
        }
        let _guard = ConnGuard::new(&self.progress);

        // how much this connection brings, over the last slow window
        let mut window = Instant::now();
        let mut mine = 0u64;
        let mut all_before = self.progress.bytes.load(Ordering::Relaxed);
        let mut slow_windows = 0u32;

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
                        return Err(NetDrop("connection timed out".into()).into());
                    }
                    continue;
                }
            };
            let Some(chunk) = chunk else { break };
            // Only when the user holds a count: the surplus goes at once,
            // mid-segment if needed. The scaler's own target changes
            // (probe/shed) act at segment boundaries instead, as before:
            // a shed after the fast work is done must not strand the
            // slow remainder on one connection.
            if self.segmented
                && self.progress.want_connections.load(Ordering::Relaxed) != 0
                && guard.shrink_to(self.effective_target())
            {
                // fewer connections wanted: this one goes at once, its
                // part stays in the table for the others (the chunk is
                // dropped, not written, so it is fetched again later)
                return Err(Leave.into());
            }
            idle = Duration::ZERO;
            if !first_byte_recorded {
                first_byte_recorded = true;
                self.adaptive.record_first_byte(request_started.elapsed());
            }
            if self.segmented {
                // a connection much slower than the others is dropped
                // and made again (the server often gives a new one full
                // speed): two slow windows in a row, and only while no
                // speed limit is holding the download back
                mine += chunk.len() as u64;
                let spent = window.elapsed();
                if spent >= SLOW_WINDOW {
                    let all_now = self.progress.bytes.load(Ordering::Relaxed);
                    let conns = self.progress.connections.load(Ordering::Relaxed).max(1) as u64;
                    let secs = spent.as_secs().max(1);
                    let my_rate = mine / secs;
                    let average = all_now.saturating_sub(all_before) / secs / conns;
                    let limited = self.shared.limiter.rate() > 0 || self.job.limiter.rate() > 0;
                    if !limited
                        && conns >= 2
                        && my_rate < SLOW_BYTES_PER_SEC
                        && my_rate * SLOW_SHARE < average
                    {
                        slow_windows += 1;
                    } else {
                        slow_windows = 0;
                    }
                    if slow_windows >= 2 {
                        return Err(SlowDrop.into());
                    }
                    window = Instant::now();
                    mine = 0;
                    all_before = all_now;
                }
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
            Err(NetDrop("connection closed early".into()).into())
        }
    }
}

/// Read a `Content-Range` answer: "bytes 100-199/1000" is (100, Some(1000));
/// "bytes 0-0/*" is (0, None). Only the start and the total size are used.
pub fn parse_content_range(header: &str) -> Option<(u64, Option<u64>)> {
    let rest = header.strip_prefix("bytes ")?;
    let (range, total) = rest.split_once('/')?;
    let start: u64 = range.split_once('-')?.0.parse().ok()?;
    let total = if total == "*" {
        None
    } else {
        Some(total.parse().ok()?)
    };
    Some((start, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_content_range_reads_start_and_total() {
        assert_eq!(
            parse_content_range("bytes 100-199/1000"),
            Some((100, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 0-0/*"), Some((0, None)));
        assert_eq!(parse_content_range("items 0-9/10"), None);
    }

    #[test]
    fn slow_drop_rule_counts_drops_not_error_text() {
        // P6: the "site sheds connections" rule is a counter, not a
        // match on the "slow connection" message. Six slow drops inside
        // 60 s while the download still made >= 1 MiB of progress halve
        // the pool; fewer drops, or drops with no progress, do not.
        let state = AdaptiveState::new(AdaptiveConfig::default(), 8, 16);
        // five drops with progress: not enough drops yet
        assert!(!state.note_slow_drop(0));
        for _ in 0..4 {
            assert!(!state.note_slow_drop(2 * 1024 * 1024));
        }
        // the sixth, with 2 MiB made: the site is shedding connections
        assert!(state.note_slow_drop(2 * 1024 * 1024));
        // the trigger clears the window: the next drop starts a new one
        assert!(!state.note_slow_drop(2 * 1024 * 1024 + 100));

        // six drops but almost no progress: a stalled download, not a
        // shedding site — no halving
        let quiet = AdaptiveState::new(AdaptiveConfig::default(), 8, 16);
        for _ in 0..6 {
            assert!(!quiet.note_slow_drop(100));
        }
    }
}
