//! Adaptive download behaviour: how many connections a file uses, how a
//! slow segment is handed to a fresh connection, and what is remembered
//! about a host for the next download.
//!
//! Design (own words, from first principles):
//!
//! * Connection scaling. A download starts with a few connections and
//!   measures the aggregate throughput over a short window. A connection
//!   is added only while the measured throughput keeps rising by a clear
//!   margin; two windows without that rise stop the growth, and a falling
//!   throughput removes a connection. Growth is capped per host, with a
//!   lower absolute ceiling for the whole program.
//! * Backoff. A rate-limit answer (429/503) is honoured: the wait the
//!   server asks for is waited out (with a small random spread), and the
//!   connection count is halved. Repeated connection resets also step the
//!   count down. Retries always continue from the last byte written.
//! * Dynamic splitting. When a worker runs out of work it takes the
//!   active segment with the most bytes still to fetch and cuts it in
//!   half at the midpoint, provided enough remains to be worth cutting.
//!   The cut happens while holding the segment-table lock, so the
//!   original worker's end boundary moves atomically with the creation
//!   of the new segment, and the new segment exists in the table before
//!   any byte of its range is written.
//! * Stall rescue. Each segment's speed is tracked against a slow-moving
//!   baseline of its own healthy speed and against the other active
//!   segments. A segment that stays far below that reference for a
//!   sustained period, after a grace period from its start, is flagged:
//!   its connection is closed and the segment is re-requested from the
//!   last byte written. Written bytes are never discarded.
//! * Per-host learning, local only. The connection count that gave the
//!   best throughput and the average segment size are kept as
//!   exponentially weighted averages in the local settings file, one
//!   entry per host, expiring after thirty days. The next download from
//!   the same host starts from those values. Nothing is sent anywhere.
//!
//! Nothing in this module performs network I/O. The download loop in
//! `http_download` owns the timers and the workers; this module owns the
//! decisions, so every decision is unit-testable without a server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// All tunable defaults of the adaptive engine, in one place. The
/// download loop uses these values as-is; there is no settings UI for
/// them in this phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveConfig {
    /// Master switch. When false the download uses a fixed connection
    /// count (`HttpJob::connections`) and no scaling, splitting beyond
    /// the existing segment table, or stall rescue takes place.
    pub enabled: bool,
    /// Connections a download starts with when nothing is known about
    /// the host (spec: start with 2-4).
    pub initial_connections: u32,
    /// Never start with fewer than this (unless the cap is lower).
    pub min_initial_connections: u32,
    /// How long one throughput measurement lasts.
    pub measure_window: Duration,
    /// Throughput must rise by at least this factor to add a connection.
    pub gain_threshold: f64,
    /// Windows without that rise after which growth stops.
    pub no_gain_intervals: u32,
    /// Most connections the scaler will grow to for one host.
    pub per_host_max: u32,
    /// Absolute ceiling, kept from the previous engine behaviour.
    pub absolute_ceiling: u32,
    /// A segment is only cut when at least this many bytes remain in it.
    pub dynamic_split_min_remaining: u64,
    /// A segment is stalled when its recent speed stays below this
    /// fraction of the reference speed.
    pub stall_fraction: f64,
    /// How long a segment must stay below the fraction to be rescued.
    pub stall_seconds: Duration,
    /// No rescue before this long after the segment started.
    pub stall_grace: Duration,
    /// Wait used for a rate-limit answer that carries no Retry-After.
    pub rate_limit_default_backoff: Duration,
    /// A Retry-After longer than this is clamped to this.
    pub max_retry_after: Duration,
    /// Random spread added to backoff waits, as a fraction (0.25 = up
    /// to 25% longer, never shorter than the asked wait).
    pub jitter_fraction: f64,
    /// Weight of a new observation in the per-host averages.
    pub learning_alpha: f64,
    /// Per-host entries older than this are ignored.
    pub learning_expiry_days: u32,
    /// Consecutive connection resets after which the count steps down.
    pub reset_reduction_after: u32,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        AdaptiveConfig {
            enabled: true,
            initial_connections: 4,
            min_initial_connections: 2,
            measure_window: Duration::from_secs(3),
            gain_threshold: 1.10,
            no_gain_intervals: 2,
            per_host_max: 16,
            absolute_ceiling: 32,
            dynamic_split_min_remaining: 1024 * 1024,
            stall_fraction: 0.15,
            stall_seconds: Duration::from_secs(10),
            stall_grace: Duration::from_secs(10),
            rate_limit_default_backoff: Duration::from_secs(5),
            max_retry_after: Duration::from_secs(300),
            jitter_fraction: 0.25,
            learning_alpha: 0.3,
            learning_expiry_days: 30,
            reset_reduction_after: 3,
        }
    }
}

