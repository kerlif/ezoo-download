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
   RESOLVED 2026-10-31 (Q27): Tensi tested the fix himself on real
   KDE Plasma and the problem is solved — the KDE Plasma dialog bug
   is FIXED in v1.1.0 and VERIFIED on real hardware by Tensi himself.
   (Earlier wording calling this unverified was corrected by
   Tensi 2026-10-31 20:10; see Q27 in the Decided section below.)
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
    SHA-256 in `docs/README.md`) could not be pushed through the
    GitHub connector (UTF-8 only) and are escalated to the parent
    for browser upload, together with the six Phase 3 `.ttf`
    binaries (the pre-existing pre-tag blocker). Until they land,
    the site renders with the declared system-font fallback stack.
    Separately, Lighthouse could not run in the Phase 9
    environment (loopback navigation blocked in Chromium;
    `file://` URLs rejected by Lighthouse), so the ≥90
    accessibility + performance gate is unmeasured: re-run
    Lighthouse against the live Pages URL after the merge to
    `main`, in both languages, and record the scores in
    `docs/README.md`.
    Verification (2026-10-31, Phase 0): the binaries HAVE landed on
    remote main — GitHub API git-trees at `d6807d8` lists 6 `.ttf`,
    4 `.woff2`, 10 screenshots PNG. Lighthouse measurement still
    unmeasured.
15. **Screenshot binaries + metainfo dimensions + Windows screenshots
    (Phase 10).** The ten Phase 10 screenshots
    (`docs/screenshots/01`–`10`, manifest with SHA-256 in
    `docs/RELEASE-REPORT.md` §10) could not be pushed through the
    GitHub connector (UTF-8 only) and are escalated to the parent
    for browser upload, together with the six Phase 3 `.ttf` and
    four Phase 9 `.woff2` binaries (Open Question 14): until they
    land, the remote tree still carries the v1.0.7-era shots and
    the live site renders with system-font fallbacks. The new
    `03-add-dialog-ar.png` is 640×488 (native dialog crop) while
    the Flatpak metainfo (Phase 8 file, not edited in Phase 10)
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
    the headers of `docs/RELEASE-REPORT.md` and
    `IMPROVEMENTS_SUMMARY.md`; `THIRD-PARTY-NOTICES.txt` covers
    the third-party components. Default: redistribute only
    within those stated terms; keep every existing copyright
    notice and credit to Ahmed Ezzat (Ezoo Tech) and the upstream
    repository; do not expand redistribution beyond what the
    stated licenses allow, and do not change any license terms.
    Resolution needs: a decision whether to add a top-level
    `LICENSE` file mirroring the MIT text in `LICENSE-DEB`, or
    to confirm `LICENSE-DEB` as the canonical copy. No license
    terms were changed by the v1.1.0 upgrade either way.

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
    in the Flatpak build.

22. **ffmpeg alias mirroring.** The johnvansickle tarball URL is
    a moving alias pinned by SHA-256; a silent change fails the
    build loudly, but the alias itself can move. Default: keep
    the pin. Resolution needs: mirror the tarball to an immutable
    URL before any Flathub submission, or if the alias changes.

23. **Flathub policy on runtime fetch of yt-dlp.** The app
    downloads a hash-checked `yt-dlp_linux` binary into its
    sandbox data directory on first use and updates it with
    `yt-dlp -U` (see `docs/FLATPAK.md` §6 and §8b). That is a
    runtime download of executable code; Flathub reviewers may
    question it. Default: keep the fetch (it is the app's
    long-standing behaviour on every platform). Resolution
    needs: a Flathub policy review before any submission.

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
    opens).

26. **Linux aria2/ffmpeg missing messages are untranslated.**
    WP3 made the missing-tool messages platform-specific with a
    pure selector (`tools.rs`), but the app UI maps only the
    Windows ffmpeg wording to its translation (`main.rs` matches
    on "ffmpeg.exe is missing"); the new Linux/Flatpak messages
    show in English until strings are added. Default: show the
    correct English message (better than a translated but wrong
    one). Resolution needs: two new strings × 8 locales
    (`crates/app/lang/rs/*.json` + `i18n.rs`, needs-review
    markers per the Q6/Q10/Q11 pattern).

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
   Resolution needs: Tensi's call — either upgrade/downgrade
   the rfd backend (a newer rfd without `--no-markup`, or the
   `gtk3` feature), or ship a dependency note; real-machine
   impact depends on which zenity the user's distro ships
   (zenity 3.x works, 4.x does not).

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
   as-is for this release. Resolution needs: Tensi's call — set the
   executable bit after the Linux download (or after every tool
   download), in a later fix; note the standalone yt-dlp ELF also
   cannot verify TLS through a MITM egress proxy, which is a VM-only
   quirk, not a product issue.

## Decided for v1.1.1 (2026-10-31, Tensi)

All of Q19–Q34 decided before Phase 0. Phase mapping per
`~/workspace/ezoo/v111/PLAN-111.md` and
`~/workspace/ezoo/v111/ZAJIL-COMPARISON.md` §5.

- **Q19 — PR target: fork-internal PR (`v111-upgrade` ← `main`), as
  PR #2 for v1.1.0.** Work happens on a separate internal branch,
  then after verification, automation and full readiness it is pushed
  to main as usual via that internal PR (merge commit, without the
  bracketed release-trigger word in any message). NEVER any PR to upstream. (Correction
  2026-10-31 20:10: an earlier reading of «الغي ال PR» as cancelling
  every PR was wrong — Tensi clarified it meant cancelling the
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
  FIXED in v1.1.0 and VERIFIED on real hardware by Tensi himself
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
  `SHA256SUMS.txt`. Decision (Tensi's Q28/Q33, 2026-10-31):
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
  dir. This is the one recorded exception to Tensi's no-asset-
  renames decision (2026-11-01): the pre-unification state left
  two XPI files + a last-wins `firefox-updates.json` in the
  release. What a signed 1.1.1 XPI needs: the AMO signing
  secrets (`AMO_JWT_ISSUER` / `AMO_JWT_SECRET`) already in the
  workflows, or a previously signed XPI placed in
  `extension-signed/` (the workflows reuse it); then the
  release's `firefox-updates.json` exists and the manifest's
  `update_url` can be repointed to the fork's releases.

## Decided for the 1.1.1 hotfix (2026-11-02, Tensi)

All of Q36–Q48 decided with the amended hotfix plan, approved by
Tensi 2026-11-02 14:34 («أوافق على الخطة المعدلة — ابدأ الكود»).
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
  at the hotfix commit wherever it lives. If Tensi prefers one
  combined PR, the phases still run in the same order; only the PR
  step merges.
- **Q48 — Mid-run 24h checks: the notice is shown once at that
  point too (still at most once per run for that version) — it does
  not wait for the next launch.** Decided 2026-11-02 14:34 with the
  amended-plan approval (Tensi tapped the plan-approval option, not
  launch-only). Implemented in H2 via the `LATER` flag; the
  `handle_available` gate does not distinguish launch checks from
  mid-run checks. Tests:
  `later_suppresses_the_notice_until_a_newer_version`,
  `handle_available_is_done_when_the_notice_is_suppressed`.

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
    `yt-dlp -U` daily thereafter (see `docs/FLATPAK.md` §6 and
    `docs/PRIVACY.md`). A pinned module (option (a)) was
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
