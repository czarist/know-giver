"""Builds the challenge sticker art (crates/ui/assets/stickers/<family>.svg).

Original art in the app's identity: jade ground, engraved ink lines, the
crescent moon. Each family of challenges has its own outline and motif, so a
sticker is known at a glance even small. The level inside a family is drawn by
the app (crates/ui/src/stickers.rs) into the marked parts: the rim's colour
(`#RIM000`) and one to three dots, never text.

    python tools/stickers.py
"""

import math
import os

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "ui", "assets", "stickers")

INK = "#1C1B19"
PAPER = "#F3EAD3"
EDGE = "#FBF8F0"
JADE = "#2F6F68"
JADE_D = "#1F4F4A"
JADE_M = "#3A8F78"
JADE_L = "#8CC2B8"
GOLD = "#B0771F"
HIGHLIGHT = "#E7C35A"
RIM = "#RIM000"


def hatch(id_, bg, line, angle=45, gap=3.2, width=1.0):
    return (
        f'<pattern id="{id_}" width="{gap}" height="{gap}" patternUnits="userSpaceOnUse" '
        f'patternTransform="rotate({angle})"><rect width="{gap}" height="{gap}" fill="{bg}"/>'
        f'<line x1="0" y1="0" x2="0" y2="{gap}" stroke="{line}" stroke-width="{width}"/></pattern>'
    )


def polygon(points, cx=64, cy=64, r=58, start=-90):
    pts = []
    for i in range(points):
        a = math.radians(start + 360 * i / points)
        pts.append(f"{cx + r * math.cos(a):.2f},{cy + r * math.sin(a):.2f}")
    return "M" + " L".join(pts) + " Z"


def wavy(cx, cy, r, bumps, depth, steps=8):
    pts = []
    n = bumps * steps
    for i in range(n):
        a = 2 * math.pi * i / n
        rr = r + depth * math.cos(bumps * a)
        pts.append(f"{cx + rr * math.cos(a):.2f},{cy + rr * math.sin(a):.2f}")
    return "M" + " L".join(pts) + " Z"


def star(cx, cy, outer, inner, points, start=-90):
    pts = []
    for i in range(points * 2):
        r = outer if i % 2 == 0 else inner
        a = math.radians(start + 180 * i / points)
        pts.append(f"{cx + r * math.cos(a):.2f},{cy + r * math.sin(a):.2f}")
    return "M" + " L".join(pts) + " Z"


# Outlines in a 128 box, and the y of their lowest point (for the dots).
SHAPES = {
    "circle": ("M6,64 A58,58 0 1,0 122,64 A58,58 0 1,0 6,64 Z", 122),
    "arch": ("M16 120 V60 A48 48 0 0 1 112 60 V120 Z", 120),
    "square": ("M28 6 H100 A22 22 0 0 1 122 28 V100 A22 22 0 0 1 100 122 H28 A22 22 0 0 1 6 100 V28 A22 22 0 0 1 28 6 Z", 122),
    "seal": (wavy(64, 64, 54, 14, 4), 122),
    "hexagon": (polygon(6, r=60, start=-90), 124),
    "shield": ("M64 6 L114 20 V62 C114 94 92 112 64 123 C36 112 14 94 14 62 V20 Z", 123),
    "octagon": (polygon(8, r=60, start=-67.5), 119),
    "temple": ("M64 6 L120 44 V120 H8 V44 Z", 120),
    "card": ("M30 6 H98 A14 14 0 0 1 112 20 V108 A14 14 0 0 1 98 122 H30 A14 14 0 0 1 16 108 V20 A14 14 0 0 1 30 6 Z", 122),
    "diamond": ("M64 4 L124 64 L64 124 L4 64 Z", 124),
    "sun": (star(64, 64, 61, 53, 16), 125),
    "oval": ("M64,4 A50,60 0 1,0 64.01,4 Z", 124),
    "pentagon": (polygon(5, r=62, start=-90), 114),
}

INNER = 0.86