impl AdaptiveConfig {
    /// The cap the scaler may grow to for one download: the per-host
    /// maximum, the caller's connection count, and the absolute ceiling
    /// all apply, the smallest wins.
    pub fn cap_for(&self, wanted: u32) -> u32 {
        wanted
            .min(self.per_host_max)
            .min(self.absolute_ceiling)
            .max(1)
    }

    /// Connections to start with: the learned starting point when the
    /// caller has one, else the default, kept inside 2..=cap (or 1 when
    /// the cap itself is 1).
    pub fn starting_connections(&self, learned: Option<u32>, cap: u32) -> u32 {
        if cap <= 1 {
            return 1;
        }
        let start = learned.unwrap_or(self.initial_connections);
        start.clamp(self.min_initial_connections, cap)
    }
}

/// Add a random spread to a wait without any new dependency: the spread
/// comes from the system clock mixed with a process-wide counter, so
/// two waits in a row do not get the same spread.
static JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn jittered(wait: Duration, fraction: f64) -> Duration {
    if wait.is_zero() || fraction <= 0.0 {
        return wait;
    }
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos
        ^ JITTER_COUNTER
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let spread = (mixed % 1000) as f64 / 1000.0;
    wait.mul_f64(1.0 + fraction * spread)
}

/// Parse a `Retry-After` header value: either a number of seconds, or an
/// HTTP date. Returns `None` when the value is not understood. A wait in
/// the past means "now". Waits are clamped to `max` so a server cannot
/// park a download for hours on one answer.
pub fn parse_retry_after(value: &str, max: Duration) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs).min(max));
    }
    // HTTP date, e.g. "Wed, 21 Oct 2015 07:28:00 GMT".
    let date = chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .or_else(|| chrono::DateTime::parse_from_str(value, "%a, %d %b %Y %H:%M:%S %Z").ok())?;
    let now = Utc::now();
    let wait = date.signed_duration_since(now).num_seconds().max(0) as u64;
    Some(Duration::from_secs(wait).min(max))
}

/// What the scaler decided after one measurement window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleDecision {
    Add,
    Remove,
    Hold,
}

/// Throughput-driven connection scaling for one download.
///
/// The scaler works by probing: it adds a connection, then measures
/// whether the aggregate throughput rose. A flat throughput at the
/// starting count cannot tell the scaler anything — on a server that
/// caps each connection, the total is flat until another connection
/// is added, so waiting for a rise *before* adding would never add
/// at all. Growth therefore continues while added connections pay
/// off, and stops after two measurement windows without a clear gain.
#[derive(Debug)]
pub struct Scaler {
    cfg: AdaptiveConfig,
    target: u32,
    cap: u32,
    last_throughput: Option<f64>,
    no_gain: u32,
    stopped: bool,
    frozen_windows: u32,
    best_throughput: f64,
    best_target: u32,
    peak_target: u32,
    min_target: u32,
    consecutive_resets: u32,
    rate_limit_hits: u32,
}

impl Scaler {
    pub fn new(cfg: AdaptiveConfig, initial: u32, cap: u32) -> Self {
        let initial = initial.clamp(1, cap.max(1));
        Scaler {
            cfg,
            target: initial,
            cap: cap.max(1),
            last_throughput: None,
            no_gain: 0,
            stopped: false,
            frozen_windows: 0,
            best_throughput: 0.0,
            best_target: initial,
            peak_target: initial,
            min_target: initial,
            consecutive_resets: 0,
            rate_limit_hits: 0,
        }
    }

