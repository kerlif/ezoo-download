# Flatpak — v1.1.0 (Phase 8)

Ezoo Download as a Flatpak, built only in GitHub Actions CI from the
fork `kerlif/ezoo-download`. Nothing in this document claims a
Flatpak was built or run locally: `flatpak-builder` and `flatpak`
are not installed in the development environment, no Flatpak build
or run happened in Phase 8, and every CI-only claim below is marked
as such. Phase 8 status: PREPARED — AWAITING CI (see
`docs/PHASES.md`).

## 1. Identity and runtime

| Item | Value |
|---|---|
| App ID | `io.github.kerlif.EzooDownload` (spec §1 default) |
| Manifest | `packaging/flatpak/io.github.kerlif.EzooDownload.yml` |
| Desktop file | `packaging/flatpak/io.github.kerlif.EzooDownload.desktop` |
| Metainfo | `packaging/flatpak/io.github.kerlif.EzooDownload.metainfo.xml` |
| Vendored sources | `packaging/flatpak/cargo-sources.json` (§2) |
| Runtime / SDK | `org.freedesktop.Platform` / `org.freedesktop.Sdk`, version `26.08` |
| Rust toolchain | `org.freedesktop.Sdk.Extension.rust-stable` (26.08 branch ships Rust 1.98.0) |
| Command | `EzooDownload` (binary installed at `/app/bin/EzooDownload`) |
| Bundle (CI artifact) | `EzooDownload-x86_64.flatpak` |

Runtime version — how `26.08` was verified (2026-10-28, not guessed):

- The Flathub OSTree refs `runtime/org.freedesktop.Platform/x86_64/26.08`,
  `runtime/org.freedesktop.Sdk/x86_64/26.08` and
  `runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08` all
  return HTTP 200 from `https://dl.flathub.org/repo/refs/heads/...`
  (checked with `curl` the same day).
- The freedesktop-sdk GitLab repository has a `release/26.08` branch,
  and Flathub's appstream data lists `freedesktop-sdk-26.08.4` as the
  latest `org.freedesktop.Platform` release.
- The rust-stable extension's `branch/26.08` manifest
  (`org.freedesktop.Sdk.Extension.rust-stable.json`) declares runtime
  `org.freedesktop.Sdk 26.08` and ships Rust 1.98.0 (channel
  2026-08-20). The local toolchain is 1.99.1 and the tree has no
  `rust-version` requirement (edition 2021), so 1.98.0 is expected
  to build it — CI is the proof, not this document.
