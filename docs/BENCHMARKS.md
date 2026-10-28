# Benchmarks — adaptive vs fixed connection counts

Date: 2026-10-28. Phase 5 of the v1.1.0 upgrade. Harness:
`crates/engine/examples/adaptive_bench.rs`, run as a release build
(`cargo run --release -p swiftget-engine --example adaptive_bench`)
on this VM, against a local server on 127.0.0.1 (ephemeral port, no
external sites). Every run verified the SHA-256 of the downloaded
file against the served bytes; all runs passed. Three runs per
configuration; the tables below report the mean. The adaptive runs
used a 500 ms measurement window (shortened from the 3 s production
default, so that scaling can act inside a short transfer); the fixed
runs used the same engine with scaling disabled. "Peak" is the most
connections the scaler reached (not applicable to fixed runs).

## Profiles

| Profile | File | Server behaviour |
|---|---|---|
| `limited` | 8 MiB | 512 KiB/s per connection + 5 ms delay per 8 KiB chunk: a bandwidth/latency-limited server that caps each connection |
| `fast` | 32 MiB | 1 ms delay per 8 KiB chunk, no per-connection cap: a fast, local-like server |

## Results

### `limited` profile (8 MiB)

| Configuration | Run 1 (s) | Run 2 (s) | Run 3 (s) | Mean (s) | Mean MiB/s | Peak connections |
|---|---|---|---|---|---|---|
| adaptive | 4.321 | 4.311 | 4.314 | **4.315** | **1.85** | 10 |
| fixed-1 | 23.361 | 23.366 | 23.375 | 23.367 | 0.34 | — |
| fixed-4 | 5.826 | 5.823 | 5.822 | 5.824 | 1.37 | — |
| fixed-8 | 2.899 | 2.908 | 2.906 | 2.904 | 2.75 | — |
| fixed-16 | 1.493 | 1.448 | 1.456 | 1.466 | 5.46 | — |

Adaptive is 5.4× faster than fixed-1 and 1.35× faster than fixed-4,
but 1.5× slower than fixed-8 and 2.9× slower than fixed-16. It grew
from 4 to 10 connections and then stopped: two measurement windows
without a ≥10% gain stopped the growth, even though the server could
deliver more at 16 connections (the fixed-16 run proves it). The
scaler is conservative by design — it stops probing rather than
overshooting — and on this profile that conservatism leaves speed on
the table.

### `fast` profile (32 MiB)

| Configuration | Run 1 (s) | Run 2 (s) | Run 3 (s) | Mean (s) | Mean MiB/s | Peak connections |
|---|---|---|---|---|---|---|
| adaptive | 1.790 | 1.798 | 1.803 | **1.797** | **17.80** | 7 |
| fixed-1 | 8.583 | 8.584 | 8.601 | 8.589 | 3.73 | — |
| fixed-4 | 2.220 | 2.187 | 2.215 | 2.207 | 14.50 | — |
| fixed-8 | 1.154 | 1.091 | 1.086 | 1.110 | 28.82 | — |
| fixed-16 | 0.555 | 0.550 | 0.563 | 0.556 | 57.55 | — |

Adaptive is 4.8× faster than fixed-1 and 1.2× faster than fixed-4,
but 1.6× slower than fixed-8 and 3.2× slower than fixed-16. Here the
limit is time, not caution: the whole download lasts ~1.8 s, about
three and a half measurement windows, so the scaler only reaches 7
connections before the file is done. A download that finishes in
seconds cannot ramp to 16 connections one window at a time. Short,
fast downloads are the overhead-dominated case for this design: the
per-host learning store (§6 of `docs/ENGINE.md`) is the mechanism
meant to help here, by starting the next download from a host that is
already known to be fast — but learning across runs was not measured
in this benchmark (each run started cold, `learned_start: None`).

## Honest summary

- Adaptive scaling beats a too-low fixed count on both profiles. It
  never beats a well-chosen fixed count on these profiles: fixed-8
  and fixed-16 win everywhere here, because they start at full
  strength while the scaler ramps one connection per window and
  stops after two flat windows.
