# Release report — Ezoo Download v1.1.0 (fork `kerlif/ezoo-download`)

Final report for the v1.1.0 upgrade, spec §4. Branch:
`v110-upgrade`. License: MIT © 2026 Ahmed Ezzat (Ezoo Tech);
all copyright notices and credits to Ahmed Ezzat / Ezoo Tech and
the upstream repository `AhmedEzo686/ezoo-download` are kept, and
no license terms were changed in any phase. This report
synthesizes `docs/PHASES.md` and the per-phase documents
(`BASELINE.md`, `FONTS.md`, `ENGINE.md`, `BENCHMARKS.md`,
`THEMES.md`, `UPDATE-CHECKER.md`, `FLATPAK.md`, `PRIVACY.md`,
`docs/README.md`); where a number below comes only from a phase
report, that is stated. Nothing in this report claims a result
that was not run: unrun work is listed in §11 and §14 as pending.

Status at writing (2026-10-28, Phase 10): Phases 0–7 and 9
passed their gates as recorded (Phase 9 with the Lighthouse
exception); Phase 8 is PREPARED — AWAITING CI; Phase 10 is
recorded in `docs/PHASES.md` with its own exceptions (binaries
pending upload, Windows screenshots pending, Lighthouse still
unmeasured). No tag `v1.1.0` exists yet and no CI run has
produced v1.1.0 artifacts.

## Phase 0 — Baseline and audit (PASSED 2026-10-28)

**What changed.** No code. Baseline inventory and behavior
checklist (`docs/BASELINE.md`), the Wayland dialog root-cause
diagnosis (proven from vendored Slint 1.18.1 / winit 0.30.13
sources), and the license audit (InstaArabi verdict: REPLACE;
`THIRD-PARTY-NOTICES.txt` had no font entry at all).

**Tests / verification run (real results).** Release build OK
(21m04s, 9 warnings, all dead code on Linux in `update.rs`);
clippy 11 warnings in `update.rs` + 5 Slint deprecations;
`cargo test` 83 passed / 0 failed (13 app-bin unit, 62 engine
unit, 4 download integration, 4 torrent integration, 1 doc-test
ignored); `cargo fmt --check` FAILED (651 hunks, queued for
Phase 1). Binary 42,904,952 B. Docs pushed to `v110-upgrade` @
`c2de3c41` (per the Phase 0 gate record).

**Not verified / why.** Startup behavior under Xvfb was pending
at gate time (covered later in Phases 6/10). The promotional
audit was completed during Phase 0 (100 text/code occurrences +
1 binary asset, concentrated in 2 dialogs) but its removal
belonged to Phase 2.

**New dependencies / licenses.** None.

**Open questions.** Raised: Wayland verification (Q2), font
license (Q1, resolved), InstaArabi replacement (Q1), variable vs
static fonts (Q3, resolved), AboutDialog buttons (Q2 in the
resolved list — recorded as resolved item 2), and the fontique
zero-font behavior (later Q5).

## Phase 1 — Safe cleanup (PASSED 2026-10-28)

**What changed.** One formatting-only `cargo fmt` commit; 11
`update.rs` dead-code warnings fixed via `#[cfg(windows)]`;
Slint viewport deprecations renamed; rustdoc added; version
1.1.0 in code and packaging. No behavior change. Structural
splits of `main.rs`/`manager.rs` deferred to Phases 5/6. No dead
code blocks, no unused dependencies; `mpd.rs` unwraps audited
(guarded, left).

