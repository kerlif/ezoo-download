# Flatpak build report — v1.1.1 Phase 8 (build step)

Date: 2026-11-01. Branch `v111-upgrade`, starting HEAD `c62f754` (after Phase 7).
Final HEAD: `01bba93` (c9, this report).
Review: `~/workspace/ezoo/v111/FLATPAK-REVIEW.md` (findings F1–F17).
Approval: `docs/PHASES.md` — "Phase 8 build approved — approver: Tensi, date: 2026-11-01".
Tensi's three confirmed decisions: (1) no release-asset renames this cycle;
(2) Flatpak stays "coming later" until the workflow_dispatch proof run
succeeds on a real machine; (3) if the aria2 module fails, apply Fallback B
— remove the torrent claim from the whole Flatpak package.

Labels: DONE = verified by a recorded check. BLOCKED = a stop condition
or a failed verification; the item is not claimed. NOT VERIFIED = could
not be checked in this environment; the owner step or CI must prove it.

Skills applied: `full-development-instructions` (`instructions.md`
rulebook + `ethical-software-reconstruction` reference). Clean-room kept:
no code copied; Ahmed Ezzat's credits untouched. Nothing was pushed,
tagged, released, or PR'd in this change set (local commits only).

## WP table

| WP | Item | Label | Commit |
|----|------|-------|--------|
| WP0 | Baseline (fmt/clippy/test/validators) | DONE | — (baseline, no commit) |
| WP1 | Bridge security: foreign origin gets no CORS and cannot read /config; every POST needs token + extension origin | DONE | `5e967da` |
| WP2 | aria2 invocation safety: RPC locked to loopback + random per-run secret; argv unit test; socket inventory | DONE | `d0e559f` |
| WP3 | Platform-correct aria2/ffmpeg messages (pure function, all branches tested) | DONE | `ae0674b` |
| WP4a | Static ffmpeg module (hash-verified) | DONE | `e08fdd9` |
| WP4b | aria2 1.37.0 module from source (hash-verified; build proven only by WP10) | DONE | `e08fdd9` |
| WP4c | Other manifest changes (rust 1.99.1 comment, runtime kept 26.08) | DONE | `e08fdd9` |
| WP5 | Desktop file + metainfo (validate, torrent bullet per WP4b, screenshots exist) | DONE | `075dfbe` |
| WP6 | CI workflow (live file only; --user fix, guards, appstreamcli step) | DONE | `c7b302d` |
| WP7 | Site corrections (privacy = PRIVACY.md, Flatpak "coming later", links = published assets) | DONE | `ca8256b` |
| WP8 | Documentation, notices, README (Q13 resolved, new open questions Q21–Q26) | DONE | `3875ae6` |
| WP9 | Verification (gates, hashes, link/asset/endpoint gates) | DONE | c9 (this report) |
| WP10 | Owner steps (push + workflow_dispatch proof run; NOT performed here) | NOT VERIFIED | — |

No stop condition was hit. No WP is BLOCKED.

## WP0 — Baseline (read-only)

- Git HEAD: `c62f754fb7e2e5d008d3d04ec12daa9b2c2c4bad`, branch `v111-upgrade`.
  Working tree at start: only `docs/PHASES.md` modified (the approval
  record, +4 lines, uncommitted); everything else clean.
- Toolchain: rustc 1.99.1, cargo 1.99.1, desktop-file-validate 0.27,
  python3 3.12.3. xmllint NOT installed (XML checked with Python
  `xml.etree`); appstreamcli NOT installed; flatpak / flatpak-builder
  NOT installed.
- `cargo fmt --check`: clean (exit 0).
- `cargo clippy --all-targets -- -D warnings`: clean (exit 0).
  Log: `~/workspace/ezoo/v111/logs/phase8-baseline-clippy.log`.
- `cargo test`: **189 passed / 0 failed / 1 ignored** (exit 0) — matches
  the Phase 7 baseline exactly.
  Log: `~/workspace/ezoo/v111/logs/phase8-baseline-test.log`.
- `desktop-file-validate`: exit 0 on both desktop files
  (`packaging/flatpak/io.github.kerlif.EzooDownload.desktop`,
  `packaging/linux/io.github.kerlif.EzooDownload.desktop`).
- XML well-formedness (Python `xml.etree`): both metainfo files parse.
- YAML parse (Python `yaml.safe_load`): Flatpak manifest parses
  (id `io.github.kerlif.EzooDownload`, 1 module) and
  `.github/workflows/linux-release.yml` parses (jobs: `linux`).

