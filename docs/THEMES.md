# Themes — v1.1.0 (Phase 6)

Nine themes live in `crates/app/ui/common.slint` (`Theme.mode` 0–8),
applied to every window by `apply_look` in `crates/app/src/main.rs`.
The choice is the existing setting `theme` (`Settings.theme`, 0–8),
picked in View > Theme or Options > Appearance and saved in
`settings.json` — no second store. A dialog created lazily (Phase 6)
gets the saved look at creation, so it never opens with the Slint
defaults. `Theme.dark` (modes 1, 3, 5, 7) also drives the standard
widgets' `Palette.color-scheme` in `ThemedWindow`.

## Palettes

| # | Theme | Window | Panel | Text | Muted | Accent | Toolbar top → bottom |
|---|---|---|---|---|---|---|---|
| 0 | Light | #f0f0f0 | #ffffff | #1b1f24 | #5f6b78 | #0f56c4 | #fdfdfd → #e7ebf0 |
| 1 | Dark | #1e1f22 | #26282c | #e8eaed | #9aa0a6 | #5b9dff | #2e3136 → #24262a |
| 2 | Ocean blue | #e8f1fb | #ffffff | #13243a | #4f6a8a | #1565c0 | #2068d0 → #1b5fc4 |
| 3 | Midnight purple | #15131d | #1d1a29 | #ece9f6 | #a59fc0 | #a78bfa | #2b2545 → #1e1a33 |
| 4 | Forest green | #eef5ef | #ffffff | #15281b | #4f7359 | #146c43 | #1e7a43 → #1f7a43 |
| 5 | Black | #000000 | #0b0b0b | #f2f2f2 | #9a9a9a | #4ea1ff | #121212 → #050505 |
| 6 | Sunset | #fbf1ea | #ffffff | #2e1a12 | #8a6352 | #b23a1a | #b93a1c → #b23a1a |
| 7 | Teal night | #0f1c1e | #142527 | #e3f4f3 | #8fb3b0 | #2ec4b6 | #1b3a3d → #132b2e |
| 8 | Rose | #fbf0f4 | #ffffff | #2e1420 | #8a5a6c | #c23d6e | #b93a68 → #c23d6e |

Dark (mode 1) is a deep slate/charcoal theme: #1e1f22 window,
#26282c panels, #2e3136/#24262a toolbar, #3a3d42 hairlines,
#2b2d31 inputs, #34373c progress-track background, with subtle
#3a3d42 borders throughout.

Semantic status colors are one value per theme (light / dark pairs),
so status text passes on light themes too:

| Role | Light themes (0, 2, 4, 6, 8) | Dark themes (1, 3, 5, 7) |
|---|---|---|
| ok | #177245 | #3fcf7a |
| warn | #8a5a00 | #e08a00 |
| err | #b3261e | #ff6b62 |

Segment-bar states reuse exactly these: downloading = accent,
done = ok, stalled = warn, idle = muted on the `bar-bg` span.

## Measured contrast (WCAG 2.1, relative luminance)

Computed from the hex values above with the WCAG formula
`(L1 + 0.05) / (L2 + 0.05)`. Normal text needs ≥ 4.5:1, large text
≥ 3:1. Every pair below is normal text on its background; all pass
4.5:1. The toolbar columns are the toolbar label color on each end
of the toolbar gradient.

| Theme | text/win | text/panel | muted/win | muted/panel | accent/win | ok/win | err/win | warn/win | toolbar/top | toolbar/bottom |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Light | 14.53 | 16.56 | 4.77 | 5.44 | 5.85 | 5.22 | 5.74 | 5.20 | 16.28 | 13.83 |
| Dark | 13.67 | 12.25 | 6.24 | 5.59 | 6.05 | 8.18 | 5.91 | 6.12 | 10.83 | 12.57 |
| Ocean blue | 13.72 | 15.66 | 4.89 | 5.58 | 5.04 | 5.22 | 5.73 | 5.19 | 5.31 | 6.03 |
| Midnight purple | 15.36 | 14.25 | 7.28 | 6.75 | 6.75 | 9.12 | 6.59 | 6.83 | 12.08 | 14.03 |
| Forest green | 14.02 | 15.54 | 4.82 | 5.34 | 5.82 | 5.37 | 5.90 | 5.35 | 5.36 | 5.35 |
| Black | 18.76 | 17.58 | 7.46 | 6.99 | 7.87 | 10.42 | 7.53 | 7.80 | 16.73 | 18.21 |
| Sunset | 14.83 | 16.50 | 4.73 | 5.27 | 5.37 | 5.35 | 5.87 | 5.33 | 5.70 | 5.98 |
| Teal night | 15.35 | 13.98 | 7.67 | 6.98 | 8.04 | 8.64 | 6.25 | 6.48 | 10.76 | 13.10 |
| Rose | 15.26 | 16.98 | 5.02 | 5.59 | 4.51 | 5.35 | 5.87 | 5.33 | 5.43 | 5.02 |

