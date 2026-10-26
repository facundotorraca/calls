"""Draws the app icon: the wordmark's "c" in Doto, with the signal square as
its full stop, on ftor's near-black over a faint dot grid. Writes a 1024px
PNG that `tauri icon` turns into every size.

    uv run --no-project --with pillow scripts/make-icon.py <out.png>
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "apps/desktop/src/fonts/Doto.ttf"

SIZE = 1024
SCALE = 4  # supersampling, for clean edges after downscaling
N = SIZE * SCALE

# styles.css tokens, in sRGB.
BACKGROUND = (17, 17, 17)  # --background oklch(0.13 0 0)
FOREGROUND = (236, 236, 236)  # --foreground oklch(0.94 0 0)
SIGNAL = (236, 60, 48)  # --signal oklch(0.62 0.24 27)


def squircle() -> Image.Image:
    """macOS icon grid: an 824px superellipse centred in the 1024px canvas.
    macOS 26 puts icons that don't fill this shape inside a grey plate."""
    mask = Image.new("L", (N, N), 0)
    px = mask.load()
    exp = 5.0
    radius = 824 * SCALE / 2
    centre = N / 2
    for y in range(int(centre - radius), int(centre + radius)):
        dy = abs((y + 0.5 - centre) / radius) ** exp
        if dy > 1:
            continue
        dx = (1 - dy) ** (1 / exp) * radius
        for x in range(int(centre - dx), int(centre + dx)):
            px[x, y] = 255
    return mask


def dot_grid() -> Image.Image:
    """styles.css .dot-grid: 24px cell, foreground at ~10%."""
    layer = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    cell = 24 * SCALE
    r = 1.6 * SCALE
    for y in range(cell // 2, N, cell):
        for x in range(cell // 2, N, cell):
            draw.ellipse([x - r, y - r, x + r, y + r], fill=FOREGROUND + (26,))
    return layer


def icon() -> Image.Image:
    mask = squircle()
    tile = Image.new("RGBA", (N, N), BACKGROUND + (255,))
    tile = Image.alpha_composite(tile, dot_grid())

    draw = ImageDraw.Draw(tile)
    font = ImageFont.truetype(str(FONT), 620 * SCALE)
    font.set_variation_by_axes([0, 900])  # roundness, weight: as .wordmark renders
    centre = N / 2
    draw.text((centre, centre + 10 * SCALE), "c", font=font, fill=FOREGROUND, anchor="mm")

    side = 64 * SCALE
    x, y = centre + 150 * SCALE, centre + 120 * SCALE
    draw.rectangle([x, y, x + side, y + side], fill=SIGNAL)

    out = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    out.paste(tile, (0, 0), mask)
    return out.resize((SIZE, SIZE), Image.LANCZOS)


if __name__ == "__main__":
    icon().save(sys.argv[1])
