#!/usr/bin/env python3
"""Bring a piece of Octavia's art onto the palette (docs/design/colors.css).

Each source colour measured in the art is moved onto its palette colour;
pixels in between move by a blend of their nearest anchors, so antialiased
edges stay smooth. Stray specks in the transparent area are removed (only
the biggest opaque shape and its soft edge are kept).

usage: scripts/recolor.py docs/design/logo.png docs/design/bow.png
writes NAME.palette.png next to each; the originals are never touched.
"""
import re, sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = Path(__file__).resolve().parent.parent
CSS = (ROOT / "docs/design/colors.css").read_text()
pal = lambda name: re.search(rf"--{name}:\s*(#[0-9A-Fa-f]{{6}})", CSS)[1]
rgb = lambda h: np.array([int(h[i:i + 2], 16) for i in (1, 3, 5)], float)

# measured in logo.png and bow.png (2026-11-25) → the palette role it plays
ANCHORS = [
    ("#36212E", "mane-outline"),     # the logo's ink
    ("#3D2134", "mane-outline"),     # the bow's outline
    ("#DECEE8", "collar"),           # the sticker border
    ("#DE6FC3", "bow-tie"),          # the bow's pink
    ("#7C3266", "bow-tie-outline"),  # the bow's creases
]


def recolor(path):
    im = np.array(Image.open(path).convert("RGBA")).astype(float)
    px, alpha = im[..., :3], im[..., 3]
    src = np.array([rgb(s) for s, _ in ANCHORS])
    dst = np.array([rgb(pal(d)) for _, d in ANCHORS])
    # inverse-distance blend of the anchors' shifts
    d = np.linalg.norm(px[..., None, :] - src, axis=-1) + 1e-3
    w = 1 / d ** 4
    w /= w.sum(-1, keepdims=True)
    px = np.clip(px + (w[..., None] * (dst - src)).sum(-2), 0, 255)
    # keep the biggest opaque shape and its soft edge, drop the specks
    solid, n = ndimage.label(alpha > 128)
    if n:
        sizes = ndimage.sum(np.ones_like(alpha), solid, range(1, n + 1))
        keep = ndimage.binary_dilation(solid == 1 + int(np.argmax(sizes)), iterations=3)
        alpha = alpha * keep
    out = np.dstack([px, alpha]).round().astype(np.uint8)
    dest = path.with_name(path.stem + ".palette.png")
    Image.fromarray(out, "RGBA").save(dest)
    print(f"{path.name} → {dest.name}")


if __name__ == "__main__":
    for p in sys.argv[1:]:
        recolor(Path(p))
