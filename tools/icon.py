"""Draws the Know Giver app icon: Thoth, ibis-headed god of writing, in
profile, as an engraving (ink line work and hatching on paper) on a jade tile.

    python tools/icon.py

writes crates/ui/assets/icon.svg (rounded tile, the app icon) and
icon-square.svg (square, for cover plates). Below 32 px the simpler logo.svg
glyph is used instead.

Everything is generated here from a few hand-placed curves, so the artwork can
be tuned by editing the numbers below. Shading is done the way an engraver
would: lines that follow the form, thicker where the light does not reach.
"""

import math
from pathlib import Path

ASSETS = Path(__file__).resolve().parent.parent / "crates/ui/assets"

TILE = "#3F8F7C"
TILE_DARK = "#2C6B5C"
INK = "#1C1B19"
PAPER = "#F4EEDF"
OCHRE = "#C98E2E"


# --- geometry -----------------------------------------------------------------

def cubic(p0, p1, p2, p3, n=48):
    pts = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        pts.append((
            u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
            u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1],
        ))
    return pts


def chain(start, *segments, n=48):
    """A polyline through cubic segments given as (c1, c2, end)."""
    pts = [start]
    cur = start
    for c1, c2, end in segments:
        pts += cubic(cur, c1, c2, end, n)[1:]
        cur = end
    return pts


def resample(pts, n):
    lengths = [0.0]
    for a, b in zip(pts, pts[1:]):
        lengths.append(lengths[-1] + math.dist(a, b))
    total = lengths[-1] or 1.0
    out, j = [], 0
    for i in range(n):
        target = total * i / (n - 1)
        while j < len(lengths) - 2 and lengths[j + 1] < target:
            j += 1
        seg = (lengths[j + 1] - lengths[j]) or 1.0
        t = (target - lengths[j]) / seg
        a, b = pts[j], pts[j + 1]
        out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return out


def lerp_lines(a, b, t):
    return [(p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t) for p, q in zip(a, b)]


def d_poly(pts, close=False):
    s = "M" + " L".join(f"{x:.1f} {y:.1f}" for x, y in pts)
    return s + (" Z" if close else "")


def d_closed(*parts):
    pts = []
    for part in parts:
        pts += part
    return d_poly(pts, close=True)


# --- drawing helpers ----------------------------------------------------------

out = []


def add(s):
    out.append(s)


def shape(d, fill, width=3.0):
    add(f'<path d="{d}" fill="{fill}" stroke="{INK}" stroke-width="{width}" stroke-linejoin="round"/>')


def line(pts, width, color=INK):
    if width <= 0.15:
        return
    add(f'<path d="{d_poly(pts)}" fill="none" stroke="{color}" stroke-width="{width:.2f}" stroke-linecap="round"/>')


def clip(name, d):
    add(f'<clipPath id="{name}"><path d="{d}"/></clipPath>')


def contour_hatch(edge_a, edge_b, count, weight, taper=(0.0, 1.0), color=INK):
    """Lines between two edges that follow the form. `weight(t)` gives the
    stroke width for the line at `t` (0 at edge_a, 1 at edge_b); `taper`
    trims each line's start and end so it fades into the light."""
    a = resample(edge_a, 60)
    b = resample(edge_b, 60)
    for i in range(1, count):
        t = i / count
        pts = lerp_lines(a, b, t)
        lo = int(taper[0] * (len(pts) - 1))
        hi = int(taper[1] * (len(pts) - 1)) + 1
        line(pts[lo:hi], weight(t), color)


def parallel_hatch(box, angle, gap, weight, color=INK):
    """Straight hatching over a box, at `angle` degrees. `weight(x, y)` gives
    the width at the line's middle."""
    x0, y0, x1, y1 = box
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    r = math.hypot(x1 - x0, y1 - y0) / 2
    dx, dy = math.cos(math.radians(angle)), math.sin(math.radians(angle))
    nx, ny = -dy, dx
    k = -r
    while k <= r:
        mx, my = cx + nx * k, cy + ny * k
        w = weight(mx, my)
        line([(mx - dx * r, my - dy * r), (mx + dx * r, my + dy * r)], w, color)
        k += gap


# --- the figure -----------------------------------------------------------------
# Thoth as the scribe of the gods, in profile facing right: ibis head under a
# large striped headdress, one arm holding his palette upright, the other
# writing on it with a reed pen. Bold ink line work, little shading.

