# Privacy — outbound network requests

Inventory complete as of Phase 7 of the v1.1.0 upgrade (surveyed from
the code; the update-checker section is verified by local-server
tests). Nothing outside this list is contacted by the program. Phases
4–5 added the probing and learning entries only; Phase 7 added the
update checker and surveyed the rest.

## Rules that apply to every request below

- No telemetry, analytics, or crash reporting anywhere in the program.
- The captured browser headers (User-Agent, Referer, Cookie) are used
  only to ask the server about the captured link itself. They are never
  written to disk, never written to a log, and never included in any
  result object. The only place a download's headers are stored is the
  download's own record, sealed as described in `secret.rs` — DPAPI-encrypted
  on Windows; on Linux the record file itself is owner-only (mode 0600,
  since v1.1.1, see `store.rs`); probing adds no new stored copy.
  The same holds for the proxy password in `settings.json`: DPAPI-encrypted
  on Windows, owner-only file (mode 0600) on Linux.
- Probing never runs yt-dlp. yt-dlp runs only when the user asks for
  video choices (the browser panel's "Download this video" flow).

## Update checker (Phase 7)

Checked at startup (3 s in, async) and every 24 hours while the
program stays open, unless turned off in Options > General. The
check is the only contact with `api.github.com`, and the only
request the program makes on its own besides the tools updater
below. Full design: `docs/UPDATE-CHECKER.md`.

| Request | Purpose |
|---|---|
| `GET https://api.github.com/repos/kerlif/ezoo-download/releases/latest` | Learn the latest release (tag + asset list). The one update source; the repository comes from the single `REPO` constant. |
| `GET` the release's `version.json` asset | The signed manifest (version, download URL, SHA-256, signature). Verified before anything is offered. |
| `GET` the release's `SHA256SUMS.txt` (else `SHA256SUMS-windows.txt`) | Checksum file the downloaded artifact is cross-checked against. |
| `GET` the artifact (installer / AppImage) | Only after the signed manifest verified, only when a newer release exists, and only on Windows (installed copies) and Linux AppImages. Downloaded in the background into a temporary directory, verified, and only then offered to the user. |

Headers sent, and nothing else: `User-Agent: EzooDownload/<version>`
(app name and version only — no user, machine, or locale data),
`Accept: application/vnd.github+json` and `X-GitHub-Api-Version:
2022-11-28` on the API request only, and `If-None-Match: <cached
ETag>` when a cached answer exists. No cookies, no authorization,
no query parameters, no request body. The proxy settings from
Options are honoured. Redirects are followed by hand (at most 3)
and only to the same origin or to GitHub's release-asset hosts
(`api.github.com`, `github.com`, `objects.githubusercontent.com`,
`release-assets.githubusercontent.com`); any other redirect is
refused.

ETag cache (so unchanged releases cost nothing): `<app data
dir>/update/checker-cache.json`, format `{"etag": "...", "body":
"<the releases/latest JSON verbatim>"}`. The body holds only public
release data (tag, asset names and URLs). A `304` reuses the cached
body, which is re-parsed and re-verified (signature included) —
the cache never bypasses verification. Cache failures are ignored.

Failures (offline, rate-limited, malformed answers) fail silently
and retry later with backoff (5 min, 15 min, 1 h, 6 h, 24 h; a
server-asked `Retry-After` / rate-limit reset wins, clamped to
1 min – 24 h). The checker never retries inside one check.

Hotfix (2026-11-02, Q36): the `releases/latest` response body (the
release notes) is shown to the user in the update notice — no new
outbound request of any kind; it is the same response the check
already fetches.

## Capture-window probing (Phase 4)

All of these go to the captured link's own server only, on the user's
action of opening/pasting a link in the "Download File Info" window,
and stop after ~5 seconds per stage.

| Request | Purpose |
|---|---|
| `HEAD` on the link | Read `Content-Length`, `Content-Disposition`, `Content-Type`, `Accept-Ranges` without downloading. |
| `GET` with `Range: bytes=0-0` | Confirm the size from the `Content-Range` total (a HEAD length alone is untrusted), and learn whether ranges/resume work. One byte is returned. |
| `GET` of an `.m3u8` playlist / `.mpd` manifest | Read the stream's variants, durations, bitrates, and DRM signals so the window can show duration, qualities, and an estimated size. |
| `GET` of variant media playlists (HLS) | Per-variant durations and segment lists (at most 8 variants). |
| `GET` with `Range: bytes=0-0` on 2–3 segments | Sample segment sizes when no bitrate tag exists, to estimate the total size. One byte per segment is returned. |
| `HEAD` on a segment | Fallback when a server ignores the ranged sample request. |

No other hosts are contacted by probing: no third-party metadata,
thumbnail, or search service is queried.

## Per-host learning store (Phase 5)

The adaptive engine remembers, for each host a file was downloaded
from, the connection count that performed best and the mean segment
size, as exponentially weighted averages. This is stored locally only:

| Item | Detail |
|---|---|
| Where | `host_learning` in `settings.json`, in the app data folder, next to the other settings. |
| Key | The lowercase host name only (no path, query, user info, or file name). |
| Contents | Two moving averages, a sample count, and a last-updated timestamp. No URLs, file names, or content are stored. |
| Expiry | Entries older than 30 days are ignored and pruned on the next write. |
| Network | None. The store is read and written locally only; no host-learning data is sent anywhere, and Phase 5 adds no new network destination of any kind. The download requests themselves are unchanged: same hosts, same headers, same Range requests as before. |

The explicit per-server connection exceptions (Options > Connection)
are a separate, user-set mechanism and are unchanged.

## Other outbound requests (surveyed, Phase 7)

Everything below is user-initiated or the download itself; none of
it is telemetry, and none of it was added or changed by Phase 7
(the checker is the only new network code in this phase).

| Activity | Trigger | Destination / data |
|---|---|---|
| Downloads (HTTP, HLS, DASH) | The user adds a link, or the browser extension / clipboard hands one over | The link's own server only. Headers: the user-agent from settings, the browser's captured headers for that link when present, and any saved site login for it. `Range` requests for segments/resume. |
| Site grabber | The user runs the grabber on a page | That site's pages, with the shared client (proxy + user-agent settings), to find links. After a crawl (or a links batch from the browser), the grabber also sends one HEAD request per shown file (at most 100, 6 at a time, 3 s each) to the same site, only to read Content-Length for the size column — no cookies, no body, no other destination; a link the server will not answer stays "unknown". |
| Tools updater (`tools.rs`) | Automatic: 20 s after startup, then every 6 h, but only fetches when a day has passed or a tool is missing; silent, no UI | `github.com/yt-dlp/yt-dlp` (the `yt-dlp` binary + `SHA2-256SUMS`, hash-checked before use) and, on Windows only, the fixed aria2 1.37.0 release zip (hash-checked). On Linux the app never fetches aria2: `.deb`/`.rpm`/AppImage need a system `aria2c` installed by the user, and the Flatpak bundles aria2 1.37.0 built from source (see `docs/FLATPAK.md` §5b). Sends the browser user-agent string (`DEFAULT_USER_AGENT`) — pre-existing behaviour, unchanged by Phase 7; the update checker does not use it. ffmpeg: on Windows, `restore_ffmpeg()` (Phase 1) fetches `ffmpeg.exe` from the fork's own release (`kerlif/ezoo-download`) when the bundled ffmpeg is missing, and only accepts it when its SHA-256 matches the hash baked in at build time (`EZOO_FFMPEG_SHA256`, injected by the fork workflows in Phase 9); until that hash is injected the function does nothing and ffmpeg is never fetched. On Linux the bundled ffmpeg inside the packages suffices and nothing is fetched (Q23). |
| yt-dlp subprocess | User action only: video choices in the browser panel, playlist lookups, Help > update yt-dlp | Whatever yt-dlp itself contacts (video sites; its own update endpoint when updating). That is yt-dlp's behaviour, not the app's; the app never runs it in the background. |
| Torrents (aria2 subprocess) | The user adds a torrent | The trackers and peers of that torrent. The app talks to aria2 only over its local JSON-RPC server on `127.0.0.1` (ephemeral port, random password per start, never through a proxy). aria2 itself also opens an ephemeral UDP port for DHT and outbound TCP connections to peers — that is the torrent's own traffic, not app traffic; the RPC listener itself is loopback-only (see `crates/engine/src/torrent.rs`). |
| Browser-extension bridge | Inbound only: the extension posts captured links | Local TCP server on `127.0.0.1` (port 9614). No outbound traffic from the app. POSTs must carry the per-install token (`X-SwiftGet-Token`, Q29, Phase 1): 32 random bytes made once per install, kept in `local-api-token` in the data folder (owner-only on Unix), handed to the extension through GET `/config`; the token never leaves the machine. GET `/ping` and `/config` stay open (liveness, and the token bootstrap). Since the Phase 3 extension, the token is required on every POST with no fallback (the 1.1.0 legacy fallback — a POST from a browser-extension origin, or with no Origin, taken on the `X-SwiftGet` header alone — is gone); `POST /quit` and `POST /open-torrent` (new in 1.0.8, merged in Phase 2) required the token from day one. The token does not protect against local processes or other extensions (they can read the token file or GET `/config`); the native-messaging host (later release) closes that. The `Host` is validated against DNS rebinding. The extension itself re-fetches HLS playlists it already saw the page request (Phase 3, upstream 1.0.8) to list their qualities — same media server, no new destination. |

Not app traffic: the Firefox add-on update manifest
(`firefox-updates.json`, a static file on the release) is fetched
by Firefox itself on its own schedule, never by this program.

Local diagnostics only: `data/events.log` records one line per start,
close, update, or failed start (timestamp, version, short message;
rotated at 128 KiB). It is written locally only and never sent
anywhere (Phase 2).

Phase 1 (v1.1.1) changes to the table above: the Tools updater row
(`restore_ffmpeg()`, Windows-only, hash-gated, inactive until Phase 9
injects the hash) and the Browser-extension bridge row (per-install
token on POSTs, legacy fallback for the 1.1.0 extension). Phase 3
(v1.1.1) removed that fallback: the token is now required on every
POST. No new outbound destination of any kind: the token never leaves the machine,
and `restore_ffmpeg()` fetches only from the fork's own release.

## Flatpak (Phase 8)

The Flatpak build adds no new outbound request of any kind, and no
new network destination:

- Downloads, probing, per-host learning, the tools updater and the
  update checker behave exactly as inventoried above. The tools
  updater's yt-dlp fetch (hash-checked `yt-dlp_linux` from the
  yt-dlp releases page) is the Flatpak's yt-dlp supply: no yt-dlp
  is shipped inside the Flatpak, and no manual user install is
  needed — see `docs/FLATPAK.md` §6 (Open Question 1, resolved).