**Tests / verification run (real results).** `cargo fmt --check`
clean; `cargo clippy --all-targets -- -D warnings` exit 0 (0
Slint warnings); `cargo test` 83 passed / 0 failed (unchanged);
release build OK (7m38s in the Phase 2 record's comparison run;
Phase 1's own record: release build OK, 42,905,080 B). Grep
gates: FluxDown 0 hits; no analytics/tracking domains.

**Not verified / why.** Nothing specific to this phase; the
Windows-target compile question (no mingw toolchain) already
applied and was carried forward.

**New dependencies / licenses.** None.

**Open questions.** None new.

## Phase 2 — Remove promotional and donation UI (PASSED 2026-10-28)

**What changed.** AboutDialog donation block + YouTube/Facebook
channel buttons and WhatsNewDialog donation block removed
(`dialogs.slint` −108 lines); both `open-link` callbacks + Rust
handlers removed; 5 live `@tr()` strings + 1 orphan PayPal-only
variant removed from `ar_ui.py` (6 keys), `ui/*.json` (6 keys ×
6 locales), `rs/*.json` + `i18n.rs` (dead 1.0.4 changelog entry);
`.po` regenerated deterministically (473→468 texts, 0 missing,
run-twice diff empty); `instapay-qr.png` deleted. Credits kept:
"Coded by Ahmed Ezzat", upstream links, installer
AppPublisher/AppPublisherURL metadata.

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 83 passed / 0
failed (unchanged); release build OK (7m38s, 42,717,560 B).
Grep gate: 0 removed-term hits in code/UI/locales/website/README
except the kept installer AppPublisherURL + docs audit history;
FluxDown 0 hits; no analytics domains. Version gate (show once
per upgrade) intact.

**Not verified / why.** Nothing specific; GUI rendering of the
edited dialogs was not screenshotted in this phase (covered by
the Phase 6/10 Xvfb runs, which show no donation content).

**New dependencies / licenses.** None. (One binary asset
removed; no license text changed.)

**Open questions.** None new. The AboutDialog-buttons decision
is recorded as resolved item 2 in `docs/OPEN-QUESTIONS.md`.

## Phase 3 — Fonts (PASSED 2026-10-28, with one documented exception)

**What changed.** InstaArabi removed (no license); Carlito
Regular/Bold + Noto Sans Arabic Regular/Bold (static, from
NotoSansArabic-v2.013 — Cairo has no upstream statics, resolved
Q3) + pristine Liberation Serif 2.1.5 Regular/Bold bundled and
registered in memory via Slint's fontique shared collection with
Arabic/Latin script fallbacks; the old per-user install
mechanism replaced by register + legacy-delete migration; saved
InstaArabi/Times New Roman choices migrate on load;
`FONTS-LICENSE.txt` rewritten, `THIRD-PARTY-NOTICES.txt` +
`LICENSE-DEB` font entries added, OFL texts ship in the Linux
doc dir (verified in a real `.deb` listing) + Windows installer
list (workflow staging escalated). Full sources, hashes, and
name-table dumps: `docs/FONTS.md`.

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 86 passed / 0
failed (83 baseline + 3 new font tests); release build OK
(8m32s, 44,513,016 B). **Exception:** the literal no-font gate
(empty fontconfig) FAILED on an upstream fontique 0.11.1 panic
at Slint platform creation (`font_sort().unwrap()`, `NoMatch`)
— no app code can avoid it; the substitute test (fontconfig
seeing exactly one font covering zero UI glyphs) rendered
pixel-identical to full-system-font runs in Arabic + English
(SHA-256 `d6b88fe5…54d91` Arabic, `687e9e23…9b685` English),
proving bundled-only rendering. Migration + legacy cleanup
verified in the same runs (seeded `InstaArabi` persisted as
`Noto Sans Arabic`; planted legacy copies deleted). Grep gate:
InstaArabi only in migration code/comments, the FONTS-LICENSE
removal note + docs audit history; FluxDown 0 hits in code/UI;
no analytics domains.

**Not verified / why.** Windows-target compile unverified
locally (no mingw toolchain; signatures checked against
windows-sys 0.59) — CI must confirm. The `.rpm`/AppImage were
not built in this phase (same staging code path as the verified
`.deb`; see §11). The software-renderer fallback path was not
exercised. Only the main window was screenshotted (both
locales).

**New dependencies / licenses.** No new crates. New licensed
content: three SIL OFL 1.1 font families (copyright lines
verbatim in `docs/FONTS.md` §Licensing and
`THIRD-PARTY-NOTICES.txt`); the OFL texts ship with the
packages.

**Open questions.** New Q5 (fontique zero-font panic, still
open); Q1/Q3 resolved by this phase. The six `.ttf` binaries
could not be pushed via the connector and remain a pre-tag
blocker (see §14).

## Phase 4 — File size and stream metadata in the capture window (PASSED 2026-10-28)

**What changed.** HEAD-first probe (Content-Length, then
ranged-GET `Content-Range` confirmation; a false HEAD length is
corrected, an unconfirmed length is flagged
`total_size_unverified` and shown as an estimate) + new
`stream_meta` engine module (HLS master/media parsing with
`AVERAGE-BANDWIDTH` fallback, separate-audio-group bandwidth
added from sampled audio segments, DASH multi-Period duration
sum with video/audio kept separate, size = (video+audio bps) ×
duration / 8 always labeled approx. with its source, 5 s probe
budget off the UI thread) + filename hardening (`..` removal,
`safe_join` verified inside the download dir) + AddDialog
duration/quality-selector/size section + DRM refusal
(Widevine/PlayReady/FairPlay UUIDs and HLS DRM KEYFORMATs;
AES-128 stays supported; buttons disabled, early-started
download discarded, `confirm_add` guard) + yt-dlp `sizeSource`
(filesize/filesize_approx/estimated) shown in the browser panel;
yt-dlp still only on user action. Captured headers never
persisted/logged (used only in request builders, absent from
all result types). `.po` regenerated deterministically (469
texts, 0 missing, run-twice diff empty); 8 new Rust strings +
`Duration` in all 8 locales (ru/tr estimation phrases marked
needs-review, Q6).

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 105 passed / 0
failed (86 before + 19 new: 7 engine unit, 12 local-server
fixtures covering direct/ranged/HEAD-403-405/HLS master+media/
multi-Period MPD/malformed/DRM/false-length/traversal);
release build OK (8m14s, 44,681,336 B). Grep gates: FluxDown 0
hits in code/UI; promo terms 0; InstaArabi only in Phase 3
migration places. Deviations found by testing and fixed: a
HEAD-first regression (servers sending Content-Disposition only
on 206) fixed by merging ranged-answer headers into the HEAD
answer; `drm_scheme()` now also recognizes DRM scheme UUIDs.

**Not verified / why.** GUI smoke test NOT run:
`libxkbcommon-x11.so` absent from the VM, so the built binary
could not start under Xvfb — dialog rendering/selector behavior
was compile-verified only (covered for real in Phase 10, which
drove the dialog through the localhost bridge and photographed
the HLS/DASH metadata). Windows-target compile unverified
locally (CI must confirm). The quality selector is display-only
by spec (Q7) — selection does not steer the download.

**New dependencies / licenses.** None.

**Open questions.** New Q6 (ru/tr needs-review), Q7
(display-only selector), Q8 (HLS separate-audio double-count
rule) — all still open.

## Phase 5 — Adaptive segmentation engine, clean room (PASSED 2026-10-28)

**What changed.** New `adaptive.rs` (scaler, stall monitor,
`Retry-After` parsing, jitter without new deps, per-host
learning) + `segments.rs` validation/split
(`validate_segment_map`, `split_largest_remaining`,
`claim_work_split`, `takeover` flag) + `http.rs` `RateLimited` +
supervisor/scaling/rescue in `http_download.rs` +
`Settings::host_learning` (EMA, 30-day expiry; server limits
kept as caps, no migration needed) + manager wiring (learned
start + record on finish for HTTP and DASH tracks) +
`docs/ENGINE.md`, `docs/BENCHMARKS.md`, `docs/PRIVACY.md`
learning entry. Clean-room rule honored: general ideas only,
own words, no FluxDown code studied or copied, no mention in
UI/website copy. Full design and parameter table:
`docs/ENGINE.md` §§1–10 (parameters reproduced in
`IMPROVEMENTS_SUMMARY.md` §5).

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 133 passed / 0
failed (105 before + 28 new: 13 adaptive unit, 6 segments unit,
1 config unit, 8 local-server integration covering
resume/429/drops/stall/no-Range/split-while-writing/corrupt-map/
scaling, all SHA-256-verified; existing download/torrent tests
unmodified except 3 struct literals gaining the new defaulted
fields); release build OK (8m10s, 44,753,784 B). Benchmarks
(release, local server, 3 runs/config, `docs/BENCHMARKS.md`,
500 ms measurement window, SHA-256 verified every run):
limited profile adaptive 4.315 s mean (peak 10 conns) vs
fixed-1 23.367 / fixed-4 5.824 / fixed-8 2.904 / fixed-16
1.466; fast profile adaptive 1.797 s (peak 7) vs fixed-1 8.589
/ fixed-4 2.207 / fixed-8 1.110 / fixed-16 0.556 — adaptive
beats low fixed counts, loses to fixed-8/16 (reported
honestly). Findings: the scaler reads "add while throughput
rises" as probe-then-measure — the literal wait-for-a-rise
reading never grew past 4 on a capped server (first bench run:
12.0 s, slower than fixed-4), recorded as Q9; stall-monitor
baselines are seeded from a segment's first bytes. Grep gates:
FluxDown 0 hits in code/UI/locales/website/README (0 in new
docs; pre-existing audit-history mentions in `PHASES.md` only);
promo terms 0; no analytics domains; no new network
destinations.

**Not verified / why.** GUI smoke test NOT run
(libxkbcommon-x11.so absent, as in Phase 4). Windows-target
compile unverified locally (CI must confirm). Per-host learning
across runs was NOT measured (each benchmark run started cold,
`learned_start: None`). Benchmarks are loopback numbers on one
VM, not internet speeds, and say nothing about any real host.

**New dependencies / licenses.** No new dependencies: sha2 was
added as a dev-dependency only; `Cargo.lock` unchanged (per the
Phase 5 gate record).

**Open questions.** New Q9 (probe-then-measure reading, still
open).

## Phase 6 — UI and dark mode (+ Wayland dialog fix) (PASSED 2026-10-28)

**What changed.** Lazy dialogs (`LazyWeak` + `ensure_*` in
`main.rs`, `extra.rs`, `grab.rs`, `browser.rs`, `update.rs`,
`options.rs`, `scheduler.rs`, `playlist.rs`, `torrent_ui.rs`;
`preview.rs` verified dialog-free; callbacks wired once;
`apply_look`/`set_language` never force-create; clipboard busy
check fixed for not-created dialogs) + segment bar in
`DetailsDialog` (per-segment spans colored by `segment_state`:
downloading/done/stalled/idle, from the engine's existing
`active`/`takeover` flags via a 5-tuple view; legend; read-only,
no engine behavior change) + toolbar overflow fixed (ToolBtn
56→52px, Start/Stop 62→56px, filter 96→88px; 927px ≤ 960px
default) + dark/light audit (`docs/THEMES.md`: all 9 themes'
text/background pairs ≥4.5:1, light ok/err/warn/accent and 4
toolbar gradients darkened; Dark values unchanged) + new strings
("Stalled", "Receiving data…", "Waiting") in all 8 locales (`.po`
regenerated twice, diff empty, 471 texts, 0 untranslated;
best-effort "Stalled" translations marked `needs-review`, Q10).

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 134 passed / 0
failed (133 before + 1 new `segment_state` unit test; no
existing test modified); release build OK (8m07s, 44,719,224 B).
Xvfb/X11 verification (1024x640, screenshots in /tmp with
SHA-256): startup exactly 1 mapped toplevel; Ctrl+N/B/F add
exactly 1 dialog toplevel each, close unmaps, reopen reuses the
same id; dark+Arabic RTL and light+English LTR inspected —
toolbar unclipped, layouts mirrored, no mirrored-text errors; a
live throttled download showed the segment bar + legend +
per-connection states in dark RTL. Grep gates: FluxDown 0 hits in
code/UI/locales/website/README (0 in new docs; audit-history
mentions in `PHASES.md` only); promo terms 0; no analytics
domains; no new outbound requests (`docs/PRIVACY.md` unchanged);
no new dependencies.

**Not verified / why.** Real KDE Plasma (Wayland) behavior NOT
verified (no compositor on the VM — Q2, still pending); the
"stalled" segment color was not seen live (no stall occurred;
covered by unit test only — Phase 10's segment-bar shot shows
downloading segments and, in one frame, a small stalled block,
but no sustained stall was staged); Windows-target compile
unverified locally (CI must confirm).

**New dependencies / licenses.** None.

**Open questions.** New Q10 ("Stalled" translations
needs-review, still open).

## Phase 7 — Update checker (extend version.json only) (PASSED 2026-10-28)

**What changed.** Fork update source (`REPO =
"kerlif/ezoo-download"`, single constant; all URLs derived) +
releases-API check with ETag cache
(`<data>/update/checker-cache.json`), hand-followed redirects
(same-origin or GitHub asset hosts only), semver comparison with
pre-release ordering, signed-manifest verification before any
offer, SHA-256 + SHA256SUMS cross-check on artifacts, 24 h
interval with backoff ladder (5m/15m/1h/6h/24h, Retry-After wins
clamped 1m–24h), silent fails, `EzooDownload/<version>`
User-Agent only, disable setting in Options > General (default
on, incl. old settings files), Windows ask-before-install with
wait-for-idle, Linux AppImage background-download + verify + ask
+ atomic replace, .deb/.rpm/Flatpak/unknown notify-only
(Flatpak ask opens the release page), package detection with
notify-only fallback, new fork Ed25519 keypair (public key
committed, private key outside the tree, CI secret pending —
Q3), `docs/UPDATE-CHECKER.md` + `docs/PRIVACY.md` inventory
completed. Full design: `docs/UPDATE-CHECKER.md` §§1–11.

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 156 passed / 0
failed (134 before + 22 new: 9 engine unit — 8 selfupdate incl.
semver/sums/redirect/backoff/cache/package/AppImage-replace, 1
config migration — and 13 local-server integration covering
offline/older/newer/bad-hash/bad-signature/malformed/
rate-limit/cross-host-redirect/ETag/missing-manifest-or-sums/
download-reuse); release build OK (~8 min, 44,859,256 B). `.po`
regenerated deterministically (472 texts, 0 missing, 3 runs
identical); 1 Slint + 4 Rust strings in all 8 locales
(non-Arabic, non-English wordings marked needs-review, Q11).
Grep gates: FluxDown 0 hits in code/UI/new docs; promo terms 0;
InstaArabi only in Phase 3 migration places; no analytics
domains; no AhmedEzo686 in update code; no DEFAULT_USER_AGENT in
the checker path. Deviations: the signed manifest covers the
Windows installer hash only — the AppImage hash is verified
against `SHA256SUMS.txt` with the signed manifest gating the
offer (Q12); the automatic Windows flow now ASKS before
installing (spec §2.5) instead of installing silently; the i18n
placeholder-parity test now skips the `needs-review` metadata
key.

**Not verified / why.** GUI smoke test NOT run
(libxkbcommon-x11.so absent, as in Phases 4–6 — the release
binary reaches Slint backend init and stops there); Windows-target
compile unverified locally (CI must confirm, incl. the
Windows-only ask/install paths which cannot be type-checked on
Linux); Wayland verification still pending (Q2). No live GitHub
API call was made by the checker in testing (all local-server
fixtures).

**New dependencies / licenses.** No new crates: reqwest added
as a direct dep of the app crate only to name the shared client
type — `Cargo.lock` gains 1 edge line, no new packages (per the
Phase 7 gate record).

**Open questions.** New Q11 (update-string translations
needs-review) and Q12 (AppImage hash cross-checked, not directly
signed) — both still open; Q3 (signing keypair/secret) code side
done, secret creation still pending with the parent.

## Phase 8 — Flatpak and CI (PREPARED — AWAITING CI, 2026-10-28; NOT passed)

**What changed.** Manifest + desktop + metainfo + vendored
sources (`packaging/flatpak/`: `io.github.kerlif.EzooDownload.yml`,
`.desktop`, `.metainfo.xml`, `cargo-sources.json`) prepared and
validated locally as far as the environment allows; workflow
files + tag/release work ESCALATED to the parent
(`~/workspace/ezoo/workflow-escalations/phase8/`). `.deb`
dependency fix (pushable files, in the branch): `debian/control`
now also Depends on `libxkbcommon-x11-0`, the `.rpm` spec now
also Requires `libxkbcommon-x11`. `THIRD-PARTY-NOTICES.txt`
Flatpak paragraph, `docs/FLATPAK.md`, `docs/PRIVACY.md` Flatpak
section, `docs/UPDATE-CHECKER.md` §10 CI step. ffmpeg-full
stanza deliberately NOT used (retired for 25.08+; codecs-extra
auto-installed — `docs/FLATPAK.md` §5; ffmpeg binary on sandbox
PATH unverified, Q13). yt-dlp: no shipped copy; the app's
hash-checked fetch is the supply (Q1 resolved). Signing
hard-error wired in the escalated Windows workflow per
`UPDATE-CHECKER.md` §10 (verbatim). Full design, permission
rationale, and the CI-only proof list: `docs/FLATPAK.md` §§1–10.

**Tests / verification run (real results).** `cargo fmt --check`
clean; clippy `-D warnings` exit 0; `cargo test` 156 passed / 0
failed (unchanged, no Rust changes, no release rebuild).
Vendored-sources completeness: 709/709 registry crates, 709/709
sha256 values equal to `Cargo.lock` (spot-checked for slint
1.18.1, ring 0.17.14, anyhow 1.0.104). YAML parse of the
manifest + both staged workflows; metainfo XML well-formed +
manual AppStream/Desktop-Entry field checks (no
`appstreamcli`/`desktop-file-validate` available). Screenshot
URLs and Flathub/runtime refs verified HTTP 200 (2026-10-28).
Grep gates: FluxDown 0 (code/UI/new docs), promo 0, InstaArabi 0
in new files, no analytics domains, no minisign, no new
outbound requests.

**Not verified / why — everything CI-only.** `flatpak-builder`
accepts the manifest and builds offline; the bundle installs and
runs under Wayland and X11 (tray, portal chooser, downloads in
real `~/Downloads`); an `ffmpeg` binary is on the sandbox PATH
and one real merge succeeds (or the static-ffmpeg follow-up is
needed, Q13); the browser bridge works from a host browser and
yt-dlp is fetched into the sandbox data dir;
`appstreamcli`/`desktop-file-validate` pass; Windows clippy +
full tests pass; the tag build produces `.deb` + `.rpm` +
AppImage + `.flatpak` + Windows installer + checksums + signed
`version.json`. Do NOT mark Phase 8 passed until the CI
verification follow-up reports. The 1.1.0 metainfo release entry
is dated 2026-10-28 (the preparation day); if the tag lands
later, correct the date before publishing the Flatpak.

**New dependencies / licenses.** No new crates. No new license
text ships in the Flatpak beyond `THIRD-PARTY-NOTICES.txt`
(runtime FFmpeg/codec libraries are the Freedesktop SDK's build;
no ffmpeg binary bundled by the manifest).

**Open questions.** Q13 (Flatpak ffmpeg binary presence, still
open); Q1 resolved in this phase. Pre-tag blocker flagged
explicitly by Phase 8: the 6 Phase 3 `.ttf` binaries must be on
the remote before tagging or every CI job fails at compile time.

## Phase 9 — Website redesign (PASSED-WITH-EXCEPTION 2026-10-28; Lighthouse pending)

**What changed.** Framework-free rebuild of `docs/index.html` +
`style.css` + `script.js` (vanilla, zero build step, no
trackers, no third-party scripts, no external fonts) + new
`docs/README.md` site notes + OFL texts under
`docs/assets/fonts/`. Bilingual Arabic (RTL, default) + English
(LTR): one 176-key dictionary swaps all 162 keyed strings,
`lang`/`dir`, title and meta description, persisted in
localStorage. Light + dark themes (localStorage,
`prefers-color-scheme` first-visit default), 27 computed
contrast pairs all ≥4.5:1 (documented in `docs/README.md`), skip
link, landmarks, visible focus, keyboard-operable
switch/toggle/simulator/copy/FAQ, meaningful alt on every image.
All sections present (hero, features, how-it-works with the
preserved simulator, downloads with OS detection, install
guides, the 3 existing v1.0.7-era screenshots, FAQ, privacy
summary, changelog link, MIT © Ahmed Ezzat / Ezoo Tech credits +
fork attribution + upstream link). Downloads use approach (a):
hardcoded v1.1.0 filenames under `/latest/download/` + documented
per-release update step in `docs/README.md` (no workflow
amendment; `version.json` not read from the static site).
Fonts: 4 WOFF2 (fontTools 4.46.0 + brotli, glyph counts verified
identical, 539,672 B total) generated from the Phase 3
originals; binaries ESCALATED (Q14), site renders via
system-font fallbacks until uploaded.

**Tests / verification run (real results).** `cargo fmt --check`
clean; `cargo test` 156 passed / 0 failed (unchanged); grep
gates — 0 hits in the site files for the clean-room name,
money-ask terms, and InstaArabi (remaining name hits in `docs/`
are the 8 pre-existing `PHASES.md` audit-history/rule mentions
only); only external host is `github.com`; no `fetch`/XHR/beacon
in `script.js`; every internal ref curl-verified 200 locally;
download links pattern-verified (1.0.7 equivalents all HTTP 200
live; 1.1.0 versioned assets 404 as expected — no v1.1.0 release
yet, latest tag v1.0.7; Setup.exe/SHA256SUMS stable names 200 via
`/latest/download/`; Flatpak filename from the staged workflow
only, no prior release). axe-core 4.14.0 (jsdom, WCAG 2A/AA): 0
violations, 27 passes, both languages (1 genuine
`aria-labelledby` finding fixed; skip-link contrast incomplete
in jsdom, covered by computation). Headless-Chromium pass
(`file://`): 0 page errors after hard reload, language/theme/
OS/simulator/tab-order/copy/FAQ all verified live; one real bug
found and fixed (TDZ `ReferenceError` aborting `script.js` at
init). Screenshots in this phase were local verification shots
only (not committed); `docs/screenshots/` untouched.

**Not verified / why.** **Lighthouse accessibility +
performance ≥90 NOT measured** — the VM's Chromium blocks
loopback navigation and Lighthouse rejects `file://` URLs (Q14);
re-run on the live Pages URL after the merge to `main`. Phase 10
re-captured the site from the current tree (Arabic light +
English dark, inspected visually, no layout defects) but that
does not replace the Lighthouse measurement.

**New dependencies / licenses.** No new crates. New licensed
content: the 4 WOFF2 site fonts (same OFL 1.1 families as Phase
3; OFL texts ship as plain text under `docs/assets/fonts/`).

**Open questions.** Q14 (site font binaries + Lighthouse, still
open).

## Phase 10 — Screenshots, documentation, and archive (recorded 2026-10-28)

**What changed.** Ten screenshots captured from real running
builds and committed locally to `docs/screenshots/` (binaries
escalated — see §10 and §14); `docs/index.html` + `docs/script.js`
text edits (screenshot note + shot-3 alt text + 640×488
dimensions for the new dialog crop); `docs/README.md`
screenshot inventory; `IMPROVEMENTS_SUMMARY.md` (bilingual, repo
root); this report; `docs/PHASES.md` Phase 10 row;
`docs/OPEN-QUESTIONS.md` Q15; the archive
`ezoo-download-1.1.0-enhanced.zip` (see §12); the Windows
screenshot amendment staged separately at
`~/workspace/ezoo/workflow-escalations/phase10/` (diff +
rationale + risk notes, Phase 8 staging untouched). No Rust,
UI, locale, or packaging code changed in this phase.

Capture method (so the shots can be reproduced): release binary
`target/release/EzooDownload` (44,859,256 B, Phase 7 build, no
Rust changes since) under Xvfb `:99` 1280x800x24 with Ubuntu
1.6.0/1.17.0 `libxkbcommon-x11` + `libxcb-xkb` debs extracted
to `/tmp/xkb/` on `LD_LIBRARY_PATH` only (no system change);
fresh `HOME` per run with a generated-then-edited
`settings.json` (language/theme, first-run prompts suppressed,
`clipboard_monitor: false`); main-window shots are 962×502
crops of the 960×500 window at +160+150; dialog shots are
native window crops. The Add-dialog runs were driven through
the app's own localhost bridge (`POST /add` with the
`X-SwiftGet` header — the browser-extension mechanism):
synthetic XTest typing never reached the Slint `LineEdit` under
Xvfb (key events and button clicks did reach the app; text
input did not), so the bridge was used and the dialog state it
produces (`open_add_dialog` + `start_probe`) is identical to a
pasted URL. The fixture server (`/tmp/phase10/fixture_server.py`,
kept out of the repo) served the Phase 4 `stream_probe.rs`
fixture playlists/manifests verbatim — one documented
adaptation: the test fixture's cookie gate on `/master.m3u8`
was removed because a typed/bridged URL carries no captured
headers; playlist/manifest bodies and segment sizes are
identical. The progress runs used a throttled 48 MiB local file
(8 KiB chunks, 30 ms apart, Range-aware) with the connection cap
seeded to 4. The website shots used headless Chromium
(152.0.7977.82) over CDP against the real `docs/index.html`
(`file://`), with `localStorage` presets applied exactly as the
site's own toggles persist them, then a reload; both renders
were inspected visually.

