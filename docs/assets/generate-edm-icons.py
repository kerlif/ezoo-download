#!/usr/bin/env python3
"""Branding pass part A — EDM v1.1.2 logo assets (2026-11-05).

Run from the repo root:  python3 docs/assets/generate-edm-icons.py
Regenerates the master crop, SVGs, hicolor/extension/app rasters, ICO,
banners, style.css retint and the small-icon comparison strip.

Source of truth: the supplied logo JPEG
  ~/workspace/user/files/1000150869_0_lg8p.jpg  (1529x2048)
Tile crop box (measured): (178, 420, 1351, 1597).

Canonical palette (mode-sampled from the JPEG, +/-2 JPEG noise):
  NAVY_DARK  #1D283A   NAVY  #253149   PAGE  #363940
  BLUE #4687C7  GREEN #54AB64  RED #C2453F (dark #9F3535)
  WHITE #FFFFFF  GREY #B0B3B8  BADGE_GREY #B8BBC2

What this script does (run from the repo root):
  1. crops the master tile -> docs/assets/edm-tile-master.png
  2. writes hand-built SVGs (no base64 JPEG anywhere):
       packaging/linux/hicolor/scalable/apps/ezoo-download.svg (full mark)
       packaging/linux/hicolor/scalable/apps/ezoo-download-tray.svg (full mark)
       packaging/linux/hicolor/scalable/apps/ezoo-download-simple.svg (D-arrow)
       assets/logo.svg, docs/assets/logo.svg (full mark tile)
       extension/icons/capture-icon.svg (full mark, 16 viewBox)
  3. regenerates the Linux hicolor PNGs (Pillow Lanczos from the tile crop;
     16/22/24 use the simplified D-arrow mark, see below)
  4. regenerates extension/icons/icon{16,32,48,128}.png
     (16/32 simplified, 48/128 full)
  5. regenerates app rasters: crates/app/ui/icon.png (256, padded bbox
     7,7,249,249), icon-window.png (256, padded bbox 36,36,220,220),
     capture-icon.png (128, bbox 1,1,127,127), tray32.rgba (32, full tile)
     and crates/app/icon.ico (multi-size ICO, PNG entries:
     16 simplified, 32/48/256 full)
  6. recolours banners (assets/banner-light.svg, assets/banner-dark.svg,
     docs/assets/hero-banner.svg) to the new palette + new tile
  7. retints docs/style.css accent variables to the palette
  8. comparison strip docs/screenshots/branding-small-icons.png

SMALL-SIZE SIMPLIFICATION (decided 2026-11-05, branding pass):
  16/22/24 px (app + tray) and extension 16/32 use the simplified mark =
  navy tile + the GREEN D-ARROW ONLY (D letterform whose stem is a
  download arrow, #54AB64). No E/M, no GitHub badge. Rationale: the full
  EDM wordmark is illegible below ~32 px; the D-arrow carries the
  download meaning alone.

No Zajil/FluxDown material. No AI-generated logo: only the supplied JPEG + hand-built
derivatives. Ahmed Ezzat's credits are untouched by this script.
"""
import os, re, struct, sys
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")) \
    if os.path.basename(os.path.dirname(os.path.abspath(__file__))) == "assets" \
    else os.getcwd()
# When run from the repo root, ROOT is the cwd; the docs/assets copy sets it above.
SRC_JPG = "/home/hatch/workspace/user/files/1000150869_0_lg8p.jpg"
TILE_BOX = (178, 420, 1351, 1597)

NAVY_DARK = (0x1D, 0x28, 0x3A)
NAVY = (0x25, 0x31, 0x49)
BLUE = (0x46, 0x87, 0xC7)
GREEN = (0x54, 0xAB, 0x64)
RED = (0xC2, 0x45, 0x3F)
WHITE = (255, 255, 255)

# ---------------------------------------------------------------- geometry
# Full EDM mark, 512 viewBox. Letters y 179..346 (from the source JPEG,
# scaled: tile 178..1350 x, 420..1596 y -> 512 box).
TILE_RX = 82