## WP1 — Local bridge security (code)

Read `crates/app/src/api.rs` and verified the live behaviour with
7 new live-server tests (real `tiny_http` server on an ephemeral
port; raw `TcpStream` requests). Results:

- The bridge NEVER emits any `Access-Control-Allow-Origin` header
  (grep: 0 hits in `crates/` and `extension/`), never answers
  OPTIONS preflights (403), validates Host against DNS rebinding,
  allows Origin only when absent or `chrome-extension://` /
  `moz-extension://` / `extension://`, requires `X-SwiftGet: 1`
  on every request, and requires the per-install token
  (`token_strict`) on every POST with no fallback.
- The extension fetches from its privileged background context
  (`extension/background.js`: `fetchWithTimeout`/`apiFetch` with
  `X-SwiftGet` + token; manifest `host_permissions: ['<all_urls>']`),
  so CORS does not apply to its fetches — no CORS change was needed.
- New tests (all pass): foreign origin cannot read `/config`
  (403, no CORS, no token leak); foreign origin gets no CORS on
  `/ping`; preflight never answered (403, no CORS); POST without
  token refused (403); POST with foreign origin refused even with
  token (403); POST with token + extension origin succeeds
  (200 + `ApiEvent::Add` received); `/config` open to extension
  origin and to no-Origin local clients (200, contains localToken,
  no CORS). Only code change besides the tests: `#[derive(Debug)]`
  on `ApiEvent` (needed for a test assertion).
