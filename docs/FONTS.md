# Fonts — v1.1.0 (Phase 3)

How Ezoo Download's interface fonts are bundled, registered, and
licensed. The program embeds its fonts in the binary and registers
them in memory at startup; it never depends on system fonts, and it
never writes fonts to the user's font folder any more.

## Bundled fonts

| File | Family / weight | Role |
|---|---|---|
| `crates/app/ui/fonts/Carlito-Regular.ttf` | Carlito, 400 | Latin UI text |
| `crates/app/ui/fonts/Carlito-Bold.ttf` | Carlito, 700 | Latin UI text, bold |
| `crates/app/ui/fonts/NotoSansArabic-Regular.ttf` | Noto Sans Arabic, 400 | Arabic UI text |
| `crates/app/ui/fonts/NotoSansArabic-Bold.ttf` | Noto Sans Arabic, 700 | Arabic UI text, bold |
| `crates/app/ui/fonts/LiberationSerif-Regular.ttf` | Liberation Serif, 400 | Document-style text only (user font choice) |
| `crates/app/ui/fonts/LiberationSerif-Bold.ttf` | Liberation Serif, 700 | Document-style text only, bold |

All six files are static TrueType (no `fvar` table), shipped
unmodified. Name-table dumps (IDs 0/1/2/4/13/14) and SHA-256 hashes
are in §Sources below; the same checks run as unit tests in
`crates/app/src/fonts.rs` (`bundled_fonts_carry_family_weight_and_ofl_notice`).

No italic faces ship: the UI uses no italic text (no `font-style:
italic` anywhere in `crates/app/ui/*.slint`), so Carlito Italic /
BoldItalic were not fetched.

## Why Noto Sans Arabic, not Cairo

The spec default was "Cairo OR Noto Sans Arabic". Cairo was checked
first and rejected on evidence, per open question 5 (see
`docs/OPEN-QUESTIONS.md`, resolved item 3):

- `google/fonts` ships Cairo only as the variable font
  `ofl/cairo/Cairo[slnt,wght].ttf` — no static instances.
- Upstream `Gue3bara/Cairo` ships only `fonts/Cairo/variable/` (a
  single `Cairo[slnt,wght].ttf`); its build config
  (`sources/cairo.yaml`) is "variable font build only (no static
  builds)".
- Generating static instances from the variable font would create a
  modified font, which the OFL Reserved Font Name rules restrict —
  so no instances were generated.

Noto Sans Arabic ships unmodified static instances upstream: release
`NotoSansArabic-v2.013` from `notofonts/arabic` contains
`hinted/ttf/NotoSansArabic-Regular.ttf` and `-Bold.ttf`. Those are the
files bundled here. No font file was modified, so no Reserved Font
Name question arises for any bundled family (Carlito declares RFN
"Carlito", Liberation declares RFN "Liberation", Noto Sans Arabic
declares none in its OFL header — all shipped byte-identical to
upstream).

One consequence of the choice: Noto Sans Arabic covers Arabic only —
its cmap has no Latin A–Z glyphs (verified: 256 Arabic-block
codepoints, 0 A–Z). Arabic UI text that contains Latin (program name,
URLs, file names) therefore depends on the Latin script fallback to
Carlito wired in `fonts::register()` (see §Mechanism). The no-font
screenshots in §Verification prove mixed text renders.

## Mechanism

Slint 1.18.1 does not use fontdb; text shaping goes through
parley/fontique (vendored sources read in Phase 3:
`i-slint-core/.../textlayout/sharedparley.rs`,
`i-slint-common/.../sharedfontique.rs`,
`fontique-0.11.1/src/collection/mod.rs`).

- `crates/app/src/fonts.rs::setup()` runs first in `main()`. It calls
  `slint::fontique_011::shared_collection()` (feature
  `unstable-fontique-011` in `crates/app/Cargo.toml`) and registers
  the six embedded fonts with `Collection::register_fonts`. The
  collection is the shared one Slint's shaper reads
  (`create_collection(true)` in `i-slint-core/.../context.rs`), so
  registration before any window exists is the path Slint's own
  `fontique_shared_collection` test covers.
