# Update checker — design (Phase 7, v1.1.0)

The checker extends the existing `version.json` mechanism; it does not
replace it with a parallel flow, and there is no second checker. All
update URLs derive from the single `REPO` constant in
`crates/engine/src/selfupdate.rs` (`kerlif/ezoo-download`). The code
lives in `selfupdate.rs` (API, verification, cache, download) and
`crates/app/src/update.rs` (scheduling, asking, platform behaviour);
the HTTP client is `http::checker_client` (same proxy handling as the
download client, redirects disabled so the checker can police them).

## 1. Fetch sequence

One check makes at most three requests, in this order, and never
retries inside the check:

1. `GET https://api.github.com/repos/kerlif/ezoo-download/releases/latest`
   — the latest release announcement.
2. `GET <browser_download_url of the "version.json" asset>` — the
   signed manifest of that release.
3. `GET <browser_download_url of "SHA256SUMS.txt" (else
   "SHA256SUMS-windows.txt")>` — the release checksum file.
4. (v1.1.1+) When the release carries a `<name>.sig` asset next to
   the checksum file, `GET` it too — the checksum file's signature
   (§5). A release without the asset (e.g. v1.1.0) keeps the old
   behaviour.

Verification order, before anything is offered:

1. The API answer must parse and its `tag_name` must be a valid
   semantic version newer than the running version (§4). Otherwise:
   up to date (older tag) or silent fail (malformed tag).
2. The manifest must parse and its Ed25519 signature must verify
   against a trusted key built into the program (§5) — the primary
   key or the offline emergency key. Missing or invalid signature:
   never offered, silent fail.
3. The manifest's version must itself be a valid semantic version
   newer than the running version. Otherwise: up to date.
4. The checksum file must be present in the release. Missing:
   silent fail.
