# Privacy — outbound network requests

Inventory in progress. The entries below cover the capture-window
probing added in Phase 4 of the v1.1.0 upgrade (complete). The full
inventory — update checker, yt-dlp, torrents, and every other request
the program makes — is completed in Phase 7; nothing outside this list
was added or changed by Phase 4.

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
