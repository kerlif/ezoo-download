#!/usr/bin/env python3
"""Render the capture icon (capture-icon.svg) to the toolbar PNGs.

Pillow only — no rsvg/inkscape in the build VM. The geometry mirrors the
SVG exactly: a 16x16 viewBox, a rounded gradient tile, a white download
arrow and an amber accent dot. Renders at 512px, then downscales with
Lanczos so the small sizes stay crisp.
Usage: python3 generate-icons.py   (writes icon16/32/48/128.png here)
"""
from PIL import Image, ImageDraw

STOPS = [(0.0, (0x8A, 0x57, 0xF5)), (0.5, (0x6A, 0x5C, 0xF6)), (1.0, (0x2E, 0x7B, 0xF6))]
WHITE = (255, 255, 255, 255)
AMBER = (0xF5, 0xA5, 0x24, 255)
SIZES = (16, 32, 48, 128)
RENDER = 512


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradient_color(t):
    for i in range(len(STOPS) - 1):
        t0, c0 = STOPS[i]
        t1, c1 = STOPS[i + 1]
        if t0 <= t <= t1:
            return lerp(c0, c1, 0.0 if t1 == t0 else (t - t0) / (t1 - t0))
    return STOPS[-1][1]


def render():
    s = RENDER
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    # diagonal gradient, clipped to a rounded rect (rx 3.5 of 16)
    grad = Image.new("RGBA", (s, s))
    px = grad.load()
    for y in range(s):
        for x in range(s):
            t = (x + y) / (2.0 * (s - 1))
            px[x, y] = gradient_color(t) + (255,)
    mask = Image.new("L", (s, s), 0)
    md = ImageDraw.Draw(mask)
    md.rounded_rectangle([s * 0.5 / 16, s * 0.5 / 16, s * 15.5 / 16, s * 15.5 / 16],
                         radius=s * 3.5 / 16, fill=255)
    img.paste(grad, (0, 0), mask)
    d = ImageDraw.Draw(img)
    u = s / 16.0  # viewBox units -> pixels
    # white download arrow, stroke 1.7, round caps and joins
    w = int(round(1.7 * u))
    d.line([(8 * u, 4.5 * u), (8 * u, 11 * u)], fill=WHITE, width=w, joint="curve")
    d.line([(4.5 * u, 8.5 * u), (8 * u, 12 * u), (11.5 * u, 8.5 * u)],
           fill=WHITE, width=w, joint="curve")
    # amber accent dot, r 2.1, white ring 0.8
    r = 2.1 * u
    ring = 0.8 * u
    cx, cy = 12.4 * u, 12.4 * u
    d.ellipse([cx - r - ring / 2, cy - r - ring / 2, cx + r + ring / 2, cy + r + ring / 2],
              fill=WHITE)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=AMBER)
    return img


def main():
    base = render()
    for n in SIZES:
        out = base.resize((n, n), Image.LANCZOS)
        out.save(f"icon{n}.png")
        print(f"icon{n}.png  {n}x{n}")


if __name__ == "__main__":
    main()