def frame(shape, body, defs=""):
    path, bottom = SHAPES[shape]
    inner = f'transform="translate(64 64) scale({INNER}) translate(-64 -64)"'
    # Halfway between the inner outline and the outer edge, at the bottom.
    band = (bottom + 64 + (bottom - 64) * INNER) / 2
    dots = ""
    for count in (1, 2, 3):
        dots += f"<!--dots{count}-->"
        for i in range(count):
            x = 64 + (i - (count - 1) / 2) * 9
            dots += f'<circle cx="{x:.1f}" cy="{band:.1f}" r="3" fill="{RIM}" stroke="{INK}" stroke-width="1"/>'
        dots += f"<!--/dots{count}-->"
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128">\n'
        "<!-- Know Giver challenge sticker, built by tools/stickers.py. -->\n"
        f'<defs><clipPath id="inside"><path d="{path}" {inner}/></clipPath>'
        + hatch("ground", JADE, JADE_D)
        + defs
        + "</defs>\n"
        f'<path d="{path}" fill="{INK}" opacity="0.16" transform="translate(0,3)"/>\n'
        f'<path d="{path}" fill="{EDGE}" stroke="{INK}" stroke-width="1.5" stroke-linejoin="round"/>\n'
        f'<g clip-path="url(#inside)"><rect width="128" height="128" fill="{JADE}"/>'
        f'<path d="M0 84 Q32 78 64 84 T128 84 V128 H0 Z" fill="url(#ground)"/></g>\n'
        f'<path d="{path}" fill="none" stroke="{INK}" stroke-width="1.6" stroke-linejoin="round" {inner}/>\n'
        f'<!--rim--><path d="{path}" fill="none" stroke="{RIM}" stroke-width="5" {inner}/><!--/rim-->\n'
        f"{body}\n{dots}\n</svg>\n"
    )


def crescent(cx, cy, r_out, ix, iy, r_in, fill=PAPER, stroke=INK, width=1.6):
    """The part of the outer circle outside the inner one."""
    d = math.hypot(ix - cx, iy - cy)
    a = (r_out * r_out - r_in * r_in + d * d) / (2 * d)
    h = math.sqrt(r_out * r_out - a * a)
    ux, uy = (ix - cx) / d, (iy - cy) / d
    mx, my = cx + a * ux, cy + a * uy
    p1 = (mx - h * uy, my + h * ux)
    p2 = (mx + h * uy, my - h * ux)
    return (
        f'<path fill="{fill}" stroke="{stroke}" stroke-width="{width}" stroke-linejoin="round" '
        f'd="M{p1[0]:.2f},{p1[1]:.2f} A{r_out},{r_out} 0 1,1 {p2[0]:.2f},{p2[1]:.2f} '
        f'A{r_in},{r_in} 0 0,0 {p1[0]:.2f},{p1[1]:.2f} Z"/>'
    )


def small_moon(x, y, r=5):
    return crescent(x, y, r, x + r * 0.4, y - r * 0.4, r * 0.82, fill=GOLD, width=1)


def scroll(x=34, y=42, w=60, h=42):
    defs = hatch("roll", PAPER, INK, angle=0, gap=2.6, width=0.9)
    body = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    for i, length in enumerate((42, 38, 42, 27)):
        yy = y + 12 + i * 7
        body += f'<path d="M{x + 9} {yy} H{x + 9 + length}" stroke="{INK}" stroke-width="1.6" stroke-linecap="round"/>'
    body += f'<rect x="{x - 7}" y="{y - 9}" width="{w + 14}" height="12" rx="6" fill="url(#roll)" stroke="{INK}" stroke-width="2"/>'
    body += f'<rect x="{x - 7}" y="{y + h - 3}" width="{w + 14}" height="12" rx="6" fill="url(#roll)" stroke="{INK}" stroke-width="2"/>'
    body += small_moon(x + w - 6, y + 14, 6)
    return body, defs


def calendar():
    body = f'<rect x="32" y="34" width="64" height="58" rx="4" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    body += f'<rect x="32" y="34" width="64" height="14" rx="4" fill="{JADE_L}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="M46 28 V40 M82 28 V40" stroke="{INK}" stroke-width="3" stroke-linecap="round"/>'
    for row in range(3):
        for col in range(5):
            x, y = 40 + col * 12, 58 + row * 11
            fill = GOLD if (row, col) == (2, 3) else JADE_M if row * 5 + col < 13 else EDGE
            body += f'<rect x="{x}" y="{y}" width="7" height="7" fill="{fill}" stroke="{INK}" stroke-width="0.8"/>'
    return body, ""


