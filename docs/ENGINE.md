# Adaptive segmentation engine — design

Phase 5 of the v1.1.0 upgrade (fork `kerlif/ezoo-download`, branch
`v110-upgrade`). This document describes the design in the author's own
words, from first principles. It covers only the direct HTTP download
path (`http_download.rs`): HLS downloads fetch small playlist segments
through their own runner, torrents go through aria2, and yt-dlp runs
only on user action — none of those paths use this engine.

## 1. What existed before

The engine already split a file into equal segments (`segments::plan`),
let an idle connection take an unfinished segment or cut the largest
active one in half (`claim_work` / `split_largest`), and kept the
segment table in the persisted download record so a download could
resume. What it did not do was choose *how many* connections to use
based on what the server actually delivered, react to rate limiting,
rescue a connection that stopped sending, remember anything about a
host between downloads, or check that a resumed segment table was
still faithful to the file. Phase 5 adds exactly those behaviours, and
nothing else.

The quality selector added in Phase 4 is still display-only (Open
Question 7): the engine downloads the byte ranges of whatever stream
it is given, at the best quality, and the adaptive machinery is
indifferent to which quality that is. If quality steering is added
later, it will change *which* stream the engine is pointed at, not how
the engine moves bytes.

## 2. Tunable defaults

Every threshold lives in one struct, `AdaptiveConfig` in
`crates/engine/src/adaptive.rs`. There is no settings UI for these
values in this phase (UI exposure belongs to Phase 6); the struct is
the single place to tune them.

| Setting | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Master switch. When false, the download uses a fixed connection count and no scaling, adaptive splitting, or stall rescue takes place. |
| `initial_connections` | 4 | Connections a download starts with when nothing is known about the host. |
| `min_initial_connections` | 2 | Never start with fewer than this, unless the cap itself is lower. |
| `measure_window` | 3 s | Length of one throughput measurement window. |
| `gain_threshold` | 1.10 | Throughput must rise by at least 10% over the previous window to add a connection. |
| `no_gain_intervals` | 2 | Windows without that rise after which growth stops. |
| `per_host_max` | 16 | Most connections the scaler will grow to for one host. |
| `absolute_ceiling` | 32 | Absolute ceiling, kept from the previous behaviour. |
| `dynamic_split_min_remaining` | 1 MiB | A segment is only cut when at least this many bytes remain in it. |
| `stall_fraction` | 0.15 | A segment is stalled when its recent speed stays below 15% of the reference speed. |
| `stall_seconds` | 10 s | How long it must stay below the fraction to be rescued. |
| `stall_grace` | 10 s | No rescue before this long after the segment started. |
| `rate_limit_default_backoff` | 5 s | Wait for a rate-limit answer that carries no `Retry-After`. |
| `max_retry_after` | 300 s | A `Retry-After` longer than this is clamped to this. |
| `jitter_fraction` | 0.25 | Random spread added to backoff waits (up to 25% longer, never shorter than the asked wait). No new dependency: the spread comes from the system clock mixed with a process-wide counter, documented in `jittered()`. |
| `learning_alpha` | 0.3 | Weight of a new observation in the per-host moving averages. |
| `learning_expiry_days` | 30 | Per-host entries older than this are ignored. |
| `reset_reduction_after` | 3 | Consecutive connection resets after which the count steps down by one. |

The effective cap for a download is the smallest of the per-host
maximum (16), the connection count the caller asked for (the user's
setting, already limited by any explicit per-server exception), and
the absolute ceiling (32).

## 3. Connection scaling