**Tests / verification run (real results, re-run in this phase).**
`cargo fmt --check`: exit 0. `cargo clippy --all-targets --
-D warnings`: exit 0. `cargo test`: **156 passed / 0 failed**
(16 + 99 + 8 + 4 + 12 + 4 + 13 across the test binaries, 1
doc-test ignored; includes the 22 Phase 7 update-checker tests
and the 13 local-server update-checker integration tests).
Linux release build: `cargo build --release` exit 0, no
recompilation needed (binary unchanged, 44,859,256 B, matching
the Phase 7 size exactly). Grep gates (whole tree except
`target/`/`.git/`): FluxDown 0 hits outside `docs/PHASES.md`
audit history; promo/money-ask terms 0 hits in
code/UI/locales/website/README/packaging/installer; analytics
domains 0 hits. Flatpak-offline + CI gates remain PENDING
(Phase 8) — stated, not claimed.

**Screenshot inspection (each PNG read visually).**
- `01-main-ar.png`: dark theme, Arabic RTL — menus mirrored,
  categories panel on the right, toolbar unclipped, empty-state
  text centered, mixed Latin ("Main download queue") rendered
  via the Carlito fallback. No clipping, no tofu.
- `02-progress-ar.png`: dark theme, Arabic RTL main window —
  one row ("download", 48.00 MB) at 24.54% with the progress bar
  in the status column, 35 s remaining, 1.01 MB/s, status bar
  "1 قيد التحميل · 1.01 MB/ث". No clipping.