def quiz_card(rows=2):
    body = f'<rect x="34" y="28" width="60" height="70" rx="4" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    body += f'<rect x="52" y="23" width="24" height="9" rx="3" fill="{JADE_L}" stroke="{INK}" stroke-width="1.6"/>'
    for i in range(rows):
        y = 46 + i * 18
        body += f'<rect x="41" y="{y - 5}" width="10" height="10" fill="{EDGE}" stroke="{INK}" stroke-width="1.4"/>'
        body += f'<path d="M42 {y} l3 4 l6 -9" fill="none" stroke="{GOLD}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>'
        body += f'<path d="M57 {y} H86" stroke="{INK}" stroke-width="1.6" stroke-linecap="round"/>'
    return body, ""


def big_check():
    body = f'<circle cx="64" cy="62" r="30" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="M48 63 L60 75 L82 49" fill="none" stroke="{INK}" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>'
    body += f'<path d="M48 63 L60 75 L82 49" fill="none" stroke="{GOLD}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>'
    return body, ""


def star_card():
    body = f'<rect x="34" y="28" width="60" height="70" rx="4" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="{star(64, 63, 22, 9, 5)}" fill="{GOLD}" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/>'
    return body, ""


def bullseye():
    body = ""
    for r, fill in ((30, PAPER), (22, JADE_L), (14, PAPER), (6, GOLD)):
        body += f'<circle cx="64" cy="64" r="{r}" fill="{fill}" stroke="{INK}" stroke-width="1.8"/>'
    body += f'<path d="M64 64 L92 36" stroke="{INK}" stroke-width="3" stroke-linecap="round"/>'
    body += f'<path d="M86 34 L94 34 L94 42" fill="none" stroke="{INK}" stroke-width="2.4"/>'
    return body, ""


def rising(full=False):
    sky = f'<path d="M0 0 H128 V90 H0 Z" fill="{JADE_D}" clip-path="url(#inside)"/>'
    moon = (
        f'<circle cx="64" cy="72" r="20" fill="{PAPER}" stroke="{INK}" stroke-width="1.8"/>'
        if full
        else crescent(64, 70, 24, 64, 58, 21)
    )
    rays = f"""<path d="M64 42 V32 M44 50 L37 43 M84 50 L91 43" stroke="{GOLD}" stroke-width="3" stroke-linecap="round"/>
<path d="M58 36 L64 28 L70 36" fill="none" stroke="{GOLD}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
<circle cx="36" cy="36" r="1.6" fill="{PAPER}"/><circle cx="94" cy="62" r="1.3" fill="{PAPER}"/><circle cx="32" cy="64" r="1.2" fill="{PAPER}"/>"""
    hills = f'<path d="M0 88 Q32 82 64 88 T128 88 V128 H0 Z" fill="url(#hills)" stroke="{INK}" stroke-width="2" clip-path="url(#inside)"/>'
    defs = hatch("hills", JADE, JADE_D, angle=-30) + '<clipPath id="above"><rect width="128" height="88"/></clipPath>'
    return sky + f'<g clip-path="url(#above)">{moon}</g>' + rays + hills, defs


def map_(marks=3):
    defs = hatch("fold", PAPER, INK, angle=60, gap=2.8, width=0.8)
    body = f"""<path d="M24 34 L48 26 L80 34 L104 26 V94 L80 102 L48 94 L24 102 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>
<path d="M48 26 L80 34 V102 L48 94 Z" fill="url(#fold)" stroke="{INK}" stroke-width="1.6" stroke-linejoin="round"/>
<path d="M34 86 C42 70 54 78 60 64 S78 46 92 44" fill="none" stroke="{JADE_D}" stroke-width="2.6" stroke-dasharray="4 3.4" stroke-linecap="round"/>"""
    spots = [(34, 86), (60, 64), (92, 44), (40, 46), (86, 80)][:marks]
    for i, (x, y) in enumerate(spots):
        fill = GOLD if i == len(spots) - 1 else JADE_M
        body += f'<circle cx="{x}" cy="{y}" r="5" fill="{fill}" stroke="{INK}" stroke-width="1.6"/>'
    return body, defs


