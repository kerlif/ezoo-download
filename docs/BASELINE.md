# Baseline — Ezoo Download fork v1.0.7 (before the v1.1.0 upgrade)

Date: 2026-10-28. Branch: `v110-upgrade`, base commit `19713a4` (fork main).
Toolchain: cargo/rustc 1.99.1, Slint 1.18.1, winit 0.30.13, edition 2021.
License: MIT, Copyright 2026 Ahmed Ezzat (see `LICENSE-DEB`). All credits to
Ahmed Ezzat / Ezoo Tech and upstream `AhmedEzo686/ezoo-download` are kept.

## 1. Source-tree inventory

| Area | Contents |
|---|---|
| `Cargo.toml` (workspace) | members `crates/engine`, `crates/app`; `workspace.package.version = "1.0.7"`; release profile opt-3, thin LTO, `strip = true` |
| `crates/engine` (`swiftget-engine`) | `manager.rs` (~91 KB, download orchestration), `http.rs`, `http_download.rs`, `mpd.rs` (~36 KB, DASH MPD parser added on the fork), `m3u8.rs`, `hls.rs`, `ytdlp.rs`, `torrent.rs`, `mux.rs`, `segments.rs` (segment table), `config.rs`, `selfupdate.rs` (version.json logic), `tools.rs`, `limiter.rs`, `grabber.rs`, `filename.rs`, `category.rs`, `secret.rs`, `store.rs`, `system.rs`, `ef2.rs`, `fileio.rs`, `model.rs`, `lib.rs`. Tests: `tests/download.rs`, `tests/torrent.rs` |
| `crates/app` (binary `EzooDownload`, package name `swiftget`) | `main.rs` (~116 KB, 3178 lines), `i18n.rs`, `extra.rs`, `options.rs`, `grab.rs`, `browser.rs`, `api.rs` (localhost bridge), `clipboard.rs`, `playlist.rs`, `preview.rs`, `scheduler.rs`, `sound.rs`, `torrent_ui.rs`, `tray.rs`, `update.rs`, `winfx.rs`, `fonts.rs`, `build.rs` (slint-build) |
| UI (`crates/app/ui/`) | `app.slint` (3090 lines: `MainWindow`, `AddDialog`, `BatchDialog`, `DetailsDialog`, `CompleteDialog`), `dialogs.slint` (4040 lines: 15 more dialogs), `common.slint` (1283 lines: `ThemedWindow`, theme, palette). Fonts in `ui/fonts/`: `InstaArabi-Regular.ttf`, `TimesNewRoman-Regular/Bold.ttf` (Liberation Serif renamed — see `FONTS-LICENSE.txt`) |
| Locales | `crates/app/lang/`: `ar`, `de`, `es`, `fr`, `pt`, `ru`, `tr` `.po` files + `rs/*.json` resources + `make_po.py`/`ar_ui.py` tooling; English is built into `i18n.rs`. 8 locales: ar, en, fr, es, de, pt, tr, ru |
| Browser extension | `extension/`: `manifest.json`, `manifest.firefox.json`, `background.js`, `content.js`, `yt-core.js`, `popup.*`, `options.*`, icons, `test/`; `extension-signed/` holds signed artifacts; version `1.0.7` in both manifests |
| Linux packaging | `packaging/linux/`: `build-linux.sh` (deb/appimage/rpm), `build-extensions.sh` (zip/xpi/crx via pinned `crx3`), `fetch-ffmpeg.sh` (pinned johnvansickle static build, MD5-checked), `debian/` (control, postinst), `rpm/ezoo-download.spec`, `ezoo-download.desktop`, `io.github.ahmedezo686.ezoo-download.metainfo.xml`, `hicolor/` icons, `README-Linux.md`, `render_icons.py` |
| Windows | `installer/ezoo.iss` (Inno Setup, per-user), `build-windows.ps1`, `tools/ffmpeg/build.sh` |
| CI (`.github/workflows/`) | `build.yml` (upstream workflow, includes the destructive `[release]` publish step — NEVER use that string in a main commit message), `linux-release.yml` (fork: deb/appimage/rpm + extensions + release on tag), `windows-release.yml` (fork: real Setup.exe + version.json + SHA256SUMS-windows.txt) |
| Website | `docs/` (GitHub Pages, served from main): `index.html`, `style.css`, `script.js` (interactive download simulator), `assets/logo.svg`, `assets/hero-banner.svg`, `screenshots/` (3 Arabic screenshots), `README-Windows.md` |
| Signing/identity | CRX signing always uses the ORIGINAL key (extension id `bilpkcfhiogfelpdfpgmnjkpblgmcbkd`), held as repo secret `CRX_PRIVATE_KEY`. Update signing uses Ed25519 (`ring`) with the public key constant in `selfupdate.rs`; the fork's `version.json` currently ships with an EMPTY `sig` (no `UPDATE_SIGNING_KEY` secret in the fork), so the in-app updater refuses fork builds — see Open Questions |