5. (v1.1.1+) When the release carries the `<name>.sig` asset, its
   signature must verify (same construction as the manifest,
   over the checksum file's SHA-256); a listed-but-unverifiable
   signature is a silent fail, exactly like a bad manifest. A
   release without the asset proceeds with the old behaviour.
6. Only then is the release offered — and the artifact itself is
   downloaded and verified (§5) before the user is asked anything.

The offered version is the manifest's version; the tag is the
release's identity and must agree that the release is newer. A
release whose tag and manifest disagree about being newer is not
offered.

A release published by the Linux workflow before the Windows
workflow has uploaded `version.json` (same release, later upload)
therefore yields no offer until the next check: the manifest asset
is simply absent, which is a silent fail, not an error.

## 2. Endpoints and headers

| Request | Method / URL | Headers sent |
|---|---|---|
| Release announcement | `GET /repos/{REPO}/releases/latest` on `api.github.com` | `User-Agent: EzooDownload/<version>`, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, `If-None-Match: <cached ETag>` (only when a cached ETag exists) |
| Manifest / checksum file | `GET <browser_download_url>` on `github.com` (redirects to the asset host, §3) | `User-Agent: EzooDownload/<version>` only |
| Artifact download | `GET <asset URL>` | `User-Agent: EzooDownload/<version>` only |

No other headers are set by the checker. No cookies, no
authorization, no query parameters, no body. Nothing identifying
the user or the machine is sent: the User-Agent carries only the app
name and version. The proxy settings from Options are honoured
(`checker_client` shares the download client's proxy handling).
Response bodies for steps 1–3 are capped at 1 MiB; artifacts are
capped at 500 MiB while streaming.

## 3. Redirects

The checker client never follows redirects on its own
(`redirect(Policy::none())`). `open_guarded` follows at most 3 hops
by hand. A hop is allowed only when:

- the target has the same host and port as the current URL (an
  http-to-https upgrade on the same host is allowed), or
- the target is `https://` on one of `api.github.com`, `github.com`,
  `objects.githubusercontent.com`, `release-assets.githubusercontent.com`
  — the hosts GitHub serves release pages and asset downloads from.

Any other redirect (a different host, a downgrade) is refused and
the check fails silently. `If-None-Match` is sent only on the first
request, never to a redirected host. Same-origin redirects are
followed (GitHub's own `releases/latest/download/...` chain lands
on the asset host above). Covered by unit tests (`redirect_allowed`)
and local-server integration tests (cross-host refused without ever
contacting the target; same-origin followed).

## 4. Version comparison

Tags and manifest versions are compared as semantic versions
(`parse_semver` / `compare_semver`, no new dependency):

- An optional leading `v`, 1–3 numeric components (missing minor /
  patch read as 0), an optional pre-release (`-alpha.1`), build
  metadata (`+...`) ignored for ordering.
- Pre-release ordering per semver: `1.0.0-alpha < 1.0.0-alpha.1 <
  1.0.0-alpha.beta < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0-beta.11 <
  1.0.0-rc.1 < 1.0.0`; numeric identifiers compare by value,
  alphanumeric lexically, numeric lower than alphanumeric, a shorter
  list lower when it is a prefix of the longer one. A release is
  higher than any of its own pre-releases.
- Malformed version strings are never "newer": the check fails
  silently instead of offering them.

The older `is_newer` (numeric-prefix comparison) is kept for
compatibility and is not used by the checker.

## 5. Integrity

- **Signature.** `version.json` carries `sig`: Ed25519 over
  `signed_message(version, sha256)` (`"ezoo-download-update\n<version>\n<sha256>\n"`),
  verified with `ring` against `UPDATE_PUBLIC_KEY` in `selfupdate.rs`.
  The signature covers the version and the Windows installer's
  SHA-256. Missing or invalid: never offered.
- **Artifact hash.** A downloaded artifact must match BOTH the
  signed manifest's SHA-256 and the release checksum file's entry
  for its file name (`parse_sums` / `sums_match`). A mismatch, or a
  checksum file with no entry for the file, refuses the download:
  the partial file is deleted and nothing is offered or installed.
  A previously downloaded copy is reused only when its hash checks
  out again. The installer re-checks the hash immediately before
  running (`run_installer`); the AppImage replace re-checks it too
  (`install_appimage`).
- **AppImage hash provenance (Q12, resolved in Phase 9).** The
  signed manifest's hash covers the Windows installer only, so the
  AppImage is verified against the release's `SHA256SUMS.txt` entry,
  and the signed manifest gates the whole offer: no offer exists at
  all unless the signed manifest verified. Since v1.1.1 the checksum
  file itself is ALSO signed: releases carry `SHA256SUMS.txt.sig`
  (Linux workflow) and `SHA256SUMS-windows.txt.sig` (Windows
  workflow), Ed25519 over
  `signed_message(version, sha256(sums_file))` — the same
  construction and the same `sign_update` example as the manifest
  (`selfupdate.rs: sums_signature_ok`, verified against the primary
  or the emergency key). The checker verifies the signature when
  the asset is present and refuses the offer when it does not
  verify; a release without the asset (e.g. v1.1.0) keeps the old
  behaviour. `CheckedRelease.sums_signature_verified` records
  which case applied. The Linux release workflow must keep
  uploading `SHA256SUMS.txt` (it generates it from the `dist/`
  files); a release without it offers nothing on Linux.
- **Size.** Artifacts over 500 MiB are refused mid-download.

## 6. ETag cache

- Location: `<app data dir>/update/checker-cache.json` (next to the
  downloaded artifacts; `default_data_dir()` — on Linux
  `~/.local/share/EzooDownload/update/`, on Windows the app-data
  folder).
- Format: `{"etag": "<ETag string or null>", "body": "<the
  releases/latest JSON verbatim>"}`. The body holds only public
  release data (tag, asset names and download URLs); no user data.
- Behaviour: the cached ETag is sent as `If-None-Match`. A `304`
  reuses the cached body (the cached payload is re-parsed and
  re-checked, including the signature step — the cache never
  bypasses verification). A `200` replaces the cache only after the
  body parses; a malformed body leaves the previous cache intact.
  Cache read/write failures are ignored (best effort): the check
  proceeds unconditionally. The cache file is written atomically
  (temporary file + rename).

## 7. Scheduling and failure modes

- First check 3 seconds after startup (async, never blocks startup),
  then every 24 hours while the program stays open.
- The check can be turned off in Options > General ("Check for
  updates automatically", on by default, including for settings
  files written before the setting existed). Off stops the
  automatic checks; Help > Check for updates still works.
- Failures fail silently and retry later with backoff. No error
  dialogs, no nagging. The checker never retries inside one check,
  so a rate limit can never become a retry storm (verified: the
  local-server rate-limit tests assert exactly one request).

| Failure | Outcome | Next try |
|---|---|---|
| Offline / connection error / timeout | silent fail | backoff ladder |
| HTTP 403 / 429 (rate limit) | silent fail, honours `Retry-After` (seconds) or `X-RateLimit-Reset` (epoch) | server-asked wait, clamped to 1 min – 24 h |
| Other HTTP errors, malformed JSON, malformed tag, missing/unsigned manifest, missing checksum file | silent fail | backoff ladder |
| Artifact fails hash / checksum verification | refused, never offered; not retried eagerly | next regular check (24 h) |
| Redirect to another host | refused, silent fail | backoff ladder |

Backoff ladder (`next_retry_delay`, consecutive failures): 5 min,
15 min, 1 h, 6 h, then 24 h (the regular interval). A server-asked
wait wins over the ladder, clamped to 1 minute – 24 hours. A
successful check resets the failure count.

A version the user declines (automatic ask) or was notified about
(notify-only) is remembered for the rest of the run and not raised
again; a newer version raises again. Help > Check for updates
always reports (it is user-initiated).

## 8. Platform behaviour

| Copy | Check finds a newer release |
|---|---|
| Windows, installed with the setup | Download the installer in the background, verify it, ask the user to install. Installation waits until no download is running. (Help > Check for updates downloads and installs at once in its window — the explicit path.) |
| Windows, portable (not installed) | No automatic check. Help > Check for updates says the copy cannot update itself. (Unchanged, Open Question 4.) |
| Linux, AppImage (`APPIMAGE` set and the file exists, or the `/tmp/.mount_*` runtime path) | Download the AppImage in the background, verify it against `SHA256SUMS.txt`, ask the user, then replace the file atomically (new file written next to the target, renamed over it, executable bit kept; a symlink target is resolved first). The new version starts on the next launch; the running copy keeps its mounted inode. If the replace fails (e.g. read-only folder), the old file is left untouched and the user is told. |
| Linux, Flatpak (`FLATPAK_ID` set) | Notify only, with a button (Yes/No question) that opens the release page in the browser. No download, no install offer. |
| Linux, .deb / .rpm (executable under `/usr/`, `/opt/`, `/snap/`) | Notify only, with the release-page link in the message. No download, no install offer. |
| Linux, anything else (unknown install location) | Notify only, as above. Ambiguous installs always fall back to notify-only, which is safe. |

Detection (`detect_package`, faked inputs in unit tests): Flatpak
first, then the `APPIMAGE` path (only when the file exists — a set
but dangling value falls through to notify-only), then the runtime
mount-point path, then system install locations. `detect_package_current`
reads the real environment and executable path.

## 9. Fork signing keypair

- Public key (committed in `selfupdate.rs`, `UPDATE_PUBLIC_KEY`):
  `a495ab88edacdf8d333ccef4e45cd8c4fc142fc3fb98792f16914098b9b640fc`
- Fingerprint (SHA-256 of the 32 public-key bytes, hex):
  `6a8ee4d63d46aeaf01a9069ab8e612c97098aa8a34be8e50e5cf4b8a15c46f64`
- Generation: 32 bytes from `/dev/urandom` as the Ed25519 seed;
  `ring::signature::Ed25519KeyPair::from_seed_unchecked` (the same
  construction the existing code and the `sign_update` example use);
  sign/verify self-check passed. Generated 2026-10-28 with a
  throwaway program outside the repository (since deleted).
- Private key: the 64-hex-character seed, in the format the
  `UPDATE_SIGNING_KEY` secret uses. It was written ONCE to a file
  outside the git tree, mode 600:
  `/home/hatch/workspace/ezoo/.update-signing-private.key`.
  It is never committed, never printed, never logged, and appears
  in no report, chat message, document, or test fixture. Tests sign
  fixtures with throwaway keys generated at runtime.
- Status: code side done; the CI secret was created 2026-10-29
  (`UPDATE_SIGNING_KEY`, Q3 resolved in Phase 0 — the v1.1.0
  release's `version.json` carries a 128-hex `sig`, verified
  2026-10-29). The hard-error rule in §10 item 2 is in place in
  `windows-release.yml` (and the Linux workflow signs its sums
  file the same way since Phase 9, §12).

## 10. CI step (Phase 8 escalation)

Phase 8 owns the workflow files (the GitHub connector cannot push
`.github/workflows/*`); this is the exact step it must put in
place. The Windows release workflow
(`.github/workflows/windows-release.yml`) ALREADY passes the secret
into the Package step and signs with the existing example — only
two things are missing:

1. **Create the secret** (parent agent, via the browser):
   repository secret `UPDATE_SIGNING_KEY`, value = the private key
   (64 hex characters) held in the file named in §9. Nothing else
   needs the value, and it must not be pasted anywhere else.

2. **Make a missing key a hard error on releases.** Today the
   workflow warns and ships `version.json` with an empty `sig`
   when the key is absent. Change the signing block so that, when
   `IS_RELEASE` is true and `UPDATE_SIGNING_KEY` is empty, the job
   FAILS instead of warning. A release must never again ship an
   unsigned manifest, or installed copies will silently refuse the
   update. The signing commands themselves are unchanged:

   ```powershell
   $hash = (Get-FileHash dist/EzooDownload-Setup.exe -Algorithm SHA256).Hash.ToLower()
   $sig = ""
   if ($env:UPDATE_SIGNING_KEY) {
     $sig = (cargo run --quiet --release -p swiftget-engine --example sign_update -- $version $hash).Trim()
     if ($LASTEXITCODE -ne 0 -or $sig.Length -ne 128) { throw "could not sign the release" }
   } elseif ($env:IS_RELEASE -eq "true") {
     throw "UPDATE_SIGNING_KEY is not set: refusing to ship an unsigned version.json for a release"
   }
   ```

   The `version.json` writing, the `SHA256SUMS-windows.txt`
   generation, and the `gh release upload` file list
   (`EzooDownload-Setup.exe`, `version.json`,
   `SHA256SUMS-windows.txt`) are unchanged. The Linux workflow
   needs no signing change, but must keep uploading
   `SHA256SUMS.txt` (it already does, via `dist/*`) and keep the
   AppImage asset name `EzooDownload-<version>-x86_64.AppImage`,
   which the checker selects by suffix.

3. **Verify after the secret exists:** a tag build's `version.json`
   carries a 128-hex `sig`, and an installed copy offers the update.
   The `sign_update` example refuses to run unless the secret's key
   matches the public key in `selfupdate.rs`, so a wrong secret
   fails the build loudly instead of shipping a bad signature.

## 11. Phase 8 note — signature reconciliation (no minisign)

The Phase 8 spec's "minisign signature" wording is reconciled
here: the independent signature for the release manifest IS this
Ed25519/`ring` mechanism (the allowed equivalent per Open
Question 3). No second signature scheme and no minisign
dependency were introduced. The Flatpak bundle is covered by the
release's `SHA256SUMS.txt` (the Linux workflow's checksum step
now includes the `.flatpak`); Flatpak copies are notify-only
(§8) and never consume the signed manifest for self-install.
Full Flatpak + CI design: `docs/FLATPAK.md`.

## 12. Phase 9 — signed checksums + offline emergency key (Q12/Q28/Q33)

- **Signed checksums.** Since v1.1.1 the release workflows sign
  the checksum files with the same key and the same `sign_update`
  example as `version.json`, and ship the signatures as release
  assets: `SHA256SUMS.txt.sig` (Linux workflow, `dist/*` upload)
  and `SHA256SUMS-windows.txt.sig` (Windows workflow, explicit
  file list). A missing `UPDATE_SIGNING_KEY` is a hard error on
  releases, exactly like the `version.json` rule. The engine
  verifies the signature when the asset is present
  (`selfupdate.rs: sums_signature_ok`, unit-tested with throwaway
  keys in `sums_signature_and_emergency_key`) and records the
  outcome in `CheckedRelease.sums_signature_verified`. Releases
  without the asset keep the old behaviour, so v1.1.0 still
  updates. `tools.json` signing stays deferred (Q33).
- **Offline emergency key.** A second keypair exists for the case
  the primary key is compromised or lost. Public half (committed
  in `selfupdate.rs`, `UPDATE_PUBLIC_KEY_EMERGENCY`):
  `6081969eeccdcc04903fc280a095f0e721ccc18deef693b6cf6ba0735ce93e44`
  (generated 2026-11-01, Phase 9, same construction as §9).
  Private half: the 64-hex seed, written ONCE to a file outside
  the git tree, mode 600:
  `/home/hatch/workspace/ezoo/.update-signing-emergency.key`.
  It is never committed, never printed, never logged, and appears
  in no report, chat message, document, or test fixture. The
  checker accepts a manifest or a sums signature made with either
  key (`signature_ok_any` / `sums_signature_ok`, both unit-tested
  at the helper level; a live check against an emergency-signed
  manifest has not been run — it needs a release signed with that
  key, which only happens if the primary is ever retired).
  **CI never uses the emergency key**: the workflows sign with
  the primary key only. If the primary is ever compromised, the
  owner provisions the emergency seed as a repository secret and
  the workflows switch to it — installed copies accept the next
  release with no code change.
- **`ffmpeg.exe` asset (related, Phase 9).** The Windows
  workflows now also bake the ffmpeg build's SHA-256 into the
  binary (`EZOO_FFMPEG_SHA256`, computed from the ffmpeg this run
  built) and upload `ffmpeg.exe` itself as a release asset, so
  `restore_ffmpeg()` can fetch it from our own release and verify
  it. Without the baked hash the function stays inert.


## 13. Hotfix (patch of 1.1.1, 2026-11-02, branch `v111-hotfix`)

The hotfix changes how an already-verified offer is *shown* and what a
Managed install *does* with it. It adds **no new outbound request** and
changes nothing about §1–§6: the check, the signed-manifest gate, the
checksum verification and the cadence are exactly as documented above.
Decisions Q36–Q48 are recorded in `docs/OPEN-QUESTIONS.md`.

### 13.1 The update notice (Q36, Q39)

`handle_available()` (`crates/app/src/update.rs`) now shows the notice
**before** any download, for automatic checks on all platforms and for
the Linux manual check: a new Slint component `UpdateNoticeDialog`
(`crates/app/ui/dialogs.slint:4006`, modelled on `WhatsNewDialog`) with
a heading ("Version X is available"), the highlights (§13.2), a
full-notes link to the release page, and two buttons — «Update now» /
«Later». There is no third "never for this version" button: the
existing disable-auto-update setting (Options > General, upstream #49,
already implemented) covers "never". Closing the window, or a dialog
that cannot be shown at all, counts as Later — nothing is downloaded
without an explicit Update now.

«Update now» proceeds into the existing platform arm (§8, as amended
by §13.4 below). On Windows the install question after the download
stays (it confirms the install itself, not the update). On Linux
AppImage the second "Replace now?" ask is dropped when the user already
said Update now, but the replace still waits until no download is
running. `WhatsNewDialog` (the post-update flow) is untouched.

### 13.2 Highlights source (Q36, option A)

The highlights come from the release body of the **already-fetched**
`releases/latest` API response (§1 step 1): `GithubPayload` gained a
`body` field, carried into `CheckedRelease.notes` (capped at 4 KiB on a
char boundary, `truncate_notes`), and parsed in the app by
`release_highlights()` — the first ≤5 bullet lines, language-matched
(Arabic bullets for an Arabic UI, the English paragraph otherwise).
Display-only, with a "full notes" link to `release_url`. A body with
no bullets degrades to a generic "see the release page" line, never to
an empty dialog or a crash (H1 tests). No new request, no workflow
change, no signature change, no risk to existing clients (old binaries
ignore the extra field). Trust note: the body is unsigned — a tampered
body could show attacker text in the notice, but it cannot trigger an
install: the offer and the artifact are still gated by the signed
manifest + signed sums. The parser treats the body as plain text (no
links executed, no markup interpreted beyond bullet splitting).
`docs/PRIVACY.md` records the one-line consequence.

### 13.3 Once-per-launch reminder (Q37, Q38, Q48)

Amended 2026-11-02 12:43 by Tensi (4h re-reminder replaced by
once-per-launch) and decided 2026-11-02 14:34 (Q48): the notice is
shown **at most once per program run for a given version** — including
when a mid-run 24h check finds the update; it does not wait for the
next launch, and it is not launch-only.

**No durable state is added.** «Later» marks the offered version in a
new in-memory per-run flag `LATER` (`update.rs:54`, same pattern as
the existing `DECLINED` / `NOTIFIED` statics); the notice does not
appear again in this run. The next launch runs the check again and
re-offers if the update is still pending. A newer version re-offers at
once. Help > Check for updates bypasses the suppression, as it
bypasses `DECLINED` today. The earlier Settings-fields proposal
(`update_remind_at` / `update_remind_version`) is dropped. The check
cadence is unchanged (launch + every 24 h, §7). Covered by the tests
`later_suppresses_the_notice_until_a_newer_version`,
`declined_versions_suppress_the_notice_too` and
`handle_available_is_done_when_the_notice_is_suppressed`.

### 13.4 Managed (.deb/.rpm) download-to-Downloads (Q40)

**This supersedes the Managed row of §8.** A Managed install is no
longer plain notify-only: when the user says «Update now» in the
notice, `notify_only()` → `download_managed()` downloads the matching
asset into `~/Downloads` (or the download folder from Settings when
`HOME` is not set), hash-verified against the release sums with the
same `download_artifact` verification the AppImage arm uses (the
asset is staged in the update dir and only then placed in Downloads),
then opens the folder so the user finds the file and tells them the
full path. The app **never runs the package manager** — the user
installs the file themselves.

The flavour comes from `/etc/os-release` (`ID` / `ID_LIKE`):
`rhel`/`fedora`/`suse`/`opensuse` → `.rpm`, anything else (including
unreadable) → `.deb` (`managed_flavour()`); the asset accessors are
`CheckedRelease.deb_asset()` / `rpm_asset()` (H1, same pattern as
`appimage_asset()`). If the release published no matching asset, the
notice falls back to the releases-page message. Flatpak stays
notify-only as in §8 (its «Update now» opens the release page
directly); an install whose kind cannot be told (`Unknown`) takes the
Managed path, which is safe — it only downloads a file.

### 13.5 About shows the running package (Q45)

`about()` fills two new `AboutDialog` properties from
`detect_package_current()` and `std::env::current_exe()`: line 1
"Version 1.1.1 (AppImage)" / "(.deb)" / "(.rpm)" / "(Flatpak)" /
"(Windows setup)" / "(portable copy)", line 2 the full executable
path in muted small text (`package_short_label()`,
`crates/app/src/main.rs:1452-1453`). Every update string now names
the running package through the single helper `package_label()`
("AppImage" / "the .deb/.rpm package" / "the Flatpak" / "the Windows
setup" / "this copy"), so no message can offer the wrong package
again — the defect Tensi found testing 1.1.1 (a .deb install offered
an AppImage) was that neither message named the package actually
running.
