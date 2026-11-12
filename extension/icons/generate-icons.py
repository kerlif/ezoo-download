#!/usr/bin/env python3
"""Render the capture icon (capture-icon.svg) to the toolbar PNGs.

Pillow only — no rsvg/inkscape in the build VM. The geometry mirrors
capture-icon.svg exactly: the EDM mark from the supplied logo
(2026-11-05) — dark-navy tile (#253149 -> #1D283A), EDM letters
(E #4687C7, D #54AB64 with a download-arrow stem, M #C2453F) plus the
small GitHub badge bottom-right.

SMALL-SIZE SIMPLIFICATION (branding pass, 2026-11-05): 16/32 px use the
simplified mark — navy tile + the GREEN D-ARROW ONLY. The full EDM
wordmark is illegible below ~32 px; the D-arrow carries the download
meaning alone. 48/128 use the full mark.

Renders at 512px, then downscales with Lanczos so the small sizes stay
crisp. Usage: python3 generate-icons.py   (writes icon16/32/48/128.png here)
"""
from PIL import Image, ImageDraw

NAVY_DARK = (0x1D, 0x28, 0x3A)
NAVY = (0x25, 0x31, 0x49)
BLUE = (0x46, 0x87, 0xC7)
GREEN = (0x54, 0xAB, 0x64)
RED = (0xC2, 0x45, 0x3F)
WHITE = (255, 255, 255)
BADGE_BG = (0xB8, 0xBB, 0xC2)  # light-grey badge circle (as supplied)
SIZES = (16, 32, 48, 128)
RENDER = 512


def _quad(p0, c, p1, n=24):
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (1 - t) ** 2 * p0[0] + 2 * (1 - t) * t * c[0] + t ** 2 * p1[0]
        y = (1 - t) ** 2 * p0[1] + 2 * (1 - t) * t * c[1] + t ** 2 * p1[1]
        pts.append((x, y))
    return pts


# Full mark (512 box) — same coordinates as capture-icon.svg / ezoo-download.svg
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

# Simplified mark (512 box): big green D-arrow, centred
S_SHAFT = [(150, 110), (194, 110), (194, 340), (150, 340)]
S_HEAD = [(128, 330), (216, 330), (172, 386)]
S_OUTER = ([(194, 110), (300, 110)] + _quad((300, 110), (382, 110), (382, 256))
           + _quad((382, 256), (382, 402), (300, 402)) + [(194, 402)])
S_INNER = ([(238, 154), (296, 154)] + _quad((296, 154), (338, 154), (338, 256))
           + _quad((338, 256), (338, 358), (296, 358)) + [(238, 358)])


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
    md.rounded_rectangle([0, 0, size - 1, size - 1],
                         radius=int(round(size * 82 / 512)), fill=255)
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(grad, (0, 0), mask)
    sheen = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    sp = sheen.load()
    for y in range(size):
        a = max(0.0, 0.10 * (1 - y / (0.38 * size)))
        for x in range(size):
            sp[x, y] = (255, 255, 255, int(round(a * 255)))
    return Image.alpha_composite(out, sheen)


def _draw_full(d, u):
    for (x0, y0, x1, y1) in E_RECTS:
        d.rounded_rectangle([x0 * u, y0 * u, x1 * u, y1 * u], radius=8 * u,
                            fill=BLUE + (255,))
    d.polygon([(x * u, y * u) for x, y in E_BOTTOM], fill=BLUE + (255,))
    d.polygon([(x * u, y * u) for x, y in D_OUTER], fill=GREEN + (255,))
    # counter cut-out BEFORE shaft/head: in the SVG the counter belongs to
    # the bowl path only, so the arrowhead is not notched by it
    d.polygon([(x * u, y * u) for x, y in D_INNER], fill=(0, 0, 0, 0))
    d.polygon([(x * u, y * u) for x, y in D_SHAFT], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in D_HEAD], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in M_LEFT], fill=RED + (255,))
    d.polygon([(x * u, y * u) for x, y in M_RIGHT], fill=RED + (255,))
    d.rectangle([M_STEM[0] * u, M_STEM[1] * u, M_STEM[2] * u, M_STEM[3] * u],
                fill=RED + (255,))
    d.polygon([(x * u, y * u) for x, y in M_FOOT], fill=RED + (255,))
    # GitHub badge: light-grey circle, dark octocat silhouette
    d.ellipse([382 * u, 383 * u, 468 * u, 469 * u], fill=BADGE_BG + (255,))
    d.polygon([(408 * u, 404 * u), (413 * u, 384 * u), (422 * u, 402 * u)],
              fill=NAVY_DARK + (255,))
    d.polygon([(428 * u, 402 * u), (437 * u, 384 * u), (442 * u, 404 * u)],
              fill=NAVY_DARK + (255,))
    d.ellipse([405 * u, 400 * u, 445 * u, 440 * u], fill=NAVY_DARK + (255,))
    d.ellipse([411 * u, 440 * u, 439 * u, 464 * u], fill=NAVY_DARK + (255,))
    d.polygon([(404 * u, 448 * u), (392 * u, 444 * u), (394 * u, 458 * u),
               (406 * u, 456 * u)], fill=NAVY_DARK + (255,))


def _draw_simple(d, u):
    d.polygon([(x * u, y * u) for x, y in S_OUTER], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in S_INNER], fill=(0, 0, 0, 0))
    d.polygon([(x * u, y * u) for x, y in S_SHAFT], fill=GREEN + (255,))
    d.polygon([(x * u, y * u) for x, y in S_HEAD], fill=GREEN + (255,))


def render(simple):
    s = RENDER
    img = _tile_base(s)
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    (_draw_simple if simple else _draw_full)(d, s / 512.0)
    return Image.alpha_composite(img, layer)


def main():
    for n in SIZES:
        simple = n <= 32
        out = render(simple).resize((n, n), Image.LANCZOS)
        out.save(f"icon{n}.png")
        print(f"icon{n}.png  {n}x{n}  {'simplified D-arrow' if simple else 'full EDM mark'}")


if __name__ == "__main__":
    main()
