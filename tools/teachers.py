"""Builds the lesson teachers from DiceBear's "Lorelei" style.

The design is "Lorelei" by Lisa Wischofsky, CC0 1.0, served by the DiceBear
API. For each teacher this script downloads the portrait with different
mouths and eyes, keeps the shared paths as `base.svg` and writes the varying
paths as fragments, so the app can animate speech and blinking by swapping
two small pieces of SVG.

Run from the repository root: python tools/teachers.py
"""

import re
import time
import urllib.request
from pathlib import Path

API = "https://api.dicebear.com/9.x/lorelei/svg"
OUT = Path("crates/ui/assets/teachers")

COMMON = {
    "seed": "know-giver",
    "frecklesProbability": "0",
    "hairAccessoriesProbability": "0",
    "beardProbability": "0",
    # Skin in the paper color of the card (see PAPER in crates/ui/src/teacher.rs).
    "skinColor": "f6f3ec",
}

TEACHERS = {
    "helena": dict(hair="variant38", eyes="variant10", eyebrows="variant05",
                   glasses="variant04", glassesProbability="100",
                   earringsProbability="0", head="variant02", nose="variant01"),
    "lia": dict(hair="variant35", eyes="variant12", eyebrows="variant06",
                glassesProbability="0", earrings="variant02",
                earringsProbability="100", head="variant01", nose="variant03"),
    "rafael": dict(hair="variant03", eyes="variant10", eyebrows="variant10",
                   glassesProbability="0", earringsProbability="0",
                   head="variant04", nose="variant04"),
    "otavio": dict(hair="variant25", eyes="variant24", eyebrows="variant12",
                   glasses="variant01", glassesProbability="100",
                   earringsProbability="0", head="variant04", nose="variant06"),
}

# Rest, slightly open, open, wide open.
MOUTHS = ["happy01", "happy06", "happy05", "sad01"]
CLOSED_EYES = "variant19"


def fetch(params: dict) -> str:
    query = "&".join(f"{k}={v}" for k, v in params.items())
    for attempt in range(5):
        try:
            with urllib.request.urlopen(f"{API}?{query}", timeout=30) as r:
                return r.read().decode("utf-8")
        except OSError:
            time.sleep(1 + attempt)
    raise SystemExit(f"could not download {query}")


def paths(svg: str) -> tuple[str, list[str]]:
    """The opening of the drawing group and its paths, without metadata."""
    match = re.search(r'<g transform="([^"]+)">(.*?)</g>', svg, re.S)
    if not match:
        raise SystemExit("unexpected SVG layout")
    return match.group(1), re.findall(r"<path [^>]*/>", match.group(2))


def differing(a: list[str], b: list[str]) -> list[int]:
    if len(a) != len(b):
        raise SystemExit("variants have different layers")
    return [i for i, (x, y) in enumerate(zip(a, b)) if x != y]


def build(name: str, options: dict) -> None:
    params = {**COMMON, **options}
    variants = {}
    for mouth in MOUTHS:
        for eyes in (options["eyes"], CLOSED_EYES):
            transform, layers = paths(fetch({**params, "mouth": mouth, "eyes": eyes}))
            variants[(mouth, eyes)] = layers
            time.sleep(0.3)
    rest = variants[(MOUTHS[0], options["eyes"])]
    mouth_at = sorted({i for m in MOUTHS[1:] for i in differing(rest, variants[(m, options["eyes"])])})
    eyes_at = differing(rest, variants[(MOUTHS[0], CLOSED_EYES)])
    if not mouth_at or not eyes_at or set(mouth_at) & set(eyes_at):
        raise SystemExit(f"{name}: cannot separate mouth {mouth_at} and eyes {eyes_at}")

    body = []
    for i, layer in enumerate(rest):
        if i == eyes_at[0]:
            body.append("<!--eyes-->")
        if i == mouth_at[0]:
            body.append("<!--mouth-->")
        if i not in mouth_at and i not in eyes_at:
            body.append(layer)
    base = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 980 980" fill="none">'
        f'<g transform="{transform}">' + "".join(body) + "</g></svg>\n"
    )
    folder = OUT / name
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "base.svg").write_text(base, encoding="utf-8", newline="\n")
    for label, eyes in (("open", options["eyes"]), ("closed", CLOSED_EYES)):
        layers = variants[(MOUTHS[0], eyes)]
        (folder / f"eyes-{label}.svg").write_text(
            "".join(layers[i] for i in eyes_at) + "\n", encoding="utf-8", newline="\n")
    for n, mouth in enumerate(MOUTHS):
        layers = variants[(mouth, options["eyes"])]
        (folder / f"mouth-{n}.svg").write_text(
            "".join(layers[i] for i in mouth_at) + "\n", encoding="utf-8", newline="\n")
    print(name, "layers", len(rest), "mouth", mouth_at, "eyes", eyes_at)


if __name__ == "__main__":
    for name, options in TEACHERS.items():
        build(name, options)