### What Phase 6 changed, and why

The Dark theme itself already passed: text 13.67:1, muted 6.24:1,
accent 6.05:1, ok 8.18:1, err 5.91:1, warn 6.12:1 — no Dark value was
changed. The failures were on light themes, where the old semantic
colors were used as status text:

- ok #1f9d55 → 3.06:1 on Light → #177245 (5.22:1).
- err #d23b32 → 4.18:1 on Light → #b3261e (5.74:1).
- warn #e08a00 → 2.36:1 on Light → #8a5a00 (5.20:1).
- accent #1f6feb → 4.07:1 on Light → #0f56c4 (5.85:1);
  Forest #1f8a4c → 3.95:1 → #146c43 (5.82:1);
  Sunset #d4513a → 3.75:1 → #b23a1a (5.37:1).
- muted Ocean #557090 → 4.48:1 → #4f6a8a (4.89:1);
  Forest #557a61 → 4.36:1 → #4f7359 (4.82:1).
- Toolbar white labels on the saturated light gradients:
  Ocean top #2f80ed 3.87:1 → #2068d0 (5.31:1);
  Forest top #2f9e5b 3.41:1 → #1e7a43 (5.36:1);
  Sunset #f07b3f 2.76:1 / #d4513a 4.17:1 → #b93a1c (5.70:1) /
  #b23a1a (5.98:1); Rose top #e05a8a 3.50:1 → #b93a68 (5.43:1).

Selected-row text on the selection background also passes
(Light 13.05:1, Dark 7.05:1) and hover text passes (Light 14.66:1,
Dark 10.42:1).

## RTL and LTR

Every window mirrors through `Theme.rtl` + the `HRow` component
(row-reverse in Arabic). The Phase 6 controls were checked in both
directions: the toolbar (filter box at the trailing end in both),
the segment bar (spans and fills grow from the leading edge in both
— the RTL `x` formulas mirror the LTR ones), and the segment legend
(swatch stays with its label when the row reverses). Directional
icons were not changed: media symbols (play/pause) keep their
direction in both, as before; no new directional icon was added.

## New strings

One new Rust string, "Stalled", in all 8 locales (English built in;
Arabic in `i18n.rs`; the other six in `lang/rs/*.json`, marked
needs-review — see `docs/OPEN-QUESTIONS.md` item 10). The segment
legend reuses the existing Slint strings "Finished",
"Receiving data…" and "Waiting" plus "Stalled" through
`ar_ui.py` + `lang/ui/*.json` → `.po` (471 texts, 0 missing,
regenerated deterministically).

---

# v1.1.2 branding — EDM palette (2026-11-05)

The supplied logo: dark-navy tile, "EDM" in blue / green / red, a download
arrow as the D's stem. The app UI is recoloured from that palette;
shapes, paths, layout, text and i18n strings are untouched, and the
flat rule, the 74 px toolbar, the flat fills, the unified badges, the
RTL toolbar-label fix and the second progress bar all still stand.

## Canonical palette (from the logo, JPEG-mode values)

| Token | Value | Role |
|---|---|---|
| EDM Navy Dark | #1d283a | dark toolbar / header surfaces |
| EDM Navy | #253149 | dark surfaces |
| EDM Page charcoal | #363940 | logo tile background; not used as a UI surface |
| EDM Blue (E) | #4687c7 | download — icon fills, badges, globe |
| EDM Green (D) | #54ab64 | add / ok — base hue only, never used raw |
| EDM Red (M) | #c2453f | stop / delete — base hue, browser disc |
| EDM Red dark | #9f3535 | stop / delete text on light themes |
| EDM White | #ffffff | labels, glyphs |
| EDM Grey | #b0b3b8 | logo grey; not used — Theme.muted stays |