A download starts with 2–4 connections: the learned starting point
for the host when one exists (§6), else 4, clamped into the cap. A
supervisor inside the download loop measures the aggregate throughput
(the progress counter's delta) once per measurement window, together
with how many rate-limit/reset errors happened in the window and the
time from request to first byte (recorded per request, reported in
`AdaptiveReport`; first-byte latency does not by itself drive a
decision — the stall timeout and the rescue path already bound it).

Decisions, in `Scaler::observe`. The scaler works by probing: it
adds a connection and then measures whether the aggregate throughput
rose. A flat throughput at the starting count cannot tell it anything
— on a server that caps each connection, the total stays flat until
another connection is added, so waiting for a rise *before* adding
would never add at all. (This was found by benchmarking: the first
implementation waited for a rise and never grew past its start on a
per-connection-capped server. See `docs/BENCHMARKS.md`.)

- The first window sets the baseline and probes with one more
  connection.
- Throughput up ≥10% over the previous window → the probe paid off
  (or conditions improved): probe again.
- Throughput down → remove one connection (never below 1).
- Flat (neither up 10% nor down) → count a no-gain window and probe
  once more; after two consecutive no-gain windows, growth stops
  until throughput rises ≥10% again, which resumes probing.

Removal is graceful: the target is lowered, and surplus workers exit
when they finish their current segment instead of taking new work —
no connection is killed mid-segment for scaling reasons. Adding a
connection spawns a worker that claims work from the segment table
like any other. The scaler remembers the connection count at the
throughput peak (`best_target`), which is what per-host learning
records.

First-byte latency and error counts are measured every window as the
spec asks; errors feed the scaler only through the reset counter
(§4), so a window full of errors does not also trigger a spurious
"throughput fell" removal on top of the halving/step-down below.

## 4. Backoff

- **429 / 503.** The response is recognised in the download loop
  before any other status handling, and its `Retry-After` header is
  parsed (`adaptive::parse_retry_after`: seconds, or an HTTP date;
  a past date means "now"). The worker waits the asked time plus
  jitter, the scaler halves the connection count and freezes growth
  for one window, and the segment is retried from the same position
  without consuming a retry. A missing header falls back to the
  5-second default.
- **Connection resets / drops.** These keep the previous behaviour
  (retry indefinitely, the part keeps what it already has), plus one
  addition: after three consecutive resets the target steps down by
  one. Any successful window resets the reset counter.

## 5. Dynamic splitting

When a worker runs out of work it first takes any unfinished segment
nobody is working on. Otherwise it cuts the *active* segment with the
largest remaining range at the midpoint of what is left, provided at
least 1 MiB remains (`split_largest_remaining`, used through
`claim_work_split`). The previous `min_split`-based split
(`split_largest`) is kept for the fixed-count path.

Invariants, all enforced while holding the segment-table lock:

1. **Atomic boundary update.** The original segment's `end` moves to
   the midpoint and the new segment is appended in the same locked
   section. No writer can observe a half-moved boundary, because
   every position update and every split goes through the same lock.
2. **Resume map before write.** The new segment exists in the table
   before any worker writes a byte of its range: the split happens in
   the claim step, and the claiming worker only then issues its
   request for `[mid, old_end)`.
3. **No overlap, no gap.** The new segment starts exactly at the old
   segment's new end. The writer for the old segment re-reads its end
   on every chunk and never writes past it (`stop = end.min(req_end)`,
   `take` limited to the room left), so bytes written before the split
   stay inside the shrunken segment and bytes after the split belong
   to the new one.
4. **Crash safety.** The table is persisted with the download record
   (the manager writes it at most every 10 s while a download runs).
   If a crash loses the newest split, the persisted table is still a
   valid cover of the file (see §7): the old segment's recorded
   position never passes its shrunken end, so at worst the new
   segment's bytes are fetched again and overwritten — never lost,
   never duplicated in the finished file.

## 6. Stall rescue

`SegmentMonitor` tracks, per active segment, a slow-moving baseline
of its healthy speed and the position/time of its current window.
The baseline is seeded by the segment's first bytes and is only ever
updated from windows that look healthy, so a slowing segment cannot
drag its own reference down with it. Once a second, the supervisor
evaluates every active segment:

- Reference speed = the higher of the segment's own baseline and the
  mean baseline of the other active segments. When all segments are
  equally slow (a server-wide limit), the reference is low and
  nothing is rescued; when one segment lags its peers, it is.
- A segment with no baseline and no progressing peers is not
  rescued: when nothing moves at all, the network-retry path owns
  the problem.
- Inside the grace period (10 s from the segment's start) nothing is
  rescued.
- A segment whose recent speed stays below 15% of the reference for
  10 consecutive seconds is flagged (`Segment::takeover`).

The flagged segment's worker notices the flag at its next chunk poll
(the connection is polled every second, while a dead connection is
still only declared dead after the unchanged 30 s stall timeout) and
closes its connection. The segment becomes idle and is re-requested
from `pos` — the last byte written — by whichever worker claims it
next, possibly the same worker on a fresh connection. **A rescue
never discards written data**: the position is the only state the new
request uses, and the new range starts exactly there.

## 7. Resume validation

`segments::validate_segment_map(table, total)` checks that a resumed
table is faithful before it is used: every segment lies inside the
file, every position lies inside its segment, and the segments cover
`[0, total)` exactly once — no gap, no overlap, in any order (splits
append, so real tables are rarely sorted). A table that fails any
check is discarded and the download is re-planned from scratch with
an empty file, rather than trusting a record that could write bytes
to the wrong offsets. The check runs in `http_download::run` before
any resumed table is used, so it covers every resume path, including
the manager's restart-after-crash path.

## 8. Per-host learning (local only)

Choice: the existing per-host memory (`Settings::connections_for`,
driven by the explicit per-server exceptions in Options > Connection)
is kept exactly as it is, as a user-set *cap*. Learning is a separate
store, `Settings::host_learning`, keyed by lowercase host, holding one
`HostLearning` per host:

```json
"host_learning": {
  "example.com": {
    "connections_ema": 7.6,
    "segment_size_ema": 524288.0,
    "samples": 3,
    "updated_at": "2026-10-28T12:00:00Z"
  }
}
```

- After a download finishes, the manager folds the run's
  best-performing connection count and mean segment size into the
  host's averages with weight 0.3 (`HostLearning::update`); the first
  observation is taken as-is. Only finished runs are recorded.
- Entries older than 30 days are ignored when read and pruned when
  the store is next written.
- The next download from the host starts from the learned count
  (rounded, clamped to 2–16), still capped by the explicit server
  exception and the absolute ceiling.
- Migration: none was needed. No per-host learning existed before
  this phase, so there are no stored values to convert; the server
  exceptions are a different mechanism and were left untouched.
  Settings files written before this phase load with an empty
  learning map (`#[serde(default)]`).

The store lives in the local `settings.json` next to the other
settings. It causes no network traffic of any kind; see
`docs/PRIVACY.md`.

## 9. Fallback: no Range support, unknown size

Unchanged from before, and deliberately so: when the server does not
support ranges or the size is unknown, the download uses exactly one
connection and restarts from zero after an error, because there is no
safe way to resume a byte range the server will not honour. The
adaptive supervisor, splitting, and rescue do not run for such
downloads. The single-connection path still verifies the finished
file the same way any other download is verified by the caller.

## 10. What is not in this phase

- No settings-UI strings: the tunables are code constants until
  Phase 6 decides how to expose them.
- No quality steering: the display-only selector from Phase 4 is
  untouched (Open Question 7).
- No HLS/torrent/yt-dlp changes.
- No new dependencies: the engine uses only `std` plus crates already
  in the tree (the jitter uses the system clock and an atomic
  counter; the tests use `sha2`, already a dependency, as a
  dev-dependency).
- No new outbound requests: the only network activity remains the
  download requests themselves (see `docs/PRIVACY.md`).
- The 30 s stall timeout, the retry/backoff behaviour for ordinary
  network errors, and the pause/resume behaviour checklist in
  `docs/BASELINE.md` §2 are unchanged except for the adaptive
  behaviours described here.