- `03-add-dialog-ar.png`: HLS master — URL, duration 00:00:10,
  quality line "720p · 30 fps · avc1.64001f · 200 kbps + 32 kbps
  صوت", size "≈ 283.20 KB (تقدير من معدل البت)", info "بث فيديو
  (HLS)". Values match the fixture exactly (200,000 video bps +
  32,000 sampled audio bps × 10 s = 290,000 B = 283.20 KiB).
- `04-main-en-light.png`: light theme, English LTR — all labels
  readable, toolbar unclipped ("Delete Done" wraps to two lines,
  pre-existing). `05-main-en-dark.png`: same layout, dark theme.
  `06-main-ar-light.png`: light theme, Arabic RTL, unclipped.
- `07-add-dialog-dash-ar.png`: multi-Period DASH — duration
  00:01:00 (two 30 s Periods), "1080p · 30 fps · avc1.640028 ·
  3000 kbps + 128 kbps صوت", "≈ 22.37 MB" ((3,000,000+128,000)
  × 60 / 8 = 23,460,000 B = 22.37 MiB), "لا يدعم الاستئناف".
- `08-details-ar.png`: progress/details window at 38% of 48 MB,
  1.02 MB/s, 29 s left, per-connection segment bar with 4
  downloading segments and one small stalled block, legend
  (downloading/stalled/done/waiting), 4 connections at ~4.6 MB
  each. The "stalled" color seen here is a single small block
  in one frame, not a staged sustained stall.
