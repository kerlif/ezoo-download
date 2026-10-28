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
3. **Fork update-signing keypair.** The fork's `version.json` ships with an
   empty `sig` (no `UPDATE_SIGNING_KEY` secret); the in-app updater
   therefore refuses fork builds. CODE SIDE DONE in Phase 7
   (2026-10-28): a NEW Ed25519 keypair was generated for the fork
   (32 random bytes from `/dev/urandom` as the seed,
   `ring::Ed25519KeyPair::from_seed_unchecked`; public key
   `a495ab88…640fc`, fingerprint SHA-256 `6a8ee4d63d46aeaf…`, full
   values in `docs/UPDATE-CHECKER.md` §9) and the public key is
   committed in `selfupdate.rs`, replacing the upstream constant;
   the existing `ring`/Ed25519 mechanism is kept (it is the
   "equivalent" the spec allows for minisign); no second checker
   was added. The private key (64-hex seed) was written ONCE to a
   file OUTSIDE the git tree, mode 600:
   `/home/hatch/workspace/ezoo/.update-signing-private.key` — it
   is in no commit, report, or test fixture. The exact CI step
   Phase 8 needs is written in `docs/UPDATE-CHECKER.md` §10.
   Resolution needs: parent creates the CI secret
   `UPDATE_SIGNING_KEY` via the browser with that private key, and
   Phase 8 makes a missing key a hard error on release builds.
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

13. **Flatpak ffmpeg binary presence.** The Flatpak manifest uses
    the Freedesktop runtime's codec libraries (codecs-extra,
    auto-installed with runtime 25.08+) and bundles no ffmpeg
    binary, because the `ffmpeg-full` extension no longer exists
    on the 25.08/26.08 branches (see `docs/FLATPAK.md` §5). The
    app's muxer needs an `ffmpeg` *binary* on `PATH`
    (`mux.rs::find_tool`). Whether the 26.08 runtime puts one
    there could not be established from documentation and no
    Flatpak run was possible in Phase 8. Default: ship the
    Flatpak as prepared; if a real run shows no `ffmpeg` on
    `PATH`, merging fails gracefully with the existing
    "ffmpeg is missing" error and the follow-up is a
    static-ffmpeg module in the manifest. Resolution needs: a
    real Flatpak run checking `command -v ffmpeg` in the
    sandbox plus one real video+audio merge.

## Resolved

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