- `24.08` was rejected: it is end-of-life (Flathub reports it "is no
  longer receiving fixes and security updates"). `25.08` also
  resolves but is older than `26.08`; there was no reason to pin an
  older runtime.

Authorship: the app ID and the homepage/bugtracker URLs point at the
fork (`github.com/kerlif/ezoo-download`), because that is where this
build's source lives. The developer credit stays with the author:
the metainfo `<developer>` block is verbatim from the existing
`.deb` metainfo (`Ahmed Ezzat`, id `io.github.ahmedezo686`), the
description names Ahmed Ezzat / Ezoo Tech and links the upstream
repository, and no authorship is rebranded. License: MIT, Copyright
2026 Ahmed Ezzat (see `LICENSE-DEB`); all notices are kept.

## 2. Offline builds — vendored Cargo sources

The manifest builds fully offline. The sources list was generated
with `flatpak-cargo-generator` from `flatpak-builder-tools`:

- Generator script: fetched from the upstream repository
  (`flatpak/flatpak-builder-tools`, `cargo/flatpak-cargo-generator.py`
  on `master`) on 2026-10-28. Latest commit touching that file, per
  the GitHub API the same day: `41c20aa10819cdb2a4f3ca171758a96d1955c018`
  ("cargo: Recover from stale git cache in fetch_git_repo").
- Generator dependencies (`aiohttp`, `tomlkit`, `PyYAML`) were
  installed into a throwaway virtualenv (`/tmp/genvenv`, since
  deleted); no repository file was changed to run it.
- Command (run from the repository root, against the Phase 7
  `Cargo.lock`, which already contains the Phase 7 reqwest edge
  change):
  ```
  python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
  ```
- Verification performed on the generated file (same day, before
  committing it):
  - 709 `archive` entries (one per registry package in the lock) +
    710 `inline` entries (one `.cargo-checksum.json` per crate + the
    cargo `config`), 1,419 entries total, 395,176 bytes.
  - Completeness: every one of the 709 `registry`-sourced packages
    in `Cargo.lock` (711 packages total: 709 registry + 2 workspace
    members, 0 git sources) has exactly one archive entry, matched
    by `cargo/vendor/<name>-<version>`.
  - Hashes: all 709 archive `sha256` values equal the `checksum`
    values in `Cargo.lock` (0 missing, 0 mismatched), spot-checked
    individually for `slint` 1.18.1, `ring` 0.17.14 and `anyhow`
    1.0.104. The generator reads the lock's own checksums, so this
    check proves the list matches the lock rather than re-downloading
    anything; flatpak-builder re-verifies each archive hash at build
    time in CI.
- Regeneration rule: whenever `Cargo.lock` changes, re-run the
  generator and commit the new `cargo-sources.json` with it. A stale
  list fails the CI build (hash/version mismatch), it does not
  silently build old dependencies.

Build dependencies, verified from the code (not assumed): the
manifest adds NO extra modules. Slint's Linux backends need the
fontconfig, freetype, xkbcommon, X11 and xcb development files, all
of which the Freedesktop SDK ships; TLS is rustls with the native
certificate store (no OpenSSL headers — see `crates/engine/Cargo.toml`);
the UI fonts are embedded in the binary (Phase 3); Slint's
`build.rs` compiles `ui/app.slint` exactly as the `.deb`/AppImage
builds do. If the SDK ever lacks one of those `-dev` files, CI is
where it shows up — see §10.

## 3. Sandbox permissions — what, and why

| Permission | Reason (evidence) |
|---|---|
| `--device=dri` | OpenGL rendering: the Linux build enables Slint's `renderer-femtovg` (see `crates/app/Cargo.toml`, Linux target deps). |
| `--share=ipc` | X11 MIT-SHM for the software-renderer fallback. The Linux build also enables `renderer-software`; its X11 backend is softbuffer 0.4.8, whose `backends/x11.rs` uses XCB SHM (`shm::PutImage`, POSIX SHM segments) when available. Without a shared IPC namespace the SHM path cannot work under `--socket=fallback-x11`; softbuffer falls back to non-SHM `PutImage`, so this is the fast path, not the only path — but it is the reason the permission is granted, and it is the only reason. |
| `--share=network` | Downloads, and the browser bridge (§7). |
| `--socket=wayland`, `--socket=fallback-x11` | Wayland first (the Phase 6 dialog fix targets Wayland compositors); X11 when Wayland is absent. |
| `--filesystem=xdg-download` | The default download folder. `Settings::default` (`crates/engine/src/config.rs`) sets `download_dir` to the XDG Downloads directory (`directories::UserDirs::download_dir()`), and downloads are written there with no file chooser in the normal flow — a chooser only appears when the user picks a different folder. Without this permission the app's default behaviour would write into the sandbox-private home, where the user cannot find the files. This is the ONLY blanket filesystem grant: no `home`, no `host`. Folders the user picks through a chooser get document-portal grants for that folder, as on any Flatpak. |
| `--talk-name=org.freedesktop.portal.*` | FileChooser portal: every file/folder dialog in the app goes through `rfd::FileDialog` (`main.rs`, `options.rs`, `grab.rs`, `playlist.rs`, `scheduler.rs`, `torrent_ui.rs`, `extra.rs`). |
| `--talk-name=org.kde.StatusNotifierWatcher` | The tray icon: `tray-icon` 0.26 with the `ksni` feature talks StatusNotifierItem over D-Bus (`crates/app/src/tray.rs`). Without this name the icon cannot register on KDE/GNOME/XFCE. |

Deliberately NOT granted:

- No `--filesystem=home` / `host`: nothing in the app needs the
  whole home directory. Settings, queue and tools live in the app
  data directory, which Flatpak redirects into the sandbox
  (`~/.var/app/io.github.kerlif.EzooDownload/`); downloads use
  `xdg-download` plus portal grants.
- No blanket `--socket=session-bus`: the two D-Bus names above are
  the only ones the app needs.
- No `--socket=pulseaudio`: event sounds spawn `paplay`/`aplay`
  (`crates/app/src/sound.rs`) and ignore failure — the sandbox has
  neither binary, so sounds are silently absent in the Flatpak.
  That is the designed degradation (fire-and-forget, no error), not
  a defect to work around with a socket the app cannot use.

## 4. Desktop file and metainfo

- The desktop file is field-identical to the `.deb`'s
  `packaging/linux/ezoo-download.desktop` (same Name/GenericName/
  Comment incl. Arabic, `Exec=EzooDownload`, `Icon=ezoo-download`,
  categories, keywords, `StartupWMClass`), renamed to the app ID.
  Icons install from the same `packaging/linux/hicolor/` tree the
  `.deb` uses (16–512 px PNGs, scalable SVG, symbolic SVG).
- The metainfo file keeps the `.deb` metainfo's description,
  Arabic summary, ratings, `provides` binary and the 1.0.7 release
  entry verbatim, and changes only: `<id>`, `<launchable>`
  (now `io.github.kerlif.EzooDownload.desktop`), homepage/bugtracker
  URLs (fork), a fork-attribution paragraph in the description, a
  1.1.0 release entry, and screenshots.
- Releases listed: exactly the releases that exist — 1.0.7
  (2026-10-26, as in the existing metainfo) and 1.1.0. No earlier
  releases are invented. The 1.1.0 entry is dated 2026-10-28, the
  day Phase 8 was prepared; if the tag lands on a later date,
  Phase 10 (or the release step) must correct the date before the
  Flatpak is published — the date is metadata, not a build input.
- Screenshots: the three existing `docs/screenshots/*.png`
  (962×502 each), served from GitHub Pages. Each URL was verified
  with `curl` on 2026-10-28 (HTTP 200, correct byte sizes:
  77,613 / 77,559 / 57,443, `image/png`):
  `https://kerlif.github.io/ezoo-download/screenshots/01-main-ar.png`,
  `.../02-progress-ar.png`, `.../03-add-dialog-ar.png`.
  Pages URLs were chosen over `raw.githubusercontent.com/.../v110-upgrade/...`
  paths (which also resolve today) because they do not name a
  branch and survive the merge to `main`. Phase 10 refreshes the
  screenshots; until then these are the v1.0.7-era shots, which is
  stated here rather than hidden.
- Validation: `desktop-file-validate` and `appstreamcli` /
  `appstream-util` are NOT installed in this environment, and
  neither validator ran. What ran instead: (1) the metainfo XML
  parses as well-formed XML (Python `xml.etree`, no errors);
  (2) the manifest, desktop and metainfo YAML/XML parse cleanly
  (Python `yaml.safe_load` for the manifest and staged workflows);
  (3) a manual field-by-field check against the freedesktop
  Desktop Entry and AppStream specifications — required desktop
  keys (`Type`, `Name`, `Exec`, `Icon`) present, `Categories`
  ends in `;`, metainfo has `id`, `launchable` matching the
  desktop file name, `name`, `summary`, `developer`, both licence
  tags, `description`, `screenshots` with `type="default"` on the
  first, `releases` with version/date/type, `content_rating` and
  `provides`. CI (and `flatpak-builder --run` / `appstreamcli
  validate` in a follow-up) is where validator-cleanliness is
  proven — see §10.

## 5. ffmpeg in the Flatpak

The spec (§2 Phase 8 item 4) says to use the
`org.freedesktop.Platform.ffmpeg-full` extension. That premise is
out of date for runtime 26.08, and the manifest deliberately does
NOT follow it literally:

- Evidence: the Freedesktop SDK 25.08 release notes state that for
  25.08+ (GNOME 49+, KDE 5.15-25.08+/6.10+) manifests that
  previously included the `ffmpeg-full` extension in
  `add-extensions`/`inherit-extensions` should REMOVE that stanza —
  the `codecs-extra` extension is installed automatically with the
  runtime and no other action is needed. Consistent with that, the
  Flathub refs `runtime/org.freedesktop.Platform.ffmpeg-full/x86_64/26.08`
  and `.../25.08` return HTTP 404, while
  `runtime/org.freedesktop.Platform.codecs-extra/x86_64/26.08-extra`
  returns HTTP 200 (all checked 2026-10-28). Adding an
  `ffmpeg-full` stanza for 26.08 would reference an extension that
  does not exist on that branch.
- What the Flatpak therefore gets: the runtime's codec libraries
  via the automatically installed `codecs-extra` extension. The
  other Linux packages keep their bundled ffmpeg exactly as before
  (no change to `packaging/linux/build-linux.sh` or
  `fetch-ffmpeg.sh`).
- Licence: the runtime's FFmpeg/codec libraries are the
  Freedesktop SDK's FFmpeg build (LGPL/GPL per the components
  enabled in that build); no ffmpeg binary is bundled by this
  manifest, so no new licence text ships in the Flatpak beyond
  `THIRD-PARTY-NOTICES.txt`, which now carries a Flatpak paragraph
  saying exactly that. The bundled johnvansickle build in the
  .deb/.rpm/AppImage stays GPL-3, as documented in
  `THIRD-PARTY-NOTICES.txt` (unchanged wording).
