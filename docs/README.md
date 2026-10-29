# Website — `docs/` (GitHub Pages)

The project site is served by GitHub Pages from `main`'s `docs/`
directory. It is **framework-free**: vanilla HTML, CSS, and JavaScript
only, **zero build step**, no trackers, no third-party scripts, and no
externally loaded fonts. Every external URL on the site points at
`github.com` (this fork, the upstream repository, or release
downloads) — see the link audit in the Phase 9 gate record in
`docs/PHASES.md`. There are no buttons asking for money anywhere
on the site.

## Files

| File | Purpose |
|---|---|
| `index.html` | All page content (Arabic default; English via `script.js`) |
| `style.css` | Light + dark themes, responsive layout |
| `script.js` | Language/theme persistence, OS detection, copy buttons, download simulator |
| `assets/logo.svg`, `assets/hero-banner.svg` | Brand art (the hero banner SVG is currently unused) |
| `assets/fonts/*.woff2` | Site fonts (see Fonts below) |
| `assets/fonts/OFL-*.txt` | OFL license texts for the site fonts |
| `screenshots/*.png` | App + site screenshots, all captured in Phase 10 (2026-10-28) from the running v1.1.0 release build under Xvfb and from headless Chromium against this tree — see the table below. The three v1.0.7-era shots were replaced; the rest are new. Binary upload via the browser is still pending (see Open Question 15), so the live Pages site shows the old shots until they land. |

### Screenshots (Phase 10)

| File | Size (px) | Bytes | What it shows |
|---|---|---|---|
| `01-main-ar.png` | 962×502 | 68,432 | Main window, dark theme, Arabic RTL, empty list |
| `02-progress-ar.png` | 962×502 | 65,750 | Main window, dark theme, Arabic RTL: a live 48 MB download at 24.54%, 1.01 MB/s, 35 s left |
| `03-add-dialog-ar.png` | 640×488 | 42,318 | Add dialog, dark theme, Arabic RTL: HLS master playlist — duration 00:00:10, 720p quality line (200 kbps video + 32 kbps sampled audio), ≈283.20 KB bitrate estimate |
| `04-main-en-light.png` | 962×502 | 64,096 | Main window, light theme, English LTR, empty list |
| `05-main-en-dark.png` | 962×502 | 64,050 | Main window, dark theme, English LTR, empty list |
| `06-main-ar-light.png` | 962×502 | 68,399 | Main window, light theme, Arabic RTL, empty list |
| `07-add-dialog-dash-ar.png` | 640×488 | 42,220 | Add dialog, dark theme, Arabic RTL: multi-Period DASH manifest — duration 00:01:00, 1080p line (3000 + 128 kbps), ≈22.37 MB bitrate estimate |
| `08-details-ar.png` | 560×572 | 41,282 | Download details/progress window, dark theme, Arabic RTL: 38% of 48 MB at 1.02 MB/s with the per-connection segment bar and legend |
| `09-site-ar-light.png` | 1280×900 | 238,630 | This site, Arabic RTL, light theme (hero still shows the old `01` shot — it refreshes when the new PNGs land) |
| `10-site-en-dark.png` | 1280×900 | 304,732 | This site, English LTR, dark theme |

Only `01`–`03` are referenced by `index.html` (hero + gallery); `04`–`10` are documentation shots kept with the release. The Flatpak metainfo (`packaging/flatpak/io.github.kerlif.EzooDownload.metainfo.xml`, Phase 8 file) still declares 962×502 for `03` — its dimensions must be corrected to 640×488 when the screenshots are uploaded (Open Question 15).

## Bilingual + themes

- Default language Arabic (RTL); the switch swaps **all** content,
  the `lang` and `dir` attributes, and the page title/meta
  description. English is LTR. The choice persists in
  `localStorage` (`ezoo-site-lang`).
- Light + dark themes; the choice persists in `localStorage`
  (`ezoo-site-theme`) and the first visit respects
  `prefers-color-scheme`.
- The interactive download simulator in "How it works" is a
  simulation only (thread slider, file-size select, per-thread
  lanes, speed chart, stats, single-connection comparison,
  auto-start on scroll). It is bilingual and theme-aware; its
  chart colours are read from the CSS variables.

## Download links — approach (a): hardcoded per-release filenames

Download links use
`https://github.com/kerlif/ezoo-download/releases/latest/download/<asset>`
with the **v1.1.0 filenames hardcoded**. The filenames are
versioned (except the Windows setup and the Flatpak bundle, whose
names carry no version), so they are **not** stable across
releases: every release MUST update the filenames in
`index.html` (and the install-guide commands) to the new version.
This is the per-release update step:

1. Read the exact asset filenames from the release workflows /
   packaging scripts (`packaging/linux/build-linux.sh`,
   `packaging/linux/build-extensions.sh`, the Flatpak bundle name
   in `.github/workflows/linux-release.yml`, `EzooDownload-Setup.exe`
   from the Windows workflow).
2. Replace the version in every `releases/latest/download/…` link
   and every install command in `index.html`.
3. Verify the pattern against the previous release (same filenames
   with the old version resolving today) before the new release
   exists, and verify the new links live once the release is
   published.

No stable-name workflow amendment was made (option (b) in the
Phase 9 spec was rejected): duplicating the large binaries under
stable aliases would change the release artifact set and needs CI
proof, while the versioned names are what the workflows already
produce. The v1.1.0 links were pattern-verified against the live
v1.0.7 release (identical filenames with `1.0.7` all resolve with
HTTP 200 today); they 404 until the v1.1.0 release exists, which
is expected — see the gate record in `docs/PHASES.md`.