    pub fn target(&self) -> u32 {
        self.target
    }
    pub fn best_target(&self) -> u32 {
        self.best_target
    }
    pub fn best_throughput(&self) -> f64 {
        self.best_throughput
    }
    pub fn peak_target(&self) -> u32 {
        self.peak_target
    }
    pub fn min_target(&self) -> u32 {
        self.min_target
    }
    pub fn rate_limit_hits(&self) -> u32 {
        self.rate_limit_hits
    }

    fn set_target(&mut self, target: u32) {
        self.target = target.clamp(1, self.cap);
        self.peak_target = self.peak_target.max(self.target);
        self.min_target = self.min_target.min(self.target);
    }

    /// Probe with one more connection, unless growth stopped or the
    /// cap is reached.
    fn probe(&mut self) -> ScaleDecision {
        if self.stopped || self.target >= self.cap {
            return ScaleDecision::Hold;
        }
        self.set_target(self.target + 1);
        ScaleDecision::Add
    }

    /// One measurement window's aggregate throughput (bytes/second) and
    /// how many rate-limit / reset errors happened inside it.
    pub fn observe(&mut self, bytes_per_sec: f64, errors: u32) -> ScaleDecision {
        if errors == 0 {
            self.consecutive_resets = 0;
        }
        if self.frozen_windows > 0 {
            self.frozen_windows -= 1;
            self.last_throughput = Some(bytes_per_sec);
            return ScaleDecision::Hold;
        }
        if bytes_per_sec > self.best_throughput {
            self.best_throughput = bytes_per_sec;
            self.best_target = self.target;
        }
        let Some(last) = self.last_throughput else {
            // first window: only a baseline — probe once, so the next
            // window has something to compare against
            self.last_throughput = Some(bytes_per_sec);
            return self.probe();
        };
        self.last_throughput = Some(bytes_per_sec);
        if bytes_per_sec >= last * self.cfg.gain_threshold {
            // the last probe (or a change in conditions) paid off:
            // keep probing
            self.no_gain = 0;
            self.stopped = false;
            return self.probe();
        }
        if bytes_per_sec < last {
            // throughput fell: shed one connection
            self.no_gain += 1;
            if self.no_gain >= self.cfg.no_gain_intervals {
                self.stopped = true;
            }
            if self.target > 1 {
                self.set_target(self.target - 1);
                return ScaleDecision::Remove;
            }
            return ScaleDecision::Hold;
        }
        // flat: no clear gain from the last probe
        self.no_gain += 1;
        if self.no_gain >= self.cfg.no_gain_intervals {
            self.stopped = true;
            return ScaleDecision::Hold;
        }
        if self.stopped {
            return ScaleDecision::Hold;
        }
        // one flat window is not enough to stop: probe once more, and
        // stop only if that window is flat too
        self.probe()
    }

    /// A rate-limit answer: halve the connections and skip growth for
    /// the next window, so the server is not hammered straight back.
    pub fn halve(&mut self) {
        self.rate_limit_hits += 1;
        self.set_target(self.target / 2);
        self.frozen_windows = 1;
        self.no_gain = 0;
        self.stopped = false;
    }

    /// A connection reset. Returns true when enough resets in a row
    /// accumulated to step the count down by one.
    pub fn note_reset(&mut self) -> bool {
        self.consecutive_resets += 1;
        if self.consecutive_resets >= self.cfg.reset_reduction_after && self.target > 1 {
            self.consecutive_resets = 0;
            self.set_target(self.target - 1);
            return true;
        }
        false
    }
}

/// Per-segment speed state used by the stall monitor.
#[derive(Debug, Clone, Copy)]
struct SegSpeed {
    started_at: Instant,
    /// position and time of the last sample that fed the baseline
    window_pos: u64,
    window_started_at: Instant,
    /// slow-moving average of healthy speeds (bytes/second)
    baseline: f64,
    /// since when the segment has been below the stall fraction, if so
    below_since: Option<Instant>,
}

/// Tracks per-segment speeds and decides which segments are stalled.
/// Times are `Instant`s supplied by the caller, so tests can drive the
/// clock without waiting.
#[derive(Debug, Default)]
pub struct SegmentMonitor {
    cfg: AdaptiveConfig,
    states: HashMap<usize, SegSpeed>,
}

