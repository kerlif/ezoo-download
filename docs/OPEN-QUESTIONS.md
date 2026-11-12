# Open questions — v1.1.0 upgrade

Recorded instead of stopping to ask. Each entry states the default chosen
and what would resolve it. Resolved entries are moved to the bottom with
the resolution and date.

## Open

2. **Wayland verification of the dialog fix.** No Wayland compositor is
   available in this environment, so the Phase 6 lazy-dialog fix can only
   be verified under Xvfb/X11 plus code-level proof from the Slint/winit
   sources. FIX SHIPPED 2026-10-28: every dialog's Slint `Window` is now
   created lazily on first open (`LazyWeak` + `ensure_*` in `main.rs` and
   the dialog-owning modules; callbacks wired exactly once; `apply_look`
   / `set_language` never force-create). X11 EVIDENCE (Xvfb, 1024x640,
   release build, screenshots in /tmp): at startup exactly ONE mapped
   toplevel (main window; plus one unmapped 1x1 helper window from the
   tray/winit stack); Ctrl+N/Ctrl+B/Ctrl+F each add exactly one mapped
   dialog toplevel; closing unmaps it; re-opening reuses the same window
   id; a live throttled download opened the progress window with the
   per-connection segment bar. Default: ship the fix, verify on X11, and
   mark real KDE Plasma (Wayland) verification as PENDING in the release
   report.
   Resolution needs: running the v1.1.0 build on KDE Plasma (Wayland)
   and confirming dialogs open one at a time as floating windows.
   RESOLVED 2026-10-31 (Q27): the maintainer tested the fix himself on real
   KDE Plasma and the problem is solved — the KDE Plasma dialog bug
   is FIXED in v1.1.0 and VERIFIED on real hardware by the maintainer himself.
   (Earlier wording calling this unverified was corrected by
   The maintainer, 2026-10-31 20:10; see Q27 in the Decided section below.)
4. **Update-check behavior for the Windows Store / portable copies.**
   `selfupdate::installed()` already gates self-update on installed
   copies. Default: keep that behavior; portable copies get notify-only.
   Resolution needs: none unless the parent directs otherwise.
5. **fontique zero-font panic.** fontique 0.11.1 (Slint 1.18.1) panics
   when fontconfig reports zero fonts (see `docs/FONTS.md`): at
   Slint platform creation, `SystemFonts::new` calls
   `FcFontSort(...).unwrap()`, which returns `NoMatch` with no fonts
   installed. Default: none — the panic is upstream and no
   application-level code can avoid it; the Phase 3 gate used a
   one-glyph substitute font instead. Resolution needs: an upstream
   Slint/fontique fix, or a pinned upgrade that carries one; real
   systems always ship at least one font, so impact is limited to
   fontless test/container environments.
   Phase 7 mitigation (2026-11-01): the .deb and .rpm now install the
   six bundled OFL fonts (Carlito, Noto Sans Arabic, Liberation Serif)
   into `/usr/share/fonts/truetype/ezoo-download/` with an `fc-cache`
   refresh in postinst/`%post`, so a fontless system that installs the
   package gains fonts and the panic no longer triggers. The AppImage
   does not get this — fontconfig on the host cannot see inside it —
   so the underlying upstream panic is still open for AppImage runs
   on a fontless host.