- `09-site-ar-light.png` / `10-site-en-dark.png`: the site in
  Arabic RTL light / English LTR dark, hero, stats, and section
  headings all rendering correctly; the hero image still shows
  the old v1.0.7 screenshot because the PNGs have not landed
  yet (expected — same path, refreshes on upload).

**Not verified / why.** Windows screenshots: NOT captured (no
Windows machine or Windows CI run available; the staged
amendment is untested — see §14). Real KDE Plasma (Wayland)
dialog behavior: NOT verified (no compositor). Lighthouse:
NOT measured (see Phase 9). Flatpak bundle + CI artifacts:
NOT built (Phase 8 gate). Live-network behavior: NOT tested
(all servers local). The `03` metainfo dimension correction
(640×488) is escalated, not applied (the metainfo file is a
Phase 8 file; Phase 10 did not edit it). The remote tree is
INCOMPLETE until the PNG binaries land (see §10).

**New dependencies / licenses.** None in this phase.

**Open questions.** New Q15 (screenshot binaries + metainfo
dimensions + Windows screenshots, still open).

## §10 — Screenshot and binary escalation manifest (Phase 10)

All ten PNGs below exist ONLY in the local working tree
(`docs/screenshots/`, committed locally by Phase 10) and in the
archive (§10.1). The GitHub connector cannot push binaries, so
the remote tree still carries the three v1.0.7-era shots until
the parent uploads these by path through the browser.
**Replacements overwrite by path; additions are new paths; no
deletions.** The remote is incomplete until every row below
lands. Hashes were computed on the final cropped files in
`docs/screenshots/` on 2026-10-28.

| # | Repo-relative path | Action | Size (px) | Bytes | SHA-256 |
|---|---|---|---|---|---|
| 1 | `docs/screenshots/01-main-ar.png` | replace | 962×502 | 68,432 | `227bbca99a038384af3d70444824525a8553592708522f2c9a948d08027707cb` |
| 2 | `docs/screenshots/02-progress-ar.png` | replace | 962×502 | 65,750 | `131ab85bfe689da76f6441fdb2274532f8fd4c84d3312d0781bb07431dea7543` |
| 3 | `docs/screenshots/03-add-dialog-ar.png` | replace | 640×488 | 42,318 | `e67c82308f2b02755ac1f9ff44bf636cccda2fe465d573e7ff76cd188cc1ee0a` |
| 4 | `docs/screenshots/04-main-en-light.png` | add | 962×502 | 64,096 | `1ecfbe5a670631ed427fd00ead6fb4ddbcdb98d10e0af5de89f4e174b2bf6496` |
| 5 | `docs/screenshots/05-main-en-dark.png` | add | 962×502 | 64,050 | `d17393863a9dcfce1dc4f6dbef9a2313e4ce7ee1f90f08649496611d8cd05e12` |
| 6 | `docs/screenshots/06-main-ar-light.png` | add | 962×502 | 68,399 | `78b8092239c544366ef40c1dcc2b45f58d0935e38c4bf4e3c6f13aee6989a37e` |
| 7 | `docs/screenshots/07-add-dialog-dash-ar.png` | add | 640×488 | 42,220 | `12ae1bd59dcb32de3c0931c976439974d92e477206ba485024c06ddf29f454e5` |
| 8 | `docs/screenshots/08-details-ar.png` | add | 560×572 | 41,282 | `5d91145ae882876d816222e600ab7c2749347a5b75ea6e716225a60bbb50909a` |
| 9 | `docs/screenshots/09-site-ar-light.png` | add | 1280×900 | 238,630 | `ca2442552220eac57853bf66bfe5a5a6f400d6a055165269388caf38835e1fab` |
| 10 | `docs/screenshots/10-site-en-dark.png` | add | 1280×900 | 304,732 | `97f43777ed62442f08653de5b7d7ff3513de6b08a07a172e2760f81ca10c9ead` |

What each shot shows is recorded in the Phase 10 section above
and in `docs/README.md`. Capture provenance: app shots from the
release binary under Xvfb with the `/tmp/xkb` workaround, driven
through the localhost bridge for the Add dialog, against a local
fixture server serving the Phase 4 fixture content; site shots
from headless Chromium against this tree. No mockups, no staged
fakes: every pixel comes from a running build or the real site
files. Windows screenshots: none exist (see §11 item 1).

### §10.1 — Archive

`ezoo-download-1.1.0-enhanced.zip`, placed OUTSIDE the repo at
`/home/hatch/workspace/ezoo/ezoo-download-1.1.0-enhanced.zip`
(never committed into the repo). Built from the local working
tree — which holds all pushed content plus the uncommitted
Phase 8 files that are already on the remote — with the STAGED
Phase 8 workflow files overlaid as `.github/workflows/` (the
in-tree `.github/workflows/` are the old versions; the archive
carries the new v1.1.0 CI content, documented here). Contents:
source, `docs/`, `packaging/`, `.github/workflows/` (staged
Phase 8 versions), `IMPROVEMENTS_SUMMARY.md`, `docs/screenshots/`
(all 10 Phase 10 PNGs). Excluded: `target/`, `node_modules/`,
`.git/`, built installers, signing keys (verified absent: grep
for `.pem`/`.key`, the CRX key file, and the Phase 7 private-key
file — the latter lives outside the tree and was NOT copied
in), and large tool binaries (ffmpeg/yt-dlp/aria2 binaries —
the fetch scripts `fetch-ffmpeg.sh` etc. ARE included).
Verification (run 2026-10-28 on the delivered file): `unzip -l`
lists 256 files; no `target/`, `.git/`, `node_modules/`, `.pem`/`.key`
files, or ffmpeg/yt-dlp/aria2 binaries are present; all 10
screenshots, `IMPROVEMENTS_SUMMARY.md`,
`docs/RELEASE-REPORT.md`, `packaging/linux/fetch-ffmpeg.sh`, and
the 4 `.woff2` site fonts are present; the two overlaid workflow
files are byte-identical to the staged Phase 8 files (SHA-256
`0c75cc07…ce732` linux, `6cf9d703…68f8` windows). Size:
3,715,325 bytes. SHA-256:
`d2e8f6e5a414dba892d8425e5986dd6eb35a5aa5707a796edf7d4dad0831a75f`.

