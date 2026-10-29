"""Builds the Know Giver app icon from the owner's tile of Thoth as a baboon
under the crescent moon, on jade (crates/ui/assets/art/thoth-tile.png).

    python tools/icon.py

The tile already has its ground and rounded corners; outside the corners it
is black. Writes:
- crates/ui/assets/icon.png: rounded with transparent corners (app icon, README);
- crates/ui/assets/icon-square.png: corners filled with jade (cover plates);
- crates/app/assets/know-giver.ico: the Windows executable icon.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "crates/ui/assets"
TILE = ASSETS / "art/thoth-tile.png"
ICO = ROOT / "crates/app/assets/know-giver.ico"

SIZE = 512
# The tile's own corners are about 20% of its width; cutting a little wider
# leaves none of the black behind them.
RADIUS = 0.235
JADE = (50, 144, 121)
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def corners(size: int) -> Image.Image:
    """White inside the rounded tile, black outside, anti-aliased."""
    big = size * 4
    mask = Image.new("L", (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, big - 1, big - 1), round(big * RADIUS), fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def main() -> None:
    tile = Image.open(TILE).convert("RGB").resize((SIZE, SIZE), Image.LANCZOS)
    mask = corners(SIZE)

    rounded = tile.convert("RGBA")
    rounded.putalpha(mask)
    rounded.save(ASSETS / "icon.png")

    square = Image.composite(tile, Image.new("RGB", (SIZE, SIZE), JADE), mask)
    square.save(ASSETS / "icon-square.png")

    rounded.save(ICO, sizes=ICO_SIZES)
    for path in (ASSETS / "icon.png", ASSETS / "icon-square.png", ICO):
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