- `register_fonts` alone does **not** add script fallbacks: fontique
  only falls back to system fonts (via fontconfig) unless fallbacks
  are appended explicitly. So `register()` appends:
  - Arabic script (`Arab`) → Noto Sans Arabic family,
  - Latin script (`Latn`) → Carlito family,
  - generic families `SansSerif` and `SystemUi` → Carlito, then Noto
    Sans Arabic, appended after the system families, so the bundled
    fonts answer only where no system font does.
- The UI font family is a single name per window
  (`Theme.font-family` → `default-font-family` in
  `crates/app/ui/common.slint`): `Look::of` resolves the stored
  choice with `fonts::resolve_font_family`, which maps an empty or
  unknown choice to the bundled default — Noto Sans Arabic for
  Arabic, Carlito otherwise (`default_family` in `main.rs`). No
  font-family chain is set in the `.slint` theme: Slint builds the
  stack as [named family, SansSerif, SystemUi], and the script
  fallbacks above cover the rest. Liberation Serif is never a
  default; it is only a user choice in the font dropdown.
- No in-memory registration existed before: v1.0.7 embedded the
  fonts with `include_bytes!` and installed them into the per-user
  font folder at first run, relying on the OS font system to pick
  them up. That mechanism is gone.

## Removed fonts and migration

- `InstaArabi-Regular.ttf`: removed. No licence could be found for
  it (see `docs/OPEN-QUESTIONS.md`, resolved item 1): name table
  "All Rights Reserved", no licence IDs, no published source.
- `TimesNewRoman-Regular.ttf` / `TimesNewRoman-Bold.ttf`: removed as
  files and replaced with pristine upstream Liberation Serif 2.1.5
  files under their own family name. The old files were Liberation
  Serif with the family name edited to "Times New Roman" (name-table
  ID 3 even says "Times New Roman alias of Liberation Serif", and
  the files are 40/96 bytes larger than the pristine builds) — the
  rename impersonated Microsoft's family name and is not kept.
- Saved settings migrate on load: `fonts::migrate_font_family` maps
  `"InstaArabi"` → `"Noto Sans Arabic"` and `"Times New Roman"` →
  `"Liberation Serif"`. `main()` persists the migrated value with
  `Manager::set_settings` right after the manager starts, and
  `Look::of` / the Options preview resolve through the same mapping,
  so a stale value can never reach the UI.
- Already-installed copies are deleted at startup by
  `fonts::cleanup_legacy()`: the three legacy filenames in the
  per-user font folder (`$XDG_DATA_HOME/fonts/Ezoo/` on Linux,
  `%LOCALAPPDATA%\Microsoft\Windows\Fonts` on Windows) plus, on
  Windows, the two registry values the old installer wrote
  (`"InstaArabi (TrueType)"`, `"TimesNewRoman (TrueType)"` under
  `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`).
  Best-effort, failures ignored; only those filenames, only that
  folder. The Options font dropdown now lists: Default, Noto Sans
  Arabic, Carlito, Tahoma, Arial, Calibri, Verdana, Liberation
  Serif, Courier New (same order as `FONTS` in `main.rs`).

## Licensing

All three families are SIL Open Font License 1.1. The exact
copyright lines, captured from each font's `OFL.txt` at fetch time:

- Carlito: `Copyright 2013 The Carlito Project Authors
  (https://github.com/googlefonts/carlito), with Reserved Font Name
  "Carlito"`
- Noto Sans Arabic: `Copyright 2022 The Noto Project Authors
  (https://github.com/notofonts/arabic)`
- Liberation Serif: `Digitized data copyright (c) 2010 Google
  Corporation` / `Copyright (c) 2012 Red Hat, Inc., with Reserved
  Font Name Liberation.`

Compliance entries:

- `THIRD-PARTY-NOTICES.txt`: one entry per family (copyright line
  verbatim, OFL-1.1, source URL). Previously it had no font entry
  at all.
- `crates/app/ui/fonts/FONTS-LICENSE.txt`: rewritten — lists the
  bundled fonts, their copyright lines, and points at the full OFL
  texts. No claim that InstaArabi ships remains.
- `LICENSE-DEB`: the old `TimesNewRoman-*.ttf` stanza is replaced
  with one OFL-1.1 stanza per bundled family, each pointing at its
  `OFL-*.txt`. The MIT `Files: *` stanza is unchanged; all credits
  to Ahmed Ezzat / Ezoo Tech and upstream are kept.