## §11 — Everything NOT built or captured (consolidated)

Explicitly, so nothing below is mistaken for done:

1. **Windows screenshots** — none captured. No Windows machine,
   no Windows CI run. A best-effort untested amendment is staged
   (§14); until it runs or a human captures them on Windows, the
   release has Linux screenshots only.
2. **Real KDE Plasma (Wayland) dialog verification** — the
   Phase 6 lazy-dialog fix is proven under Xvfb/X11 + code-level
   proof only. Q2 open.
3. **Flatpak bundle + CI artifacts** — no `.flatpak` built, no
   Flatpak run, no `ffmpeg`-on-PATH check, no real merge, no
   browser-bridge check from a host browser, no
   `appstreamcli`/`desktop-file-validate` run. Phase 8 gate
   entirely pending; tag `v1.1.0` not created.
4. **Lighthouse scores** — accessibility + performance ≥90
   unmeasured (Q14). Phase 9's axe-core + headless pass and
   Phase 10's visual re-capture are not substitutes.
5. **Live-network behavior** — every probe, benchmark, and
   update-checker test used local servers. No real host, no real
   GitHub API call, no real download from the internet was
   exercised in any phase.
6. **`.rpm` / AppImage builds in Phase 3** — only a real `.deb`
   listing was checked; the `.rpm` spec and AppImage staging use
   the same tree, but neither artifact was built there.
7. **Per-host learning across runs** — the learning store is
   implemented and unit/integration-tested, but benchmarks ran
   cold every time; no measurement shows a second download
   starting from learned values.
8. **GUI smoke tests where skipped** — Phases 4–7 recorded the
   smoke test as not run (missing `libxkbcommon-x11.so`); the
   gap was closed for the main window, dialogs, and progress
   window by the Phase 6 Xvfb runs and the Phase 10 screenshot
   runs (with the recreated `/tmp/xkb` workaround), but no
   phase ran a smoke test for every dialog (About, Settings,
   Scheduler, Batch, torrent, playlist, preview) on camera.
9. **Windows-target compile** — never verified locally in any
   phase (no mingw toolchain); the escalated Windows clippy +
   full-test steps are the first check. Includes the
   Windows-only updater ask/install paths and font cleanup code.
10. **Signed `version.json` in a real release** — the signing
    mechanism is unit/integration-tested with throwaway keys and
    the `sign_update` example refuses a wrong secret, but no
    release has shipped a signed manifest yet (secret pending,
    Q3; hard-error step untested in CI).
11. **Firefox `.xpi` signed artifact / Chrome `.crx` acceptance**
    — carried over from the v1.0.7 release work; the `.xpi` is
    unsigned without AMO keys and Chrome acceptance of the `.crx`
    is untestable in CI (recorded in the v1.0.7 report, unchanged
    by this upgrade).
12. **Quality steering** — the display-only selector (Q7) was
    never plumbed through; no phase built it.
13. **Sustained "stalled" segment-bar state** — seen only as a
    small block in one Phase 10 frame; no stall was staged and
    held on camera.
14. **Screenshot binaries on the remote** — the 10 Phase 10 PNGs
    (plus the 6 Phase 3 `.ttf` and 4 Phase 9 `.woff2` files) are
    local-only until the browser upload happens; the remote tree
    and the live site are stale until then.

## §12 — New dependencies and licenses (all phases)

| Phase | Dependency change | License consequence |
|---|---|---|
| 0–2 | None | None |
| 3 | None (no new crates) | +3 OFL 1.1 font families (Carlito, Noto Sans Arabic, Liberation Serif), shipped unmodified; OFL texts ship in packages; `THIRD-PARTY-NOTICES.txt` + `LICENSE-DEB` entries added; InstaArabi + renamed Liberation files removed |
| 4 | None | None |
| 5 | sha2 as dev-dependency only; `Cargo.lock` unchanged | None (sha2 already in the tree) |
| 6 | None | None |
| 7 | reqwest as a direct dep of the app crate only (to name the shared client type); `Cargo.lock` gains 1 edge line, no new packages | None |
| 8 | None | None (runtime codec libraries via the Freedesktop SDK; no ffmpeg binary bundled by the Flatpak manifest) |
| 9 | None (no new crates) | +4 WOFF2 site fonts (same OFL 1.1 families as Phase 3); OFL texts ship as plain text |
| 10 | None | None |

App license throughout: MIT © 2026 Ahmed Ezzat. `THIRD-PARTY-NOTICES.txt`
covers Slint (Royalty-free 2.0), yt-dlp (Unlicense), FFmpeg
(LGPL-3 Windows / GPL-3 johnvansickle Linux), LAME (LGPL),
aria2 (GPL-2+), Rust crates (MIT/Apache-2.0), and the three OFL
font families. No license terms were changed by any phase; all
existing copyright notices and credits to Ahmed Ezzat / Ezoo
Tech and upstream were kept (verified in every phase's grep
gate: promo removal never removed a credit).

## §13 — Open questions status

| # | Subject | Status |
|---|---|---|
| 1 | InstaArabi license / Flatpak yt-dlp (resolved item numbering in file: Q1 yt-dlp resolved Phase 8; font license resolved item 1) | Resolved: REPLACE fonts (Phase 3); yt-dlp via hash-checked fetch (Phase 8) |
| 2 | Wayland verification of the dialog fix | OPEN — fix shipped, X11 evidence only |
| 3 | Fork update-signing keypair / `UPDATE_SIGNING_KEY` secret | OPEN — code done, secret pending with parent |
| 4 | Update-check behavior for Store/portable copies | OPEN — keep current behavior unless directed |
| 5 | fontique zero-font panic | OPEN — upstream blocker, substitute test only |
| 6 | Phase 4 ru/tr needs-review translations | OPEN |
| 7 | Display-only quality selector | OPEN — steering not built |
| 8 | HLS separate-audio bandwidth double-count rule | OPEN — keep literal ADD rule |
| 9 | Scaler probe-then-measure reading | OPEN — keep probe reading |
| 10 | Phase 6 "Stalled" needs-review translations | OPEN |
| 11 | Phase 7 update-string needs-review translations | OPEN |
| 12 | AppImage hash cross-checked, not directly signed | OPEN — keep |
| 13 | Flatpak ffmpeg binary presence | OPEN — needs a real Flatpak run |
| 14 | Site font binaries + Lighthouse | OPEN — binaries escalated, Lighthouse unmeasured |
| 15 | Screenshot binaries + metainfo dimensions + Windows screenshots | OPEN — see §10/§14 |
| 16 | Browser-extension overlay redesign (future work) | OPEN — no code in 1.1.0; redesign only, see `docs/OPEN-QUESTIONS.md` Q16 |
| 17 | No top-level LICENSE file; MIT terms stated in LICENSE-DEB only | OPEN — redistribution only within the stated terms; see `docs/OPEN-QUESTIONS.md` Q17 |
| 18 | On-disk protection of stored browser-session headers on Linux | OPEN — DPAPI confirmed on Windows; no mode-0600 code found; see `docs/OPEN-QUESTIONS.md` Q18 |