- UNVERIFIED — Open Question 13: the app's muxer needs an `ffmpeg`
  *binary* (`mux.rs::find_tool` probes the bundled locations, then
  `ffmpeg` on `PATH`). Whether the 26.08 runtime/codecs-extra
  puts an `ffmpeg` executable on the sandbox `PATH` could not be
  established from documentation alone, and no Flatpak run was
  possible here. If it does not, video+audio merging in the
  Flatpak fails with the existing graceful "ffmpeg is missing"
  error (no crash, no silent corruption — `mux.rs::merge` bails
  with the `MISSING` message) and the follow-up is to add a
  static-ffmpeg module to the manifest. CI/a real Flatpak run
  must check `flatpak run --command=sh ... command -v ffmpeg`
  and one real merge before the Flatpak is announced as
  fully working.

## 6. yt-dlp in the Flatpak (Open Question 1 — resolved)

Decision: option (b) — do NOT ship a fixed yt-dlp inside the
Flatpak — as implemented by the app's existing automatic fetch,
not by a manual user install.

Rationale, from the code (`crates/engine/src/tools.rs`,
`crates/engine/src/ytdlp.rs`, `crates/engine/src/mux.rs`):

- The app already fetches yt-dlp itself: 20 seconds after
  startup (then every 6 hours) `tools::update_all` runs
  `yt_dlp()`, which, when no bundled copy exists, downloads the
  current `yt-dlp_linux` standalone build from the yt-dlp
  releases page and checks it against that release's published
  `SHA2-256SUMS` before running it; when a copy exists it runs
  `yt-dlp -U` at most once a day. The download goes to the
  first writable tool directory — in the Flatpak that is the
  sandbox data directory (`ProjectDirs::data_dir()/tools`),
  which is writable. It needs only network access, which the
  sandbox has. No user action, no manual install, and no new
  code was needed.
