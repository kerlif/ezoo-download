# v1.1.0 upgrade — phase status

Target: fork `kerlif/ezoo-download`, `v110-upgrade` branch → v1.1.0.
Spec: user brief §§0–4 (2026-10-28). One commit series per phase,
conventional commits. Gates per spec §3 run after each phase.

| Phase | Scope | Status | Gate result |
|---|---|---|---|
| 0 | Baseline and audit | PASSED 2026-10-28 | Build OK (21m04s, 9 warnings); clippy 11 warnings (update.rs) + 5 Slint deprecations; tests 83 passed / 0 failed; fmt --check fails (651 hunks, queued for Phase 1). Docs pushed to `v110-upgrade` @ c2de3c41. Wayland root cause proven. Font verdict: InstaArabi REPLACE. Promo audit pending at gate time — results recorded in Phase 2. |
| 1 | Safe cleanup (no behavior change) | PASSED 2026-10-28 | fmt --check clean; clippy -D warnings exit 0 (0 Slint warnings); tests 83/0; release build OK (v1.1.0, 42,905,080 B). fmt baseline commit; update.rs warnings fixed via #[cfg(windows)]; Slint viewport deprecations renamed; rustdoc added; version 1.1.0 (code/packaging; README/site links deferred to release). Structural splits of main.rs/manager.rs DEFERRED to Phases 5/6. No dead code blocks, no unused deps; mpd.rs unwraps audited (guarded, left). FluxDown grep: 0 hits. No analytics/tracking domains. |
| 2 | Remove promotional and donation UI | PASSED 2026-10-28 | AboutDialog donation block + YouTube/Facebook channel buttons and WhatsNewDialog donation block removed (dialogs.slint −108 lines); both `open-link` callbacks + Rust handlers removed; 5 live @tr() strings + 1 orphan PayPal-only variant removed from ar_ui.py (6 keys), ui/*.json (6 keys × 6 locales), rs/*.json + i18n.rs (dead 1.0.4 changelog entry), .po regenerated deterministically (473→468 texts, 0 missing, run-twice diff empty); instapay-qr.png deleted. Credits kept: "Coded by Ahmed Ezzat", upstream links, installer AppPublisher/AppPublisherURL metadata. Gates: fmt --check clean; clippy --all-targets -D warnings exit 0; tests 83 passed / 0 failed (unchanged); release build OK (7m38s, 42,717,560 B). Grep gate: 0 removed-term hits in code/UI/locales/website/README except the kept installer AppPublisherURL + docs audit history; FluxDown 0 hits; no analytics domains. Version gate (show once per upgrade) intact. |
| 3 | Fonts | NOT STARTED | — |
| 4 | File size and stream metadata in the capture window | NOT STARTED | — |
| 5 | Adaptive segmentation engine (clean-room) | NOT STARTED | — |
| 6 | UI and dark mode (+ Wayland dialog fix) | NOT STARTED | — |
| 7 | Update checker (extend version.json only) | NOT STARTED | — |
| 8 | Flatpak and CI | NOT STARTED | — |
| 9 | Website redesign (docs/) | NOT STARTED | — |
| 10 | Screenshots, documentation, and archive | NOT STARTED | — |

## Rules in force

- Remote reconciliation (2026-10-28): remote `v110-upgrade` tip `a25ad951`
  (squashed snapshot) verified tree-identical to local HEAD `5deb389`
  (`git diff --name-only` empty; `a25ad951` is NOT an ancestor). The
  ordered force-with-lease history rewrite could NOT be performed: direct
  `git push` has no HTTPS credentials (`could not read Username`), SSH to
  github.com is blocked, `gh` is not logged in, and the GitHub connector
  (`push_files`) cannot rewrite history, force-push, or delete branches.
  Remote therefore stays at `a25ad951`; phase pushes stack snapshot commits
  on top of it via the connector (sync record
  `~/workspace/ezoo/v110.muse-github-snapshot-sync.json`). The detailed
  local history is retained in `~/workspace/ezoo/v110/`. Escalated to the
  parent: only a browser-side force-push could replace the remote history.
- Never touch upstream (`AhmedEzo686/ezoo-download`): no comments, no review
  requests, no merges, no PR #29 activity.
- Never put the string `[release]` in a commit message pushed to main
  (fork `build.yml` deletes older releases/tags on such pushes).
- The GitHub App cannot push `.github/workflows/*` (403) and cannot create
  tags/releases: workflow files, tags, and releases are escalated to the
  parent agent with exact contents/parameters.
- No telemetry, analytics, or crash reporting. No DRM circumvention.
- Clean room for the segmentation engine: general ideas only, from first
  principles, described in own words in `docs/ENGINE.md`. Never copy or
  paraphrase FluxDown code, never study its source, never mention it in
  UI/website copy.
- Screenshots only from real running builds. Never fabricate results.