def compass():
    body = f'<circle cx="64" cy="64" r="30" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="{star(64, 64, 26, 7, 4)}" fill="{JADE_L}" stroke="{INK}" stroke-width="1.6"/>'
    body += f'<path d="{star(64, 64, 16, 5, 4, start=-45)}" fill="{PAPER}" stroke="{INK}" stroke-width="1.2"/>'
    body += f'<path d="M64 38 L70 64 L64 60 L58 64 Z" fill="{GOLD}" stroke="{INK}" stroke-width="1.2"/>'
    return body, ""


def lunar_seal(days=7):
    body = f'<circle cx="64" cy="66" r="25" fill="{JADE_D}" stroke="{INK}" stroke-width="1.6"/>'
    body += crescent(64, 68, 17, 71, 62, 14)
    for i in range(days):
        a = math.radians(200 + i * (140 / max(days - 1, 1)))
        x, y = 64 + 36 * math.cos(a), 66 + 36 * math.sin(a)
        fill = GOLD if i == days - 1 else PAPER
        body += f'<circle cx="{x:.1f}" cy="{y:.1f}" r="3.6" fill="{fill}" stroke="{INK}" stroke-width="1.2"/>'
    return body, ""


def lotus(cx=64, base=88, scale=1.0):
    body = ""
    for angle, fill in [(-38, JADE_L), (38, JADE_L), (-18, PAPER), (18, PAPER), (0, PAPER)]:
        body += (
            f'<path d="M{cx} {base} C{cx - 14 * scale} {base - 16 * scale} {cx - 12 * scale} {base - 36 * scale} '
            f'{cx} {base - 50 * scale} C{cx + 12 * scale} {base - 36 * scale} {cx + 14 * scale} {base - 16 * scale} {cx} {base} Z" '
            f'fill="{fill}" stroke="{INK}" stroke-width="1.6" transform="rotate({angle} {cx} {base})"/>'
        )
    return body


def lotus_full():
    body = lotus()
    body += f'<path d="M36 90 Q64 100 92 90" fill="none" stroke="{INK}" stroke-width="2" stroke-linecap="round"/>'
    body += small_moon(64, 30, 8)
    return body, ""


def laurel():
    body = ""
    for side in (-1, 1):
        body += f'<path d="M64 96 C{64 + side * 34} 88 {64 + side * 38} 56 {64 + side * 24} 30" fill="none" stroke="{INK}" stroke-width="2"/>'
        for i in range(6):
            t = i / 5
            x = 64 + side * (8 + 28 * math.sin(t * 2.6))
            y = 92 - t * 58
            body += (
                f'<ellipse cx="{x:.1f}" cy="{y:.1f}" rx="8" ry="4" fill="{JADE_L}" stroke="{INK}" '
                f'stroke-width="1.2" transform="rotate({side * (40 + t * 50):.0f} {x:.1f} {y:.1f})"/>'
            )
    body += lotus(64, 80, 0.6)
    return body, ""


def ankh():
    body = f'<ellipse cx="64" cy="42" rx="13" ry="16" fill="none" stroke="{INK}" stroke-width="10"/>'
    body += f'<ellipse cx="64" cy="42" rx="13" ry="16" fill="none" stroke="{GOLD}" stroke-width="6"/>'
    body += f'<rect x="38" y="58" width="52" height="11" rx="2" fill="{GOLD}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="M58 69 H70 L72 104 H56 Z" fill="{GOLD}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>'
    return body, ""


def hourglass():
    body = f"""<rect x="40" y="28" width="48" height="8" rx="3" fill="{GOLD}" stroke="{INK}" stroke-width="1.8"/>
<rect x="40" y="92" width="48" height="8" rx="3" fill="{GOLD}" stroke="{INK}" stroke-width="1.8"/>
<path d="M46 36 C46 54 60 58 60 64 C60 70 46 74 46 92 H82 C82 74 68 70 68 64 C68 58 82 54 82 36 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>
<path d="M52 44 H76 C74 52 66 56 64 60 C62 56 54 52 52 44 Z" fill="{JADE_M}"/>
<path d="M50 90 C54 80 60 78 64 76 C68 78 74 80 78 90 Z" fill="{JADE_M}"/>
<path d="M64 62 V76" stroke="{JADE_M}" stroke-width="1.6"/>"""
    return body, ""