- Commit `5e967da` ("test(api): live bridge tests — foreign origins
  get no CORS, POSTs need token"). Full suite after c1: 196 passed /
  0 failed / 1 ignored (+7 vs baseline).

## WP2 — aria2 invocation safety (code)

Read `crates/engine/src/torrent.rs`, `crates/engine/src/tools.rs`
and `crates/engine/src/mux.rs`. aria2c runs as a CLI subprocess;
its RPC listener is locked down, not disabled (the app drives it
over JSON-RPC):

- `--enable-rpc=true`, `--rpc-listen-all=false`,
  `--rpc-allow-origin-all=false`, `--rpc-max-request-size=32M`,
  `--rpc-listen-port={ephemeral}` (port picked via
  `TcpListener::bind("127.0.0.1:0")`, dropped immediately after),
  `--rpc-secret={secret}` where `random_secret()` is 16 bytes from
  ring's `SystemRandom` (CSPRNG), hex-encoded (32 chars), with a
  time+pid fallback if the RNG fails; `--stop-with-process={pid}`;
  stdin/stdout/stderr null; `raw_call` posts to
  `http://127.0.0.1:{port}/jsonrpc` with `token:{secret}` as first
  param. The secret is held in memory only (in the `Daemon`
  struct); no logging of it (grep shows no log/file writes).
- Refactor: extracted pure function `aria2_args(port, secret, dht,
  pid) -> Vec<String>` (all 22 fixed flags + port/secret/pid/
  dht-path args) used by `ensure()`; 2 unit tests
  (`aria2_rpc_is_loopback_only_with_a_secret`,
  `rpc_secret_is_random_per_run`).
- Socket inventory (all listening sockets now in `docs/PRIVACY.md`):
  (1) bridge listener 127.0.0.1:9614 (tiny_http, app crate);
  (2) aria2 JSON-RPC listener 127.0.0.1:&lt;ephemeral&gt; (aria2
  subprocess, loopback-only); (3) transient
  `TcpListener::bind("127.0.0.1:0")` only to pick the RPC port,
  dropped immediately (small TOCTOU race: the port could be taken
  before aria2 binds; aria2 then fails to start and `ensure()`
  bails "aria2c did not start" — pre-existing, out of scope);
  (4) aria2 DHT UDP (ephemeral, `--enable-dht=true`,
  `--bt-enable-lpd=true`) + outbound peer TCP connections;
  (5) outbound TCP only (reqwest: downloads, update checker,
  tools updater).
- Commit `d0e559f` ("engine(torrent): aria2 RPC argv as a tested
  pure function, loopback-only with random secret"). Full suite
  after c2: 198 passed / 0 failed / 1 ignored (+2 vs c1).

## WP3 — Platform-correct messages (code)

- Pure selectors in `crates/engine/src/tools.rs`:
  `aria2_missing_message(is_windows, is_flatpak, bundled)` and
  `ffmpeg_missing_message(is_windows, is_flatpak, bundled)`,
  plus `in_flatpak()` (FLATPAK_ID env var, same check as
  `selfupdate::detect_package`).
  - Windows: keep the existing wording verbatim (aria2 fetch
    wording; ffmpeg wording byte-identical so the UI's
    `main.rs` translation mapping keeps matching).
  - Linux, not Flatpak: install aria2c / ffmpeg with the system
    package manager.
  - Flatpak, absent: torrents are not available in this build /
    ffmpeg is missing from this Flatpak build.
  - Bundled: no message (`None`).
- Call sites: `torrent.rs::ensure()` (aria2) and `mux.rs`
  (`missing_message()` helper replacing the `MISSING` const at
  all 4 `bail!` sites). yt-dlp messages left alone: yt-dlp IS
  fetched by the app on Linux/Flatpak, so its fetch wording stays
  accurate everywhere.
- One unit test covers every branch (12 assertions). Known gap,
  recorded as open question Q26: the app UI maps only the Windows
  ffmpeg wording to its translation, so the new Linux/Flatpak
  messages show in English until 2 strings × 8 locales are added.
- Commit `ae0674b` ("engine: platform-correct aria2/ffmpeg
  missing messages (pure selector, tested)"). Full suite after
  c3: 199 passed / 0 failed / 1 ignored (+1 vs c2).

## WP4 — Flatpak manifest

Gate verified first: `docs/PHASES.md` contains "Phase 8 build
approved — approver: Tensi, date: 2026-11-01".

### WP4a — Static ffmpeg module — DONE

- Re-downloaded the johnvansickle alias tarball via
  `packaging/linux/fetch-ffmpeg.sh` into
  `~/workspace/ezoo/v111/tmp/ffmpeg-check/`: MD5 OK, SHA-256
  `abda8d77ce8309141f83ab8edf0596834087c52467f6badf376a6a2a4c87cf67`
  (matches the review value, the `fetch-ffmpeg.sh` pin, and the
  alias `.md5` file `7fa72b652e19bf84c9461e332ea1cdf3`); version
  `ffmpeg version 7.0.2-static`; GPLv3.txt present (35,147 B).
- Manifest module `ffmpeg` (buildsystem: simple): pins the alias
  URL with that SHA-256; installs to
  `${FLATPAK_DEST}/lib/ezoo-download/ffmpeg`; installs
  `ffmpeg-version.txt` (from running the binary),
  `ffmpeg-LICENSE.txt` (GPLv3 from the tarball) and
  `SOURCE-OFFER.txt` (`packaging/flatpak/ffmpeg-SOURCE-OFFER.txt`)
  to `${FLATPAK_DEST}/share/doc/ezoo-download/ffmpeg/`.
- The install path was verified against the code, not assumed:
  `tool_dirs()` (`crates/engine/src/tools.rs`) on Linux includes
  `<exe-parent>/lib/ezoo-download` and `ours_ffmpeg()` checks the
  same path — zero code changes needed.
- SOURCE-OFFER.txt gives the ffmpeg 7.0.2 source URL
  https://ffmpeg.org/releases/ffmpeg-7.0.2.tar.xz (verified HTTP
  200; downloaded copy 10,795,332 B, SHA-256
  `8646515b638a3ad303e23af6a3587734447cb8fc0a0c064ecdb8e95c4fd8b389`,
  `RELEASE` file inside reads `7.0.2`; no second published digest
  was consulted for the source tarball). Open question Q22: mirror
  the tarball to an immutable URL before Flathub submission or if
  the alias changes.

### WP4b — aria2 module — DONE (definition; build proven only by WP10)

- Source: aria2 1.37.0 tarball from the official GitHub release.
  SHA-256 `60a420ad7085eb616cb6e2bdf0a7206d68ff3d37fb5a956dc44242eb2f79b66b`
  (downloaded file 1,705,256 B — size matches the release asset
  exactly; the release body publishes no SHA-256). Cross-checked
  against a second source: the Gentoo Manifest lists the same
  file (1,705,256 B) with BLAKE2B and SHA-512 hashes that both
  match the downloaded file exactly. (Debian `.dsc` cross-check
  failed: 404; Arch PKGBUILD fetch returned an empty 14-byte
  file — not used.)
- Manifest module `aria2` (buildsystem: autotools): builds from
  source, installs to `${FLATPAK_DEST}/bin/aria2c`; configure
  flags `--disable-nls --disable-metalink --disable-websocket
  --without-gnutls --without-libnettle --without-sqlite3
  --without-libxml2 --without-libexpat --without-libcares
  --without-libssh2` (TLS stays OpenSSL, zlib stays, BitTorrent
  stays on; libaria2 stays off by default). The flags were
  verified with a local `./configure` run (exit 0; summary:
  OpenSSL yes, Zlib yes, Bittorrent yes, Metalink/WebSocket/
  XML-RPC no). The app only uses aria2 for torrents/magnets
  (RPC methods addUri, tellStatus, forceRemove,
  removeDownloadResult, getFiles), so the disabled features are
  all unused by the app. A post-install step greps the built
  binary's `--version` output for `BitTorrent` and `HTTPS`, so a
  misconfigured build fails loudly. aria2's licence notice
  (GPL-2.0-or-later, COPYING) ships as `aria2-COPYING.txt` in
  the Flatpak doc directory and in `THIRD-PARTY-NOTICES.txt`.
- **Build proof: NOT VERIFIED locally** — no flatpak-builder in
  this environment. If the module does not build in the WP10
  proof run, Fallback B applies (open question Q21).

### WP4c — Other manifest changes — DONE

- Runtime kept at `org.freedesktop.Platform` 26.08. No
  `ffmpeg-full` stanza (retired; refs 404). No yt-dlp module.
- finish-args unchanged (8 args, no new permissions).
- rust-stable comment updated to 1.99.1, citing
  freedesktop-sdk 26.08.4 (2026-10-26) per the review.
- Build command kept: `cargo --offline build --release --locked`.
- `cargo-sources.json` NOT regenerated: the Cargo.lock registry
  package set is unchanged — verified in WP9 (709/709, exact
  set match, spot-check hashes for slint/ring/anyhow match).
- Commit `e08fdd9` ("flatpak: static ffmpeg module and aria2
  1.37.0 module (WP4a/WP4b)"). Full suite after c4: 199 passed /
  0 failed / 1 ignored.

## WP5 — Desktop file and metainfo

- `desktop-file-validate` exits 0 on both desktop files.
- Metainfo: the 1.1.0 release entry is kept (2026-10-29, the tag
  date); NO 1.1.1 entry (WP10 only). Developer (Ahmed Ezzat,
  id `io.github.ahmedezo686`), `metadata_license` FSFAP,
  `project_license` MIT (matches `LICENSE-DEB`), `content_rating`
  oars-1.1, `provides` binary — all present.
- Torrent bullet kept ("Torrents and magnets (aria2)") — the
  aria2 module is defined, so per R2 the bullet stays, conditional
  on the WP10 proof run (recorded in `docs/FLATPAK.md` §5b and
  here; Fallback B if the module fails).
- Screenshots: only the three existing files (01/02/03,
  962×502), verified present in `docs/screenshots/`.
- Description corrected: the Flatpak metainfo now says the
  Flatpak "bundles ffmpeg for merging video and audio" (was:
  "uses the runtime's codec libraries" — false after WP4a).
- False claim removed from BOTH metainfo files: "FTP via aria2"
  — verified in the code that nothing routes `ftp://` to aria2
  (`is_torrent_source` matches only magnets and local `.torrent`
  files; the ef2 parser accepts `ftp://` lines but they go to
  the HTTP path and fail). The same correction was applied to
  the `.deb` metainfo in the same commit (one coherent change
  set); no other file mentions FTP.
- `appstreamcli validate`: NOT VERIFIED — appstreamcli is not
  installed in this environment. It was added as a blocking step
  to the Linux CI job in WP6.
- Commit `075dfbe` ("flatpak: metainfo bundles-ffmpeg wording,
  drop unverified FTP claim (both metainfo files)").

## WP6 — CI workflow (live file only)

File: `.github/workflows/linux-release.yml`, edited directly as a
LOCAL commit (c6). Pushing it needs the browser route — the
GitHub connector cannot push `.github/workflows/*` (403), exactly
as in previous phases. No push was attempted. The staged
escalation file was NOT re-applied (finding F4).

- Flatpak step: `sudo flatpak remote-add --system` line DELETED;
  `flatpak-builder` now runs with `--user`
  (`--force-clean --user --install-deps-from=flathub`); the
  `flatpak remote-add --user` line is kept; `continue-on-error:
  true` kept ONLY on the Flatpak step, only until WP10 step 2
  succeeds.
- `appstream` added to the apt install list; a new blocking
  `appstreamcli validate --no-net` step on the metainfo runs
  right after the Flatpak build.
- Publishing guards verified: release creation runs only when
  `github.ref_type == 'tag'` (the trigger accepts only `v*` tags,
  so this is the `refs/tags/v*` guard); the Firefox signing step
  has the same guard; `upload-artifact` has no condition, so
  `workflow_dispatch` runs upload the bundle as a workflow
  artifact and never create a release.
- Release notes: the Flatpak line is added only when
  `dist/*.flatpak` exists (conditional step).
- Ordering verified: the Flatpak step is step 3 of 17, before any
  step that writes `target/` (rust-toolchain/cache/cargo build
  are steps 5–7); the `.flatpak` lands in `dist/` before the
  Checksums step.
- Header comment updated (static ffmpeg + aria2 modules, no
  "runtime codecs-extra" claim). YAML validated (Python
  `yaml.safe_load`). No new secrets, no new permissions
  (`permissions: contents: write` unchanged). Windows job
  untouched (this file has only the `linux` job; `build.yml`
  untouched).
- Commit `c7b302d` ("ci(linux): Flatpak --user build,
  appstreamcli validation, conditional Flatpak release notes").

## WP7 — Site (docs/)

- Privacy section (`privacy.2`, ar + en, in `docs/index.html`
  and `docs/script.js`): the aria2-fetch claim is gone — it now
  says aria2 is fetched on Windows only (on Linux install it
  with the system package manager; in the Flatpak build it comes
  bundled). The endpoint list otherwise mirrors `docs/PRIVACY.md`
  exactly: update checker, tools updater, downloads, capture
  probing (incl. "no third-party metadata, thumbnail, or search
  service is queried" — verified in `PRIVACY.md` and in the
  code, which probes only the link's own server), per-host
  learning (fields + 30-day expiry verified in
  `crates/engine/src/adaptive.rs`: `learning_expiry_days: 30`,
  `expired()` with `num_days() > expiry_days`, unit test with
  the 30-day edge), bridge, yt-dlp.
- `faq.7a` corrected (was stale after Phase 6): the quality
  selector now downloads the chosen quality (HLS by variant
  playlist URL, DASH by quality key); "not yet / planned work"
  is gone in both languages.
- `faq.6a` completed per R6: AppImage updates in-app after
  confirmation; .deb/.rpm/Flatpak notify only.
- Flatpak card stays "coming later" (R1); no Flatpak install
  commands anywhere on the site. Version text stays 1.1.0
  (26 mentions in `index.html`, 0 of 1.1.1). Download links use
  the exact asset names of the last published release v1.1.0,
  verified live against the GitHub API (all 7 binaries present:
  Setup.exe, .deb, AppImage, .rpm, .crx, .zip, .xpi).
  Screenshots reference only existing files (01/02/03).
- No FluxDown / "inspired by" mention on the site; only one
  `<script src>` and it is the local `script.js` (no
  third-party scripts). Lighthouse: NOT VERIFIED (not measured).
- Link gate: all 10 internal anchors resolve; no missing
  relative files; all 7 release download links return 302
  (GitHub redirect to the asset, expected) and the 4 GitHub
  pages return 200 (checked with `curl -I`, 2026-11-01).
- `node --check docs/script.js` OK; AR static HTML matches the
  script.js AR dictionary for the three changed keys.
- Commit `ca8256b` ("docs(site): aria2 Windows-only wording,
  Q7 steering FAQ, R6 update wording").

## WP8 — Documentation

- `docs/FLATPAK.md`: title v1.1.1; §1 rust 1.99.1 + x86_64-only
  row; §2 "NO extra modules" corrected (two modules); §4
  validation paragraph (desktop-file-validate exit 0 locally,
  appstreamcli NOT VERIFIED, now a blocking CI step), 1.1.0
  date corrected to 2026-10-29, screenshots now the v1.1.0-era
  set (sizes re-verified live: 77,613 / 77,559 / 57,443 B);
  §5 rewritten with the WP4a outcome; new §5b aria2 outcome
  (R2, Fallback B); §7 bridge header corrected to
  `X-SwiftGet-Token`; new §8b Flathub submission caveats
  (prebuilt ffmpeg + runtime yt-dlp fetch, both to be reviewed
  against Flathub policy before any submission); §9 rewritten
  for the live workflow (staged escalation files marked stale,
  must not be re-applied); §10 proof list updated (rust 1.99.1,
  ffmpeg + aria2 checks, appstreamcli in CI, Windows clippy
  marked proven by the v1.1.0 release runs) and local build
  commands updated with `--user`.
- `docs/OPEN-QUESTIONS.md`: Q13 moved to Resolved with evidence
  (static-ffmpeg module; remaining proof is the WP10 run).
  New open questions Q21–Q26: aria2 module status, ffmpeg alias
  mirroring, Flathub policy on runtime yt-dlp fetch, WM_CLASS
  verification, Wayland and tray verification on a real desktop,
  untranslated Linux messages (WP3 i18n gap).
- `docs/PRIVACY.md`: tools-updater row states aria2 is fetched
  on Windows only + the Linux behaviour; torrents row adds the
  DHT/peer socket line (WP2 inventory); Flatpak section
  rewritten with the WP4a/WP4b outcomes (the stale "no ffmpeg
  binary is bundled" paragraph is gone).
- `THIRD-PARTY-NOTICES.txt`: Flatpak ffmpeg paragraph updated
  (bundled static 7.0.2, GPLv3, source offer); aria2 Flatpak
  paragraph added (1.37.0 from source, GPL-2.0-or-later).
- `README.md` / `README.ar.md`: the updater sentence now says
  aria2 is fetched on Windows only (R3). The Flatpak lines were
  already R1-conditional ("not attached to the 1.1.0 release
  yet", "planned for a later release") — unchanged. The
  corrected staging kit (`~/workspace/ezoo/readme-push/`) was
  updated with the same wording; the originals as supplied by
  Tensi (`~/workspace/ezoo/readme-improvements/ezoo-readme/`)
  never contained the aria2 sentence and were left untouched.
- `packaging/linux/README-Linux.md`: no aria2 claim exists
  there — nothing to correct. (Observation, not a fix: it also
  never states that torrents need a system aria2c; the WP3
  message now tells the user at the point of failure.)
- `docs/PHASES.md`: approval record + Phase 8 build status
  section with the commit list and this report's path.
  `~/workspace/ezoo/v111/PLAN-111.md` §Phase 8: build status
  added (workspace file, not committed).
- Commit `3875ae6` ("docs: Flatpak WP4 outcomes, Windows-only
  aria2, Q13 resolved, Q21–Q26 open").

## WP9 — Verification (local)

- `cargo fmt --check`: clean (exit 0).
- `cargo clippy --all-targets -- -D warnings`: clean (exit 0).
- `cargo test`: **199 passed / 0 failed / 1 ignored** (189
  baseline + 7 WP1 + 2 WP2 + 1 WP3; no regressions).
- WP1 bridge tests: all 11 `api::tests` pass (4 pre-existing +
  7 new).
- `desktop-file-validate`: exit 0 on both desktop files. XML
  well-formedness: both metainfo files parse. YAML parse:
  manifest + workflow parse.
- `appstreamcli validate`: NOT VERIFIED — not installed; added
  as a blocking CI step in WP6.
- Hashes: ffmpeg tarball SHA-256 + MD5 match (see table below);
  aria2 tarball SHA-256 matches (see table below); ffmpeg
  source tarball SHA-256 recorded.
- Grep gates (every hit reviewed against §2):
  - aria2 across README*, docs/, packaging/, site files: all
    hits are R2/R3-consistent (Windows-only fetch; Flatpak
    bundled, conditional on WP10; Linux system install). No
    stale fetch claim remains.
  - ffmpeg-full: hits only in `docs/FLATPAK.md` §5 (the
    historical explanation of why it is NOT used),
    `docs/PHASES.md` / `docs/RELEASE-REPORT.md` (audit history),
    and the manifest header comment ("NO ffmpeg-full stanza…
    retired"). No stanza, no code use.
  - FluxDown / "inspired": 0 hits in product, site, README,
    or packaging (hits exist only in `docs/PHASES.md` /
    `docs/RELEASE-REPORT.md` audit history).
  - Version strings: 1.1.1 appears nowhere in the public texts
    (README, site, metainfo); 1.1.0 is the shown version;
    `Cargo.toml` version is 1.1.1. R7 holds.
  - yt-dlp: all 40 hits are R5-consistent (fetch wording,
    user-action-only, "no yt-dlp is shipped", hash-checked).
- Asset-name gate: every `releases/latest/download/<name>` on
  the site and in README (9 names: 7 binaries + SHA256SUMS.txt
  + SHA256SUMS-windows.txt) is an asset of the published
  v1.1.0 release (verified via the GitHub API, 2026-11-01).
- Link gate: every relative link/image in README and the site
  exists in the tree; all internal anchors resolve (WP7).
- Endpoint gate: every `http://` / `https://` literal in
  `crates/` is either a test fixture (`example.com` /
  `127.0.0.1` / integration tests), a UI string ("must start
  with http:// or https://"), or an endpoint already in
  `docs/PRIVACY.md` (api.github.com / github.com update
  checker, yt-dlp + aria2 release URLs, `restore_ffmpeg()`
  from the fork's own release, the YouTube watch-URL built
  from yt-dlp's JSON for display only). No undocumented
  destination.
- `cargo-sources.json`: NOT regenerated — the Cargo.lock
  registry set is unchanged (709/709, exact set match;
  spot-check hashes for slint 1.18.1, ring 0.17.14, anyhow
  1.0.104 match the lock).

## Hashes verified

| Artifact | Source | SHA-256 (local computation) | Match |
|----------|--------|------------------------------|-------|
| ffmpeg static tarball (`ffmpeg-release-amd64-static.tar.xz`, 7.0.2-static) | johnvansickle alias, 2026-11-01 | `abda8d77ce8309141f83ab8edf0596834087c52467f6badf376a6a2a4c87cf67` | Review value + `fetch-ffmpeg.sh` MD5 pin `7fa72b652e19bf84c9461e332ea1cdf3` (alias `.md5` file agrees) |
| ffmpeg source tarball (`ffmpeg-7.0.2.tar.xz`, 10,795,332 B) | https://ffmpeg.org/releases/ffmpeg-7.0.2.tar.xz (HTTP 200; `RELEASE` file inside reads `7.0.2`) | `8646515b638a3ad303e23af6a3587734447cb8fc0a0c064ecdb8e95c4fd8b389` | Recorded; no second published digest consulted |
| aria2 1.37.0 tarball (`aria2-1.37.0.tar.xz`, 1,705,256 B) | https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0.tar.xz (size matches the release asset exactly; release body publishes no SHA-256) | `60a420ad7085eb616cb6e2bdf0a7206d68ff3d37fb5a956dc44242eb2f79b66b` | Gentoo Manifest cross-check: SHA-512 and BLAKE2B of the same file match Gentoo's published distfile hashes exactly |

## Commands run

- WP0: `cargo fmt --check` (clean); `cargo clippy --all-targets
  -- -D warnings` (clean, log `~/workspace/ezoo/v111/logs/
  phase8-baseline-clippy.log`); `cargo test` (189/0/1, log
  `~/workspace/ezoo/v111/logs/phase8-baseline-test.log`);
  `desktop-file-validate` (exit 0, both files); Python
  `xml.etree` + `yaml.safe_load` parses.
- WP1: `cargo test -p swiftget api::` (11 passed, incl. 7 new);
  full `cargo test` (196/0/1).
- WP2: `cargo clippy` (clean); full `cargo test` (198/0/1).
- WP3: `cargo fmt --check` (clean after one `cargo fmt` pass);
  `cargo test -p swiftget-engine tools::` + `mux::` (new test
  ok); full `cargo test` (199/0/1).
- WP4a: `packaging/linux/fetch-ffmpeg.sh` re-download + MD5 /
  SHA-256 verification; `ffmpeg -version` (7.0.2-static);
  `curl -I` on the ffmpeg source URL (HTTP 200) + SHA-256 of
  the downloaded source tarball.
- WP4b: `curl` download of the aria2 tarball + SHA-256;
  Gentoo Manifest fetch + BLAKE2B/SHA-512 cross-check;
  `tar --no-same-owner -xJf` extraction; `./configure
  --disable-nls --disable-metalink --disable-websocket
  --without-gnutls --without-libnettle --without-sqlite3
  --without-libxml2 --without-libexpat --without-libcares
  --without-libssh2` (exit 0; OpenSSL yes, Zlib yes,
  Bittorrent yes, Metalink/WebSocket/XML-RPC no).
- WP5: `desktop-file-validate` (exit 0, both); Python
  `xml.etree` parse of both metainfo files + field checks
  (developer, licences, content rating, releases, screenshots,
  torrent bullet, FTP claim gone).
- WP6: Python `yaml.safe_load` on the workflow + step
  inventory (17 steps; Flatpak step 3rd, before all cargo
  steps; `continue-on-error` only on the Flatpak step;
  publish + Firefox-sign gated on `github.ref_type == 'tag'`).
- WP7: GitHub API `releases/tags/v1.1.0` (all 7 binaries
  present); `curl -I` on 7 download links (302) + 4 GitHub
  pages (200); `node --check docs/script.js` (OK); anchor +
  relative-file checks (all resolve).
- WP8: greps for aria2 / yt-dlp / ffmpeg-full / FluxDown /
  version strings across README*, docs/, packaging/ (see WP9).
- WP9: `cargo fmt --check` (clean); `cargo clippy
  --all-targets -- -D warnings` (clean); `cargo test`
  (199/0/1); `cargo test -p swiftget api::` (11 passed);
  `desktop-file-validate` (exit 0); XML/YAML parses;
  `sha256sum`/`md5sum` re-verification of both tarballs;
  Python comparison of `cargo-sources.json` vs `Cargo.lock`
  (709/709 exact set match); endpoint-gate Python scan of
  `crates/` URL literals.

## WP10 — Owner steps (not performed here)

1. Push `v111-upgrade` (the workflow file needs the browser route —
   the GitHub connector cannot push `.github/workflows/*`) and trigger
   `workflow_dispatch` on that branch. This creates no release.
2. Proof checklist on a real machine: install the bundle with
   `flatpak install --user`; run under Wayland and under X11;
   `/ping` returns 1.1.1; `command -v ffmpeg` inside the sandbox
   finds the bundled binary, and one real video plus audio merge
   succeeds; if aria2 is bundled, `command -v aria2c` succeeds and
   one magnet completes; yt-dlp is fetched on first use with its
   hash verified; the host browser bridge works from the extension;
   a captured link reaches the app; downloads land in the host
   Downloads folder; the tray icon appears; the portal file chooser
   opens; appstreamcli and desktop-file-validate pass;
   `sha256sum -c` passes on the release set. Also check
   `StartupWMClass=EzooDownload` on the app's window (Q24).
3. Only after the proof passes: remove `continue-on-error` from the
   Flatpak step; update the Flatpak wording on the site, README, and
   docs (R1); add the 1.1.1 metainfo release entry; update version
   text (R7); tag v1.1.1.

## Residual risks

- Flathub policy on the runtime fetch of yt-dlp and on the prebuilt
  ffmpeg binary — must be reviewed against Flathub policy before
  any submission (open questions Q23 and §8b of `docs/FLATPAK.md`).
- The GPL source offer for the bundled ffmpeg (SOURCE-OFFER.txt in
  the Flatpak doc directory; ffmpeg 7.0.2 sources at
  https://ffmpeg.org/releases/ffmpeg-7.0.2.tar.xz, SHA-256 recorded
  above).
- The ffmpeg alias is a moving alias: the pinned SHA-256 in the
  manifest fails the build loudly if the alias moves; mirror the
  tarball to an immutable URL before Flathub submission or if the
  alias changes (open question Q22).
- WM_CLASS (`StartupWMClass=EzooDownload`) unverified — the proof
  run must check it (open question Q24).
- Wayland and tray behaviour on a real desktop unverified — the
  proof run must check them (open question Q25).
- The aria2 module is defined but its build is proven only by the
  WP10 proof run (no flatpak-builder in this VM); if it fails
  there, Fallback B applies (remove the torrent claim from the
  whole Flatpak package) (open question Q21).
- The new Linux/Flatpak missing-tool messages (WP3) are
  untranslated until 2 strings × 8 locales are added (open
  question Q26).

## Commits (branch `v111-upgrade`, `c62f754` → `3875ae6` + c9)

| # | Commit | Subject |
|---|--------|---------|
| c1 | `5e967da` | test(api): live bridge tests — foreign origins get no CORS, POSTs need token |
| c2 | `d0e559f` | engine(torrent): aria2 RPC argv as a tested pure function, loopback-only with random secret |
| c3 | `ae0674b` | engine: platform-correct aria2/ffmpeg missing messages (pure selector, tested) |
| c4 | `e08fdd9` | flatpak: static ffmpeg module and aria2 1.37.0 module (WP4a/WP4b) |
| c5 | `075dfbe` | flatpak: metainfo bundles-ffmpeg wording, drop unverified FTP claim (both metainfo files) |
| c6 | `c7b302d` | ci(linux): Flatpak --user build, appstreamcli validation, conditional Flatpak release notes |
| c7 | `ca8256b` | docs(site): aria2 Windows-only wording, Q7 steering FAQ, R6 update wording |
| c8 | `3875ae6` | docs: Flatpak WP4 outcomes, Windows-only aria2, Q13 resolved, Q21–Q26 open |
| c9 | `01bba93` | docs: Flatpak build report (WP9 verification artifacts) |