`version.json` is NOT read from the static site: it is a release
asset consumed by the in-app updater (signed manifest), and
loading it cross-origin from Pages adds nothing the
`/latest/download/` links do not already provide.

## Fonts

The site uses the same families as the app (Phase 3): Carlito
(Latin) and Noto Sans Arabic, as WOFF2 generated locally with
fontTools 4.46.0 + brotli from the byte-identical Phase 3 originals
in `crates/app/ui/fonts/`. Glyph counts were verified identical
after conversion (Carlito 2783, Noto Sans Arabic 1399). Liberation
Serif is not used on the site (no document-style text).

Font files under `docs/assets/fonts/`:

| File | Bytes | SHA-256 |
|---|---|---|
| `Carlito-Regular.woff2` | 189856 | `6b8cada275bac8eff2d5f5bc4ccffa7f06ba208a506bf8466fc191172060f117` |
| `Carlito-Bold.woff2` | 197952 | `e8f1f2ec4b276aa7f7272efaaef3b4c68a3e75329fc3abf34882917250d2ba96` |
| `NotoSansArabic-Regular.woff2` | 74176 | `85bd1ed450edace6e4d136117b378236db3a4cb37687948debb4b2ebb63ffe97` |
| `NotoSansArabic-Bold.woff2` | 77688 | `25815546c6e8ae5ad72e5fba177f7bc07a3186bb3c0760b83d3e2b489d82d5ab` |

**Binary-upload status: ESCALATED, pending.** Binary files cannot
be pushed through the GitHub connector (UTF-8 only), so the
`.woff2` files are not in the pushed tree until they are uploaded
through the browser. The CSS declares a robust system-font fallback
stack (`"Segoe UI", Tahoma, Arial, sans-serif`), so the site renders
correctly with or without the font files; `@font-face` with
`font-display: swap` means text is never invisible while fonts load.
The OFL license texts (`OFL-Carlito.txt`, `OFL-NotoSansArabic.txt`)
are plain text, are in the tree, and are linked from the License
section. No font is hotlinked from Google Fonts or any CDN.

## Contrast ratios (WCAG 2.1, computed from the hex values in `style.css`)

Normal text needs ≥ 4.5:1. All key pairs pass in both themes:

| Pair | Light | Dark |
|---|---:|---:|
| Body text / background | 17.46 | 16.09 |
| Body text / panel | 17.46 | 14.42 |
| Muted text / background | 6.79 | 8.68 |
| Muted text / panel | 6.79 | 7.78 |
| Link / background | 6.70 | 10.12 |
| White / button gradient (both ends) | 7.10 / 6.70 | 7.10 / 6.70 |
| Version badge text / badge background | 7.10 | 9.04 |
| Code text / code background | 14.58 | 16.21 |
| Simulator result / result background | 8.64 | 12.42 |
| Step number / step background | 17.46 | 16.09 |

Focus is always visible (`:focus-visible`, 3 px outline), sections
use semantic landmarks with a skip link, and every image carries
meaningful alt text in both languages.

## Performance / Lighthouse — PENDING (see below)

Lighthouse could **not** be run in the Phase 9 environment, and no
scores are claimed: the available Chromium blocks navigation to
loopback addresses (`localhost` / `127.0.0.1` are refused with a
"blocked" interstitial, and `/etc/hosts` is read-only so no alias
hostname could be added), while Lighthouse rejects `file://` URLs
(`INVALID_URL`). The substitute evidence, all measured 2026-10-28:

- **axe-core 4.14.0** (jsdom, WCAG 2.0 A/AA rules, both languages):
  **0 violations, 27 passes** in Arabic and in English. One rule
  stayed `incomplete` in jsdom (color-contrast on the off-screen
  skip link, which jsdom cannot lay out) — covered by the computed
  ratios in the table above (skip link: 17.46:1 light, 16.09:1 dark).
  One genuine finding (an `aria-labelledby` on a generic `<div>`)
  was fixed and the audit re-run clean.
- **Headless-Chromium pass** (real browser, `file://` origin):
  zero page exceptions after a cache-bypassing reload; language
  switch swaps all 162 keyed strings, `lang`/`dir`, and the title;
  theme toggle swaps palettes and persists; OS detection reorders
  the matching card; the simulator auto-starts on scroll and its
  lanes/chart/stats update; keyboard Tab order starts at the skip
  link and proceeds in DOM order; copy buttons and FAQ keyboard
  operation verified.
- **Budget audit** (exact file sizes): HTML 37,860 B + CSS 17,473 B
  + JS 56,347 B + screenshots 212,615 B + WOFF2 fonts 539,672 B =
  863,967 B total on disk. One render-blocking stylesheet, one
  deferred script, zero third-party requests (no `fetch`/`XHR`/
  beacon anywhere in `script.js`; every `href`/`src`/`url()` is
  local or `github.com`).

**Follow-up:** run Lighthouse (accessibility + performance) against
the live Pages URL once the branch merges to `main`, in both
languages, and record the scores here. Phase 10 (2026-10-28)
re-captured the site from this tree in headless Chromium (Arabic
light + English dark, viewport 1280×900, `localStorage` presets
applied exactly as the site's own toggles persist them) and both
renders were inspected visually with no layout defects; that
re-capture does not replace the Lighthouse measurement, which is
still pending. Until then Phase 9 is
recorded as PASSED-WITH-EXCEPTION in `docs/PHASES.md`.

## Credits

MIT, © 2026 Ahmed Ezzat (Ezoo Tech). This fork:
`kerlif/ezoo-download`; upstream:
`AhmedEzo686/ezoo-download`. All copyright notices and credits are
kept; no license terms were changed.