- The full OFL texts ship verbatim in the packages:
  `OFL-Carlito.txt`, `OFL-NotoSansArabic.txt`,
  `OFL-LiberationSerif.txt` in `crates/app/ui/fonts/`; Linux
  packages copy them to `/usr/share/doc/ezoo-download/` from
  `packaging/linux/build-linux.sh` (verified in a real `.deb`
  listing, see §Verification); the Windows installer lists them in
  `installer/ezoo.iss` (`[Files]`, skipped when absent) — the three
  `Copy-Item` lines that stage them in
  `.github/workflows/windows-release.yml` still need a browser-side
  workflow edit, escalated to the coordinator (exact diff in the
  Phase 3 report). The fonts themselves need no package file
  entries: they are embedded in the binary.

## Sources

Fetched 2026-10-28 from authoritative upstream sources only.
"Commit" is the source-tree state the file came from.

| File | Source URL | Commit / tag | SHA-256 | Bytes |
|---|---|---|---|---|
| Carlito-Regular.ttf | https://raw.githubusercontent.com/googlefonts/carlito/main/fonts/ttf/Carlito-Regular.ttf | main @ 3a810cab78ebd6e2e4eed42af9e8453c4f9b850a | f6418f708baede9789daef5d458c0f53d2a888af9820e8062934e504fedc6595 | 628032 |
| Carlito-Bold.ttf | https://raw.githubusercontent.com/googlefonts/carlito/main/fonts/ttf/Carlito-Bold.ttf | main @ 3a810cab78ebd6e2e4eed42af9e8453c4f9b850a | bb5d20f79b82599ec72983597437373a80f2d2085fa91fc144fd74e876a594db | 682468 |
| NotoSansArabic-Regular.ttf | https://github.com/notofonts/arabic/releases/download/NotoSansArabic-v2.013/NotoSansArabic-v2.013.zip (`NotoSansArabic/hinted/ttf/`) | tag NotoSansArabic-v2.013 | bdff3e5659d67e67def05b33f749683b9376ae819d65d3dd62ac4640b3aaef48 | 234892 |
| NotoSansArabic-Bold.ttf | same release archive | tag NotoSansArabic-v2.013 | 4e5462d2e8be880317b9f49b5b2da109ddb6a3563d91cc604b67f3535832a555 | 261460 |
| LiberationSerif-Regular.ttf | http://deb.debian.org/debian/pool/main/f/fonts-liberation/fonts-liberation_2.1.5-3_all.deb (extracted from the .deb; built from upstream tag 2.1.5) | upstream 2.1.5 | 9caef765d2e891c10dd73658894f01e660fe0c1c83e0bda7d6edf561d8f623d4 | 393692 |
| LiberationSerif-Bold.ttf | same .deb | upstream 2.1.5 | 443a4dbd694b6e4689925947b456e0532e10b19c90fa32eb39ea6c2cd9db9ebd | 370196 |

Name-table dumps (via fontTools 4.46.0; IDs 0/1/2/4/13/14; all six
files: no `fvar` table, GSUB present):