## 2. Behavior checklist (regression reference)

Verified by reading code; each item is re-checked after every phase.

- **Queue**: downloads listed in `MainWindow` with states (queued/downloading/paused/done/error); add via dialog, clipboard monitor, browser extension, or batch dialog.
- **Pause and resume**: per-download pause/resume; resume map persisted (`segments.rs` plan + `store.rs`); partial files kept and verified on resume.
- **Capture**: browser extension bridge — `tiny_http` server on `127.0.0.1` (port 9614), `POST /add` with `X-SwiftGet` token header and Host validation against DNS rebinding (`api.rs`); `GrabberDialog` shows captured links; clipboard watching (`clipboard.rs`).
- **Browser extension bridge**: token-authenticated localhost mechanism, same on all platforms (Flatpak sandbox limitation documented in Phase 8).
- **Tray**: `tray-icon` 0.26 Linux (ksni/D-Bus StatusNotifierItem) / 0.19 Windows; single left-click; `tray32.rgba` icon; close-to-tray setting.
- **Scheduling**: `scheduler.rs` + `SchedulerDialog` + `CountdownDialog` (shutdown/sleep after downloads).
- **Speed limits**: `limiter.rs`, global and per-download limits, settings UI.
- **File organization**: categories (`category.rs`), per-category folders, filename sanitizing (`filename.rs`).
- **Update checker**: Windows-only auto-check (see §4); Linux shows a message pointing at the Releases page. `Help > Check for updates` window (`UpdateDialog`).
- **Language switching**: `i18n.rs` + `.po`/`.json` resources; switch applies immediately to all windows (`set_language`/`apply_look`).
- **DASH**: `mpd.rs` parser (Periods, Representations, SegmentTemplate, DRM scheme detection via `drm_scheme()`), `run_mpd_manifest()` path in the engine; HLS path unchanged from upstream.
- **HLS**: `m3u8.rs` master/media parsing (`BANDWIDTH`, `EXTINF`, `EXT-X-KEY` AES-128), `hls.rs` segment download/decrypt/merge.
- **Torrents**: `torrent.rs` + aria2 via `tools.rs`.
- **yt-dlp**: `ytdlp.rs`, user-triggered only.
- **ffmpeg muxing**: `mux.rs`, bundled ffmpeg preferred (Linux: `<exe>/lib/ezoo-download/ffmpeg` via `tools.rs` `tool_dirs()`; Windows: `tools/` folder).
- **Fonts**: embedded via `include_bytes!` AND installed to the per-user font folder at first run by `fonts::install()` (`$XDG_DATA_HOME/fonts/Ezoo/` on Linux, per-user Fonts + registry on Windows). No in-memory registration — the OS font system must pick them up. (Phase 3 replaces this mechanism.)
- **Autostart**: `autostart.rs` (Windows `#[cfg(windows)]` module + Linux desktop-file path).
- **Sounds**: `sound.rs` completion sounds; `preview.rs` media preview.

## 3. Located components (for later phases)