def _quad(p0, c, p1, n=24):
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t ** 2 * p1[0]
        y = (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t ** 2 * p1[1]
        pts.append((x, y))
    return pts

# D bowl (full mark): outer outline + counter, as sampled polygons.
D_OUTER = ([(178, 179), (252, 179)] + _quad((252, 179), (323, 179), (323, 262))
           + _quad((323, 262), (323, 346), (252, 346)) + [(178, 346)])
D_INNER = ([(210, 213), (258, 213)] + _quad((258, 213), (292, 213), (292, 262))
           + _quad((292, 262), (292, 311), (258, 311)) + [(210, 311)])
D_SHAFT = [(178, 179), (210, 179), (210, 275), (178, 275)]
D_HEAD = [(158, 272), (230, 272), (193, 330)]
M_LEFT = [(300, 179), (338, 179), (382, 308)]
M_RIGHT = [(414, 179), (450, 179), (382, 308)]
M_STEM = (432, 179, 464, 346)  # x0,y0,x1,y1
M_FOOT = [(325, 310), (297, 346), (329, 346)]
E_RECTS = [(48, 179, 80, 346), (48, 179, 163, 213), (48, 245, 140, 279)]  # x0,y0,x1,y1
E_BOTTOM = [(48, 312), (170, 312), (186, 346), (48, 346)]

# Simplified mark (512 box): big green D-arrow, centred.
S_SHAFT = [(150, 110), (194, 110), (194, 340), (150, 340)]
S_HEAD = [(128, 330), (216, 330), (172, 386)]
S_OUTER = ([(194, 110), (300, 110)] + _quad((300, 110), (382, 110), (382, 256))
           + _quad((382, 256), (382, 402), (300, 402)) + [(194, 402)])
S_INNER = ([(238, 154), (296, 154)] + _quad((296, 154), (338, 154), (338, 256))
           + _quad((338, 256), (338, 358), (296, 358)) + [(238, 358)])

# ---------------------------------------------------------------- SVG text
MARK_FULL = """  <!-- tile: navy two-tone, left #253149 | right #1D283A (hard split) -->
  <rect width="512" height="512" rx="82" fill="url(#edm-bg)"/>
  <rect width="512" height="512" rx="82" fill="url(#edm-sheen)"/>
  <!-- E #4687C7 (bottom bar ends in a diagonal slant) -->
  <g fill="#4687C7">
    <rect x="48" y="179" width="32" height="167" rx="8"/>
    <rect x="48" y="179" width="115" height="34" rx="8"/>
    <rect x="48" y="245" width="92" height="34" rx="8"/>
    <polygon points="48,312 170,312 186,346 48,346"/>
  </g>
  <!-- D #54AB64: bowl + download-arrow stem (tip stops short of the base) -->
  <g fill="#54AB64">
    <rect x="178" y="179" width="32" height="96"/>
    <polygon points="158,272 230,272 193,330"/>
    <path fill-rule="evenodd" d="M178 179 H252 Q323 179 323 262 Q323 346 252 346 H178 Z M210 213 H258 Q292 213 292 262 Q292 311 258 311 H210 Z"/>
  </g>
  <!-- M #C2453F: slanted left leg, V point, vertical right stem, foot triangle -->
  <g fill="#C2453F">
    <polygon points="300,179 338,179 382,308"/>
    <polygon points="414,179 450,179 382,308"/>
    <rect x="432" y="179" width="32" height="167"/>
    <polygon points="325,310 297,346 329,346"/>
  </g>
  <!-- GitHub badge: light-grey circle, dark octocat silhouette (as supplied) -->
  <g>
    <circle cx="425" cy="426" r="43" fill="#B8BBC2"/>
    <polygon fill="#1D283A" points="408,404 413,384 422,402"/>
    <polygon fill="#1D283A" points="428,402 437,384 442,404"/>
    <circle cx="425" cy="420" r="20" fill="#1D283A"/>
    <ellipse cx="425" cy="452" rx="14" ry="12" fill="#1D283A"/>
    <path d="M404 452 q-14 -2 -12 10" fill="none" stroke="#1D283A" stroke-width="5" stroke-linecap="round"/>
  </g>"""

MARK_SIMPLE = """  <!-- tile: navy gradient, left #253149 -> right #1D283A -->
  <rect width="512" height="512" rx="82" fill="url(#edm-bg)"/>
  <rect width="512" height="512" rx="82" fill="url(#edm-sheen)"/>
  <!-- simplified small-size mark: green D-arrow only (#54AB64) -->
  <g fill="#54AB64">
    <rect x="150" y="110" width="44" height="230"/>
    <polygon points="128,330 216,330 172,386"/>
    <path fill-rule="evenodd" d="M194 110 H300 Q382 110 382 256 Q382 402 300 402 H194 Z M238 154 H296 Q338 154 338 256 Q338 358 296 358 H238 Z"/>
  </g>"""

DEFS = """  <defs>
    <linearGradient id="edm-bg" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#253149"/>
      <stop offset="0.5" stop-color="#253149"/>
      <stop offset="0.5" stop-color="#1D283A"/>
      <stop offset="1" stop-color="#1D283A"/>
    </linearGradient>
    <linearGradient id="edm-sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity=".10"/>
      <stop offset=".38" stop-color="#ffffff" stop-opacity="0"/>
    </linearGradient>
  </defs>"""

def svg_doc(mark, label, w=512, h=512):
    return f"""<svg width="{w}" height="{h}" viewBox="0 0 512 512" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="{label}">
  <!-- Ezoo Download Manager (EDM) — hand-built vector of the supplied
       logo (2026-11-05). Dark-navy tile, EDM letters: E #4687C7,
       D #54AB64 with download-arrow stem, M #C2453F. No embedded raster. -->
{DEFS}
{mark}
</svg>
"""

# ---------------------------------------------------------------- Pillow
def _tile_base(size):
    """Navy horizontal-gradient tile, rounded corners, transparent outside."""
    # hard two-tone split at the middle, as in the supplied logo
    grad = Image.new("RGB", (256, 1))
    px = grad.load()
    for x in range(256):
        px[x, 0] = NAVY if x < 128 else NAVY_DARK
    grad = grad.resize((size, size), Image.LANCZOS).convert("RGBA")
    mask = Image.new("L", (size, size), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle([0, 0, size - 1, size - 1], radius=int(round(size * 82 / 512)), fill=255)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(grad, (0, 0), mask)
    # subtle top sheen
    sheen = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    sp = sheen.load()
    for y in range(size):
        a = max(0.0, 0.10 * (1 - y / (0.38 * size)))
        for x in range(size):
            sp[x, y] = (255, 255, 255, int(round(a * 255)))
    out = Image.alpha_composite(out, sheen)
    return out

def _poly_layer(size, draw_fn):
    layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    draw_fn(d, size / 512.0)
    return layer

def _draw_full(d, u):
    for (x0, y0, x1, y1) in E_RECTS:
        d.rounded_rectangle([x0 * u, y0 * u, x1 * u, y1 * u], radius=8 * u, fill=BLUE + (255,))
    d.polygon([(x * u, y * u) for x, y in E_BOTTOM], fill=BLUE + (255,))
    d.polygon([(x * u, y * u) for x, y in D_OUTER], fill=GREEN + (255,))
    # counter cut-out BEFORE shaft/head: in the SVG the counter belongs to
    # the bowl path only, so the arrowhead is not notched by it
    d.polygon([(x * u, y * u) for x, y in D_INNER], fill=(0, 0, 0, 0))
    d.polygon([(x * u, y * u) for x, y in D_SHAFT], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in D_HEAD], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in M_LEFT], fill=RED + (255,))
    d.polygon([(x * u, y * u) for x, y in M_RIGHT], fill=RED + (255,))
    d.rectangle([M_STEM[0] * u, M_STEM[1] * u, M_STEM[2] * u, M_STEM[3] * u], fill=RED + (255,))
    d.polygon([(x * u, y * u) for x, y in M_FOOT], fill=RED + (255,))
    # GitHub badge: light-grey circle, dark octocat silhouette
    d.ellipse([382 * u, 383 * u, 468 * u, 469 * u], fill=(0xB8, 0xBB, 0xC2, 255))
    d.polygon([(408 * u, 404 * u), (413 * u, 384 * u), (422 * u, 402 * u)], fill=NAVY_DARK + (255,))
    d.polygon([(428 * u, 402 * u), (437 * u, 384 * u), (442 * u, 404 * u)], fill=NAVY_DARK + (255,))
    d.ellipse([405 * u, 400 * u, 445 * u, 440 * u], fill=NAVY_DARK + (255,))
    d.ellipse([411 * u, 440 * u, 439 * u, 464 * u], fill=NAVY_DARK + (255,))
    d.polygon([(404 * u, 448 * u), (392 * u, 444 * u), (394 * u, 458 * u), (406 * u, 456 * u)],
              fill=NAVY_DARK + (255,))