6. **Phase 4 translations marked needs-review.** The Russian and
   Turkish wordings of the two size-estimation labels ("estimated from
   bitrate" / "estimated from sampled segments") are marked
   `needs-review` in the locale files (`crates/app/lang/rs/*.json` and
   `crates/app/lang/ui/*.json`, key `"needs-review"`). Default: ship
   them as written; they are short technical phrases and the UI always
   shows the number first. Resolution needs: a native speaker's check
   of those four strings.
7. **Capture-window quality selector is display-only in Phase 4.**
   The selector shows every parsed variant/representation with the
   estimated size of the selected quality, but choosing one does not
   steer the download: the engine still downloads the best quality,
   exactly as before Phase 4. Steering a download to the selected
   variant/representation needs new plumbing through `NewDownload` /
   the HLS and DASH runners and was not in the Phase 4 spec's
   no-download-behavior-change rule. Default: keep display-only until
   a later phase orders the plumbing. Resolution needs: a decision on
   whether selection should steer downloads, and if so, in which phase.
   Decision Q26 (2026-10-31): implement the quality-selector plumbing
   in v1.1.1. Plumbing DONE in Phase 5 (2026-11-01): `quality_key`
   travels `/add` → `AddRequest` → `VideoExtra` → `NewDownload` →
   `DownloadRecord` → `downloads.json`, with a storage test
   (`quality_key_is_stored_with_the_record`). STEERING DONE in Phase 6
   (2026-11-01): the AddDialog pick now steers the download — HLS by
   the picked variant's own playlist (`stream_variant_url` on
   `NewDownload`/`DownloadRecord`, the runner fetches it instead of
   the master; no pick = the best variant, as before), DASH by the
   quality key (`Manifest::pick`, a height like "720"/"720p"; unknown
   heights and audio-only picks fall back to the best track). The
   dialog reads the pick from the probed variants at confirm time; an
   early download that started with the best quality is dropped and
   re-added when the pick differs (its temp files are id-scoped, so
   no stale segments can leak into the fresh download). Proven by a
   cargo test (`hls::tests::variant_url_downloads_that_variant_only`:
   the low variant's bytes only) and a `Manifest::pick` unit test.
   Known limits: a picked variant URL can go stale on signed links
   (refreshing the link clears it and falls back to the best variant);
   a DASH manifest with video tracks offers no audio-only download
   (the runner still needs a video track); the probe keeps the best 8
   variants of a master, so a master with more than 8 qualities drops
   its lowest ones.

9. **Scaling rule read as probe-then-measure.** The spec says to add
   a connection only while throughput rises by ≥10% and to stop
   after two intervals without that gain. Phase 5 reads this as
   probe-then-measure: the scaler adds a connection and then judges
   the next window, because on a server that caps each connection the
   aggregate is flat at the starting count and waiting for a rise
   before adding would never add at all (proven by the first
   benchmark run: adaptive never grew past 4 connections on the
   capped server). The implemented rule still adds only following a
   rise, removes on a fall, and stops after two no-gain windows.
   Default: keep the probe reading. Resolution needs: a decision if
   the literal wait-for-a-rise reading was intended instead, in
   which case scaling would effectively be off for capped servers.
   Decision Q32 (2026-10-31): split-half work-stealing — a free
   connection takes half of the slowest segment's remainder —
   replaces/complements the scaler in v1.1.1 Phase 1; this resolves Q9.

10. **Phase 6 "Stalled" translations marked needs-review.** The new
    segment-state label "Stalled" (Rust `t()` string in the connections
    view, plus the segment-bar legend in Slint) is translated
    best-effort into the six non-English, non-Arabic locales
    (fr "Bloquée", es "Bloqueada", de "Blockiert", pt "Bloqueada",
    tr "Takıldı", ru "Зависший") and marked `needs-review` in
    `crates/app/lang/rs/*.json` and `crates/app/lang/ui/*.json`
    (the legend's "Receiving data…" / "Waiting" Slint strings in
    `ui/*.json` are marked too). Default: ship them as written; the
    state is also shown by the segment bar's colour. Resolution
    needs: a native speaker's check of those strings.

11. **Phase 7 translations marked needs-review.** The new update
    strings — one Slint string ("Check for updates automatically",
    Options > General) and four Rust strings (the Windows install
    ask, the AppImage replace ask, the AppImage-replaced
    confirmation, "Open the release page in the browser?") — are
    translated best-effort into the six non-English, non-Arabic
    locales and marked `needs-review` in `crates/app/lang/ui/*.json`
    (Slint string) and `crates/app/lang/rs/*.json` (Rust strings);
    the Arabic wordings were written directly (Arabic table in
    `i18n.rs`, `ar_ui.py`). The `i18n` placeholder-parity test now
    skips the `needs-review` metadata key, whose value lists source
    strings that legitimately contain `{}` placeholders. Default:
    ship them as written. Resolution needs: a native speaker's
    check of those strings.

12. **AppImage update hash is cross-checked, not directly signed.**
    The signed `version.json` manifest's signature covers the
    version and the Windows installer's SHA-256 only, so a Linux
    AppImage update is verified against the release's
    `SHA256SUMS.txt` entry, while the signed manifest gates the
    whole offer (no offer exists unless the signed manifest
    verified). The checksum file itself is unsigned. Default:
    keep, as documented in `docs/UPDATE-CHECKER.md` §5; extending
    the signed message to cover per-platform hashes would change
    the signing contract shared with the release workflows.
    Resolution needs: a decision if per-platform hashes should be
    signed in a later phase.
    Decision Q28/Q33 (2026-10-31): sign `SHA256SUMS.txt` in v1.1.1
    (Phase 9), plus an offline emergency signing key; signed
    `tools.json` deferred to a later release.

14. **Site font binaries + Lighthouse measurement (Phase 9).**
    The four WOFF2 site fonts (`docs/assets/fonts/`, manifest with
    SHA-256) could not be pushed through the
    GitHub connector (UTF-8 only) and are escalated to the parent
    for browser upload, together with the six Phase 3 `.ttf`
    binaries (the pre-existing pre-tag blocker). Until they land,
    the site renders with the declared system-font fallback stack.
    Separately, Lighthouse could not run in the Phase 9
    environment (loopback navigation blocked in Chromium;
    `file://` URLs rejected by Lighthouse), so the ≥90
    accessibility + performance gate is unmeasured: re-run
    Lighthouse against the live Pages URL after the merge to
    `main`, in both languages, and record the scores.
    Verification (2026-10-31, Phase 0): the binaries HAVE landed on
    remote main — GitHub API git-trees at `d6807d8` lists 6 `.ttf`,
    4 `.woff2`, 10 screenshots PNG. Lighthouse measurement still
    unmeasured.
15. **Screenshot binaries + metainfo dimensions + Windows screenshots
    (Phase 10).** The ten Phase 10 screenshots
    (`docs/screenshots/01`–`10`, manifest with SHA-256) could not be pushed through the
    GitHub connector (UTF-8 only) and are escalated to the parent
    for browser upload, together with the six Phase 3 `.ttf` and
    four Phase 9 `.woff2` binaries (Open Question 14): until they
    land, the remote tree still carries the v1.0.7-era shots and
    the live site renders with system-font fallbacks. The new
    `03-add-dialog-ar.png` is 640×488 (native dialog crop) while
    the Linux metainfo (`packaging/linux/io.github.kerlif.
    EzooDownload.metainfo.xml`, not edited in Phase 10)
    still declares 962×502 for it — correct the `<image>` width /
    height for `03` to 640 / 488 in the same browser pass that
    uploads the PNGs. Windows screenshots were NOT captured: no
    Windows machine or Windows CI run was available to this phase
    (a small, untested, continue-on-error amendment is staged at
    `~/workspace/ezoo/workflow-escalations/phase10/` for the
    coordinator to accept or reject); until Windows shots exist,
    the release report states them as PENDING. Default: ship the
    Linux screenshots, capture Windows shots on Windows.
    Verification (2026-10-31, Phase 0): the 10 screenshots + 6 `.ttf`
    + 4 `.woff2` ARE on remote main (API git-trees at `d6807d8`).
    Still open: metainfo `03` dimensions (640×488) + Windows
    screenshots.
16. **Browser-extension overlay redesign — FUTURE work, NOT part
    of v1.1.0.** The extension (`extension/content.js`) already
    shows a floating "Download this video" button on videos, with
    the count of captured items in its label, and its dropdown
    panel already lists captured items with their quality and size
    where known. The redesign requested for a later release is a
    visual/UX one only: an IDM-style floating "Download this
    video (N)" overlay button appearing on videos, more aesthetic
    and clearer than the current button, highlighting the captured
    files with their quality and size more prominently in the
    overlay presentation. A reference screenshot sent by the user
    is the design reference; it is not in the repository and no
    design detail is claimed from it here. No code for this
    redesign exists in v1.1.0 and no capture, plumbing, or
    download-behaviour change is implied by recording it: the
    current button and panel stay exactly as they are until a
    later phase orders the redesign. Default: keep the current
    button/panel unchanged. Resolution needs: a design decision
    in a later phase, made against the reference screenshot, and
    only then implementation in the extension.
    Decision (2026-10-31, Q19–Q22): the redesign is now the v1.1.1
    Phase 5/6 work — counter-badge pill, rows with name + size +
    Preview/Copy-link buttons, Preview = new browser tab,
    GrabberDialog same row pattern.
17. **No top-level LICENSE file; MIT terms stated in LICENSE-DEB
    only.** The task brief named a root `LICENSE` file, but no
    such file exists in this tree (verified: `git ls-files`
    lists `LICENSE-DEB` and `THIRD-PARTY-NOTICES.txt` only). The MIT terms
    (© 2026 Ahmed Ezzat) are stated in `LICENSE-DEB` in Debian
    copyright format (`Files: *`, full MIT text) and repeated in
    the release headers; `THIRD-PARTY-NOTICES.txt` covers
    the third-party components. Default: redistribute only
    within those stated terms; keep every existing copyright
    notice and credit to Ahmed Ezzat (Ezoo Tech) and the upstream
    repository; do not expand redistribution beyond what the
    stated licenses allow, and do not change any license terms.
    Resolution needs: a decision whether to add a top-level
    `LICENSE` file mirroring the MIT text in `LICENSE-DEB`, or
    to confirm `LICENSE-DEB` as the canonical copy. No license
    terms were changed by the v1.1.0 upgrade either way.



21. **aria2 module status (Flatpak).** The `aria2` module is
    defined in the manifest (aria2 1.37.0 from source, SHA-256
    verified, configure flags verified by a local `./configure`
    run) but its build is proven only by the WP10 proof run —
    no flatpak-builder exists in this environment. Default: keep
    the torrent bullet in the Flatpak metainfo, conditional on
    that proof. Resolution needs: the proof run (aria2c builds,
    `command -v aria2c` succeeds, one magnet completes); if the
    module does not build, apply Fallback B — remove the torrent
    bullet from the metainfo and document torrents as unavailable
    in the Flatpak build. **P8-bis (2026-11-08): MOOT — Flatpak
    packaging was deleted by the maintainer's decision; the aria2 module no
    longer exists and no Flatpak ships in v1.1.2.**

22. **ffmpeg alias mirroring.** The johnvansickle tarball URL is
    a moving alias pinned by SHA-256; a silent change fails the
    build loudly, but the alias itself can move. Default: keep
    the pin. Resolution needs: mirror the tarball to an immutable
    URL before any Flathub submission, or if the alias changes.

23. **Flathub policy on runtime fetch of yt-dlp.** The app
    downloads a hash-checked `yt-dlp_linux` binary into its
    sandbox data directory on first use and updates it with
    `yt-dlp -U` (documented in `docs/PRIVACY.md`). That is a
    runtime download of executable code; Flathub reviewers may
    question it. Default: keep the fetch (it is the app's
    long-standing behaviour on every platform). Resolution
    needs: a Flathub policy review before any submission.
    **P8-bis (2026-11-08): MOOT for v1.1.2 — no Flatpak ships,
    so no Flathub submission; the runtime fetch itself continues
    on every platform as the app's long-standing behaviour.**

24. **WM_CLASS verification.** Both desktop files declare
    `StartupWMClass=EzooDownload`. Default: keep. Resolution
    needs: a real run confirming the app's window actually
    carries that WM_CLASS (Slint/winit sets it from the app
    name; unverified in any environment so far).

25. **Wayland and tray verification on a real desktop
    (Flatpak).** The Flatpak runs under Wayland first with X11
    fallback and registers its tray icon via
    `org.kde.StatusNotifierWatcher`, but no Flatpak run was
    possible in this environment. Default: ship as configured.
    Resolution needs: the WP10 proof run on a real desktop
    (Wayland + X11, tray icon visible, portal file chooser
    opens). **P8-bis (2026-11-08): MOOT — no Flatpak ships in
    v1.1.2; Wayland/tray verification for the .deb/AppImage builds
    is covered by the normal desktop smoke tests.**

26. **Linux aria2/ffmpeg missing messages are untranslated.**
    WP3 made the missing-tool messages platform-specific with a
    pure selector (`tools.rs`), but the app UI maps only the
    Windows ffmpeg wording to its translation (`main.rs` matches
    on "ffmpeg.exe is missing"); the new Linux messages
    show in English until strings are added. Default: show the
    correct English message (better than a translated but wrong
    one). Resolution needs: two new strings × 8 locales
    (`crates/app/lang/rs/*.json` + `i18n.rs`, needs-review
    markers per the Q6/Q10/Q11 pattern).



## Decided for v1.1.1 (2026-10-31, maintainer decision)

All of Q19–Q34 decided before Phase 0. Phase mapping per
`~/workspace/ezoo/v111/PLAN-111.md` and
`~/workspace/ezoo/v111/ZAJIL-COMPARISON.md` §5.

- **Q19 — PR target: fork-internal PR (`v111-upgrade` ← `main`), as
  PR #2 for v1.1.0.** Work happens on a separate internal branch,
  then after verification, automation and full readiness it is pushed
  to main as usual via that internal PR (merge commit, without the
  bracketed release-trigger word in any message). NEVER any PR to upstream. (Correction
  2026-10-31 20:10: an earlier reading of «الغي ال PR» as cancelling
  every PR was wrong — the maintainer clarified it meant cancelling the
  UPSTREAM PR only. `PLAN-111.md` §7/§8/Phase 11 still records the
  superseded "no PR at all / direct merge" version and must be
  corrected to this.)
- **Q20 — Preview: new browser tab.**
- **Q21 — GrabberDialog: yes, same row pattern** (name + size +
  Preview/Copy-link buttons).
- **Q22 — Video button: IDM-style counter-badge pill**
  («Download video (N) ▾»).
- **Q23 — `restore_ffmpeg()` on Linux: the bundled ffmpeg inside the
  Linux packages suffices** — no fetching.
- **Q24 — Remove the old-release-deletion step from `build.yml`:
  yes** (Phase 9). Implemented in Phase 9 (commit 64b6d8f):
  the deletion loop (`gh release delete … --cleanup-tag`) is
  removed from `.github/workflows/build.yml`; verified by grep
  (no `release delete` / `cleanup-tag` remains in the file).
  Nothing else depended on it — the publish step only creates
  the release for the new version, and the "already released"
  guard in `linux-release.yml` / `windows-release.yml` is
  untouched. Old releases are now kept.
- **Q25 — Star banner: keep.**
- **Q26 — Q7 quality-selector plumbing: implement in v1.1.1.**
  Plumbing done in Phase 5 (2026-11-01): `quality_key` is carried
  from the overlay to the stored record; steering DONE in Phase 6
  (2026-11-01): HLS by the picked variant's playlist, DASH by the
  quality key — see Q7 above.
- **Q27 — Wayland on real hardware: the KDE Plasma dialog bug is
  FIXED in v1.1.0 and VERIFIED on real hardware by the maintainer himself
  (2026-10-31 — he tested the fix and the problem is solved).**
  See also the resolution note on Q2 above.
- **Q28 — Sign `SHA256SUMS.txt`: yes** (Phase 9; confirmed by Q33).
  Implemented in Phase 9: `linux-release.yml` signs
  `SHA256SUMS.txt` (new "Sign SHA256SUMS.txt" step, commit
  af62455) and `windows-release.yml` signs
  `SHA256SUMS-windows.txt` (commit 434fb4d), both with the
  existing `sign_update` example and the `UPDATE_SIGNING_KEY`
  secret, hard error on releases when the key is missing; the
  `.sig` files ship as release assets. Engine side (commit
  85099a5): `sums_signature_ok` verifies the signature when the
  asset is present and refuses the offer when it does not verify;
  releases without the asset (e.g. v1.1.0) keep the old behaviour.
  See Q12 below.
- **Q29 — Extension channel security: per-install secret token in
  v1.1.1 + native-messaging host later.** Implemented in Phase 1:
  token generated once per install (32 random bytes, hex, in
  `local-api-token`, mode 0600 on Unix), delivered via GET `/config`,
  enforced on all POST endpoints. The legacy fallback for the 1.1.0
  extension (accept `X-SwiftGet`-only from browser-extension origins
  or with no Origin) was removed in Phase 3, when the extension
  started sending the token on every POST; GET `/ping` and `/config`
  stay token-free. `/quit` (Phase 2) requires the token with NO legacy
  fallback. Native-messaging host is a later release.
- **Q30 — Per-site connection limits: yes, in v1.1.1.**
  Implemented in Phase 1 (engine): `ServerLimit.max_downloads`
  (separate from the per-download connection cap), shared counter in
  `Manager::pump`; a waiting download takes no queue place. UI for
  setting it is Phase 2.
- **Q31 — Refresh-link for expired links: yes, in v1.1.1.**
  Engine groundwork in Phase 1: `etag` captured on probe and stored
  on the record, `Manager::refresh_link` accepts a new link only when
  it is the same file (same size + same strong validator). UI button
  is Phase 2.
- **Q32 — Split-half work-stealing: yes** — a free connection takes
  half of the slowest segment's remainder; replaces/complements the
  adaptive scaler and resolves Q9. Implemented in Phase 1 (ETA-based
  split, stalled connection split first, 0.5 s judgement floor).
- **Q33 — Sign `SHA256SUMS` + offline emergency signing key: yes,
  in v1.1.1; signed `tools.json` deferred** (Phase 9).
  Implemented in Phase 9 (commits 85099a5, 434fb4d, af62455):
  sums signing as in Q28; the offline emergency keypair generated
  2026-11-01, public half committed in `selfupdate.rs`
  (`UPDATE_PUBLIC_KEY_EMERGENCY`), private seed held offline
  outside the repo (mode 600, see `docs/UPDATE-CHECKER.md` §12);
  the checker accepts a manifest or sums signature made with
  either key (`signature_ok_any` / `sums_signature_ok`,
  unit-tested with throwaway keys). CI signs with the primary key
  only — the emergency key is never provisioned in CI until the
  owner decides to. A live check against an emergency-signed
  manifest has not been run (it needs a release signed with that
  key); the helper-level tests cover the verification logic.
- **Q34 — Testserver + property-based segment tests in CI: yes,
  in v1.1.1** (Phase 9). Property half DONE in Phase 9 (commit
  22bf5d4): `crates/engine/tests/segments_property.rs`, proptest
  dev-dependency, 3 properties, 1,056 randomized cases
  (400 plan partitions + 400 progress/split sequences + 256
  split-only), all passing — plans always partition the file
  exactly, random progress + work-stealing splits never break
  the map, splits alone never lose a byte. Testserver half
  PARTIAL: the integration tests (`tests/download.rs`,
  `tests/adaptive.rs`, `tests/update_checker.rs`) already run
  against local HTTP servers with Range support, slow regions,
  and resume — but each file carries its own inline server; the
  shared reusable "testserver" module Zajil has: DONE in Phase 10
  (commit 6a85e21): `crates/engine/tests/testserver/mod.rs` — one
  accept loop + request parser + response writer; handlers return
  a plain `Response`, a chunked `StreamResponse` (delays, holds,
  per-chunk stop hook), or `Drop`. All nine inline servers
  (`download.rs`, `adaptive.rs`, `update_checker.rs`) migrated
  onto it with their fault behaviour kept (including the lying
  `Content-Length` of the truncated-body servers). Full suite
  still 203 passed / 0 failed / 1 ignored, no coverage loss.
- **Q12 — AppImage hash signed via `SHA256SUMS.txt`: DECIDED
  with evidence in Phase 9 (2026-11-01).** The v1.1.0 gap was
  that the signed manifest's hash covered the Windows installer
  only, and the AppImage went through the unsigned
  `SHA256SUMS.txt`. Decision (the maintainer's Q28/Q33, 2026-10-31):
  sign the sums file. Implemented in Phase 9 (commits 85099a5,
  434fb4d, af62455): the workflows sign `SHA256SUMS.txt` /
  `SHA256SUMS-windows.txt` and ship the `.sig` assets; the
  checker verifies the signature when present and refuses the
  offer when it does not verify; `CheckedRelease.
  sums_signature_verified` records which case applied. Unit
  test: `selfupdate::tests::sums_signature_and_emergency_key`
  (200 passed / 0 failed / 1 ignored, Phase 9 full run). What
  still needs a CI run: the workflows' sign steps and the `.sig`
  assets in a real release (local YAML validation passed only).
- **Q35 — Firefox `update_url` still points at upstream: OPEN**
  (Phase 9/10). `extension/manifest.firefox.json` still points at
  `https://github.com/AhmedEzo686/ezoo-download/releases/latest/download/firefox-updates.json`.
  The fork's workflows (`build.yml`, `linux-release.yml`,
  `windows-release.yml`) do generate `firefox-updates.json` with a
  `github.repository`-based `update_link` and publish it with the
  release — but only when a signed XPI for that version exists, and
  the repo holds only `extension-signed/ezoo-download-firefox-1.0.4.xpi`,
  so the v1.1.0 release carries no `firefox-updates.json`. Do NOT
  repoint the URL until a signed 1.1.1 XPI exists and a fork release
  publishes `firefox-updates.json`; until then the upstream URL stays
  (it serves upstream's file, not the fork's — Firefox would not find
  a fork update there).
  Phase 9 prep (2026-11-01): the `update_link` in
  `linux-release.yml` was FIXED (commit af62455) — it pointed at
  the unversioned `ezoo-download-firefox.xpi` while the asset is
  the versioned `ezoo-download-firefox-$EXT_VERSION.xpi`
  (Firefox would 404); it now points at the versioned name.
  Phase 10 unification DONE (2026-11-01, this branch): all three
  workflows now publish ONE name — the versioned
  `ezoo-download-firefox-$EXT_VERSION.xpi` (Linux already did;
  `windows-release.yml` and `build.yml` changed their `dist/`
  copies, `update_link`s and publish conditions to the versioned
  name). The stage copies (`$p/ezoo-download-firefox.xpi`) stay
  unversioned on purpose: `browser.rs` (`FIREFOX_XPI`) and
  `installer/ezoo.iss` look the unversioned name up in the stage
  dir. This is the one recorded exception to the maintainer's no-asset-
  renames decision (2026-11-01): the pre-unification state left
  two XPI files + a last-wins `firefox-updates.json` in the
  release. What a signed 1.1.1 XPI needs: the AMO signing
  secrets (`AMO_JWT_ISSUER` / `AMO_JWT_SECRET`) already in the
  workflows, or a previously signed XPI placed in
  `extension-signed/` (the workflows reuse it); then the
  release's `firefox-updates.json` exists and the manifest's
  `update_url` can be repointed to the fork's releases.

## Decided for the 1.1.1 hotfix (2026-11-02, maintainer decision)

All of Q36–Q48 decided with the amended hotfix plan, approved by
The maintainer, 2026-11-02 14:34 («أوافق على الخطة المعدلة — ابدأ الكود»).
Implemented on branch `v111-hotfix`, H1–H5; H6 is docs only.

- **Q36 — Highlights source for the new-version notice: option A —
  the GitHub release body from the already-fetched `releases/latest`
  API response.** Implemented in H1: `GithubPayload.body` →
  `CheckedRelease.notes` (capped at 4 KiB, `truncate_notes`), parsed
  by `release_highlights()` (first ≤5 bullets, language-matched).
  No new outbound request, no workflow change, no signature change,
  no risk to existing clients. Option B (a signed notes field in
  `version.json`) rejected: it needs a workflow change + a
  `signed_message` format change, and old 1.1.1 clients would fail
  to verify the new format — breaking updates for the installs
  already out there. Trust note: the body is unsigned, so a tampered
  body could show attacker text in the notice, but it cannot trigger
  an install (the offer and the artifact stay gated by the signed
  manifest + signed sums). `docs/PRIVACY.md` gains the one line.
- **Q37 — Where the "remind me" state lives: no durable state at
  all.** A per-run in-memory flag `LATER` (`crates/app/src/update.rs:54`),
  reusing the `DECLINED` / `NOTIFIED` pattern. The notice shows at
  most once per program run; «Later» sets the flag for the offered
  version, and the next launch's check re-offers if the update is
  still pending. The earlier Settings-fields proposal
  (`update_remind_at` / `update_remind_version`) is dropped with the
  4h timer.
- **Q38 — "Later" semantics: suppress the notice until the next
  launch, for this version only.** A newer version re-offers
  immediately; manual Help > Check for updates bypasses the
  suppression (as it bypasses `DECLINED` today). No timer, no
  wall-clock arithmetic: the re-offer point is simply the next
  program launch.
- **Q39 — Notice dialog shape: a new `UpdateNoticeDialog`**
  (heading + highlights + «Update now» / «Later», modelled on
  `WhatsNewDialog`). No third "never for this version" button — the
  existing disable-auto-update setting (upstream #49, already
  implemented) covers "never".
- **Q40 — What "Update now" does for Managed (.deb/.rpm):
  download the matching asset into `~/Downloads`, hash-verified
  against the release sums, then reveal the file / tell the user
  where it is.** Flavour from `/etc/os-release` (`ID_LIKE`
  containing `debian` → `.deb`, `rhel`/`fedora`/`suse` → `.rpm`);
  `.deb` is the fallback when unreadable. The app never runs the
  package manager. Implemented in H2 (`deb_asset()` /
  `rpm_asset()`, `managed_flavour()`, `download_managed()`).
- **Q41 — Reverse numbering shape: numbering only** (list order
  unchanged; bottom/oldest row = #1), per the requester's literal
  wording. Reversing the list order too is rejected: it changes the
  list the user already reads. Implemented in H4.
- **Q42 — Skip matching rule: exact computed target filename** in
  the target folder (same sanitize + number + ext as the add loop).
  The skipped count is reported. No URL-history matching (no history
  store exists; `downloads.json` holds only the current list).
  Implemented in H4 (9 new playlist tests).
- **Q43 — Panel fix scope: all three gaps** (§2.4 of the hotfix
  plan): gate `swiftget-open-panel` on a fresh config fetch +
  `panelAllowed()`, periodic config refresh, last-known-setting
  fallback in `chrome.storage.local` when the app is unreachable.
  Implemented in H5 (extension sims green; real-browser behaviour
  is an owner step).
- **Q44 — Delete-all re-seed: a `categories_seeded: bool` Settings
  field** (serde default `false`, set `true` on first seed);
  defaults are seeded only when `!categories_seeded`. Implemented
  in H3 (`crates/engine/src/config.rs:318`, `:592`), with a
  config round-trip test (delete-all stays empty after reload).
- **Q45 — About dialog wording: line 1 `Version 1.1.1
  (AppImage)` / `(.deb)` / `(.rpm)` / `(Flatpak)` / `(Windows
  setup)` / `(portable copy)`, line 2 the full executable path in
  muted small text.** Implemented in H2 (`package_short_label()`,
  `crates/app/src/main.rs:1452-1453`).
- **Q46 — Menu gate for General: `enabled: root.target !=
  "General"`** on the "Delete category" entry (the row's `target`
  is the category name); `builtin` keeps its rename-lock meaning
  in `CategoryDialog`. Implemented in H3.
- **Q47 — Phase 11 ordering: merge Phase 11 (`v111-upgrade` →
  main) first, then the hotfix PR (`v111-hotfix` → main).** The tag
  move (§8 of the hotfix plan) does not wait for either — it points
  at the hotfix commit wherever it lives. If the maintainer prefers one
  combined PR, the phases still run in the same order; only the PR
  step merges.
- **Q48 — Mid-run 24h checks: the notice is shown once at that
  point too (still at most once per run for that version) — it does
  not wait for the next launch.** Decided 2026-11-02 14:34 with the
  amended-plan approval (the maintainer tapped the plan-approval option, not
  launch-only). Implemented in H2 via the `LATER` flag; the
  `handle_available` gate does not distinguish launch checks from
  mid-run checks. Tests:
  `later_suppresses_the_notice_until_a_newer_version`,
  `handle_available_is_done_when_the_notice_is_suppressed`.

## Decided for v1.1.2 P2 (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. Neither of these
is a decision in the maintainer's own words.

- **Q53 — Extension-channel signing: (a) HMAC over the 0600 token,
  single port 9614 — DECIDED BY DEFAULT (a), implemented in P2.**
  Every POST carries `X-Ezoo-Time` / `X-Ezoo-Nonce` / `X-Ezoo-Sig`
  (HMAC-SHA256 over `METHOD\npath\ntime\nonce\n<body>`, ±300 s window,
  each nonce accepted once; `crates/app/src/api_sign.rs`). The old
  `X-SwiftGet-Token` bearer header is gone — HMAC-only, no fallback.
  No second port, no `/pair`, no `token.json`; the token stays in
  the existing 0600 `local-api-token` file only. `/quit` and `/show`
  need a valid signature too. The extension signs every POST in
  `extension/background.js` (`crypto.subtle`, HMAC-SHA256); the
  simulation test recomputes the signature independently.
  Upstream's literal scheme (b) was rejected: two ports, `/pair`
  returning the raw token, and a world-readable `token.json` next
  to the executable (see `REVIEW-109.md` §ج.2). Flagged for the maintainer's
  confirmation; if he prefers (b) or (c), the bearer header would
  have to be restored, which re-opens replay of captured requests.
- **Q54 — Clipboard monitoring default: (a) OFF by default —
  DECIDED BY DEFAULT (a), implemented in P2.** Fresh defaults and
  the v6 migration both turn it off. Settings v6 re-applies both
  v5 migrations: the fork's (update checks on — a v5 file that has
  the key keeps the user's own choice; a file without the key gets
  `true`) and upstream's (clipboard off). Exact migration rule for
  clipboard: a file with `version < 6` gets `clipboard_monitor =
  false` UNLESS `auto_start_clipboard` is true — auto-start
  defaults off and only works with monitoring, so that file belongs
  to a user who demonstrably uses the clipboard. Limitation, stated
  plainly: 1.1.1 wrote `clipboard_monitor: true` into every saved
  file as the unset default, so a stored `true` with auto-start off
  cannot be told apart from a user who explicitly turned monitoring
  on alone; those users migrate to off and re-enable it with one
  toggle in Options (the toggle itself is unchanged). Flagged for
  the maintainer's confirmation; if he prefers (b) (on by default), the
  default and the v6 migration both flip back.

## Decided for v1.1.2 P3 (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Q60 — Quality plumbing (Q7) + Arabic strings: (a) yes —
  DECIDED BY DEFAULT (a), applied in P3.** P0 had already verified
  the Q7 plumbing landed in 1.1.1 (quality key / variant URL /
  pick, HLS by variant URL, DASH by `pick(quality_key)`), so no
  plumbing was reimplemented in P3 — only verified end to end. The Arabic-strings half of (a) was applied
  to the genuinely new user-visible strings of this phase, in
  `crates/app/src/i18n.rs` (AR table, `// P3 (1.1.2)` comment) and
  `crates/app/lang/ar/LC_MESSAGES/swiftget.po`:
  - 403 handover notes: "The site refused the download, so it was
    opened in the browser." / "The site refused the download again,
    so the browser keeps it."
  - AV1 tip: "This video is in AV1 (the same picture in a much
    smaller file). If your player plays only its sound, without the
    picture, play it with VLC — or install your distribution's AV1
    codec package." (Linux wording; no store link.)
  - Extension repair: "The extension files were repaired. If the
    browser still says files are missing, remove the extension and
    add it again from this folder." / "The extension files could not
    be repaired: the folder could not be written." — plus the
    "Repair extension files" button label in `dialogs.slint` (`@tr`).
  - The quality window itself gained no new strings in P3 (its rows
    are quality labels + size/bitrate/codec tags, already present),
    and the `/check` states are invisible (fail-open), so nothing
    else needed translating; the whole extension was not
    re-translated. Flagged for the maintainer's confirmation; if he prefers
    (b) (quality selector display-only), the Q7 wiring stays as
    shipped in 1.1.1 and only the Arabic strings remain.

