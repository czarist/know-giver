"""Builds the Know Giver app icon from the owner's engraving of Thoth as a
baboon under the crescent moon (crates/ui/assets/art/thoth-baboon.png).

    python tools/icon.py

The drawing itself is not changed: only the white around it becomes the
jade ground, and the tile gets rounded corners. Writes, in crates/ui/assets:
icon.png (rounded, the app icon) and icon-square.png (square, cover plates).
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ASSETS = Path(__file__).resolve().parent.parent / "crates/ui/assets"
ART = ASSETS / "art/thoth-baboon.png"

SIZE = 512
RADIUS = 112
JADE = (63, 143, 124)
# How far from white still counts as the paper around the drawing.
PAPER_TOLERANCE = 28


def background_mask(art: Image.Image) -> Image.Image:
    """White reachable from the edges: the paper around the figure, not the
    light parts inside it (the face, the moon)."""
    marked = art.convert("RGB").copy()
    w, h = marked.size
    border = [(x, 0) for x in range(w)] + [(0, y) for y in range(h)] + [(w - 1, y) for y in range(h)]
    for xy in border:
        if min(marked.getpixel(xy)) >= 255 - PAPER_TOLERANCE:
            ImageDraw.floodfill(marked, xy, (255, 0, 255), thresh=PAPER_TOLERANCE)
    r, g, b = marked.split()
    magenta = ImageChops.multiply(
        r.point(lambda v: 255 if v == 255 else 0),
        ImageChops.multiply(b.point(lambda v: 255 if v == 255 else 0), g.point(lambda v: 255 if v == 0 else 0)),
    )
    # A soft edge, so the fur blends into the ground.
    return magenta.filter(ImageFilter.GaussianBlur(0.6))


def build(rounded: bool) -> Image.Image:
    art = Image.open(ART).convert("RGBA")
    flat = Image.new("RGB", art.size, "white")
    flat.paste(art, mask=art.split()[3])
    mask = background_mask(flat)
    ground = Image.new("RGB", art.size, JADE)
    figure = Image.composite(ground, flat, mask)

    # Fit the width, sitting on the bottom edge (the fur is cut there).
    scale = SIZE * 0.96 / figure.width
    figure = figure.resize((round(figure.width * scale), round(figure.height * scale)), Image.LANCZOS)
    tile = Image.new("RGB", (SIZE, SIZE), JADE)
    tile.paste(figure, ((SIZE - figure.width) // 2, SIZE - figure.height))

    out = tile.convert("RGBA")
    if rounded:
        corners = Image.new("L", (SIZE * 4, SIZE * 4), 0)
        ImageDraw.Draw(corners).rounded_rectangle((0, 0, SIZE * 4 - 1, SIZE * 4 - 1), RADIUS * 4, fill=255)
        out.putalpha(corners.resize((SIZE, SIZE), Image.LANCZOS))
    return out


for name, rounded in (("icon.png", True), ("icon-square.png", False)):
    build(rounded).save(ASSETS / name)
    print(f"wrote {ASSETS / name}")