- Option (a) (a pinned module updated with each release) was
  rejected: a module-installed copy under `/app` is read-only,
  so the app's own `-U` updater could not update it; it would
  duplicate the hash-checked fetch the app already performs;
  and it would freeze the extractor at release time while video
  sites change constantly — the updater exists precisely
  because yt-dlp must stay current.
- Graceful absence (verified, not assumed): every yt-dlp entry
  point resolves the tool through `find_tool("yt-dlp")` and
  returns a plain error when it is absent
  (`ytdlp.rs`: "yt-dlp.exe is missing. Ezoo Download gets it by
  itself in a minute (internet needed), then try again" /
  "yt-dlp.exe was not found next to EzooDownload.exe"); the
  video-choices flow only runs on user action, probing never
  runs yt-dlp, and no download behaviour changes. The only
  Flatpak-specific wrinkle is the wording "next to
  EzooDownload.exe" in two of those messages, which is
  pre-existing text on all platforms and was left alone.
- Privacy: the fetch is the already-inventoried tools-updater
  traffic in `docs/PRIVACY.md` (yt-dlp release + sums, hash-checked,
  browser user-agent) — no new destination, no new request, and
  `docs/PRIVACY.md` now says so for the Flatpak explicitly.

Open Question 1 is resolved with this decision; the resolution
text lives in `docs/OPEN-QUESTIONS.md`.

## 7. Browser-extension bridge in the Flatpak

The bridge is the same token-authenticated localhost mechanism as
on every platform: `tiny_http` on `127.0.0.1:9614`, `POST /add`
with the `X-SwiftGet` token header and `Host` validation against
DNS rebinding (`crates/app/src/api.rs`). Nothing about it changes
for the Flatpak, and no code changed in Phase 8.

