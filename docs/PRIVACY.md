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
  download's own record, encrypted as before (see `secret.rs`); probing
  adds no new stored copy.
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
| Site grabber | The user runs the grabber on a page | That site's pages, with the shared client (proxy + user-agent settings), to find links. |
| Tools updater (`tools.rs`) | Automatic: 20 s after startup, then every 6 h, but only fetches when a day has passed or a tool is missing; silent, no UI | `github.com/yt-dlp/yt-dlp` (the `yt-dlp` binary + `SHA2-256SUMS`, hash-checked before use) and the fixed aria2 1.37.0 release zip (hash-checked). Sends the browser user-agent string (`DEFAULT_USER_AGENT`) — pre-existing behaviour, unchanged by Phase 7; the update checker does not use it. ffmpeg is never fetched: it ships with the install. |
| yt-dlp subprocess | User action only: video choices in the browser panel, playlist lookups, Help > update yt-dlp | Whatever yt-dlp itself contacts (video sites; its own update endpoint when updating). That is yt-dlp's behaviour, not the app's; the app never runs it in the background. |
| Torrents (aria2 subprocess) | The user adds a torrent | The trackers and peers of that torrent. The app talks to aria2 only over its local JSON-RPC server on `127.0.0.1` (ephemeral port, random password per start, never through a proxy). |
| Browser-extension bridge | Inbound only: the extension posts captured links | Local TCP server on `127.0.0.1` (port 9614). No outbound traffic; requests carry a token header and the `Host` is validated against DNS rebinding. |

Not app traffic: the Firefox add-on update manifest
(`firefox-updates.json`, a static file on the release) is fetched
by Firefox itself on its own schedule, never by this program.

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
- The Flatpak's ffmpeg/codec libraries come from the Freedesktop
  runtime (codecs-extra extension, installed automatically with
  the runtime); no ffmpeg binary is bundled by the Flatpak
  manifest — see `docs/FLATPAK.md` §5 and Open Question 13.