## Decided for v1.1.2 P5 (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Q56 — Private-tracker leak: (a) verify live first — VERIFIED
  CLOSED, leak proven and fixed in P5.** Live test, aria2 1.37.0
  (Debian 1.37.0+debian-1build3), 2026-11-05, standalone probe
  (`/tmp/q56/probe2.py`, kept only on the VM): two local HTTP
  trackers on 127.0.0.1 — A, the torrent's own announce URL, and X,
  an extra tracker passed only via `--bt-tracker`. The torrent's own
  tracker was dead (port 1), so aria2 fell through to the extra
  tracker. Public torrent: X saw 2 announces carrying the torrent's
  infohash. Private torrent (`private=1`): X saw 2 announces
  carrying the private torrent's infohash (canonical SHA-1
  `b96f9f1a…`, verified by recomputation) — **aria2 does NOT
  suppress `--bt-tracker` for private torrents: the leak is real**.
  (With a working own tracker, aria2 did not announce to the extra
  tracker within 20 s, so the leak only bites when the torrent's own
  trackers fail or are absent — which is exactly when extra trackers
  matter.) Fix, landed in P5: no `--bt-tracker` in the daemon
  arguments at all; the 11 public trackers are passed per download
  (`bt-tracker` RPC option) only when the torrent is known public
  (`TorrentInfo.private == false`, read from the .torrent file in
  `Aria2::add` and carried in `DownloadRecord.torrent_private`);
  private torrents and magnet links (privacy unknown until the file
  list comes) get none. Proven by the live integration test
  `private_torrent_is_never_announced_to_added_trackers`
  (`crates/engine/tests/torrent.rs`, via `Aria2::with_trackers`
  pointed at a loopback recording tracker): the private torrent
  produced 0 announces in 20 s, the public torrent announced its
  infohash. DHT/PEX suppression for private torrents is aria2's
  own BEP-27 behaviour and was NOT live-verified here; the magnet
  metadata fetch necessarily uses the link's own trackers and DHT
  to learn the file list, as every client must.