Sandbox expectation (DOCUMENTED, NOT TESTED — no Flatpak run was
possible): `--share=network` shares the host network namespace,
so the sandbox and the host browser see the same loopback
interface, and the host browser extension's POST to
`127.0.0.1:9614` reaches the sandboxed app exactly as it reaches
the unsandboxed one. That is the standard Flatpak behaviour for
`--share=network` (the sandbox has no separate loopback), stated
here as the documented expectation. What the user must do:
nothing extra — install the extension from the app's Extensions
dialog as usual. If a real Flatpak run shows the bridge
unreachable, the fallback is the clipboard monitor (already the
app's other capture path). This must be confirmed in a real
Flatpak run before the Flatpak is announced; see §10.

Related Flatpak behaviours, from the code, no changes made:

- Update checker: `detect_package` checks `FLATPAK_ID` first
  (`crates/engine/src/selfupdate.rs`), so a Flatpak copy is
  notify-only with a release-page button — no download, no
  self-install. That detection works unchanged in the sandbox
  (Flatpak sets `FLATPAK_ID` to the app ID).
- Autostart: `autostart.rs` writes
  `~/.config/autostart/ezoo-download.desktop`; inside the
  sandbox that path is the sandbox-private config directory, so
  the setting has no effect on host login. The user can autostart
  the Flatpak manually with `flatpak run
  io.github.kerlif.EzooDownload`. Left as a documented limitation;
  fixing it (e.g. a portal-based autostart) is not Phase 8 work.
- Extension files: the app finds them at
  `<exe>/../share/ezoo-download/` (`browser.rs::extension_dir`),
  i.e. `/app/share/ezoo-download/` — the manifest installs them
  there, mirroring the `.deb` layout.

## 8. Signatures and the update manifest (no minisign)

Reconciliation with the spec's "minisign signature" wording
(spec §2 Phase 8 item 7): the independent signature IS the
Phase 7 Ed25519/`ring` mechanism — the allowed equivalent per
Open Question 3. There is NO second signature scheme and NO
minisign dependency anywhere in Phase 8:

- `version.json` is signed in CI with the fork Ed25519 keypair
  (public key in `selfupdate.rs`, private key in the
  `UPDATE_SIGNING_KEY` secret; exact step in
  `docs/UPDATE-CHECKER.md` §§9–10). The Windows workflow now
  FAILS a release build when the key is missing (Phase 7's
  requirement, wired in the escalated workflow — see §9).
- The Flatpak bundle itself is not signed with a separate key:
  it is covered by the release's `SHA256SUMS.txt` (generated
  from `dist/*` in the Linux job, which now includes the
  `.flatpak`), and the signed manifest gates the update offer
  as a whole (Open Question 12's limit applies unchanged: the
  checksum file itself is unsigned, the signed manifest gates
  the offer).
- Flatpak copies never consume the signed manifest for
  self-install (notify-only, §7), so no Flatpak-specific signing
  step exists.

## 9. CI — what the escalated workflows do

The workflow files are ESCALATED ONLY (the GitHub App cannot push
`.github/workflows/*`). Full contents are staged at
`~/workspace/ezoo/workflow-escalations/phase8/` — see its
`README-ESCALATION.md` for target paths, base commit, commit
message and order of operations. Summary:

Linux workflow (`.github/workflows/linux-release.yml`):

- The Flatpak steps run FIRST in the `linux` job (before any
  cargo/packaging step creates `target/` or `dist/`), because
  the manifest's `type: dir, path: ../..` source copies the
  tree as-is: add the Flathub remote, build with
  `flatpak-builder --force-clean --install-deps-from=flathub`,
  bundle with `flatpak build-bundle --runtime-repo=<Flathub>`
  to `dist/EzooDownload-x86_64.flatpak`.
- The existing `sha256sum * > SHA256SUMS.txt` step then covers
  the `.flatpak` together with the `.deb`, AppImage, `.rpm`
  and extensions, and the existing tag-release step uploads
  `dist/*` — so the `.flatpak` reaches the release through the
  same path as every other Linux artifact, with no second
  release-creation step.