- version.json logic: `crates/engine/src/selfupdate.rs` — **`pub const REPO: &str = "AhmedEzo686/ezoo-download"` (UPSTREAM — Phase 7 must change this to the fork in this single constant)**; `signed_message()` covers `version + sha256`; `signature_ok()` verifies Ed25519 via `ring`; `check()`/`download()` in `update.rs` (Windows auto-check every 6 h, first check 3 s after start; Linux returns early with a message).
- DASH MPD parser: `crates/engine/src/mpd.rs` — `parse()` (line 582), `Manifest`/`Track`/`Segment` structs, `parse_iso_duration`, `representation_segments`, `drm_scheme()` (line 372), `MpdJob`.
- HLS handling: `crates/engine/src/m3u8.rs` (`best_variant`, `parse_media`, `parse_attrs`) and `crates/engine/src/hls.rs` (segment download, AES-128 `decrypt()`).
- Font loading: `crates/app/src/fonts.rs` (`install()`, `write_font()` per OS) + `ui/fonts/FONTS-LICENSE.txt`.
- Segment engine primitives (already present): `crates/engine/src/segments.rs` — `plan()`, `downloaded()`, `all_finished()`, `claim_work()`, `split_largest()`; per-host connection memory via `settings.connections_for()` in `manager.rs` (line 1214/1254); hard ceiling 32 connections (`manager.rs` line 359).
- Promotional/donation strings: audit in progress (parallel investigation; results feed Phase 2). Known pointer: `crates/app/ui/instapay-qr.png` is referenced from UI code — purpose to be confirmed by the audit.

## 4. Wayland dialog bug — root-cause diagnosis (proven from vendored sources)

**Symptom** (user report, KDE Plasma Wayland, both .deb and source builds): ALL
secondary dialogs/modals (About, Download File Info, Add batch download,
Download complete, …) render simultaneously, side-by-side in one stretched
view, as flat tabs with their own close buttons, instead of independent
floating pop-ups.

**Architecture**: `main.rs` creates ~20 Slint `Window` components eagerly at
startup (`AddDialog::new()`, `BatchDialog::new()`, `SettingsDialog::new()`,
`CompleteDialog::new()`, …) and keeps them as `Weak` handles in `Ui`. Each is
shown on demand via `open_window!` (`show()` + `place()` + `to_front!`).

**Root cause chain** (Slint 1.18.1 + winit 0.30.13, vendored sources read
2026-10-28):

1. Every Slint `Window` adapter registers itself in the backend's
   `inactive_windows` list at creation
   (`i-slint-backend-winit/.../winitwindowadapter.rs`, `new()`, `register_inactive_window`).
2. The backend calls `create_inactive_windows()` on event-loop `resumed` AND
   on every `about_to_wait` (`event_loop.rs:109,242`), which calls
   `ensure_window()` for EVERY registered window — including dialogs that
   were never shown.
3. `ensure_window()` creates the native winit window with
   `with_visible(false)` ("Never show the window right away"), but on
   Wayland that attribute is meaningless: winit's Wayland backend implements
   `set_visible` as a no-op — `// Not possible on Wayland.`
   (`winit-0.30.13/.../wayland/window/mod.rs:253`), and `is_visible()`
   returns `None`. Creating a Wayland window creates an xdg_toplevel that
   the compositor maps on first frame commit. Slint's own code says it:
   *"Wayland doesn't support hiding a window, only destroying it entirely"*
   and *"creating the window on wayland implies making it visible.
   Unfortunately, winit won't allow creating a window on wayland that's not
   visible."* (`winitwindowadapter.rs`, `suspend()`).
4. On X11/Windows `with_visible(false)` IS honored, so the dialogs stay
   unmapped until `show()` — which is why the bug is Wayland-only.

**Consequence**: on Wayland, all ~20 dialog toplevels exist and are mapped
from the first event-loop iteration. `hide()` cannot fix it: it only works
after a show/hide cycle (on Wayland it destroys the window, and the window
is then NOT re-registered as inactive). Calling `hide()` at startup is a
no-op (the adapter already reports `Hidden`).

**Fix (Phase 6)**: create each dialog's Slint `Window` component LAZILY —
only when first opened (create + wire callbacks on first access, keep the
`Weak` handle afterwards). A component that does not exist cannot be
registered as inactive and gets no native window until the user opens it.
This also removes ~20 up-front window creations on all platforms. The
refactor touches `main.rs` startup wiring and every module that uses a
dialog `Weak` handle (`extra.rs`, `grab.rs`, `browser.rs`, `update.rs`,
`options.rs`, `scheduler.rs`, `playlist.rs`, `torrent_ui.rs`, `preview.rs`).