## Decided for v1.1.2 P4 (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Q57 — Exact cut (re-encode): (a) quick first, exact only if
  the check passes — DECIDED BY DEFAULT (a), implemented in P4.**
  The CI-staged pinned ffmpeg (johnvansickle 7.0.2-static, staged
  with `packaging/linux/fetch-ffmpeg.sh`, tarball SHA-256
  `abda8d77ce8309141f83ab8edf0596834087c52467f6badf376a6a2a4c87cf67`)
  was verified in P4 to carry `http`/`https` protocols and the
  `libx264` + `aac` encoders, so the exact cut ships: re-encode via
  libx264 `-preset veryfast`. No `libopenh264` in the package and none
  required — the encoder probe tries libx264 first, then
  libopenh264, then (Windows) h264_mf, fresh on every call. Flagged
  for the maintainer's confirmation; if he prefers (b) (exact cut mandatory),
  nothing changes on Linux — the check already passed — but a
  Windows ffmpeg build with OpenH264 v2.6.0 would still be needed
  before the exact cut can be promised on Windows.

## Decided for v1.1.2 P4-bis (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Extension clip UI is English-only (no Arabic strings).**
  The extension (`extension/content.js`) has no i18n mechanism at
  all — every string it shows is English, before and after P4-bis
  (the new "All qualities" / "Download part" windows included).
  The app's ClipDialog strings do have Arabic. Default: leave the extension English
  for 1.1.2; translating it needs a locale mechanism + a full
  pass over all its strings, not just the clip ones. Flagged for
  The maintainer: if the extension should also be in Arabic, that is a separate
  piece of work.