impl SegmentMonitor {
    pub fn new(cfg: AdaptiveConfig) -> Self {
        SegmentMonitor {
            cfg,
            states: HashMap::new(),
        }
    }

    /// A worker started (or re-started) this segment, at this position.
    pub fn start(&mut self, idx: usize, pos: u64, now: Instant) {
        self.states.insert(
            idx,
            SegSpeed {
                started_at: now,
                window_pos: pos,
                window_started_at: now,
                baseline: 0.0,
                below_since: None,
            },
        );
    }

    /// The segment is finished or abandoned: forget its speed history.
    pub fn finish(&mut self, idx: usize) {
        self.states.remove(&idx);
    }

    /// Bytes were written: remember the position. The baseline is only
    /// updated from windows that look healthy, so a slowing segment
    /// cannot drag its own reference down with it. The first bytes
    /// seed the baseline at once (with the window floored at 50 ms, so
    /// a burst that arrives in milliseconds does not read as infinite
    /// speed): without a seed, a segment that receives a fast burst
    /// and then goes silent would have no reference speed to be
    /// judged against at all.
    pub fn sample(&mut self, idx: usize, pos: u64, now: Instant) {
        let Some(state) = self.states.get_mut(&idx) else {
            return;
        };
        let elapsed = now.duration_since(state.window_started_at).as_secs_f64();
        if state.baseline == 0.0 && pos > state.window_pos {
            let effective = elapsed.max(0.05);
            state.baseline = (pos.saturating_sub(state.window_pos)) as f64 / effective;
            state.window_pos = pos;
            state.window_started_at = now;
            return;
        }
        if elapsed >= 1.0 {
            let speed = (pos.saturating_sub(state.window_pos)) as f64 / elapsed;
            if speed > 0.0 && speed >= state.baseline * self.cfg.stall_fraction {
                state.baseline = if state.baseline == 0.0 {
                    speed
                } else {
                    state.baseline * 0.8 + speed * 0.2
                };
            }
            state.window_pos = pos;
            state.window_started_at = now;
        }
    }

    /// Which of the given active segments are stalled right now.
    /// `segments` is (index, current position, active). A segment with
    /// no healthy baseline and no progressing peers is not stalled:
    /// when nothing moves at all, the network-retry path owns the
    /// problem, not the rescue path.
    pub fn evaluate(&mut self, segments: &[(usize, u64, bool)], now: Instant) -> Vec<usize> {
        let mut stalled = Vec::new();
        // reference speeds first, so one segment's check cannot move
        // another's reference mid-evaluation
        let mut refs: HashMap<usize, f64> = HashMap::new();
        for (idx, _, active) in segments {
            if !active {
                continue;
            }
            let Some(state) = self.states.get(idx) else {
                continue;
            };
            let peer_sum: f64 = segments
                .iter()
                .filter(|(other, _, other_active)| {
                    *other != *idx && *other_active && self.states.contains_key(other)
                })
                .map(|(other, _, _)| self.states[other].baseline)
                .sum();
            let peer_count = segments
                .iter()
                .filter(|(other, _, other_active)| {
                    *other != *idx && *other_active && self.states.contains_key(other)
                })
                .count();
            let peer_avg = if peer_count > 0 {
                peer_sum / peer_count as f64
            } else {
                0.0
            };
            refs.insert(*idx, peer_avg.max(state.baseline));
        }
        for (idx, pos, active) in segments {
            if !active {
                continue;
            }
            let Some(reference) = refs.get(idx).copied() else {
                continue;
            };
            let Some(state) = self.states.get_mut(idx) else {
                continue;
            };
            if reference <= 0.0 {
                state.below_since = None;
                continue;
            }
            if now.duration_since(state.started_at) < self.cfg.stall_grace {
                state.below_since = None;
                continue;
            }
            // recent speed over the current window; a segment that
            // received nothing for over a second is at zero
            let elapsed = now.duration_since(state.window_started_at).as_secs_f64();
            let recent = if elapsed >= 1.0 {
                let bytes = pos.saturating_sub(state.window_pos) as f64;
                let speed = bytes / elapsed;
                // roll the window forward so the next check measures
                // fresh time, keeping the baseline rule from sample()
                if speed > 0.0 && speed >= state.baseline * self.cfg.stall_fraction {
                    state.baseline = if state.baseline == 0.0 {
                        speed
                    } else {
                        state.baseline * 0.8 + speed * 0.2
                    };
                }
                state.window_pos = *pos;
                state.window_started_at = now;
                speed
            } else {
                // less than a second of window: do not judge yet
                continue;
            };
            if recent < reference * self.cfg.stall_fraction {
                let since = *state.below_since.get_or_insert(now);
                if now.duration_since(since) >= self.cfg.stall_seconds {
                    stalled.push(*idx);
                }
            } else {
                state.below_since = None;
            }
        }
        stalled
    }
}