- Everything else in the Linux workflow is unchanged: triggers
  (push to `main`/`linux-support`, tags `v*`,
  `workflow_dispatch`; docs/README-only pushes ignored), the
  tag-must-match-`Cargo.toml` check, the extension tests, the
  engine-tests-warn-only step, the Firefox signing step, and
  `build.yml` is untouched (its destructive `[release]`
  publish step is still there, and the string `[release]`
  appears in no commit message in this upgrade).
- `.deb` dependency fix (pushable files, in the branch, not the
  escalation): `packaging/linux/debian/control` now also
  `Depends` on `libxkbcommon-x11-0` and
  `packaging/linux/rpm/ezoo-download.spec` now also `Requires`
  `libxkbcommon-x11` — `docs/FONTS.md` flagged the missing
  dependency for Phase 8: winit on X11 needs
  `libxkbcommon-x11.so.0`, and this VM's GUI runs needed the
  `/tmp/xkb` workaround for exactly that library. (The `.deb`
  already depended on `libxkbcommon0`; the `-x11` split library
  is a separate package.)

Windows workflow (`.github/workflows/windows-release.yml`):

- What was ALREADY there (read before editing): the installer
  build, engine tests (warn-only), extension tests,
  `cargo-audit`, the `version.json` signing block, the
  wait-for-release upload loop. What was MISSING and is now
  added: (1) `cargo clippy --all-targets --locked -- -D warnings`
  on Windows — the Linux clippy gate cannot see Windows-only
  code (`winfx.rs`, Windows-only paths in `update.rs`/`fonts.rs`);
  (2) a full-workspace `cargo test --locked` step — only the
  engine crate was tested on Windows, so the app crate's tests
  (fonts, i18n) and the Windows-only paths never ran in CI.
  Both new steps fail the job on failure (unlike the
  warn-only engine step, which is kept as-is).
- Signing hard error, verbatim from `docs/UPDATE-CHECKER.md`
  §10: when `IS_RELEASE` is true and `UPDATE_SIGNING_KEY` is
  empty, the Package step now `throw`s instead of warning, so
  a release can never ship an unsigned `version.json`. The
  signing commands themselves are unchanged.
- Also included (flagged so the coordinator can strip it):
  the three Phase 3 `Copy-Item` lines staging the OFL licence
  texts into the installer stage, which `installer/ezoo.iss`
  already lists — Phase 3 recorded them as still needing a
  browser-side workflow edit.
- `SHA256SUMS-windows.txt` generation is unchanged and still
  covers the Setup.exe + `version.json` only, as the checker
  expects.

## 10. What CI / a real run must still prove

Phase 8 is PREPARED — AWAITING CI. None of the following was
provable in this environment, and none of it is claimed:

1. `flatpak-builder` accepts the manifest and the vendored
   sources build offline (Rust 1.98.0, SDK `-dev` files, no
   network in the build sandbox).
2. The bundle installs and the app starts under Wayland and
   under X11 (fallback), with the tray icon, portal file
   chooser, and downloads landing in the real `~/Downloads`.
3. An `ffmpeg` binary is on the sandbox `PATH` (§5) and one
   real video+audio merge succeeds — or the static-ffmpeg
   follow-up is needed (Open Question 13).
4. The browser bridge works from a host browser (§7), and
   yt-dlp is fetched into the sandbox data directory on first
   use (§6).
5. `appstreamcli validate` / `desktop-file-validate` pass on
   the metainfo/desktop files (§4).
6. The Windows clippy + full-test steps pass on
   `windows-latest` (first time either has run there).
7. The tag build produces `.deb`, `.rpm`, AppImage,
   `.flatpak`, the Windows installer, checksums and a signed
   `version.json` — the Phase 8 gate, runnable only after the
   parent creates the tag and CI runs.

Local build (for a machine WITH flatpak-builder installed):

```
flatpak remote-add --user --if-not-exists flathub \
    https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak-builder --force-clean --install-deps-from=flathub \
    --repo=flatpak-repo flatpak-build \
    packaging/flatpak/io.github.kerlif.EzooDownload.yml
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    flatpak-repo EzooDownload-x86_64.flatpak io.github.kerlif.EzooDownload
flatpak install --user EzooDownload-x86_64.flatpak
flatpak run io.github.kerlif.EzooDownload
```

Regenerate the sources list after any `Cargo.lock` change (§2)
before rebuilding.
