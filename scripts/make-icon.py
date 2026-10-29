"""Draws the app icon: the wordmark's "c" in Doto, with the signal square as
its full stop, on ftor's near-black over a faint dot grid. Writes a 1024px
PNG that `tauri icon` turns into every size, and next to it the menu bar
icons (tray.png, tray-recording.png).

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

    font = ImageFont.truetype(str(FONT), 620 * SCALE)
    font.set_variation_by_axes([0, 900])  # roundness, weight: as .wordmark renders
    glyph = Image.new("L", (N, N), 0)
    ImageDraw.Draw(glyph).text((0, 0), "c", font=font, fill=255)
    left, top, right, bottom = glyph.getbbox()  # the ink, not the em box

    # "c." as a group, centred in the canvas; the square sits on the baseline.
    side, gap = 64 * SCALE, 24 * SCALE
    x = (N - (right - left + gap + side)) // 2 - left
    y = (N - (bottom - top)) // 2 - top
    tile.paste(FOREGROUND, (x, y, x + N, y + N), glyph)
    dot_x, dot_y = x + right + gap, y + bottom - side
    ImageDraw.Draw(tile).rectangle([dot_x, dot_y, dot_x + side - 1, dot_y + side - 1], fill=SIGNAL)

    out = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    out.paste(tile, (0, 0), mask)
    return out.resize((SIZE, SIZE), Image.LANCZOS)


# The "c" as Doto draws it, one dot per cell.
C = [".###.", "#...#", "#....", "#...#", ".###."]


def tray(recording: bool) -> Image.Image:
    """Menu bar icon, at 2x of macOS's 18pt height. Black on transparent: macOS uses it
    as a template and tints it for the menu bar. The square, the full stop, only shows
    while recording."""
    pitch, dot = 6, 5
    width, height = 2 + 6 * pitch + 2, 36
    out = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    draw = ImageDraw.Draw(out)
    cells = [(col, row) for row, line in enumerate(C) for col, c in enumerate(line) if c == "#"]
    if recording:
        cells.append((5, 4))
    top = (height - (len(C) * pitch - 1)) // 2
    for col, row in cells:
        x, y = 2 + col * pitch, top + row * pitch
        draw.rectangle([x, y, x + dot - 1, y + dot - 1], fill=(0, 0, 0, 255))
    return out


if __name__ == "__main__":
    out = Path(sys.argv[1])
    icon().save(out)
    tray(recording=False).save(out.with_name("tray.png"))
    tray(recording=True).save(out.with_name("tray-recording.png"))