/// What one download learned, for the per-host learning store. Kept in
/// the local settings file only; nothing about it ever leaves the
/// machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostLearning {
    /// Exponentially weighted average of the best connection counts.
    pub connections_ema: f64,
    /// Exponentially weighted average of the mean segment size, bytes.
    pub segment_size_ema: f64,
    pub samples: u32,
    pub updated_at: DateTime<Utc>,
}

impl HostLearning {
    pub fn new(connections: u32, segment_size: u64, now: DateTime<Utc>) -> Self {
        HostLearning {
            connections_ema: connections as f64,
            segment_size_ema: segment_size as f64,
            samples: 1,
            updated_at: now,
        }
    }

    /// Fold one more observation in. The first observation is taken
    /// as-is; later ones move the averages by `alpha` towards the new
    /// value, so old observations fade without ever vanishing at once.
    pub fn update(&mut self, connections: u32, segment_size: u64, alpha: f64, now: DateTime<Utc>) {
        let alpha = alpha.clamp(0.0, 1.0);
        self.connections_ema += alpha * (connections as f64 - self.connections_ema);
        self.segment_size_ema += alpha * (segment_size as f64 - self.segment_size_ema);
        self.samples += 1;
        self.updated_at = now;
    }

    pub fn expired(&self, now: DateTime<Utc>, expiry_days: u32) -> bool {
        now.signed_duration_since(self.updated_at).num_days() > expiry_days as i64
    }

    /// Starting connection count for the next download from this host.
    pub fn starting_connections(&self, cfg: &AdaptiveConfig) -> u32 {
        (self.connections_ema.round() as u32).clamp(cfg.min_initial_connections, cfg.per_host_max)
    }
}

/// The store key for a URL: its lowercase host, without any path, query,
/// user info, or port details. Learning is per host, not per file.
pub fn host_key(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default()
}

/// Summary of one adaptive run, filled in by the download loop for the
/// caller (the manager records the per-host learning from it).
#[derive(Debug, Default, Clone)]
pub struct AdaptiveReport {
    /// Connection count at the throughput peak.
    pub best_target: u32,
    /// Connection count when the download ended.
    pub final_target: u32,
    /// Lowest connection count reached (after rate-limit halving, ...).
    pub min_target: u32,
    /// Highest connection count reached.
    pub peak_target: u32,
    /// Best aggregate throughput seen, bytes/second.
    pub best_throughput_bps: f64,
    /// Mean time from request to first byte, milliseconds.
    pub avg_first_byte_ms: f64,
    /// Rate-limit answers honoured.
    pub rate_limit_hits: u32,
    /// Stalled segments handed to a fresh connection.
    pub rescues: u32,
    /// Segments in the table when the download ended.
    pub segments_final: usize,
    /// Bytes downloaded in this run (progress counter delta).
    pub bytes_downloaded: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AdaptiveConfig {
        AdaptiveConfig {
            measure_window: Duration::from_millis(100),
            stall_grace: Duration::from_secs(1),
            stall_seconds: Duration::from_secs(2),
            ..AdaptiveConfig::default()
        }
    }

