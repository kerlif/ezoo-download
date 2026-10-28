# v1.1.0 upgrade — phase status

Target: fork `kerlif/ezoo-download`, `v110-upgrade` branch → v1.1.0.
Spec: user brief §§0–4 (2026-10-28). One commit series per phase,
conventional commits. Gates per spec §3 run after each phase.

| Phase | Scope | Status | Gate result |
|---|---|---|---|
| 0 | Baseline and audit | IN PROGRESS | — |
| 1 | Safe cleanup (no behavior change) | NOT STARTED | — |
| 2 | Remove promotional and donation UI | NOT STARTED | — |
| 3 | Fonts | NOT STARTED | — |
| 4 | File size and stream metadata in the capture window | NOT STARTED | — |
| 5 | Adaptive segmentation engine (clean-room) | NOT STARTED | — |
| 6 | UI and dark mode (+ Wayland dialog fix) | NOT STARTED | — |
| 7 | Update checker (extend version.json only) | NOT STARTED | — |
| 8 | Flatpak and CI | NOT STARTED | — |
| 9 | Website redesign (docs/) | NOT STARTED | — |
| 10 | Screenshots, documentation, and archive | NOT STARTED | — |

## Rules in force

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