- **HLS (.m3u8) clip: still NOT supported — DECIDED BY DEFAULT,
  recorded as an open question for the maintainer.** The clip window says
  so plainly and offers no Cut button for streams; the background
  refuses a YouTube clip instead of silently downloading the
  whole video. Building HLS clip needs `parts_between()` + HLS
  job plumbing (plan §3.4) and is a bigger engine piece than
  P4-bis. Flagged for the maintainer: whether HLS clip belongs in 1.1.2
  at all, or in a later version.
- **ClipDialog shows the quality, it does not pick one.** The
  dialog cuts one file or one address, which has exactly one
  quality — so after "Read length" it shows the probed quality
  ("720p · 1280×720 · h264") instead of a picker. The quality
  choice lives in the extension's "Download part" window, where
  several qualities exist to choose from. Flagged for the maintainer only
  if he expected a picker in the app dialog too.

## Decided for v1.1.2 UI/UX pass (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Flat icon language (no gradients on functional glyphs)
  — DECIDED BY DEFAULT, applied.** The toolbar icons, the
  file-type icons, the tip bulb tile, the browser-card discs
  and the progress-window fill were glossy vertical gradients
  next to flat icons — two visual languages in one row. They
  are now flat fills in the same hues (same paths, no redraw),
  measured ≥3:1 on light and dark surfaces except the yellow
  folder and the silver disc, whose edges are carried by their
  strokes (recorded honestly in the report). The rule: gradients
  live only where they ARE the brand (the "E + arrow" tile and
  the bubble tile). Toolbar/header surface gradients stay —
  they are near-flat in the light theme and carry the saturated
  themes' white labels for WCAG AA.
- **Download badges unified to blue — DECIDED BY DEFAULT,
  applied.** Grabber's badge was green while Playlist's and
  Torrent's were blue, for the same "download" meaning; all
  three are now blue `#1f6feb`. Green stays reserved for "add"
  (the Add-URL circle, the Add-Batch badge).
- **Toolbar taller for two-line Arabic labels — DECIDED BY
  DEFAULT, applied.** Four Arabic toolbar labels were clipped
  to one word ("إضافة مجموعة" → "إضافة", "حذف المكتمل" →
  "حذف", "إيقاف القائمة" → "إيقاف", "قائمة تشغيل" →
  "قائمة"), so Delete / Delete Done and Stop / Stop Queue read
  identically. Two Arabic lines need ~34 px at 10 px type, so
  the button went 56 → 66 px, the label cap 26 → 36 px, the
  bar 64 → 74 px, the queue-menu popup y 66 → 76 px.
  Small-buttons mode untouched.
- **Q67 — Remove the sheen from the logo tile? OPEN.**
  `docs/assets/logo.svg` has a white 14% gloss overlay over
  the top 38% of the brand tile — the last glossy decoration
  on the mark. Rendered both ways
  (`docs/screenshots/logo-sheen-compare.png`, left = now,
  right = without): the difference is subtle at large sizes,
  invisible at 16–48 px. Removing it touches the brand mark
  and every derived raster (regenerated with the P7 scripts),
  so it waits for his word. NOT applied.
- **Q68 — Redraw the toolbar icons as a flat single-weight
  line set? OPEN.** An IDM-like set (one stroke weight, no
  badges overlapping glyphs) would be new original art, a
  bigger piece than a refinement. NOT started.
- **Q69 — Flatten the extension bubble button gradient?
  OPEN.** `.btn` in `content.js` (blue `#3f8cf5→#1d63d6`,
  plus one gradient per look) → flat `#1d63d6`. Small, but
  unverifiable in this VM (no headless Chromium can start
  here — the bubble was audited from code only), so it needs
  a real-browser screenshot before it lands. NOT applied.
- **Q70 — Add dialog shows the size twice? OPEN.** For a
  direct file the probe size appears under the file-type icon
  AND in the Size row (`main.rs:3055` vs `main.rs:3156`).
  Proposal: hide the variant size caption when there is
  exactly one variant. NOT applied.

## Decided for v1.1.2 P7 (2026-11-05, plan defaults — the maintainer's confirmation pending)

Per `PLAN-112.md` §4: when the maintainer does not answer, the plan's
recommended default is applied and recorded here as
decided-by-default, flagged for the maintainer's confirmation. This is not a
decision in the maintainer's own words.