    #[test]
    fn scaler_probes_then_stops_after_two_flat_windows() {
        let mut s = Scaler::new(cfg(), 4, 16);
        // first window sets the baseline and probes once
        assert_eq!(s.observe(1000.0, 0), ScaleDecision::Add);
        assert_eq!(s.target(), 5);
        // the probe paid off: keep probing
        assert_eq!(s.observe(1200.0, 0), ScaleDecision::Add);
        assert_eq!(s.target(), 6);
        // flat once: probe once more before giving up
        assert_eq!(s.observe(1200.0, 0), ScaleDecision::Add);
        assert_eq!(s.target(), 7);
        // flat twice in a row: stopped
        assert_eq!(s.observe(1200.0, 0), ScaleDecision::Hold);
        assert_eq!(s.target(), 7);
        // a small rise is not enough to resume
        assert_eq!(s.observe(1300.0, 0), ScaleDecision::Hold);
        // a clear rise resumes probing
        assert_eq!(s.observe(1500.0, 0), ScaleDecision::Add);
        assert_eq!(s.target(), 8);
    }

    #[test]
    fn scaler_removes_on_falling_throughput() {
        let mut s = Scaler::new(cfg(), 4, 16);
        s.observe(1000.0, 0);
        s.observe(1200.0, 0);
        assert_eq!(s.observe(800.0, 0), ScaleDecision::Remove);
        assert_eq!(s.target(), 5);
    }

    #[test]
    fn scaler_halves_on_rate_limit_and_respects_cap() {
        let mut s = Scaler::new(cfg(), 8, 16);
        s.halve();
        assert_eq!(s.target(), 4);
        assert_eq!(s.rate_limit_hits(), 1);
        s.halve();
        assert_eq!(s.target(), 2);
        let mut capped = Scaler::new(cfg(), 15, 16);
        assert_eq!(capped.observe(100.0, 0), ScaleDecision::Add);
        assert_eq!(capped.target(), 16);
        assert_eq!(capped.observe(200.0, 0), ScaleDecision::Hold);
        assert_eq!(capped.observe(400.0, 0), ScaleDecision::Hold);
        assert_eq!(capped.target(), 16, "never above the cap");
    }

    #[test]
    fn scaler_steps_down_after_repeated_resets() {
        let mut s = Scaler::new(cfg(), 4, 16);
        assert!(!s.note_reset());
        assert!(!s.note_reset());
        assert!(s.note_reset());
        assert_eq!(s.target(), 3);
    }

    #[test]
    fn scaler_never_below_one() {
        let mut s = Scaler::new(cfg(), 1, 16);
        s.halve();
        assert_eq!(s.target(), 1);
        // the window frozen by the halving holds, and a fall from the
        // single connection sheds nothing
        assert_eq!(s.observe(100.0, 0), ScaleDecision::Hold);
        assert_eq!(s.observe(50.0, 0), ScaleDecision::Hold);
        assert_eq!(s.target(), 1);
    }

    #[test]
    fn retry_after_seconds_and_http_date() {
        let max = Duration::from_secs(300);
        assert_eq!(
            parse_retry_after("120", max),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            parse_retry_after("99999", max),
            Some(Duration::from_secs(300)),
            "clamped"
        );
        assert_eq!(parse_retry_after("nonsense", max), None);
        assert_eq!(parse_retry_after("", max), None);
        // a date far in the future parses (clamped); a past date is "now"
        let future = "Wed, 01 Jan 2098 00:00:00 GMT";
        assert_eq!(parse_retry_after(future, max), Some(max));
        let past = "Wed, 01 Jan 2020 00:00:00 GMT";
        assert_eq!(parse_retry_after(past, max), Some(Duration::ZERO));
    }

    #[test]
    fn jitter_never_shortens_and_stays_within_fraction() {
        let wait = Duration::from_secs(10);
        for _ in 0..50 {
            let j = jittered(wait, 0.25);
            assert!(j >= wait && j <= wait.mul_f64(1.25));
        }
        assert_eq!(jittered(wait, 0.0), wait);
    }