# Torso, cut by the tile's lower edge.
torso_d = d_closed(
    chain((34, 262), ((32, 210), (34, 160), (46, 128)), ((56, 108), (84, 100), (108, 108))),
    chain((108, 108), ((118, 128), (116, 170), (110, 200)), ((106, 226), (106, 246), (108, 262))),
)

# Head: the front of the skull; the headdress covers the rest.
head_d = d_closed(
    chain((70, 58), ((76, 38), (100, 34), (110, 48))),
    chain((110, 48), ((114, 58), (108, 70), (96, 72)), ((84, 74), (72, 70), (70, 58))),
)

# Beak: long, curved down toward the palette.
beak_top = chain((106, 44), ((136, 44), (160, 64), (172, 98)))
beak_bottom = chain((172, 98), ((154, 74), (132, 62), (106, 62)))
beak_d = d_closed(beak_top, beak_bottom)

# Headdress: over the crown, a long lappet down the back and a short one in
# front of the shoulder.
nemes_d = d_closed(
    chain((92, 32), ((74, 18), (40, 22), (28, 50))),
    chain((28, 50), ((20, 80), (22, 120), (30, 150))),
    [(58, 148)],
    chain((58, 148), ((56, 120), (60, 92), (70, 74))),
)
lappet_d = d_closed(
    chain((72, 70), ((80, 84), (86, 104), (90, 132))),
    [(76, 136)],
    chain((76, 136), ((72, 112), (68, 92), (64, 76))),
)

# Far arm: straight out to the palette, holding it.
far_arm_d = d_closed(
    chain((84, 112), ((120, 106), (170, 104), (206, 106))),
    [(206, 122)],
    chain((206, 122), ((170, 122), (120, 126), (86, 132))),
)
# Near arm: down to the elbow, then up to the pen.
upper_arm_d = d_closed(
    chain((88, 136), ((98, 152), (108, 166), (120, 182))),
    [(136, 176)],
    chain((136, 176), ((124, 160), (112, 146), (104, 132))),
)
forearm_top = chain((122, 172), ((142, 166), (162, 158), (182, 148)))
forearm_bottom = chain((190, 160), ((170, 170), (150, 180), (132, 188)))
forearm_d = d_closed(forearm_top, forearm_bottom)

# Scribe's palette, upright at the right, with pens standing in it.
PAL = (208, 56, 24, 150)  # x, y, width, height