(Resolved items 1–4 in the file's Resolved section — InstaArabi
verdict, AboutDialog buttons, Cairo statics, Flatpak yt-dlp —
are recorded with their evidence in `docs/OPEN-QUESTIONS.md`.)

## §14 — Escalation inventory (still pending with the parent)

Everything below is OUTSIDE this branch's pushed text tree and
must be handled by the parent agent (browser work). Nothing here
was created, tagged, or released by any phase worker.

### A. Binaries for browser upload (connector cannot push binaries)

| Group | Files (repo-relative path) | Action | Bytes / SHA-256 |
|---|---|---|---|
| Phase 3 fonts | `crates/app/ui/fonts/Carlito-Regular.ttf` | add | 628,032 / `f6418f70…c6595` (full hash in `docs/FONTS.md`) |
| | `crates/app/ui/fonts/Carlito-Bold.ttf` | add | 682,468 / `bb5d20f7…94db` |
| | `crates/app/ui/fonts/NotoSansArabic-Regular.ttf` | add | 234,892 / `bdff3e56…ef48` |
| | `crates/app/ui/fonts/NotoSansArabic-Bold.ttf` | add | 261,460 / `4e5462d2…555` |
| | `crates/app/ui/fonts/LiberationSerif-Regular.ttf` | add | 393,692 / `9caef765…23d4` |
| | `crates/app/ui/fonts/LiberationSerif-Bold.ttf` | add | 370,196 / `443a4dbd…9ebd` |
| Phase 9 site fonts | `docs/assets/fonts/Carlito-Regular.woff2` | add | 189,856 / `6b8cada2…0f117` |
| | `docs/assets/fonts/Carlito-Bold.woff2` | add | 197,952 / `e8f1f2ec…2ba96` |
| | `docs/assets/fonts/NotoSansArabic-Regular.woff2` | add | 74,176 / `85bd1ed4…fe97` |
| | `docs/assets/fonts/NotoSansArabic-Bold.woff2` | add | 77,688 / `25815546…d5ab` |
| Phase 10 screenshots | See §10 manifest | 3× replace + 7× add | See §10 |

Full hashes for the fonts are in `docs/FONTS.md` (Phase 3) and
`docs/README.md` (Phase 9). **Pre-tag blocker:** if the 6 `.ttf`
files are still absent at tag time, every CI job fails at
compile time (they are `include_bytes!` inputs). Upload them
BEFORE creating the tag.

### B. Workflow files (Phase 8 staging, browser-only)

- `~/workspace/ezoo/workflow-escalations/phase8/linux-release.yml`
  → `.github/workflows/linux-release.yml`; `windows-release.yml`
  → `.github/workflows/windows-release.yml`. Base commit and
  commit message in the Phase 8 `README-ESCALATION.md` §2
  (message contains no `[release]`). `build.yml` untouched.
- Phase 10's Windows-screenshot amendment is staged SEPARATELY
  at `~/workspace/ezoo/workflow-escalations/phase10/` (diff +
  rationale + risk notes) and must NOT be applied together with
  the Phase 8 files without the coordinator's decision; the
  Phase 8 staging was not modified.

### C. Secrets (browser, before the tag)

- `UPDATE_SIGNING_KEY` — private Ed25519 seed (64 hex chars) in
  `/home/hatch/workspace/ezoo/.update-signing-private.key` (mode
  600, outside the tree). Public key `a495ab88…640fc`,
  fingerprint SHA-256 `6a8ee4d63d46aeaf…` (full values in
  `docs/UPDATE-CHECKER.md` §9). **Never include the private
  value in any file, message, or report.** A tag build without
  it FAILS by design (signing hard error).
- `CRX_PRIVATE_KEY` — already exists (original Chrome extension
  key, id `bilpkcfhiogfelpdfpgmnjkpblgmcbkd`). No action.
- `AMO_JWT_ISSUER` / `AMO_JWT_SECRET` — optional; Firefox add-on
  ships unsigned with a warning without them.

### D. Tag / release (after A–C)

- Tag `v1.1.0` (must equal `Cargo.toml` version `1.1.0`) on the
  commit whose tree contains the Phase 8 files + uploaded font
  binaries. Do NOT pre-create the GitHub Release: the Linux
  workflow creates it (`gh release create "v$VERSION" dist/*`);
  the Windows workflow waits and uploads Setup.exe +
  `version.json` + `SHA256SUMS-windows.txt` with `--clobber`.
  Enrich the release body afterwards (v1.0.7 pattern: long
  bilingual note). Never use `[release]` in any commit message
  pushed to `main` (upstream `build.yml` deletes older
  releases/tags on such pushes).
- Metainfo correction in the same browser pass as the PNG
  uploads: `packaging/flatpak/io.github.kerlif.EzooDownload.metainfo.xml`,
  the `<image>` for `03-add-dialog-ar.png`: width 962 → 640,
  height 502 → 488. (Also correct the 1.1.0 release-entry date
  if the tag lands after 2026-10-28.)

### E. Verification follow-ups (after the tag build)

- CI gate (Phase 8 §7 of its escalation README): `.deb` +
  `.rpm` + AppImage + `.flatpak` + Windows installer +
  `SHA256SUMS.txt` + `SHA256SUMS-windows.txt` + signed
  `version.json` (128-hex `sig`) all in release `v1.1.0`;
  Windows clippy + full tests green; Flatpak install/run checks
  (`command -v ffmpeg`, one merge, bridge, yt-dlp fetch, tray,
  portal chooser, Wayland + X11).
- Lighthouse on the live Pages URL, both languages (Q14).
- Real KDE Plasma (Wayland) dialog check (Q2).
- Windows screenshots, on Windows (Q15).

## §15 — Phase 10 gates re-run (summary)

