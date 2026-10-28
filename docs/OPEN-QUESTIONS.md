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
