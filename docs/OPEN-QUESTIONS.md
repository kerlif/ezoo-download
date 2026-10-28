# Open questions — v1.1.0 upgrade

Recorded instead of stopping to ask. Each entry states the default chosen
and what would resolve it. Resolved entries are moved to the bottom with
the resolution and date.

## Open

1. **Flatpak yt-dlp.** The spec offers two options (pinned module updated
   with each release, or user-install instruction). Default: decided in
   Phase 8 after the manifest is drafted; documented in the manifest and
   `docs/PRIVACY.md`.
2. **Wayland verification of the dialog fix.** No Wayland compositor is
   available in this environment, so the Phase 6 lazy-dialog fix can only
   be verified under Xvfb/X11 plus code-level proof from the Slint/winit
   sources. Default: ship the fix, verify on X11, and mark real KDE
   Plasma (Wayland) verification as PENDING in the release report.
   Resolution needs: running the v1.1.0 build on KDE Plasma (Wayland)
   and confirming dialogs open one at a time as floating windows.
3. **Fork update-signing keypair.** The fork's `version.json` ships with an
   empty `sig` (no `UPDATE_SIGNING_KEY` secret); the in-app updater
   therefore refuses fork builds. Default: generate a NEW Ed25519 keypair
   for the fork in Phase 7, commit the public key in `selfupdate.rs`
   (replacing the upstream public-key constant), and escalate the private
   key to the parent agent for storage as a CI secret — the private key
   must never be committed. The existing `ring`/Ed25519 mechanism is kept
   (it is the "equivalent" the spec allows for minisign); no second
   checker is added. Resolution needs: parent creates the CI secret via
   the browser; CI signs `version.json` in the release workflows.
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

## Resolved

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