Roles before colours, as before: blue = download, green = add/ok,
red = stop/delete, amber = warn (amber is functional, not brand, and
is kept: `Theme.warn` is unchanged, #8a5a00 light / #e08a00 dark).

## Derivation — base colour → variant → contrast

Raw logo colours fail WCAG 2.2 AA as text, so every text role is a
derived darker/lighter variant, computed with the WCAG luminance
formula (Python, same method as Phase 6). Two corrections found by
computing instead of guessing:

- The suggested green #2f7d3e measures **4.47:1** on the Light window
  (#f0f0f0) — just under AA — so the light ok text is **#2b7439**
  (5.03:1), EDM Green darkened one step further.
- The suggested blue #3570a6 passes on window/panel (4.58:1 /
  5.22:1) but fails as accent-coloured text on a selected row
  (4.13:1 on the selection background), so the light accent is
  **#2f6699** (5.29:1 on window, 4.77:1 on selection).

| Variant | Derived from | Used as | Contrast (governing surface) |
|---|---|---|---|
| #2f6699 | EDM Blue #4687c7 darkened | accent, light themes; TipDialog tile; Edge disc; Ocean toolbar top | 5.29:1 on #f0f0f0; 4.77:1 on select #d6e7f6; white-on 6.03:1; bulb #ffd54a 4.27:1 |
| #5a97dc | EDM Blue #4687c7 lightened | accent, dark themes | 4.85:1 on panel #26282c; white-on 3.05:1 (RTL check-box tick) |
| #2b7439 | EDM Green #54ab64 darkened | ok text, light themes; Forest toolbar top | 5.03:1 on #f0f0f0; white-on 5.74:1 |
| #3fa34f | EDM Green #54ab64 darkened slightly | ok text, dark themes | 4.61:1 on panel #26282c; white-on 3.20:1 (BrowserCard badge arrow — raw #54ab64 gives 2.84:1) |
| #2e8b47 | EDM Green #54ab64 darkened | ok/add **fills** (icons, Chrome disc) | ≥3.46:1 on light and dark toolbars; white-on 4.29:1 |
| #9f3535 | EDM Red dark variant (given) | err text, light themes; Sunset toolbar top | 6.07:1 on #f0f0f0; white-on 6.92:1 |
| #e5736b | EDM Red #c2453f lightened | err text, dark themes | 4.92:1 on panel #26282c (raw #c2453f gives 2.96:1) |
| #c94b44 | EDM Red #c2453f lightened slightly | stop/delete **fills** (icons) | ≥3.23:1 on light and dark toolbars; white-on 4.59:1 |
| #c2453f | EDM Red, raw | Opera browser disc | white globe 4.98:1 |
| #c85a00 | amber family (warn hue) | Firefox browser disc | white globe 4.27:1 |
| #d63f14 | orange-red | Brave browser disc | white globe 4.58:1 |

Text needs ≥ 4.5:1, non-text graphics ≥ 3:1. A fill and a text colour
in the same role are deliberately different values: a text colour
must pass 4.5:1 on a light window, a fill must pass 3:1 on both the
light and the dark toolbar, and no single green/red satisfies both —
hence ok-text #2b7439 vs ok-fill #2e8b47, err-text #9f3535 vs
err-fill #c94b44.

## What changed

- `Theme.accent` is now **one EDM Blue value per light/dark pair**
  (#2f6699 / #5a97dc) in all nine themes. The per-theme accents are
  gone (Ocean #1565c0, Forest #146c43, Sunset #b23a1a, Rose #c23d6e,
  Midnight #a78bfa, Teal #2ec4b6, …): blue is the download role, so
  the download colour no longer changes with the theme. The theme
  now lives in the window / panel / toolbar / header backgrounds.
  **Decision for the maintainer:** if he wants the per-theme accent hues back,
  this is one array to revert.
- `Theme.ok` → EDM Green family (#2b7439 / #3fa34f), `Theme.err` →
  EDM Red family (#9f3535 / #e5736b), `Theme.warn` unchanged (amber).
- `Theme.select` / `select-line` / `hover` are unified to the EDM
  Blue family too (light: #d6e7f6 / #93bfe6 / #eaf2f9; dark: #23405f
  / #3a6ea5 / #1f2c3a), so a selected row reads the same in every
  theme. `Theme.toolbar-hover` is deliberately **unchanged**
  (per-theme hover tint for toolbar buttons only, not a role colour).
- Toolbar stops: Light keeps near-white stops (its labels are dark
  text — navy stops would break them; only the bottom moved,
  #e7ebf0 → #e4e9f0, a navy-grey tint). Dark themes move toward EDM
  Navy; their tops are kept at luminance ≤ 0.021 so every toolbar
  icon fill still measures ≥ 3:1 against the top stop (Dark #1f2836,
  Midnight #202634, Teal #182830, Black #1d283a). Saturated themes
  use the AA-safe dark variant of their palette hue (Ocean #2f6699 →
  #2b5f8f, Forest #2b7439 → #235e2e, Sunset #9f3535 → #8c2f2f,
  Rose #b03a4e → #9c3244). Header stops are pale navy tints on light
  themes and navy-shifted on dark themes; saturated themes keep a
  pale tint of their hue.
- Toolbar icons (`Icon`): add circle / resume triangle / batch badge /
  playlist tile / start-queue play #23934a / #2e9e4f / #1e9e4c →
  green fill #2e8b47; grabber globe #3c8bf5, all three download badges
  #1f6feb, options knobs / scheduler ring #3b82f6 → EDM Blue #4687c7
  (white arrow on a badge: 3.78:1); torrent magnet #e33c5f, stop-all
  front #da443b, delete X / limiter needle / stop-queue square
  #e0443b → red fill #c94b44; stop-all back square #f2a7a2 → #f0b3ad
  (a tint of the new red); delete-done check #3fcf7a → #2e8b47.
  Kept: amber pause bars #d97d00, yellow folder (documented
  exception, 1.79:1 — a yellow folder is light by nature, edge
  carried by its stroke), find magnifier (neutral), warn/bulb/folder
  glyphs. File-type icons (`TypeIcon`) are **unchanged** — their
  colours are per-type identity, not theme; out of this pass's scope.
- TipDialog bulb tile #2563eb → #2f6699 (bulb #ffd54a: 4.27:1, was
  3.66:1).
- BrowserCard discs (`crates/app/src/browser.rs`, both cfg blocks):
  Chrome #1fa463 → #2e8b47, Edge #0c8cd8 → #2f6699, Firefox #e86a10
  → #c85a00 (amber-orange — orange is not in the palette, amber is
  its nearest functional hue), Opera #d61f2b → #c2453f, Brave
  #fb542b → #d63f14. All five stay distinct (green / blue /
  amber-orange / red / orange-red); white globe ≥ 4.27:1 on each.
  The badge on the disc is `Theme.ok` (green = add), unchanged in
  role. **Judgment call for the maintainer:** Firefox/Brave were retinted
  away from their brand oranges; reverting is five lines per block.
- DetailsDialog / list-row bars need no code change: the main fill
  is `Theme.ok` (now EDM Green: 4.60:1 light / 3.73:1 dark against
  `bar-bg`), the second/step bar is `Theme.warn` (amber, unchanged:
  4.75:1 light / 4.44:1 dark). Segment states reuse the same tokens
  (downloading = accent, done = ok, stalled = warn, idle = muted).
- ClipDialog: progress fill is `Theme.accent` (now EDM Blue); its
  track changed from `Theme.muted` to `Theme.bar-bg`, the standard
  track every other bar uses (muted-as-track was the odd one out).
  The buttons are the standard Slint `Button`s — their primary
  colour comes from Slint's `Palette`, not from `Theme` tokens, so
  no token change applies to them. ClipDialog has no step label or
  step bar; its "step colours" are the DetailsDialog/list-row second
  bars above.

## New palettes (v1.1.2)

| # | Theme | Window | Panel | Text | Muted | Accent | Toolbar top → bottom |
|---|---|---|---|---|---|---|---|
| 0 | Light | #f0f0f0 | #ffffff | #1b1f24 | #5f6b78 | #2f6699 | #fdfdfd → #e4e9f0 |
| 1 | Dark | #1e1f22 | #26282c | #e8eaed | #9aa0a6 | #5a97dc | #1f2836 → #1d283a |
| 2 | Ocean blue | #e8f1fb | #ffffff | #13243a | #4f6a8a | #2f6699 | #2f6699 → #2b5f8f |
| 3 | Midnight purple | #15131d | #1d1a29 | #ece9f6 | #a59fc0 | #5a97dc | #202634 → #1d283a |
| 4 | Forest green | #eef5ef | #ffffff | #15281b | #4f7359 | #2f6699 | #2b7439 → #235e2e |
| 5 | Black | #000000 | #0b0b0b | #f2f2f2 | #9a9a9a | #5a97dc | #1d283a → #0a0f18 |
| 6 | Sunset | #fbf1ea | #ffffff | #2e1a12 | #8a6352 | #2f6699 | #9f3535 → #8c2f2f |
| 7 | Teal night | #0f1c1e | #142527 | #e3f4f3 | #8fb3b0 | #5a97dc | #182830 → #16242c |
| 8 | Rose | #fbf0f4 | #ffffff | #2e1420 | #8a5a6c | #2f6699 | #b03a4e → #9c3244 |

| Role | Light themes (0, 2, 4, 6, 8) | Dark themes (1, 3, 5, 7) |
|---|---|---|
| ok | #2b7439 | #3fa34f |
| warn | #8a5a00 | #e08a00 |
| err | #9f3535 | #e5736b |
| select | #d6e7f6 | #23405f |
| select-line | #93bfe6 | #3a6ea5 |
| hover | #eaf2f9 | #1f2c3a |

## Measured contrast, v1.1.2 (WCAG 2.2, relative luminance)

Every pair below is normal text on its background unless noted;
all pass 4.5:1.

| Theme | text/win | text/panel | muted/win | muted/panel | accent/win | accent/panel | ok/win | ok/panel | err/win | err/panel | warn/win | warn/panel |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Light | 14.53 | 16.56 | 4.77 | 5.44 | 5.29 | 6.03 | 5.03 | 5.74 | 6.07 | 6.92 | 5.20 | 5.93 |
| Dark | 13.67 | 12.25 | 6.24 | 5.59 | 5.41 | 4.85 | 5.15 | 4.61 | 5.49 | 4.92 | 6.12 | 5.49 |
| Ocean blue | 13.72 | 15.66 | 4.89 | 5.58 | 5.29 | 6.03 | 5.03 | 5.74 | 6.07 | 6.92 | 5.19 | 5.93 |
| Midnight purple | 15.36 | 14.25 | 7.28 | 6.75 | 6.03 | 5.60 | 5.74 | 5.33 | 6.12 | 5.68 | 6.83 | 6.34 |
| Forest green | 14.02 | 15.54 | 4.82 | 5.34 | 5.44 | 6.03 | 5.18 | 5.74 | 6.24 | 6.92 | 5.35 | 5.93 |
| Black | 18.76 | 17.58 | 7.46 | 6.99 | 6.89 | 6.46 | 6.56 | 6.15 | 7.00 | 6.56 | 7.80 | 7.32 |
| Sunset | 14.83 | 16.50 | 4.73 | 5.27 | 5.42 | 6.03 | 5.16 | 5.74 | 6.22 | 6.92 | 5.33 | 5.93 |
| Teal night | 15.35 | 13.98 | 7.67 | 6.98 | 5.72 | 5.21 | 5.45 | 4.96 | 5.81 | 5.29 | 6.48 | 5.90 |
| Rose | 15.26 | 16.98 | 5.02 | 5.59 | 5.42 | 6.03 | 5.16 | 5.74 | 6.22 | 6.92 | 5.33 | 5.93 |

| Theme | toolbar label/top | toolbar label/bottom | text/header-top | text/header-bottom | text/select | text/hover |
|---|---:|---:|---:|---:|---:|---:|---:|
| Light | 16.28 | 13.57 | 15.69 | 14.59 | 13.11 | 14.64 |
| Dark | 12.31 | 12.30 | 10.99 | 12.48 | 8.84 | 11.78 |
| Ocean blue | 6.03 | 6.70 | 13.85 | 12.58 | 12.39 | 13.84 |
| Midnight purple | 12.64 | 12.39 | 11.57 | 13.18 | 8.91 | 11.86 |
| Forest green | 5.74 | 7.74 | 14.43 | 12.96 | 12.30 | 13.74 |
| Black | 13.24 | 17.14 | 15.10 | 16.78 | 9.52 | 12.68 |
| Sunset | 6.92 | 8.19 | 14.75 | 13.22 | 13.06 | 14.59 |
| Teal night | 13.36 | 13.99 | 11.82 | 13.63 | 9.39 | 12.50 |
| Rose | 5.90 | 7.11 | 15.62 | 14.00 | 13.44 | 15.01 |

Non-text (need ≥ 3:1), light / dark themes:

| Element | Light | Dark |
|---|---|---|
| White tick on accent fill (RTL check-box) | 6.03 | 3.05 |
| White arrow on ok fill (BrowserCard badge) | 5.74 | 3.20 |
| Accent fill vs `bar-bg` (running bar, segments) | 4.83 | 3.92 |
| Ok fill vs `bar-bg` (main progress bar) | 4.60 | 3.73 |
| Warn fill vs `bar-bg` (second/step bar) | 4.75 | 4.44 |
| Bulb #ffd54a on TipDialog tile #2f6699 | 4.27 | 4.27 |
| White arrow on download badge #4687c7 | 3.78 | 3.78 |
| White glyph on icon fills (green / blue / red / amber) | 4.29 / 3.78 / 4.59 / 3.05 | same |
| White globe on browser discs (Chrome / Edge / Firefox / Opera / Brave) | 4.29 / 6.03 / 4.27 / 4.98 / 4.58 | same |

Icon fills vs each theme's own toolbar (worst case = top stop; need
≥ 3:1): green #2e8b47 — Light 4.21, Dark 3.46, Midnight 3.53, Teal
3.54, Black 3.46; blue #4687c7 — Light 3.72, Dark 3.92, Midnight
4.00, Teal 4.01, Black 3.92; red #c94b44 — Light 4.51, Dark 3.23,
Midnight 3.29, Teal 3.30, Black 3.23. The saturated themes are the
documented exception below.

Coloured status text on a selected row (queue column is always
accent; status column is ok/err/warn/accent by state): light themes
4.77 (accent) / 4.54 (ok) / 5.48 (err) / 4.69 (warn) — all pass.
Dark themes are the documented exception below.

## What could NOT reach AA — flagged, not silently shipped

1. **Coloured status text on a selected row, dark themes only:
   accent 3.50, ok 3.33, err 3.55, warn 3.96 (need 4.5).**
   Pre-existing in kind (old dark values were 3.12 / 4.21 / 3.05 /
   3.16 — three of the four are better now, ok is worse because the
   badge-arrow constraint forced ok darker). It cannot be fixed
   without breaking something else: the role colours must be light
   enough to read as text on the dark window (≥ 4.5:1) and dark
   enough that a white glyph on the role fill reads ≥ 3:1, and those
   two together forbid 4.5:1 on any usable selection background — a
   selection dark enough for ok-text 4.5:1 would be ~1.14:1 against
   the window, i.e. invisible as a selection. What was done
   instead: the selection background stays visible (1.57:1 against
   the window, plus the select-line border), and the coloured status
   text on a dark selected row stays above 3:1, with its meaning
   also carried by the row's other columns and the progress bar.
   Light themes pass, so this is dark-only.
2. **Icon fills on the saturated-theme toolbars (Ocean / Forest /
   Sunset / Rose): 1.25–2.17:1.** A blue fill on a blue toolbar (or
   green on green, red on red) cannot read by fill-vs-background —
   the toolbar IS the theme colour and the icons keep their role
   colours. Pre-existing, no regression: the old design measured
   the same (globe on Ocean 1.57 → 1.59 now; add on Forest 1.36 →
   1.34; magnet on Sunset 1.38 → 1.51). What carries the meaning
   there instead: the white inner glyph on the fill (3.05–4.59:1)
   and the bold text label under every button (white on those
   stops: 5.74–8.19:1). No new backing disc was added — that would
   change the icon language, not just its colours.
3. **Amber pause bars (#d97d00) on the Light toolbar gradient:
   3.00 (2.999) on the top stop, 2.50 on the bottom.** Unchanged
   from the UIUX pass (it reported 3.00); the Light bottom moved
   #e7ebf0 → #e4e9f0, which moved the bottom number 2.55 → 2.50.
   The amber is kept because it is the functional warn colour, and
   the Stop button carries a bold text label. The yellow folder
   (1.79:1) and silver disc (2.56:1) exceptions from the UIUX pass
   are unchanged.
