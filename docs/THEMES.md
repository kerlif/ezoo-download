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