def columns():
    body = f'<path d="M30 52 L64 32 L98 52 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>'
    body += small_moon(64, 45, 5)
    for x in (36, 52, 68, 84):
        body += f'<rect x="{x - 4}" y="56" width="8" height="32" fill="{PAPER}" stroke="{INK}" stroke-width="1.6"/>'
        body += f'<path d="M{x - 1.5} 60 V84 M{x + 1.5} 60 V84" stroke="{INK}" stroke-width="0.8"/>'
    body += f'<rect x="28" y="88" width="72" height="8" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    return body, ""


def open_book(highlights=False):
    body = f"""<path d="M64 40 C54 34 40 32 28 34 V88 C40 86 54 88 64 94 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>
<path d="M64 40 C74 34 88 32 100 34 V88 C88 86 74 88 64 94 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>
<path d="M64 40 V94" stroke="{INK}" stroke-width="1.6"/>"""
    for i in range(4):
        y = 46 + i * 10
        if highlights and i in (1, 3):
            body += f'<rect x="69" y="{y - 3.5}" width="25" height="7" fill="{HIGHLIGHT}"/>'
            body += f'<rect x="34" y="{y - 3.5}" width="20" height="7" fill="{HIGHLIGHT}"/>'
        body += f'<path d="M35 {y} H57 M71 {y} H93" stroke="{INK}" stroke-width="1.3" stroke-linecap="round"/>'
    if not highlights:
        body += f'<path d="M86 30 V48 L90 44 L94 48 V30 Z" fill="{GOLD}" stroke="{INK}" stroke-width="1.2"/>'
    return body, ""


def page_stack():
    body = ""
    for i, (dx, dy) in enumerate(((10, -8), (5, -4), (0, 0))):
        fill = EDGE if i < 2 else PAPER
        body += f'<rect x="{32 + dx}" y="{38 + dy}" width="52" height="62" fill="{fill}" stroke="{INK}" stroke-width="1.8"/>'
    for i in range(4):
        body += f'<path d="M40 {52 + i * 9} H{74 - (i % 2) * 8}" stroke="{INK}" stroke-width="1.4" stroke-linecap="round"/>'
    body += f'<path d="M98 26 C86 40 80 56 76 76 L80 78 C86 60 94 44 98 26 Z" fill="{JADE_L}" stroke="{INK}" stroke-width="1.6"/>'
    body += f'<path d="M76 76 L74 84 L80 78" fill="{INK}"/>'
    return body, ""


def nib(extra=None):
    body = f"""<path d="M64 26 L84 58 L74 98 H54 L44 58 Z" fill="{PAPER}" stroke="{INK}" stroke-width="2" stroke-linejoin="round"/>
<path d="M64 26 V62" stroke="{INK}" stroke-width="1.6"/>
<circle cx="64" cy="66" r="4" fill="{JADE_D}" stroke="{INK}" stroke-width="1.4"/>
<rect x="52" y="96" width="24" height="10" rx="2" fill="{GOLD}" stroke="{INK}" stroke-width="1.8"/>"""
    if extra == "star":
        body += f'<path d="{star(92, 40, 9, 3.5, 4)}" fill="{HIGHLIGHT}" stroke="{INK}" stroke-width="1.2"/>'
    if extra == "halo":
        body = f'<circle cx="64" cy="60" r="34" fill="none" stroke="{HIGHLIGHT}" stroke-width="4"/>' + body
        body += f'<path d="{star(92, 34, 8, 3, 4)}" fill="{HIGHLIGHT}" stroke="{INK}" stroke-width="1.2"/>'
        body += f'<path d="{star(36, 40, 6, 2.4, 4)}" fill="{HIGHLIGHT}" stroke="{INK}" stroke-width="1.2"/>'
    return body, ""