| Gate | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo test` | 156 passed / 0 failed (1 doc-test ignored) |
| Linux release build | exit 0, binary 44,859,256 B (unchanged) |
| Promo / FluxDown / analytics greps | 0 hits outside `docs/PHASES.md` audit history |
| Update-checker mocked tests | included in `cargo test` (22 Phase 7 tests) |
| Flatpak-offline + CI gates | PENDING (Phase 8) — not claimed |

## §16 — Strengths, fixes, and later work

Additive summary only: every claim below is already evidenced in
§§Phase 0–10 above or in the phase documents they cite. Nothing
here adds a new measurement, and nothing here upgrades a pending
item to verified.

### Strengths

- **Bundled fonts, no system dependency** (Phase 3): Carlito, Noto
  Sans Arabic, and Liberation Serif, all SIL OFL 1.1 and shipped
  unmodified, registered in memory at startup through Slint's
  shared fontique collection. The substitute test (fontconfig
  seeing exactly one font covering zero UI glyphs) rendered
  pixel-identical to full-system-font runs, proving bundled-only
  rendering; the literal empty-fontconfig gate fails inside
  upstream fontique itself (Q5).
- **Lazy-dialog Wayland fix** (Phase 6): every dialog is created
  lazily on first open instead of ~20 windows at startup. Xvfb/X11
  verification showed exactly 1 mapped toplevel at startup, one
  dialog toplevel per opened dialog, unmapping on close, and reuse
  of the same window id on re-open. Real KDE Plasma (Wayland)
  verification is still pending (Q2) — the fix claim is X11
  evidence plus code-level proof from the Slint/winit sources, and
  is stated that way everywhere in this report.
- **Adaptive segmentation engine with per-host learning**
  (Phase 5): clean-room design, scaler / stall rescue / dynamic
  splitting, and a local-only learning store (EMA weight 0.3,
  30-day expiry, host-name key only, no network traffic). Benchmarks
  are loopback numbers on one VM, reported honestly: adaptive beats
  fixed-1 and fixed-4 and loses to fixed-8 and fixed-16 on both
  profiles (`docs/BENCHMARKS.md`); per-host learning across runs was
  not measured.
- **Signed update checker** (Phase 7): single fork source
  (`kerlif/ezoo-download`), Ed25519-signed manifest verified before
  anything is offered, dual SHA-256 matching, ETag cache,
  hand-followed redirects to GitHub hosts only, silent failures with
  backoff, and an opt-out in Options > General. No live GitHub API
  call was made in testing; a signed `version.json` in a real
  release is still pending (Q3, §11 item 10).
- **DRM refusal** (Phase 4): Widevine/PlayReady/FairPlay scheme
  UUIDs and HLS DRM KEYFORMATs refuse the download early; AES-128
  stays supported. No DRM circumvention anywhere.
- **No telemetry** (all phases): no analytics, tracking, or crash
  reporting; every outbound request is inventoried in
  `docs/PRIVACY.md`, and every phase's grep gate confirmed no
  analytics domains and no new network destinations beyond the
  inventoried ones.
- **Flatpak preparation** (Phase 8, prepared — awaiting CI):
  manifest, desktop file, and metainfo for
  `io.github.kerlif.EzooDownload`, vendored sources 709/709 crates
  matching `Cargo.lock`, minimal permissions (no `home`, no
  `host`). No Flatpak artifact was built or run; the ffmpeg-on-PATH
  question is open (Q13).
- **Bilingual website** (Phase 9): framework-free rebuild, Arabic
  RTL by default and English LTR, no trackers, no third-party
  scripts, no external fonts, axe-core 0 violations in both
  languages. Lighthouse scores were not measured (Q14).

### Fixed in 1.1.0

- The Wayland side-by-side dialog layout: dialogs rendered as flat
  tabs in one stretched view; fixed by lazy creation (Phase 6),
  subject to the pending real-KDE verification above.
- Unlicensed-font removal: InstaArabi (no published license) and
  the misleadingly renamed Liberation Serif files removed and
  replaced by the three OFL 1.1 families (Phase 3).
- Donation and promotional UI removal: donation blocks, channel
  buttons, and the InstaPay QR asset removed, with all credits to
  Ahmed Ezzat / Ezoo Tech and the upstream repository kept
  (Phase 2).
- Capture-window size accuracy: HEAD-first probing with ranged-GET
  confirmation corrects lying HEAD lengths, and HLS/DASH estimates
  are always labeled as estimates with their source (Phase 4).
- Update-source correction: the checker now points at the fork's
  releases instead of the upstream repository (Phase 7).

### Planned for later releases (not in 1.1.0)

- Real KDE Plasma (Wayland) verification of the dialog fix (Q2).
- Windows screenshots (Q15).
- Lighthouse measurement of the live site (Q14).
- A Flatpak CI artifact: the Phase 8 build/run verification
  follow-up.
- Quality-selector steering of downloads, instead of the current
  display-only selector (Q7).
- Browser-extension overlay redesign (Q16): an IDM-style floating
  "Download this video (N)" overlay button appearing on videos,
  more aesthetic and clearer, highlighting the captured files with
  their quality and size. No code for it exists in 1.1.0.

## §17 — Selective merge from the alt-build (2026-10-29)

The user built v1.1.0 a second time in a different container under
the same spec and sent the resulting tree for review. This merge
quotes ONLY what benefits the project from that tree
(`~/workspace/ezoo/alt-build-110/` as preserved); it is not a
wholesale replacement and the alt-build is not a reference import.
Nothing from it was taken on trust: every adopted item was
re-verified in this tree, and every rejected item is listed below
with its reason.

Adopted:

- Font evidence: `wire_fallbacks` extracted from `register()` in
  `crates/app/src/fonts.rs` (same behavior, now testable) plus two
  tests pinning that, with no system font in the collection, Arab
  script fallback resolves to Noto Sans Arabic, Latn to Carlito,
  and empty/unknown stored choices resolve to the bundled default
  for all 8 languages. `docs/FONTS.md` records the verdict
  (Arabic = Noto Sans Arabic, Latin = Carlito, serif = Liberation
  Serif) and corrects the generic-family order (bundled fonts are
  consulted before system generics). The 6-font set is kept; the
  alt-build's lack of a serif font was NOT copied. No font
  attribution text was adopted: our `FONTS-LICENSE.txt` is a
  strict superset of the alt-build's.
- Docs: §11 (strengths / fixed / planned, bilingual) in
  `IMPROVEMENTS_SUMMARY.md`; §16 in this report; the
  paired-comparison methodology description in
  `docs/BENCHMARKS.md` (no alt-build numbers copied); Q16
  (extension overlay redesign, future work) and Q17 (no
  top-level LICENSE file) in `docs/OPEN-QUESTIONS.md`; a website
  paragraph in `docs/PRIVACY.md`. Q18 (Linux secret-storage
  mode) added during this merge.
- Code: `crates/engine/examples/gen_update_key.rs`, a local
  key-derivation utility matching our Ed25519 scheme and the
  committed inline public key (docstrings corrected to our secret
  name; never executed, nothing generated).
- Staged outside the repo
  (`~/workspace/ezoo/workflow-escalations/merge110/`): the
  corrected metainfo file (one line: screenshot 03 dimensions
  962x502 to 640x488, verified against the PNG headers; the
  1.1.0 release date still needs correcting to the actual tag
  date at browser-commit time).

Rejected, with reasons: the alt-build's `probe.rs`/
`probe_fixtures.rs` (every behavior already covered equally or
better by `stream_meta.rs`; its false-length handling is worse);
its `flatpak.yml` (EOL runtime 24.08, retired ffmpeg-full
extension, wrong sources file, git-source rewrite our manifest
does not have, duplicate release path); its `packaging/keys/`
public-key file (our public key stays inline in `selfupdate.rs`
as the single source); its "dialog architecture not rewritten"
statement (describes the alt-build; our lazy-dialog fix stands);
all of its benchmark numbers, privacy-details, and engine
parameters that contradict this tree's verified evidence.

Gates re-run after this merge (this container, 2026-10-29):
`cargo fmt --check` clean; `cargo clippy --all-targets -- -D
warnings` exit 0; `cargo test` 158 passed / 0 failed / 1 ignored
(156 at Phase 7 + 2 new font tests); `cargo build --release`
exit 0 in 462 s, binary 44,859,256 B. Keypair check: the
committed inline public key matches the private key file
(hash comparison, boolean only). Xvfb re-verification (release
binary, `/tmp/xkb` workaround recreated, 1280x800): fresh
profile shows the main window plus the first-run
browser-extension prompt as a separate floating window; after
closing it exactly 1 mapped toplevel; the Add dialog is created
on first open, unmaps on close, and the same window id is reused
on reopen; the re-captured Arabic main window is visually
identical to `docs/screenshots/06-main-ar-light.png`, so no
screenshot was replaced. Ctrl+N via XTest did not open the
dialog in this harness (toolbar click did); the shortcut handler
is unchanged since Phase 6 and this is recorded as a harness
limitation, not a regression. Windows-target compile, real KDE
Plasma/Wayland behavior, Flatpak build/run, and Lighthouse
remain unverified (see §11); Phase 8 is still NOT passed — no
Flatpak/CI artifact exists yet.