- The browser-extension bridge stays local-only: `--share=network`
  shares the host network namespace, so the host browser's POST to
  `127.0.0.1:9614` reaches the sandboxed app over the same
  loopback. Documented expectation, not a tested fact — no
  Flatpak run was possible in Phase 8; see `docs/FLATPAK.md` §7.
- The Flatpak bundles a static ffmpeg (johnvansickle 7.0.2-static,
  GPLv3) at `/app/lib/ezoo-download/ffmpeg` — the app's own search
  path, verified in the code — with its licence text and GPLv3
  source offer in the Flatpak's documentation directory; see
  `docs/FLATPAK.md` §5 (Open Question 13 resolved by the
  static-ffmpeg module; the proof run still has to confirm one
  real merge). Torrents in the Flatpak use the bundled aria2
  1.37.0 built from source — see `docs/FLATPAK.md` §5b. The
  aria2 JSON-RPC server (loopback only, ephemeral port, random
  per-run password) and its DHT/peer connections are the same as
  on the other Linux packages.

## The website

The project website (`docs/`, published on GitHub Pages) is static
files only: no trackers, no third-party scripts, no externally
loaded fonts, and no `fetch`/XHR/beacon calls in its script. The
only external host it references is `github.com` (release download
links and the upstream link). It sets no cookies beyond the
language/theme preferences the page itself stores locally in the
browser. The website makes no request on behalf of the program,
and the program makes no request on behalf of the website.