- Carlito Regular v1.104, ID 0 "Copyright 2013 The Carlito Project
  Authors (https://github.com/googlefonts/carlito)", ID 13 OFL 1.1
  statement, ID 14 `https://scripts.sil.org/OFL`, OS/2 weight 400.
- Carlito Bold v1.104, same copyright/licence IDs, weight 700.
- Noto Sans Arabic Regular v2.013, ID 0 "Copyright 2022 The Noto
  Project Authors (https://github.com/notofonts/arabic)", ID 13 OFL
  1.1 statement, ID 14 `https://openfontlicense.org`, weight 400,
  cmap: 256 Arabic-block codepoints, 0 Latin A–Z.
- Noto Sans Arabic Bold v2.013, same copyright/licence IDs, weight
  700.
- Liberation Serif Regular v2.1.5, ID 0 "Digitized data copyright
  (c) 2010 Google Corporation. Copyright (c) 2012 Red Hat, Inc.",
  ID 13 "Licensed under the SIL Open Font License, Version 1.1",
  ID 14 `http://scripts.sil.org/OFL`, weight 400.
- Liberation Serif Bold v2.1.5, same copyright/licence IDs, weight
  700.

OFL texts shipped verbatim (SHA-256): OFL-Carlito.txt
`58402f82…990a00` (from `googlefonts/carlito` main, OFL.txt at the
same commit), OFL-NotoSansArabic.txt `a7a5a25e…ee13af` (the
`OFL.txt` inside the v2.013 release archive; byte-identical to the
`notofonts/arabic` main OFL.txt), OFL-LiberationSerif.txt
`93fed460…61c19b` (the `LICENSE` file at
`liberationfonts/liberation-fonts` tag 2.1.5).

Cairo evidence (rejected): `google/fonts` `ofl/cairo/` contains only
`Cairo[slnt,wght].ttf` (variable) + OFL.txt; `Gue3bara/Cairo`
`fonts/Cairo/` contains only `variable/Cairo[slnt,wght].ttf`
(599,548 bytes); the upstream build config is variable-only. Cairo
OFL header (for the record, not shipped): `Copyright 2009 The Cairo
Project Authors (https://github.com/Gue3bara/Cairo)`.

## Verification

Runs of 2026-10-28, release binary `target/release/EzooDownload`
(44,513,016 bytes; Phase 2 binary was 42,717,560 — the growth is the
six embedded fonts minus the three removed ones). Harness: Xvfb
`:99 -screen 0 1280x800x24`, no window manager, screenshots with
`ffmpeg -f x11grab`. Each run used a fresh `HOME` with seeded
`settings.json` (`~/.local/share/ezoodownload/settings.json`):
language ar/en, `extension_prompted: true`, `show_tips: false`,
`last_version: "1.1.0"`, `autostart_set: true`,
`clipboard_monitor: false`. This VM lacks
`libxkbcommon-x11.so.0`/`libxcb-xkb.so.1`, without which winit cannot
start on X11 at all; the matching Ubuntu 1.6.0/1.17.0 packages were
extracted to `/tmp/xkb/ext` and put on `LD_LIBRARY_PATH` for the
runs only (no system change). Note: `debian/control` lists
`libxkbcommon0` but not `libxkbcommon-x11-0` — flagged for Phase 8;
a minimal install would hit the same winit failure.

### No-font render test (Phase 3 gate) — literal variant BLOCKED upstream

The literal gate (fontconfig pointed at an empty directory,
`fc-list` showing empty) could not run to a screenshot: with zero
fonts visible to fontconfig, the program panics at startup inside
Slint's own platform initialization, before any application font
code runs:

```
thread 'main' panicked at fontique-0.11.1/src/backend/fontconfig.rs:685:61:
called `Result::unwrap()` on an `Err` value: NoMatch
```

Backtrace (debug build, `RUST_BACKTRACE=1`): `main` →
`fonts::setup` → `shared_collection` → `with_global_context` →
`set_platform` → `SlintContext::new` → `create_collection` →
`fontique::Collection::new` → `Inner::new` → `System::new` →
`SystemFonts::new` → `config.font_sort(&pattern, true).unwrap()`.
`SystemFonts::new` eagerly builds the generic-family map and
unwraps `FcFontSort`, which returns `NoMatch` when fontconfig has
no fonts. The panicking code runs for every Slint 1.18.1
application at platform creation (the same `SlintContext::new`
path is taken by the first `Window::new`); no application-level
code can avoid it short of patching the pinned fontique. Recorded
as open question 6 in `docs/OPEN-QUESTIONS.md`. The literal
empty-directory variant is therefore reported as FAILED (upstream
blocker), not passed.

The substitute test below proves the requirement's intent — the UI
renders entirely from the bundled fonts, with no usable system
font — and is the evidence the phase rests on.

### Substitute test: exactly one unusable system font

fontconfig was pointed at a directory containing a single font:
`ZZZNoCoverage`, DejaVu Sans subsetted with fontTools to the single
codepoint U+2603 (SNOWMAN) and renamed, 3,900 bytes; its cmap was
verified to contain only `0x2603`. `fc-list` under the test config
showed exactly one entry
(`/tmp/ezoo-font-test/oneglyph/OneGlyph.ttf: ZZZNoCoverage`), vs
1,875 under the normal config. That font cannot render any UI text
(no Latin, no Arabic, no digits, no punctuation).

Four runs: Arabic + English, each under the one-glyph config and
under the normal config. Results:

- Arabic, one-glyph: full RTL main window renders — menus, toolbar,
  column headers, categories, empty-state text, status bar — with
  correctly shaped, connected Arabic text, no tofu/boxes, no
  clipping at the tested size. Mixed Latin ("Main download queue")
  renders via the Carlito fallback. Screenshot SHA-256
  `d6b88fe5…54d91`.
- Arabic, normal fonts: screenshot SHA-256 identical
  (`d6b88fe5…54d91`) — pixel-identical to the one-glyph run.
- English, one-glyph: full LTR main window renders in Carlito, no
  tofu, no clipping of text (see the toolbar note below).
  Screenshot SHA-256 `687e9e23…9b685`.
- English, normal fonts: screenshot SHA-256 identical
  (`687e9e23…9b685`) — pixel-identical to the one-glyph run.

Pixel-identical rendering with zero usable system fonts proves
every UI glyph comes from the bundled fonts. The screenshots were
inspected visually (Arabic shaping/RTL/Latin checked by eye); they
are kept in `/tmp/ezoo-font-test/` (harness + shots + logs), not
committed as product screenshots — Phase 10 owns those.

### Migration and legacy cleanup, verified in the same runs

- Arabic runs seeded `font_family: "InstaArabi"`: after the run,
  `settings.json` contained `"Noto Sans Arabic"` — the migration
  persists (English runs kept `""`, the bundled-default choice).
- Legacy per-user copies planted before the Arabic one-glyph run
  (`$XDG_DATA_HOME/fonts/Ezoo/InstaArabi-Regular.ttf` and
  `TimesNewRoman-Regular.ttf`, taken from the v1.0.7 tree) were
  both deleted by the run, and the empty `Ezoo` directory was
  removed.

### Packaging check

A real `.deb` was built with
`SKIP_CARGO_BUILD=1 SKIP_FFMPEG=1 bash packaging/linux/build-linux.sh
deb` (ffmpeg skipped: it is fetched at packaging time and is
unrelated to fonts; `umask 022` was needed because this VM's
default umask makes `dpkg-deb` reject the control directory).
`dpkg-deb -c` lists in `/usr/share/doc/ezoo-download/`:
`OFL-Carlito.txt`, `OFL-NotoSansArabic.txt`,
`OFL-LiberationSerif.txt`, `THIRD-PARTY-NOTICES.txt`, `README.md`.
The `.rpm` spec (`%files`) and the AppImage staging use the same
staged tree wholesale (`/usr/share/doc/ezoo-download/`), so the
OFL files ship in all three Linux formats by the same code path;
the `.rpm`/AppImage were not built here. The Windows installer
file list was checked by reading `installer/ezoo.iss` only — the
OFL staging lines in the Windows workflow are escalated, and
Windows rendering was not verified on this VM (CI must confirm).

### Observations (not defects)

- The bold black X in the middle of every screenshot is the Xvfb
  mouse pointer parked at screen center, captured by x11grab — not
  a glyph. It sits at the same position in all four shots.
- In the English shot, the toolbar's filter box is clipped at the
  window's right edge at the default window size (the toolbar row
  is wider than the window). This is layout overflow, not a font
  failure: Carlito is metric-compatible with Calibri and narrower
  than the system default the old build used on Linux, so the font
  change cannot have caused it. Flagged for Phase 6 (UI), not
  fixed here.
- No window manager was running, so window placement/size is
  winit's default; no Wayland compositor exists in this
  environment, so all rendering verification is X11-only.

### Limits of the test

- The literal zero-font variant panics upstream (see above); the
  substitute uses one font with zero UI coverage. fontconfig could
  not see any other font path in the substitute runs: `HOME`,
  `XDG_DATA_HOME`, `XDG_CONFIG_HOME` were fresh temp dirs and
  `FONTCONFIG_FILE` replaced the config wholesale.
- Only the main window was screenshotted (both locales). Dialogs
  use the same `Theme.font-family` machinery but were not opened
  on camera; the font dropdown itself was verified by code +
  translation-table review, not by screenshot.
- Windows: compile of the Windows-only cleanup code could not run
  here (no mingw toolchain; `cargo check --target
  x86_64-pc-windows-gnu` fails in `cc-rs` for a missing C
  compiler). The registry/file calls were verified against the
  vendored `windows-sys` 0.59 signatures instead, and mirror the
  previously CI-verified registry code shapes. CI must confirm.
- The software-renderer fallback path was not exercised (the Xvfb
  runs used the femtovg/OpenGL renderer).