**Verification limits**: no Wayland compositor is installed in this
environment (checked 2026-10-28: no weston/cage/labwc/kwin_wayland; only
`wayland-scanner`). Xvfb/X11 IS available, so the fix is verified there
(dialogs absent at startup, exactly one window per opened dialog, correct
hide/re-show). Real Wayland verification on KDE Plasma remains PENDING and
is listed as unverified in the release report.

## 5. Build metrics (baseline, v1.0.7 tree)

- `cargo build --release`: **SUCCEEDED** 2026-10-28 (21m 04s, cold build).
  9 warnings in the `swiftget` bin, all dead code on Linux (Windows-only
  update paths): `update.rs` — `downloading`, `apply`, `apply_as`,
  `round`, `Shown`, `size_text` (the tail of the log shows 6; 3 more
  above the visible tail — full warning list to be recounted in Phase 1).
  Toolchain note: `clippy`/`rustfmt` components were NOT installed for
  the default toolchain and were added via `rustup component add` on
  2026-10-28 before measuring (clippy 0.1.99, rustfmt 1.10.0-stable).
- `cargo clippy --all-targets`: finished with warnings only (7m 56s).
  Engine crate: **0 warnings**. App bin: **11 warnings**, all in
  `crates/app/src/update.rs` — 9 dead-code items (unused `Ordering`
  import; `BUSY`, `update_dir`, `downloading`, `apply`, `apply_as`,
  `round`, `Shown`, `size_text` — Windows-only update paths, dead on
  Linux) + 2 `clippy::needless_return` (lines 59, 148). Slint compiler:
  **5 deprecation warnings** in `app.slint` (`viewport-x`/`viewport-width`
  → `content-x`/`content-width`, lines 1570–1571, 1616–1621).
- `cargo test`: **83 passed, 0 failed** (13 app-bin unit tests, 62 engine
  unit tests, 4 download integration, 4 torrent integration; 1 engine
  doc-test ignored). Total wall time ~16 s for the test binaries
  themselves (debug build time excluded).
- Binary size (`target/release/EzooDownload`): 42,904,952 bytes (~41 MB).
- `cargo fmt --check`: **FAILS** in the baseline — 651 diff hunks across
  43 files (whole tree formatted with an older rustfmt style; e.g.
  `let-else` layout). Phase 1 will include one formatting-only `cargo fmt`
  commit so the §3 gate can pass from then on.
- Startup behavior under Xvfb: PENDING (checked in Phase 6/10 with screenshots).
- Also noted: `crates/engine/examples/sign_update.rs` exists (update-signing
  example — relevant to Phase 7).

## 6. License audit (Phase 0)

- `LICENSE-DEB`: MIT, Copyright 2026 Ahmed Ezzat, Source upstream URL. Covers `Files: *`.
- `THIRD-PARTY-NOTICES.txt`: Slint (Royalty-free license 2.0), yt-dlp (Unlicense), FFmpeg (LGPL-3 Windows build / GPL-3 johnvansickle Linux build), LAME (LGPL), aria2 (GPL-2+), Rust crates (MIT/Apache-2.0).
- Fonts: `TimesNewRoman-*.ttf` = Liberation Serif, OFL-1.1 (per `FONTS-LICENSE.txt`, verified against the files' own name tables: ID 13/14 carry the OFL 1.1 statement/URL; family renamed to "Times New Roman"). **`InstaArabi-Regular.ttf`: RESOLVED — REPLACE.** Font-licensing investigation (2026-10-28, read-only, web + name-table evidence): name table reads `Typeface © (Osama Al‑Najdi-Abdoaroof Aburziza). 2026. All Rights Reserved`, license IDs 13/14 absent; `FONTS-LICENSE.txt` says only "provided with the project by the user"; no stanza in `LICENSE-DEB`/`THIRD-PARTY-NOTICES.txt`; zero web hits for the font or author — no published license, repo, or source. Redistribution not permitted. Replacement candidates Carlito (OFL 1.1, verified verbatim from its OFL.txt), Cairo and Noto Sans Arabic (OFL 1.1) — exact copyright lines and static-vs-variable TTF availability to be confirmed from the authoritative repos at fetch time in Phase 3. Pre-existing compliance gap: `THIRD-PARTY-NOTICES.txt` has NO font entry at all (OFL §2 requires the notice + license with each copy); `FONTS-LICENSE.txt` contains no full OFL text.
- No other LICENSE files exist in the tree.