- The first benchmark run (before the scaler fix described below)
  was worse: adaptive never grew past its starting 4 connections on
  the `limited` profile (mean 12.0 s, slower than fixed-4's 5.8 s),
  because that implementation waited for throughput to rise *before*
  adding a connection, and throughput on a per-connection-capped
  server is flat until another connection is added. The scaler was
  changed to probe-then-measure (add, then judge the next window);
  the numbers above are from the fixed implementation. The probe
  reading is recorded as Open Question 9.
- These are loopback numbers on one VM, not internet speeds. They
  compare configurations against each other on identical servers;
  they say nothing about any real host.

## Reproducing

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release -p swiftget-engine --example adaptive_bench
```

The harness is deterministic data (byte `i * 31 % 251`) served from
memory; the two profile definitions and the per-run output format are
in the example source. The raw output of the run reported here is
reproduced in the appendix below.

## Appendix: raw output of the reported run

```
profile=limited file: 8388608 bytes sha256=a6a7387ed918a4fe3e11b7ffa8055ba6a1dfccb86e4f64df283ed832b9fe0ad2
RUN profile=limited config=adaptive run=1 secs=4.321 mib_per_s=1.85 peak_target=10 best_mib_per_s=2.84
RUN profile=limited config=adaptive run=2 secs=4.311 mib_per_s=1.86 peak_target=10 best_mib_per_s=2.81
RUN profile=limited config=adaptive run=3 secs=4.314 mib_per_s=1.85 peak_target=10 best_mib_per_s=2.84
SUMMARY profile=limited config=adaptive mean_secs=4.315 mean_mib_per_s=1.85 peak_target=10 best_mib_per_s=2.84
RUN profile=limited config=fixed-1 run=1 secs=23.361 mib_per_s=0.34 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-1 run=2 secs=23.366 mib_per_s=0.34 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-1 run=3 secs=23.375 mib_per_s=0.34 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=limited config=fixed-1 mean_secs=23.367 mean_mib_per_s=0.34 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-4 run=1 secs=5.826 mib_per_s=1.37 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-4 run=2 secs=5.823 mib_per_s=1.37 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-4 run=3 secs=5.822 mib_per_s=1.37 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=limited config=fixed-4 mean_secs=5.824 mean_mib_per_s=1.37 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-8 run=1 secs=2.899 mib_per_s=2.76 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-8 run=2 secs=2.908 mib_per_s=2.75 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-8 run=3 secs=2.906 mib_per_s=2.75 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=limited config=fixed-8 mean_secs=2.904 mean_mib_per_s=2.75 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-16 run=1 secs=1.493 mib_per_s=5.36 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-16 run=2 secs=1.448 mib_per_s=5.52 peak_target=0 best_mib_per_s=0.00
RUN profile=limited config=fixed-16 run=3 secs=1.456 mib_per_s=5.49 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=limited config=fixed-16 mean_secs=1.466 mean_mib_per_s=5.46 peak_target=0 best_mib_per_s=0.00
profile=fast file: 33554432 bytes sha256=c5687da1bba2c01d25ffab55d03d2b0e8754c21499f8732c6c63dacb9d9f2edc
RUN profile=fast config=adaptive run=1 secs=1.790 mib_per_s=17.87 peak_target=7 best_mib_per_s=22.14
RUN profile=fast config=adaptive run=2 secs=1.798 mib_per_s=17.79 peak_target=7 best_mib_per_s=21.75
RUN profile=fast config=adaptive run=3 secs=1.803 mib_per_s=17.75 peak_target=7 best_mib_per_s=22.23
SUMMARY profile=fast config=adaptive mean_secs=1.797 mean_mib_per_s=17.80 peak_target=7 best_mib_per_s=22.23
RUN profile=fast config=fixed-1 run=1 secs=8.583 mib_per_s=3.73 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-1 run=2 secs=8.584 mib_per_s=3.73 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-1 run=3 secs=8.601 mib_per_s=3.72 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=fast config=fixed-1 mean_secs=8.589 mean_mib_per_s=3.73 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-4 run=1 secs=2.220 mib_per_s=14.41 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-4 run=2 secs=2.187 mib_per_s=14.63 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-4 run=3 secs=2.215 mib_per_s=14.45 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=fast config=fixed-4 mean_secs=2.207 mean_mib_per_s=14.50 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-8 run=1 secs=1.154 mib_per_s=27.74 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-8 run=2 secs=1.091 mib_per_s=29.33 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-8 run=3 secs=1.086 mib_per_s=29.47 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=fast config=fixed-8 mean_secs=1.110 mean_mib_per_s=28.82 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-16 run=1 secs=0.555 mib_per_s=57.65 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-16 run=2 secs=0.550 mib_per_s=58.20 peak_target=0 best_mib_per_s=0.00
RUN profile=fast config=fixed-16 run=3 secs=0.563 mib_per_s=56.83 peak_target=0 best_mib_per_s=0.00
SUMMARY profile=fast config=fixed-16 mean_secs=0.556 mean_mib_per_s=57.55 peak_target=0 best_mib_per_s=0.00
```