def build(rounded):
    add('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">')
    add("<!-- Generated by tools/icon.py: Thoth the scribe, in ink, on a jade tile. -->")
    add("<defs>")
    clip("tile", "M0 0 H256 V256 H0 Z" if not rounded else "M56 0 H200 A56 56 0 0 1 256 56 V200 A56 56 0 0 1 200 256 H56 A56 56 0 0 1 0 200 V56 A56 56 0 0 1 56 0 Z")
    clip("nemes", nemes_d)
    clip("lappet", lappet_d)
    clip("torso", torso_d)
    clip("beak", beak_d)
    clip("far", far_arm_d)
    clip("upper", upper_arm_d)
    clip("fore", forearm_d)
    px, py, pw, ph = PAL
    clip("palette", f"M{px} {py} h{pw} v{ph} h{-pw} Z")
    add("</defs>")
    add('<g clip-path="url(#tile)">')
    add(f'<rect width="256" height="256" fill="{TILE}"/>')
    # Clear of the rounded corners.
    add('<g transform="translate(4 14)">')

    # Torso with a broad sash across the chest and a belt.
    shape(torso_d, PAPER)
    add('<g clip-path="url(#torso)">')
    parallel_hatch((20, 100, 120, 262), 80, 3.2, lambda x, y: max(0.0, 1.5 - (x - 34) * 0.05))
    for k, w in enumerate((5, 1.4, 5, 1.4, 5)):
        off = k * 7
        line([(40 + off, 120), (112 + off, 236)], w)
    add(f'<rect x="20" y="226" width="110" height="14" fill="{OCHRE}" stroke="{INK}" stroke-width="2.6"/>')
    for x in range(30, 120, 9):
        line([(x, 228), (x, 238)], 1.4)
    add("</g>")
    shape(torso_d, "none")

    # Headdress: bold stripes, as in the paintings.
    shape(nemes_d, PAPER)
    add('<g clip-path="url(#nemes)">')
    y = 34
    while y < 160:
        add(f'<path d="M10 {y + 6} L100 {y - 6} L100 {y - 1} L10 {y + 11} Z" fill="{INK}"/>')
        y += 12
    add("</g>")
    shape(nemes_d, "none")

    # Head and beak.
    shape(head_d, PAPER)
    shape(beak_d, PAPER)
    add('<g clip-path="url(#beak)">')
    contour_hatch(beak_top, beak_bottom[::-1], 6, lambda t: max(0.0, (t - 0.45) * 2.4), taper=(0.05, 0.95))
    add("</g>")
    shape(beak_d, "none")
    line(chain((110, 52), ((134, 54), (154, 72), (168, 94))), 1.0)
    # Headband edge over the brow, and the eye with its painted line.
    line(chain((70, 72), ((78, 52), (88, 38), (94, 32))), 3.0)
    line(chain((96, 50), ((90, 50), (84, 52), (78, 56))), 2.4)
    add(f'<circle cx="99" cy="49" r="4.2" fill="{INK}"/>')
    add(f'<circle cx="97.8" cy="47.8" r="1.2" fill="{PAPER}"/>')

    # Front lappet over the shoulder, striped.
    shape(lappet_d, PAPER)
    add('<g clip-path="url(#lappet)">')
    for y in range(78, 140, 9):
        add(f'<path d="M58 {y} L96 {y + 3} L96 {y + 7} L58 {y + 4} Z" fill="{INK}"/>')
    add("</g>")
    shape(lappet_d, "none")

    # Palette, with pens standing in its top and two ink wells.
    for k, (dx, h) in enumerate(((-6, 14), (0, 18), (6, 12))):
        x = px + pw / 2 + dx
        line([(x, py), (x, py - h)], 2.4)
    shape(f"M{px} {py} h{pw} v{ph} h{-pw} Z", PAPER, 3.0)
    add('<g clip-path="url(#palette)">')
    parallel_hatch((px, py, px + pw, py + ph), 90, 2.6,
                   lambda x, y: max(0.0, (x - (px + pw * 0.6)) * 0.16))
    add("</g>")
    add(f'<circle cx="{px + pw / 2}" cy="{py + 12}" r="4.6" fill="{INK}"/>')
    add(f'<circle cx="{px + pw / 2}" cy="{py + 25}" r="4.6" fill="{OCHRE}" stroke="{INK}" stroke-width="1.6"/>')
    add(f'<rect x="{px + pw / 2 - 2.5}" y="{py + 38}" width="5" height="{ph - 54}" rx="2.5" fill="none" stroke="{INK}" stroke-width="1.6"/>')

    # Far arm holding the palette, with a wrist band.
    shape(far_arm_d, PAPER)
    add('<g clip-path="url(#far)">')
    contour_hatch(chain((84, 112), ((120, 106), (170, 104), (206, 106))),
                  chain((86, 132), ((120, 126), (170, 122), (206, 122))), 5,
                  lambda t: max(0.0, (t - 0.45) * 2.6))
    add("</g>")
    shape(far_arm_d, "none")
    add(f'<rect x="186" y="104" width="9" height="19" fill="{OCHRE}" stroke="{INK}" stroke-width="2"/>')
    # Fingers around the palette's edge.
    shape("M204 104 C214 102 220 106 220 112 C220 118 214 124 204 123 Z", PAPER, 2.4)
    line([(210, 110), (219, 110)], 1.1)
    line([(210, 116), (219, 116)], 1.1)

    # Near arm writing.
    shape(upper_arm_d, PAPER)
    add('<g clip-path="url(#upper)">')
    parallel_hatch((80, 130, 140, 190), 50, 3.0, lambda x, y: max(0.0, (y - 150) * 0.03))
    add("</g>")
    shape(upper_arm_d, "none")
    shape(forearm_d, PAPER)
    add('<g clip-path="url(#fore)">')
    contour_hatch(forearm_top, forearm_bottom[::-1], 5, lambda t: max(0.0, (t - 0.45) * 2.6), taper=(0.05, 0.95))
    add("</g>")
    shape(forearm_d, "none")
    add(f'<path d="M168 154 L176 150 L180 162 L172 166 Z" fill="{OCHRE}" stroke="{INK}" stroke-width="2"/>')
    # Reed pen to the palette, held in the fist.
    line([(186, 152), (210, 128)], 2.6)
    shape("M180 142 C188 136 198 140 198 150 C198 158 190 162 184 160 C180 156 178 148 180 142 Z", PAPER, 2.4)
    for k in range(3):
        y0 = 146 + k * 4
        line(chain((188, y0), ((192, y0 - 1), (196, y0 + 1), (197, y0 + 4))), 1.1)

    add("</g>")
    add("</g>")
    add("</svg>")


for name, rounded in (("icon.svg", True), ("icon-square.svg", False)):
    out.clear()
    build(rounded)
    path = ASSETS / name
    path.write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {path}")
