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
5. **Variable vs static fonts for Cairo / Noto Sans Arabic.** Current
   Google Fonts distributions are variable fonts; Phase 3 must confirm
   static Regular/Bold instances exist upstream and that Slint 1.18 /
   fontdb resolves them (including weight selection), or generate static
   instances only after checking Reserved Font Name status. Default:
   prefer unmodified upstream static TTFs; if none exist, fall back to
   the family that ships them. Resolution needs: repo-listing check at
   fetch time in Phase 3.

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