def sundial():
    body = f'<circle cx="64" cy="64" r="34" fill="{PAPER}" stroke="{INK}" stroke-width="2"/>'
    for i in range(12):
        a = math.radians(i * 30)
        r1 = 22 if i % 3 == 0 else 26
        body += (
            f'<path d="M{64 + r1 * math.cos(a):.1f} {64 + r1 * math.sin(a):.1f} '
            f'L{64 + 31 * math.cos(a):.1f} {64 + 31 * math.sin(a):.1f}" stroke="{INK}" stroke-width="1.5"/>'
        )
    body += f'<path d="M64 64 L64 42 M64 64 L80 72" stroke="{INK}" stroke-width="2.6" stroke-linecap="round"/>'
    body += small_moon(64, 64, 5)
    return body, ""


def radiant_sun():
    body = f'<path d="{star(64, 64, 38, 26, 12)}" fill="{HIGHLIGHT}" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/>'
    body += f'<circle cx="64" cy="64" r="20" fill="{GOLD}" stroke="{INK}" stroke-width="2"/>'
    body += f'<path d="M55 62 Q58 58 61 62 M67 62 Q70 58 73 62 M56 71 Q64 77 72 71" fill="none" stroke="{INK}" stroke-width="1.8" stroke-linecap="round"/>'
    return body, ""


def phases():
    body = ""
    for i, y in enumerate((30, 50, 70, 90)):
        if i in (0, 3):
            body += crescent(64, y, 8, 64 + (4 if i == 0 else -4), y - 1, 7)
        else:
            body += f'<circle cx="64" cy="{y}" r="8" fill="{PAPER if i == 2 else JADE_D}" stroke="{INK}" stroke-width="1.6"/>'
    for x in (40, 88):
        for y in (40, 64, 88):
            body += f'<circle cx="{x}" cy="{y}" r="2.2" fill="{PAPER}"/>'
    return body, ""


def map_stack():
    body = ""
    for i, (dx, dy) in enumerate(((12, -10), (6, -5), (0, 0))):
        fill = EDGE if i < 2 else PAPER
        body += (
            f'<path d="M{28 + dx} {44 + dy} L{48 + dx} {38 + dy} L{72 + dx} {44 + dy} L{92 + dx} {38 + dy} '
            f'V{94 + dy} L{72 + dx} {100 + dy} L{48 + dx} {94 + dy} L{28 + dx} {100 + dy} Z" '
            f'fill="{fill}" stroke="{INK}" stroke-width="1.8" stroke-linejoin="round"/>'
        )
    body += f'<path d="M36 88 C46 76 56 82 62 70 S76 56 84 54" fill="none" stroke="{JADE_D}" stroke-width="2.4" stroke-dasharray="4 3"/>'
    body += f'<circle cx="84" cy="54" r="5" fill="{GOLD}" stroke="{INK}" stroke-width="1.4"/>'
    return body, ""


def scroll_stack():
    defs = hatch("roll", PAPER, INK, angle=0, gap=2.6, width=0.9)
    body = ""
    for x, y in ((30, 70), (54, 52), (40, 34)):
        body += f'<rect x="{x}" y="{y}" width="46" height="14" rx="7" fill="url(#roll)" stroke="{INK}" stroke-width="2"/>'
        body += f'<ellipse cx="{x + 46}" cy="{y + 7}" rx="3.5" ry="7" fill="{PAPER}" stroke="{INK}" stroke-width="1.6"/>'
        body += f'<path d="M{x + 10} {y + 14} V{y + 18}" stroke="{GOLD}" stroke-width="3"/>'
    body += small_moon(36, 98, 6)
    return body, defs


def card_stack():
    body = ""
    for i, angle in enumerate((-14, -4, 8)):
        fill = EDGE if i < 2 else PAPER
        body += f'<rect x="38" y="32" width="52" height="64" rx="4" fill="{fill}" stroke="{INK}" stroke-width="1.8" transform="rotate({angle} 64 64)"/>'
    for i in range(3):
        y = 48 + i * 15
        body += f'<path d="M48 {y} l3 4 l6 -9" fill="none" stroke="{GOLD}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" transform="rotate(8 64 64)"/>'
        body += f'<path d="M62 {y} H80" stroke="{INK}" stroke-width="1.5" stroke-linecap="round" transform="rotate(8 64 64)"/>'
    return body, ""