    #[test]
    fn stall_monitor_flags_a_dead_segment_after_grace_and_sustained_drop() {
        let c = cfg();
        let mut m = SegmentMonitor::new(c);
        let t0 = Instant::now();
        m.start(0, 0, t0);
        m.start(1, 0, t0);
        // both healthy for a while: 1000 B/s each
        m.sample(0, 1000, t0 + Duration::from_secs(1));
        m.sample(1, 1000, t0 + Duration::from_secs(1));
        m.sample(0, 2000, t0 + Duration::from_secs(2));
        m.sample(1, 2000, t0 + Duration::from_secs(2));
        // segment 0 stops dead; segment 1 keeps going
        m.sample(1, 3000, t0 + Duration::from_secs(3));
        let segs = [(0, 2000u64, true), (1, 3000u64, true)];
        let stalled = m.evaluate(&segs, t0 + Duration::from_secs(3));
        assert!(stalled.is_empty(), "below-threshold must persist first");
        m.sample(1, 4000, t0 + Duration::from_secs(4));
        let stalled = m.evaluate(&segs, t0 + Duration::from_secs(4));
        assert!(stalled.is_empty(), "only 1s below threshold so far");
        m.sample(1, 5000, t0 + Duration::from_secs(5));
        let stalled = m.evaluate(&segs, t0 + Duration::from_secs(5));
        assert_eq!(stalled, vec![0], "2s below 15% of the peer after grace");
    }

    #[test]
    fn stall_monitor_ignores_grace_period_and_all_stalled() {
        let c = cfg();
        let mut m = SegmentMonitor::new(c);
        let t0 = Instant::now();
        m.start(0, 0, t0);
        // nothing received at all, still inside grace
        let segs = [(0, 0u64, true)];
        assert!(m
            .evaluate(&segs, t0 + Duration::from_millis(500))
            .is_empty());
        // past grace, but no peer and no baseline: not a rescue case
        assert!(m.evaluate(&segs, t0 + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn stall_monitor_recovers_when_speed_returns() {
        let c = cfg();
        let mut m = SegmentMonitor::new(c);
        let t0 = Instant::now();
        m.start(0, 0, t0);
        m.start(1, 0, t0);
        m.sample(0, 1000, t0 + Duration::from_secs(1));
        m.sample(1, 1000, t0 + Duration::from_secs(1));
        m.sample(1, 2000, t0 + Duration::from_secs(2));
        let segs = [(0, 1000u64, true), (1, 2000u64, true)];
        assert!(m.evaluate(&segs, t0 + Duration::from_secs(2)).is_empty());
        m.sample(1, 3000, t0 + Duration::from_secs(3));
        assert!(m.evaluate(&segs, t0 + Duration::from_secs(3)).is_empty());
        // segment 0 resumes at full speed before the stall window elapses
        m.sample(0, 2500, t0 + Duration::from_secs(3));
        m.sample(1, 4000, t0 + Duration::from_secs(4));
        let segs = [(0, 2500u64, true), (1, 4000u64, true)];
        assert!(m.evaluate(&segs, t0 + Duration::from_secs(4)).is_empty());
        assert!(m.evaluate(&segs, t0 + Duration::from_secs(5)).is_empty());
    }

    #[test]
    fn host_learning_ema_and_expiry() {
        let c = cfg();
        let now = Utc::now();
        let mut h = HostLearning::new(4, 1_000_000, now);
        assert_eq!(h.starting_connections(&c), 4);
        h.update(16, 2_000_000, 0.3, now);
        // 4 + 0.3*(16-4) = 7.6 -> 8
        assert_eq!(h.starting_connections(&c), 8);
        assert!(!h.expired(now, 30));
        let old = now - chrono::Duration::days(31);
        let h2 = HostLearning::new(4, 1_000_000, old);
        assert!(h2.expired(now, 30));
        let edge = now - chrono::Duration::days(30);
        let h3 = HostLearning::new(4, 1_000_000, edge);
        assert!(!h3.expired(now, 30), "30 days exactly is still fresh");
    }

    #[test]
    fn host_key_is_lowercase_host_only() {
        assert_eq!(host_key("https://EXAMPLE.com:8443/path?q=1"), "example.com");
        assert_eq!(host_key("not a url"), "");
    }

    #[test]
    fn config_cap_and_starting_point() {
        let c = AdaptiveConfig::default();
        assert_eq!(c.cap_for(32), 16);
        assert_eq!(c.cap_for(8), 8);
        assert_eq!(c.cap_for(0), 1);
        assert_eq!(c.starting_connections(None, 16), 4);
        assert_eq!(c.starting_connections(Some(10), 16), 10);
        assert_eq!(c.starting_connections(Some(30), 16), 16);
        assert_eq!(c.starting_connections(Some(1), 16), 2);
        assert_eq!(c.starting_connections(None, 1), 1);
    }
}