- **Q61 — Hold-to-delete (press-and-hold delete): PENDING the maintainer's
  explicit approval — NOT implemented in 1.1.2.** The plan's option
  (a) ("yes, if the maintainer approves, with progress indicator, cancel on
  release/focus loss and a keyboard alternative") has no approval
  from the maintainer, so no hold-to-delete behaviour exists anywhere in the
  app: no long-press delete, no press-and-hold timer on any delete
  button. Delete still works exactly as in 1.1.1 (plain button /
  menu entry, with the existing confirm checkbox where one exists).
  Skipped in 1.1.2 unless the maintainer approves explicitly.
- **Q63 — "EDM" naming in the site and README: (a) yes —
  DECIDED BY DEFAULT (a), applied in P7.** "Ezoo Download Manager
  (EDM)" is the display name everywhere, with the full attribution
  sentence on first mention (§3.7) in `README.md`, `README.ar.md`,
  the About dialog, and the site license section + footer. Links,
  file names, package names and version numbers are untouched —
  the version/link bump is P10/P11, not P7.
- **Q51 — Rename scope: (a) display name only — applied in P7.**
  The direction (rename to "Ezoo Download Manager (EDM)") was
  confirmed by the maintainer, 2026-11-04 21:04; the scope stays (a): UI
  strings, window titles, About, extension manifests, metainfo
  `<name>`, desktop `Name`, installer display name, README and the
  site. Binary name (`EzooDownload`), install paths
  (`/usr/bin/EzooDownload`, `%LOCALAPPDATA%\Programs\Ezoo Download`,
  the hardcoded `DefaultDirName` in `installer/ezoo.iss`), Flatpak
  ID (deleted in P8-bis, 2026-11-08), updater asset names, extension IDs, registry keys and the
  `ezoo-download://` protocol are all unchanged. `detect_package`
  does not read the display name at all (Flatpak ID / `APPIMAGE` /
  exe path only), so package detection is unaffected by the rename.

## Open for v1.1.2 P8 (2026-11-05, regression)

Found in P8; none of these is fixed in P8 (the About fix below the
list is the one P8 change). All need the maintainer's decision or a later
phase; nothing here is closed by default.

- **Q71 — ffmpeg packaging: the pinned johnvansickle 7.0.2-static
  segfaults (exit 139) on every MPEG-TS input — DECIDED 2026-11-08
  (P8-bis): keep the pinned build; packaging UNCHANGED.** Evidence (P8, this VM,
  AMD EPYC 9D25): reproduced on ANY TS input — stream-copy
  (+faststart and not), libx264 re-encode, decode-to-null,
  video-only, audio-only, `-threads 1`, TS over HTTP; `ffprobe`
  from the same tarball segfaults too. Trace (`-loglevel trace`)
  shows the crash during MPEG-TS demuxer SDT parsing, before any
  stream data; even a 10-packet truncated TS crashes, while 188
  zero bytes are rejected cleanly. MP4→MP4 copy, MP4→MP3, MKV and
  WebM copy all work under the pinned build — the crash is
  TS-demuxer-specific, codec-independent (mpeg4 and H264+AAC TS
  both crash). A second johnvansickle build (git snapshot
  N-71064, 20240629) segfaults identically, so the bug is in the
  build environment, not the FFmpeg version. What works: BtbN
  8.1.3 static (`ffmpeg-n8.1-latest-linux64-gpl-8.1`,
  n8.1.3-34-ga35157abb9, gcc 16.2.0, GPL, includes ffprobe,
  libx264 + libopenh264 + libmp3lame) passes ALL TS cases —
  but its binary is 166 MB vs the pinned 77 MB; system ffmpeg
  8.1.2 also passes all TS cases. Whether the segfault reproduces
  on user machines is UNPROVEN — say so plainly; it was seen
  only on this VM. Options: (a) keep the pinned 7.0.2 — TS
  clip/merge stays broken for bundled-ffmpeg users (system
  ffmpeg works around it); (b) switch the pinned build to BtbN
  8.1.3 — works, ~2.2× the binary size, same GPL class, needs
  the pin + `fetch-ffmpeg.sh` changed; (c) use
  system ffmpeg — no bundled binary, breaks offline/portable
  installs; (d) vendor a different static build — needs
  evaluation. Recommendation (P8): (b) IF the segfault is
  confirmed on a real user machine (the maintainer's machine or a second
  VM); otherwise keep (a) and document TS as system-ffmpeg-only.
  **DECIDED by the maintainer, 2026-11-08 (P8-bis): option (a) — KEEP the
  pinned johnvansickle 7.0.2-static; bet on the user's system
  ffmpeg for TS. Fallback trigger: the first real-world
  complaint/issue about TS/HLS merge/cut → switch to BtbN 8.1.3
  and accept the ~166 MB size cost. The release notes carry the
  same decision.** The targeted test
  `mux::tests::stream_to_mp4` fails under the pinned ffmpeg
  (pre-existing, known) and passes with system ffmpeg.
  **P8-ter (2026-11-08): the documented workaround is now CODE.**
  `mux::run_ffmpeg_watched` retries ONCE with the system ffmpeg
  from PATH when the bundled build dies from a signal (merge,
  TS→MP4, MP3, cut, and the `clip::look` probe); an ordinary
  failure never retries, and the bundled-first order is unchanged.
  When no system ffmpeg exists, the user gets a clear message
  (`tools::ffmpeg_crashed_message`): the bundled ffmpeg crashed
  on this file, install ffmpeg via the package manager. The site,
  READMEs and release notes now state the TS limitation instead
  of promising "nothing extra to install" unconditionally. The
  BtbN 8.1.3 fallback trigger is unchanged (first real-world
  TS/HLS complaint). Still NOT proven: the segfault itself on a
  real user machine (seen only on this VM), and the AppImage +
  libsecret path (UNVERIFIED — see the release notes).
- **Q72 — Settings dialog shows ~260 px dead space on the left
  in Arabic, with موافق/إلغاء detached at bottom-left — OPEN,
  pre-existing in 1.1.1, cosmetic.** In EN the dialog is tight.
  The layout is untouched in P0–P7 (branch diff is text-only
  renames), so this is almost certainly a 1.1.1 RTL wart, not a
  1.1.2 regression. NOT fixed in P8 (out of scope). Resolution
  needs: a later-phase RTL layout pass on the Settings dialog
  (`preferred-width: 820px`, `preferred-height: 600px`).
- **Q73 — DetailsDialog overlaps in Arabic: the "Time left" row
  is missing/hidden and the "Resume capability" label overlaps
  the "Show details >>" button — OPEN, pre-existing in 1.1.1.**
  In EN all 6 info rows render cleanly. Same Arabic line-height
  measurement class as the P8 About fix (Finding 1), but a window
  min-height would NOT fix it — the overlap is between layout
  children positioned by measured heights, not window clipping;
  the proper fix needs row height/width slack for Arabic.
  NOT fixed in P8. Resolution needs: a later-phase fix in the
  DetailsDialog info-rows layout (`crates/app/ui/app.slint:2538`).
  Evidence: P8 smoke shots `p8-88-details-ar.png` (AR, broken)
  vs `p8-58` (EN, clean).
- **Q74 — RTL row-menu hit offset (Xvfb only, unconfirmed) —
  OPEN, needs a real-desktop check before calling it a product
  bug.** In the Xvfb smoke rig, clicks on the Arabic row menu at
  the DRAWN item positions consistently missed (5+ attempts, 3
  items, instant and press-hold clicks, keyboard ignored); the
  same items responded ~50 px to the LEFT of their drawn position.
  The EN row menu works at drawn positions. The menu is rendered
  by Slint inside the main window (no separate X window), so this
  smells like a real RTL popup hit-testing offset — but it could
  be a Slint+Xvfb interaction. Resolution needs: confirmation on
  a real desktop (the maintainer's machine) before any fix is attempted.
- **Flaky test note (not a question):**
  `magnet_link_learns_its_name` failed ONCE in a full-suite run
  while a 152 MB file was downloading/extracting in parallel and
  ffmpeg tests ran (still `Downloading` after the 60 s window);
  it passes 7/7 alone and passed in both final full P8 suites.
  Recorded as flaky-under-load, not a regression. If it flakes
  again on a quiet machine, it becomes a real question.

## Accepted risks from the P8 security review (2026-11-05, recorded — NOT decided)

Found by the independent P8 security review, fixed where a fix existed, and
recorded here for the maintainer's awareness. Nothing here is decided by
default; the code documents each residual as it stands.

- **Q75 — Clip token rides the ffmpeg command line on Windows
  only — ACCEPTED RISK, needs the maintainer's awareness.** On Unix the
  served-URL token reaches ffmpeg through the `EZOO_CLIP_URL`
  environment variable, never on the command line. On Windows
  there is no `sh`, so the token rides the ffmpeg command line
  (`clip.rs` else-branches). Mitigations in place: one-use token,
  revoked the moment the cut ends (`manager.rs` drops the ticket);
  the URL is loopback-only. Recorded, not decided: if the maintainer wants
  the Windows path hardened further, that is a new phase, not a
  P8 fix.
- **Q76 — DHT/PEX suppression for private torrents is aria2's
  own BEP-27 behaviour, not live-verified here — ACCEPTED RISK,
  needs the maintainer's awareness.** The code gates added trackers to
  public torrents only (`torrent.rs: extra_trackers()`), derives
  `private` from the `.torrent` file itself, passes no trackers
  for metadata fetches of private torrents, and the live test
  `private_torrent_is_never_announced_to_added_trackers` proves
  0 announces to added trackers for private torrents (it skips
  entirely where `aria2c` is absent). What is NOT live-verified:
  that aria2 actually suppresses DHT/PEX for a private torrent
  (that is aria2's BEP-27 behaviour, trusted, not observed), and
  the magnet metadata fetch necessarily uses the link's own
  trackers/DHT to learn the file list. Recorded, not decided.
- **Q77 — Temporarily accepting the old 1.1.1 extension bearer token (X-SwiftGet-Token) as a compatibility fallback — OPEN, needs the maintainer's decision.** The 1.1.2 app refuses any POST without a valid HMAC signature (P2); the 1.1.1 extension only sends the old bearer token, so capture breaks for 1.1.1-extension users after the app update until the extension is updated to 1.1.2. Accepting the old token again — even temporarily — reopens the replay risk closed in P2 (a captured request replayable forever). Options: (a) no fallback — users must update the extension (warning in the release notes + in-app; manual install from the release page while the stores review, typically 1–2 weeks for a first listing with broad permissions); (b) a time-limited fallback with a sunset version. No code change in P8-quater; the decision is needed before or around the release. Store uploads are an owner step.
- **Q78 — Release/update split: the tag release publishes every file EXCEPT `version.json` — DECIDED 2026-11-08 (P8-quinquies), the maintainer's requirement.** The signed `version.json` is published later by the `publish-updater-manifest` workflow_dispatch job in `.github/workflows/windows-release.yml`, only after the 1.1.2 extension is approved in BOTH stores (Chrome Web Store + AMO). Until then installed copies see no update: a missing manifest asset is a silent fail in `selfupdate.rs` (proved by `manifest_asset_listed_but_404_is_a_silent_fail` in `crates/engine/tests/update_checker.rs`, which also covers the asset-listed-but-fetch-404 case). The dispatch job derives `version.json` from the PUBLISHED `EzooDownload-Setup.exe` (download + hash + sign with `UPDATE_SIGNING_KEY`, same guard as the release jobs) and never replaces an already-present manifest (no `--clobber`). `dist/version.json` is still written and still bundled in the artifact-only `EzooDownload-windows.zip`; the updater never reads it from there. `SHA256SUMS-windows.txt` covers the Setup.exe only (the manifest is authenticated by its own Ed25519 signature, never by the sums file). Related, same batch: the release carries all three extension files for manual install — Chrome `.crx` (signed with the original key), `.zip` (unpacked load), Firefox `.xpi` (unsigned — standard Firefox release refuses it; Developer Edition/Nightly or a temporary load via `about:debugging` only; the signed AMO version comes after store approval, Q35 open).
- **Q79 — Protected signing environment + verify-what-is-signed — DECIDED 2026-11-08 (P8-sexies), the maintainer's requirement.** (a) `UPDATE_SIGNING_KEY` moves from a repository secret into the GitHub environment `release-signing` with required reviewers; every job consuming it (`publish-updater-manifest` + `windows` in `windows-release.yml`, `linux` in `linux-release.yml`, `windows` in legacy `build.yml`) references that environment. The `extension_approved_in_both_stores` checkbox stays a human confirmation only. OWNER STEP (repository settings, after the squashed branch is pushed / P9, before the P10 tag): create the environment, set required reviewers, re-enter the key as an environment secret, then delete the repository copy. Until then the jobs read the repository secret and run ungated — the workflow change alone protects nothing. Consequence accepted: every run of a consuming job (tag AND main-push runs; legacy `build.yml` on every push) pauses for a reviewer's approval, because the gate is at job level. (b) The dispatch job verifies the published `Setup.exe` before signing it: it downloads `SHA256SUMS-windows.txt` + `.sig` from the same release, verifies the `.sig` with the new `crates/engine/examples/verify_sums.rs` (the same `selfupdate::sums_signature_ok` the updater uses), then hash-compares the Setup.exe against the signed sums entry — and refuses (fails, signs nothing) on any missing/invalid/mismatched piece. Proved locally against the real published v1.1.1 sums + sig (valid passes, tampered/missing/wrong-version rejected) and an independent OpenSSL-signed fixture. Details: `docs/UPDATE-CHECKER.md` §15.
- **Informational notes from the security review (N2–N5), read,
  no action taken in P8:**
  - **N2 — clipserve listener lives for the whole process once
    started.** After every ticket is dropped the server answers
    404 to everything (no access remains), but the port stays
    bound and the threads stay alive for the app's lifetime. By
    design, not a leak per cut.
  - **N3 — On Linux, site passwords and session cookies are
    stored in plaintext (pre-existing design, not a 1.1.2
    regression) — FIXED in P8-bis (2026-11-08).** `secret.rs` now
    stores them in the Secret Service keyring via libsecret
    (`keyring:<id>` on disk, schema `io.github.kerlif.EzooDownload`);
    when no keyring answers (headless, no D-Bus, locked collection)
    it falls back to the plain text in the owner-only 0600 file and
    warns once on stderr. Old plaintext values read as-is and are
    re-sealed into the keyring on the next save. `local_token.rs`
    now also re-applies 0600 on every load (P8-bis F4). Known wart:
    every seal stores a NEW keyring item under a fresh random id, so
    each run orphans the previous run's items; a later pass should
    store one master key and encrypt locally instead. CI needs
    `libsecret-1-dev` on Ubuntu; the .deb Depends on `libsecret-1-0`,
    the .rpm Requires `libsecret`, linuxdeploy bundles it into the
    AppImage. The live keyring path still needs a real-machine check:
    gnome-keyring is installed in this VM but its default collection
    cannot be brought up unlocked headless (the reference CLI
    `secret-tool store` fails with the same "no collection/login"
    error), so the store/lookup round-trip was verified only in the
    fallback mode here.
  - **N4 — HMAC window is ±300 s, not the plan's ±60 s**
    (deliberate widening from P2, against clock skew). Single-use
    nonces (remembered 600 s) mean a captured request still
    cannot be replayed; the wider window only extends the
    delivery window of a captured-but-undelivered legitimate
    request, which a loopback attacker could issue directly
    anyway.
  - **N5 — The API reads the POST body (up to 16 MiB) before
    verifying the signature.** A local process could make the API
    thread read 16 MiB per unsigned request — a trivial
    local-only denial-of-service surface, no data exposure. Not
    worth restructuring the handler for.

## P8 close (2026-11-05) — security-review fixes landed

The review's four open findings are FIXED in the P8 close: N1 served-URL cuts now run the resolved
ffmpeg via `EZOO_FFMPEG` (and the fix exposed + repaired a latent
P4 bug: every Unix served cut failed with "unexpected cut
arguments" because `cut_command` expected `-ss` at index 0 while
`cut_args` puts six common flags first); F1 the temp cookie file
is created 0600 on Unix; F2 `netscape_cookies()` drops cookies
whose domain or path contains a tab or line break (and `\r` in
any field); F3 `av1_tip()` sets its shown-flag only after the tip
actually displays. The review's F1 defence-in-depth suggestion
(delete the temp cookie file even if the yt-dlp call panics) was
NOT implemented — the file is deleted on every normal and error
return; only a panic between creation and deletion would leave
it, now at 0600.

## Resolved

13. **Flatpak ffmpeg binary presence** (resolved 2026-11-01,
    Phase 8 build WP4a). The follow-up the question itself
    named — a static-ffmpeg module — is now in the manifest:
    the `ffmpeg` module downloads the johnvansickle 7.0.2-static
    tarball pinned by SHA-256 (`abda8d77…`, verified 2026-11-01
    against the review value and
    `packaging/linux/fetch-ffmpeg.sh`) and installs the binary
    at `/app/lib/ezoo-download/ffmpeg`, the app's own search
    path (`tools.rs::tool_dirs` / `ours_ffmpeg`, verified in
    the code), with `ffmpeg-version.txt`, the GPLv3 text and
    `SOURCE-OFFER.txt` in `/app/share/doc/ezoo-download/ffmpeg/`.
    The metainfo now says the Flatpak bundles ffmpeg;
    `THIRD-PARTY-NOTICES.txt` and `docs/PRIVACY.md` were
    corrected the same way. Remaining proof: a real Flatpak
    run checking `command -v ffmpeg` in the sandbox plus one
    real video+audio merge (WP10 step 2).

3. **Fork update-signing keypair** (resolved 2026-10-31, Phase 0).
   The `UPDATE_SIGNING_KEY` secret was created 2026-10-29 via the
   browser. Re-verified 2026-10-31 (Phase 0) from the published
   v1.1.0 release (tag `v1.1.0`, target `5de9f77`): `version.json`
   downloaded from the release carries `sig` of 128 lowercase hex
   chars (non-empty) and a 64-hex `sha256` — the in-app updater
   accepts it. The stale claim that the fork ships an empty `sig`
   no longer holds.

4. **Flatpak yt-dlp** (resolved 2026-10-28, Phase 8). Decision:
    option (b) — no fixed yt-dlp is shipped inside the Flatpak —
    as implemented by the app's existing automatic fetch, not by
    a manual user install: `tools.rs::yt_dlp` downloads the
    current hash-checked `yt-dlp_linux` standalone build into
    the sandbox data directory when none exists, and runs
    `yt-dlp -U` daily thereafter (formerly documented in the deleted
    Flatpak doc §6; see `docs/PRIVACY.md`). A pinned module (option (a)) was
    rejected: a read-only `/app` copy could not be updated by
    the app's own updater, would duplicate the existing
    hash-checked fetch, and would freeze the extractor at
    release time. Missing-yt-dlp behaviour is the existing
    graceful error in `ytdlp.rs`; no download behaviour
    changed.

1. **InstaArabi font license** (resolved 2026-10-28, font-licensing
   investigation). Verdict: REPLACE. Evidence: embedded name table reads
   `Typeface © (Osama Al‑Najdi-Abdoaroof Aburziza). 2026. All Rights
   Reserved`, name IDs 13/14 (license) absent; `FONTS-LICENSE.txt` says
   only "provided with the project by the user"; no license stanza in
   `LICENSE-DEB` or `THIRD-PARTY-NOTICES.txt`; web search for the font
   and author returned zero relevant hits — no published license, repo,
   or download source. Redistribution is therefore not permitted.
   Phase 3 replaces it with Cairo or Noto Sans Arabic (both SIL OFL 1.1,
   verified) and must migrate/delete already-installed copies and saved
   `font_family: "InstaArabi"` settings. Carlito (OFL 1.1, verified
   verbatim from its OFL.txt) covers Latin; Liberation Serif (OFL 1.1,
   verified from the bundled files' own name tables) covers document
   text. Exact copyright lines for Cairo/Noto must be captured from the
   downloaded `OFL.txt` files at fetch time.

2. **AboutDialog YouTube/Facebook buttons** (resolved 2026-10-28, promo
   audit). The About dialog carries `YouTube` → `https://www.youtube.com/@Ezoo-Tech`
   and `Facebook` → `https://www.facebook.com/profile.php?id=61558130707674`
   buttons (`dialogs.slint:3744-3752`), plus `AppPublisherURL` in
   `installer/ezoo.iss:42`. Decision: the two buttons are promotional UI
   and are REMOVED in Phase 2 with the donation block; the "Coded by Ahmed
   Ezzat" text credit, all textual author/upstream credits, upstream
   links, and the installer publisher name/URL metadata are KEPT as
   attribution (installer metadata is not user-visible promotional UI).
   Rationale: Phase 2 removes promotional UI and keeps credits; channel
   buttons drive traffic to the author's channels and are promotion, while
   name/upstream attribution is credit.

3. **Variable vs static fonts for Cairo / Noto Sans Arabic** (resolved
   2026-10-28, Phase 3 fetch evidence). Verdict: Cairo has NO
   upstream static instances — `google/fonts` ships only
   `Cairo[slnt,wght].ttf` (variable) and upstream `Gue3bara/Cairo`
   ships only `fonts/Cairo/variable/` (variable-only build config) —
   so per the default, the family that ships statics was chosen:
   Noto Sans Arabic, release `NotoSansArabic-v2.013` from
   `notofonts/arabic`, whose archive contains static hinted
   `NotoSansArabic-Regular.ttf` and `-Bold.ttf`. No static instances
   were generated, so no Reserved Font Name question arises (all
   bundled files ship byte-identical to upstream). Slint 1.18 does
   not use fontdb: shaping goes through parley/fontique, and static
   weight selection (400/700, verified from the files' OS/2 tables
   and by unit test) works through the registered families. Full
   evidence, hashes and name-table dumps: `docs/FONTS.md`.

18. **On-disk protection of stored browser-session headers on Linux**
    (resolved 2026-11-01, Phase 7). `store.rs` now writes both
    secret-bearing files owner-only on Unix: the temporary file is
    created with mode 0600 (`OpenOptions::mode`, so the secret never
    sits world-readable even briefly) and the mode is re-asserted
    before the atomic rename, so `settings.json` (proxy password) and
    `downloads.json` (per-download cookies and auth headers) land at
    0600; a file left world-readable by an older version heals itself
    on the next save. Covered by the test
    `store::tests::written_files_are_owner_only`. The Windows half
    (DPAPI `CryptProtectData`, `dpapi:<hex>`) was already confirmed.
    `docs/PRIVACY.md` now states the Linux mode-0600 fact.

**Disable auto-update checks (upstream #49)** (resolved 2026-11-02,
hotfix H2 — verify, don't build). The setting ALREADY EXISTS in
v1.1.1: `Settings.update_check` (`crates/engine/src/config.rs:401`,
default `true` at :519), checkbox "Check for updates automatically"
in Options > General (`crates/app/ui/dialogs.slint:2307`, wired in
`crates/app/src/options.rs:86` / `:205`), and the spawn loop honors
it (`crates/app/src/update.rs`: off → sleep 300 s and re-check, so no
automatic round runs; Help > Check for updates never consults the
flag, so a manual check still works). H2 extracted the loop's guard
into `auto_checks_enabled()` and covered it with the test
`update::tests::auto_checks_follow_the_setting` (on → automatic
rounds run; off → none). No new setting, no new UI, no new behaviour
was added.

8. **HLS BANDWIDTH with a separate audio group.** When a variant
   refers to a separate audio group, Phase 4 treats its
   AVERAGE-BANDWIDTH/BANDWIDTH tag as the video bitrate and ADDS the
   audio bitrate (sampled from the audio rendition's segments), as the
   spec orders. If the audio bitrate cannot be sampled, the tag is
   used on its own and no audio bitrate is added, rather than showing
   no size at all. This follows the spec's ADD rule literally when the
   audio bitrate is measurable; servers whose BANDWIDTH tag already
   includes the audio will show a slightly high estimate in that case.
   Default: keep. Resolution needs: none unless a server is found
   where the double count is visibly wrong.
   Status (checked 2026-10-31, Phase 1): still open. The upstream
   1.0.8 HLS byte-range merge does not touch bitrate counting, and
   whether a packager's BANDWIDTH already includes the audio cannot be
   told from the playlist alone — it needs an empirical comparison
   (estimated size vs actual downloaded bytes) on real streams.
   RESOLVED 2026-11-05 (P1, Q8): the probe now tells the two cases
   apart — when a variant has a separate audio group and a BANDWIDTH tag,
   the video segments are sampled first; if the video alone is clearly
   below the tag (sampled x 100 < tag x 85), the tag already includes the
   audio and nothing is added on top of it. Otherwise the sampled audio
   is added as before; when the video cannot be sampled, the old
   behaviour is the fallback. `stream_meta.rs::probe_hls`, test
   `hls_tag_that_includes_audio_is_not_doubled` in
   `crates/engine/tests/stream_probe.rs` (tag 400000, video samples to
   240000, audio_bps stays 0, estimate 500000); the existing
   `hls_master_and_media_metadata` test still passes (tag 200000,
   video samples to 240000, audio added, estimate 290000).

19. **A corrupt `downloads.json` is still overwritten on shutdown
    (Phase 1 wart, inherited from upstream 1.0.8).** `store.rs`
    copies a corrupt list to `downloads.broken.json` before returning
    an empty list (Phase 1), so the bytes survive — strictly better
    than before, when the overwrite lost them entirely. But
    `Manager::shutdown()` then saves that empty list over
    `downloads.json`, so the list still vanishes from the UI until
    the backup is restored by hand. Default: keep the upstream
    behaviour for now. Resolution needs: a guard that refuses to
    save an empty list over a file that failed to load (only when the
    load was corrupt and nothing was added since), in a later phase.
   RESOLVED 2026-11-05 (P1, item 19): `store::load_downloads_status`
   now reports whether the list was corrupt, and `Manager::new` keeps
   that flag (`Inner::load_corrupt`): `tick()` and `shutdown()` refuse
   to save an empty list over a corrupt file until a download is
   actually added (`added_since_load`, set in `add()`). Saves are also
   hardened the same way a state file should be: the temporary file is
   synced to disk before the rename, and the directory is synced after
   it (`write_tmp` + `sync_dir`). A symlink or hard link in the list's
   place counts as corrupt and is never followed — the list holds
   logins. Tests: `store::tests::corrupt_list_is_reported_and_kept_aside`.

20. **The scaler sheds a connection when the fast work is done
    (Phase 1 finding).** `Scaler::observe` reads a fall in aggregate
    throughput as "too many connections" and sheds one — but when a
    download's fast segments finish and only a slow segment is left,
    the fall means the fast work is done, not that a connection is
    useless. Shedding then strands the slow remainder on one
    connection (measured in Phase 1: the Q32 work-stealing test went
    from 13.3 s to 20.3 s when the shed was allowed to act at once).
    Phase 1 works around it by letting a shed act only at segment
    boundaries, while an explicit user-held count still drops surplus
    workers mid-segment. Default: keep the workaround. Resolution
    needs: teach the scaler to tell "fast work finished" apart from
    "too many connections" (e.g. judge per-segment speeds, not the
    aggregate), in a later adaptive-tuning phase.
   RESOLVED 2026-11-05 (P1, item 20): the scaler now judges with the
   number of segments actually being fetched (`Scaler::observe_work`,
   called from the supervisor's scale tick with the live active count):
   when fewer segments are active than the target, the aggregate fall
   is finished work, not a useless connection, so the scaler holds
   instead of shedding. `observe()` keeps the old behaviour (active =
   target) for existing callers. Test:
   `adaptive::tests::scaler_holds_when_work_finished_instead_of_shedding`.

27. **Native confirm/message dialogs are dead on systems with
   zenity 4.x (found 2026-11-02, hotfix H3 smoke).** `rfd` 0.15.4
   (no `gtk3` feature) runs its Linux message dialogs through
   `zenity --no-markup …`, but zenity 4.x (Ubuntu 24.04 noble
   ships 4.0.1) removed `--no-markup`: the call exits at once
   with "This option is not available", so every `ask()` /
   `message()` in the app — including the delete-category
   confirm — silently takes the Cancel/No path and nothing is
   ever shown. Verified in the VM: the exact rfd command line
   exits 1 immediately, while the same call without
   `--no-markup` opens and waits. This is pre-existing (not an
   H3 regression) and NOT fixed in the hotfix: the H3 smoke
   worked around it with a test-only `zenity` shim that strips
   the unsupported flag and delegates to the real zenity 4
   (see `~/workspace/ezoo/v111-hotfix/bin/zenity`, smoke-only,
   not in the repo). Default: leave as-is for this release.
   Resolution needs: the maintainer's call — either upgrade/downgrade
   the rfd backend (a newer rfd without `--no-markup`, or the
   `gtk3` feature), or ship a dependency note; real-machine
   impact depends on which zenity the user's distro ships
   (zenity 3.x works, 4.x does not).
   RESOLVED 2026-11-05 (P1, Q27): upgrading rfd does NOT fix it —
   rfd 0.17.2 passes `--no-markup` unconditionally too (verified from
   the downloaded crate). Linux dialogs now go through
   `crates/app/src/native_dialog.rs`, which calls `zenity` directly
   with the same arguments rfd used minus that one flag, so they work
   on zenity 3.x and 4.x alike; message text is escaped for Pango
   markup instead (the job `--no-markup` used to do), and
   `set_directory` is honoured (rfd's zenity backend ignored it). The
   API mirrors rfd exactly, so call sites are unchanged (`winfx.rs`
   re-exports the native types on Linux, rfd on Windows — rfd stays a
   dependency for Windows). Tests: `native_dialog::tests` (argument
   checks, a fake-`zenity` runtime test, missing-zenity behaviour).
   NOTE: zenity is not installed in this VM, so the dialogs are
   covered by the fake-zenity tests only — a real zenity 4.x run on
   the maintainer's machine is still the live proof.

28. **Tool updater downloads yt-dlp without the executable bit on
   Linux (found 2026-11-02, hotfix H4 smoke; Q49).** On startup
   `tools::spawn_updater` (`crates/app/src/main.rs:463`) fetches
   `yt-dlp_linux` into the `tools/` folder next to the binary when
   `bundled("yt-dlp")` is none (`crates/engine/src/tools.rs:256-263`),
   but the written file lands `-rw-rw----`: the first playlist /
   video fetch then fails with "could not start yt-dlp: Permission
   denied (os error 13)" until something chmods it. Verified in
   the VM: the updater downloaded a 41 MB `target/release/tools/yt-dlp`
   at 15:58 with no +x, and the fetch failed until the file was made
   executable. Pre-existing (not an H4 regression) and NOT fixed in
   the hotfix — the H4 smoke worked around it by placing an
   executable copy in `tools/` for the run (see
   `~/workspace/ezoo/v111-hotfix/smoke-h4.sh`). Default: leave
   as-is for this release. Resolution needs: the maintainer's call — set the
   executable bit after the Linux download (or after every tool
   download), in a later fix; note the standalone yt-dlp ELF also
   cannot verify TLS through a MITM egress proxy, which is a VM-only
   quirk, not a product issue.
   RESOLVED 2026-11-05 (P1, Q49): `tools::replace` now sets mode
   0755 on Unix after every tool replacement (yt-dlp and aria2 both
   go through it), and `yt_dlp()` heals the bit on every check when
   the bundled copy exists, so copies downloaded by older versions
   are fixed too. Test: `tools::tests::replace_and_find` now asserts
   the mode (Unix).