def check_trio():
    body = ""
    for x, y, r in ((44, 52, 14), (84, 52, 14), (64, 80, 16)):
        body += f'<circle cx="{x}" cy="{y}" r="{r}" fill="{PAPER}" stroke="{INK}" stroke-width="1.8"/>'
        body += (
            f'<path d="M{x - r * 0.5} {y} L{x - r * 0.1} {y + r * 0.4} L{x + r * 0.55} {y - r * 0.45}" '
            f'fill="none" stroke="{GOLD}" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>'
        )
    return body, ""


def lotus_field():
    body = lotus(42, 90, 0.55) + lotus(86, 90, 0.55) + lotus(64, 84, 0.75)
    body += f'<path d="M24 92 Q64 102 104 92" fill="none" stroke="{INK}" stroke-width="2" stroke-linecap="round"/>'
    return body, ""


def album():
    body = f'<rect x="28" y="30" width="72" height="68" rx="4" fill="{JADE_D}" stroke="{INK}" stroke-width="2"/>'
    body += f'<rect x="34" y="36" width="60" height="56" fill="{PAPER}" stroke="{INK}" stroke-width="1.6"/>'
    shapes = [
        f'<circle cx="48" cy="52" r="8" fill="{JADE_M}" stroke="{INK}" stroke-width="1.2"/>',
        f'<path d="{polygon(6, 80, 52, 9)}" fill="{GOLD}" stroke="{INK}" stroke-width="1.2"/>',
        f'<path d="{wavy(48, 77, 8, 8, 1.6)}" fill="{JADE_L}" stroke="{INK}" stroke-width="1.2"/>',
        f'<rect x="72" y="69" width="16" height="16" rx="4" fill="{EDGE}" stroke="{INK}" stroke-width="1.2" stroke-dasharray="2 2"/>',
    ]
    body += "".join(shapes)
    return body, ""


FAMILIES = {
    "lessons": ("circle", scroll),
    "lesson_days": ("card", calendar),
    "quizzes": ("hexagon", quiz_card),
    "right_answers": ("hexagon", big_check),
    "perfect_quizzes": ("hexagon", star_card),
    "high_quizzes": ("hexagon", bullseye),
    "recoveries": ("arch", rising),
    "clean_notebook": ("arch", lambda: rising(full=True)),
    "coverage": ("square", lambda: map_(3)),
    "coverage_share": ("square", lambda: map_(5)),
    "subjects_started": ("square", compass),
    "weeks": ("seal", lambda: lunar_seal(7)),
    "plan_complete": ("seal", lambda: lunar_seal(9)),
    "mastered": ("shield", lotus_full),
    "subjects_mastered": ("shield", laurel),
    "notice_mastered": ("shield", ankh),
    "reviews": ("octagon", hourglass),
    "past_exams": ("temple", columns),
    "handouts_read": ("card", open_book),
    "highlights": ("card", lambda: open_book(highlights=True)),
    "handouts_made": ("card", page_stack),
    "essays": ("diamond", nib),
    "essays_high": ("diamond", lambda: nib("star")),
    "essay_top": ("diamond", lambda: nib("halo")),
    "hours": ("sun", sundial),
    "long_day": ("sun", radiant_sun),
    "study_days": ("oval", phases),
    "plans": ("pentagon", map_stack),
    "all_lessons": ("pentagon", scroll_stack),
    "all_quizzes": ("pentagon", card_stack),
    "all_right_answers": ("pentagon", check_trio),
    "all_mastered": ("pentagon", lotus_field),
    "stickers": ("pentagon", album),
}


def main():
    os.makedirs(OUT, exist_ok=True)
    for old in os.listdir(OUT):
        if old.endswith(".svg"):
            os.remove(os.path.join(OUT, old))
    for name, (shape, motif) in FAMILIES.items():
        body, defs = motif()
        with open(os.path.join(OUT, f"{name}.svg"), "w", encoding="utf-8", newline="\n") as f:
            f.write(frame(shape, body, defs))
    print(f"{len(FAMILIES)} families in {os.path.normpath(OUT)}")


if __name__ == "__main__":
    main()