def _draw_simple(d, u):
    d.polygon([(x * u, y * u) for x, y in S_OUTER], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in S_INNER], fill=(0, 0, 0, 0))
    d.polygon([(x * u, y * u) for x, y in S_SHAFT], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in S_HEAD], fill=GREEN + (255,))

def render_mark(size, simple):
    img = _tile_base(512)
    layer = _poly_layer(512, _draw_simple if simple else _draw_full)
    img = Image.alpha_composite(img, layer)
    return img.resize((size, size), Image.LANCZOS) if size != 512 else img

# ---------------------------------------------------------------- helpers
def w(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    print("wrote", os.path.relpath(path, ROOT))

def main():
    os.chdir(ROOT)
    src = Image.open(SRC_JPG).convert("RGB")
    assert src.size == (1529, 2048), src.size
    master = src.crop(TILE_BOX)  # 1173x1177
    master_path = "docs/assets/edm-tile-master.png"
    master.save(master_path)
    print("master crop", master.size, "->", master_path)

    # 1. SVGs
    full_svg = svg_doc(MARK_FULL, "Ezoo Download Manager")
    simple_svg = svg_doc(MARK_SIMPLE, "Ezoo Download Manager (simplified D-arrow mark)")
    scal = "packaging/linux/hicolor/scalable/apps"
    w(f"{scal}/ezoo-download.svg", full_svg)
    # NOTE: replace ONLY the root <svg> tag's size (count=1 on the tag
    # string). A blanket width/height replace would also shrink the two
    # tile <rect width="512" height="512"> elements — that bug shipped
    # in the first tray SVG and was fixed 2026-11-05.
    tray_svg = full_svg.replace('<svg width="512" height="512"',
                                '<svg width="64" height="64"', 1)
    tray_svg = tray_svg.replace('aria-label="Ezoo Download Manager"',
                                'aria-label="Ezoo Download Manager tray"')
    w(f"{scal}/ezoo-download-tray.svg", tray_svg)
    w(f"{scal}/ezoo-download-simple.svg", simple_svg)
    w("assets/logo.svg", full_svg)
    w("docs/assets/logo.svg", full_svg)
    capture_svg = """<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" role="img" aria-label="Ezoo Download Manager capture icon">
  <!--
    Ezoo Download Manager capture icon — hand-built vector of the supplied
    EDM logo (2026-11-05): navy tile + EDM letters with the
    green D-arrow. Toolbar PNGs (icon16/32/48/128.png) are rendered
    from this geometry with icons/generate-icons.py (Pillow);
    16/32 use the simplified D-arrow-only mark. Do not hand-edit the PNGs.
  -->
  <g transform="scale(0.03125)">
""" + DEFS + "\n" + MARK_FULL + "\n  </g>\n</svg>\n"
    w("extension/icons/capture-icon.svg", capture_svg)

    # 2. hicolor PNGs from the tile crop (full) / simplified Pillow mark
    def from_crop(size):
        return master.resize((size, size), Image.LANCZOS).convert("RGBA")
    for s in (16, 22, 24, 32, 48, 64, 128, 256, 512):
        img = render_mark(s, simple=True) if s < 32 else from_crop(s)
        d = f"packaging/linux/hicolor/{s}x{s}/apps"
        os.makedirs(d, exist_ok=True)
        img.save(f"{d}/ezoo-download.png")
        print(f"hicolor app {s}px {'simple' if s < 32 else 'crop'}")
    for s in (16, 22, 24, 32, 48, 64):
        img = render_mark(s, simple=True) if s < 32 else from_crop(s)
        img.save(f"packaging/linux/hicolor/{s}x{s}/apps/ezoo-download-tray.png")
        print(f"hicolor tray {s}px {'simple' if s < 32 else 'crop'}")

    # 3. extension icons
    for s in (16, 32, 48, 128):
        img = render_mark(s, simple=(s <= 32))
        img.save(f"extension/icons/icon{s}.png")
        print(f"extension icon{s}.png {'simple' if s <= 32 else 'full'}")

    # 4. app rasters (padded-bbox convention from P7)
    def padded(canvas, inner):
        tile = master.resize((inner, inner), Image.LANCZOS).convert("RGBA")
        out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
        o = (canvas - inner) // 2
        out.paste(tile, (o, o), tile)
        return out
    icon = padded(256, 242)
    icon.save("crates/app/ui/icon.png")
    print("icon.png bbox", icon.getbbox(), "(expect (7,7,249,249))")
    iconw = padded(256, 184)
    iconw.save("crates/app/ui/icon-window.png")
    print("icon-window.png bbox", iconw.getbbox(), "(expect (36,36,220,220))")
    cap = padded(128, 126)
    cap.save("crates/app/ui/capture-icon.png")
    print("capture-icon.png bbox", cap.getbbox(), "(expect (1,1,127,127))")
    tray32 = from_crop(32)
    with open("crates/app/ui/tray32.rgba", "wb") as f:
        f.write(tray32.tobytes())
    print("tray32.rgba 32x32 RGBA, full tile")

    # 5. ICO (manual, PNG entries: 16 simplified, 32/48/256 full)
    def png_bytes(im):
        import io
        b = io.BytesIO()
        im.save(b, "PNG")
        return b.getvalue()
    entries = []
    for s, simple in ((16, True), (32, False), (48, False), (256, False)):
        entries.append((s, png_bytes(render_mark(s, simple=simple))))
    out = bytearray(struct.pack("<HHH", 0, 1, len(entries)))
    offset = 6 + 16 * len(entries)
    for s, data in entries:
        out += struct.pack("<BBBBHHII", 0 if s == 256 else s, 0 if s == 256 else s,
                           0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data in entries:
        out += data
    with open("crates/app/icon.ico", "wb") as f:
        f.write(out)
    print("icon.ico", len(out), "bytes; entries", [s for s, _ in entries])

    # 6. comparison strip: full vs simplified at 16 and 32 px
    cell, gap, lab = 256, 24, 34
    W = gap * 5 + cell * 4
    H = lab + cell + gap
    strip = Image.new("RGB", (W, H), (255, 255, 255))
    d = ImageDraw.Draw(strip)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 22)
    except Exception:
        font = ImageFont.load_default()
    panels = [("Full 16px", from_crop(16)), ("Simple 16px", render_mark(16, True)),
              ("Full 32px", from_crop(32)), ("Simple 32px", render_mark(32, True))]
    for i, (label, im) in enumerate(panels):
        x = gap + i * (cell + gap)
        d.text((x + 8, 6), label, fill=(20, 20, 20), font=font)
        strip.paste(im.resize((cell, cell), Image.NEAREST), (x, lab))
    strip.save("docs/screenshots/branding-small-icons.png")
    print("comparison strip -> docs/screenshots/branding-small-icons.png")

    # 7. banners — recolour + new tile (strip stale C2PA manifests: the
    # files are rewritten, so the old signed manifests no longer match)
    tile_g_light = ('<g transform="translate(110 70) scale(0.1875)">\n'
                    + DEFS + "\n" + MARK_FULL + "\n  </g>")
    for path, dark in (("assets/banner-light.svg", False), ("assets/banner-dark.svg", True)):
        txt = open(path, encoding="utf-8").read()
        txt = re.sub(r"<metadata>.*?</metadata>", "", txt, flags=re.S)
        txt = re.sub(r'<g transform="translate\(110 70\)">.*?</g>', tile_g_light, txt, flags=re.S, count=1)
        txt = txt.replace("#2BA59A", "#54AB64").replace("#F5A524", "#C2453F")
        # PRE-EXISTING FIX (2026-11-05): the 56px title overran the segment
        # bars even in Arial metrics (ends x773, bars start x640); 44px ends
        # ~x631 and clears them.
        txt = txt.replace("font: 600 56px", "font: 600 44px")
        if dark:
            txt = txt.replace('fill: #F3F6F8', 'fill: #FFFFFF').replace('fill: #9FB3C8', 'fill: #B0B3B8')
            txt = txt.replace('fill="#0D1B2A"', 'fill="#363940"').replace('fill="#17293C"', 'fill="#1D283A"')
            txt = txt.replace('.track { fill: #1A3046; }', '.track { fill: #253149; }')
        else:
            txt = txt.replace('.title { font: 600 56px "IBM Plex Sans", "Segoe UI", Helvetica, Arial, sans-serif; fill: #0D1B2A; }',
                              '.title { font: 600 56px "IBM Plex Sans", "Segoe UI", Helvetica, Arial, sans-serif; fill: #1D283A; }')
            txt = txt.replace('fill: #4B5D70', 'fill: #363940')
        open(path, "w", encoding="utf-8").write(txt)
        print("recoloured", path)
    hero = open("docs/assets/hero-banner.svg", encoding="utf-8").read()
    hero = hero.replace("#8A57F5", "#253149").replace("#6A5CF6", "#1D283A").replace("#2E7BF6", "#1D283A")
    # PRE-EXISTING FIX (2026-11-05): the 88px hero title overran the right
    # edge even in Arial metrics; 60px fits (ends ~x1040 Arial / ~x1161
    # DejaVu). The tile group MUST carry the edm gradient defs (first pass
    # forgot them, leaving the tile unpainted).
    hero = re.sub(r"<!-- E-arrow mark -->.*?\n  </g>", "<!-- EDM tile mark -->\n  <g transform=\"translate(92 96) scale(0.40625)\">\n" + DEFS + "\n" + MARK_FULL + "\n  </g>", hero, flags=re.S, count=1)
    hero = hero.replace('font-size="88"', 'font-size="60"').replace('font-size="30"', 'font-size="26"')
    open("docs/assets/hero-banner.svg", "w", encoding="utf-8").write(hero)
    print("recoloured docs/assets/hero-banner.svg")

    # 8. docs/style.css accent retint
    css = open("docs/style.css", encoding="utf-8").read()
    repl = [
        ("--violet: #6d28d9;", "--violet: #4687C7;"),
        ("--blue: #1d4ed8;", "--blue: #2E6DA8;"),
        ("--link: #1d4ed8;", "--link: #2E6DA8;"),
        ("--badge-fg: #6d28d9;", "--badge-fg: #4687C7;"),
        ("--focus: #1d4ed8;", "--focus: #2E6DA8;"),
        ("--hero-1: #f3efff;", "--hero-1: #EBF3FA;"),
        ("--hero-2: #eef4ff;", "--hero-2: #E7EFF7;"),
        ("--hero-3: #e8f6ff;", "--hero-3: #E2EDF6;"),
        ("rgba(106, 92, 246, .35)", "rgba(70, 135, 199, .35)"),
        ("--link: #a9bdff;", "--link: #7FB3E0;"),
        ("--badge-fg: #c4b5fd;", "--badge-fg: #8FBFE8;"),
        ("--focus: #a9bdff;", "--focus: #7FB3E0;"),
        ("--hero-1: #232347;", "--hero-1: #1D283A;"),
        ("--hero-2: #1a2140;", "--hero-2: #253149;"),
        ("--hero-3: #14273a;", "--hero-3: #1A2433;"),
    ]
    for a, b in repl:
        css = css.replace(a, b)  # no-op on re-run (idempotent)
    open("docs/style.css", "w", encoding="utf-8").write(css)
    print("retinted docs/style.css")
    print("DONE")

if __name__ == "__main__":
    main()
