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

# Headdress (nemes) behind the head, falling to the shoulders.
nemes_front = chain((140, 84), ((128, 72), (104, 70), (86, 82)))
nemes_back = chain((86, 82), ((66, 96), (60, 132), (54, 170)), ((50, 200), (46, 230), (42, 262)))
nemes_inner = chain((126, 262), ((122, 220), (118, 176), (124, 140)), ((128, 120), (134, 100), (140, 84)))
nemes_d = d_closed(nemes_front, nemes_back, [(40, 262)], nemes_inner)

# Neck.
neck_back = chain((132, 124), ((124, 170), (120, 220), (118, 262)))
neck_front = chain((178, 262), ((166, 220), (156, 176), (160, 126)))
neck_d = d_closed(neck_back, neck_front, [(160, 126), (132, 124)])

# Head: the skull under the headdress, and the face.
head_top = chain((124, 104), ((132, 84), (156, 80), (170, 92)))
head_bottom = chain((170, 92), ((176, 100), (176, 112), (170, 120)),
                    ((160, 132), (140, 136), (130, 130)), ((122, 124), (120, 112), (124, 104)))
head_d = d_closed(head_top, head_bottom)

# Beak: long, curved down, like a reed pen.
beak_top = chain((166, 92), ((196, 102), (226, 144), (240, 204)))
beak_bottom = chain((240, 204), ((222, 162), (198, 132), (168, 120)))
beak_d = d_closed(beak_top, beak_bottom)

# Lunar disc and crescent.
DISC = (122, 42, 19)


def build(rounded):
    add('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">')
    add("<!-- Generated by tools/icon.py: Thoth in profile, engraved, on a jade tile. -->")
    add("<defs>")
    clip("tile", "M0 0 H256 V256 H0 Z" if not rounded else "M56 0 H200 A56 56 0 0 1 256 56 V200 A56 56 0 0 1 200 256 H56 A56 56 0 0 1 0 200 V56 A56 56 0 0 1 56 0 Z")
    clip("nemes", nemes_d)
    clip("neck", neck_d)
    clip("head", head_d)
    clip("beak", beak_d)
    clip("disc", f"M{DISC[0]-DISC[2]} {DISC[1]} A{DISC[2]} {DISC[2]} 0 1 0 {DISC[0]+DISC[2]} {DISC[1]} A{DISC[2]} {DISC[2]} 0 1 0 {DISC[0]-DISC[2]} {DISC[1]} Z")
    add("</defs>")
    add('<g clip-path="url(#tile)">')
    add(f'<rect width="256" height="256" fill="{TILE}"/>')
    # Rays behind the disc, engraved in the tile's darker green.
    for i in range(-6, 7):
        a = math.radians(-90 + i * 13)
        x0, y0 = DISC[0] + math.cos(a) * 30, DISC[1] + math.sin(a) * 30
        x1, y1 = DISC[0] + math.cos(a) * 64, DISC[1] + math.sin(a) * 64
        line([(x0, y0), (x1, y1)], 2.2 - abs(i) * 0.12, TILE_DARK)

    # Disc.
    cx, cy, r = DISC
    add(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{OCHRE}" stroke="{INK}" stroke-width="3"/>')
    add('<g clip-path="url(#disc)">')
    # Curved lines on the side away from the light.
    for k in range(1, 8):
        off = 30 - k * 3.4
        add(f'<circle cx="{cx - off:.1f}" cy="{cy - off:.1f}" r="{r + off * 1.2:.1f}" fill="none" stroke="{INK}" stroke-width="{0.35 + k * 0.17:.2f}"/>')
    add("</g>")
    crescent = f"M{cx-27} {cy+6} A27 27 0 0 0 {cx+27} {cy+6} A27 17 0 0 1 {cx-27} {cy+6} Z"
    shape(crescent, OCHRE, 2.6)

    # Headdress with its stripes.
    shape(nemes_d, PAPER)
    add('<g clip-path="url(#nemes)">')
    y = 70
    band = 0
    while y < 262:
        h = 11
        if band % 2 == 0:
            # Dark stripe: dense engraved lines, almost solid.
            for j in range(7):
                yy = y + 0.8 + j * 1.6
                line([(30, yy + 6), (160, yy - 6)], 1.35)
        else:
            line([(30, y + h / 2 + 6), (160, y + h / 2 - 6)], 0.45)
        y += h
        band += 1
    # Shadow at the back edge.
    contour_hatch(nemes_back, resample(nemes_inner[::-1], 60), 8, lambda t: max(0.0, 1.6 - t * 3.5))
    add("</g>")
    shape(nemes_d, "none")

    # Neck, shaded on the throat side.
    shape(neck_d, PAPER)
    add('<g clip-path="url(#neck)">')
    contour_hatch(neck_back, neck_front[::-1], 14, lambda t: 0.25 + t * t * 2.0)
    add("</g>")
    shape(neck_d, "none")

    # Head.
    shape(head_d, PAPER)
    add('<g clip-path="url(#head)">')
    jaw = chain((118, 112), ((128, 140), (160, 138), (178, 108)))
    crown = chain((118, 98), ((132, 74), (160, 76), (178, 98)))
    contour_hatch(crown, jaw, 10, lambda t: max(0.0, (t - 0.6) * 3.2), taper=(0.1, 0.9))
    add("</g>")
    shape(head_d, "none")

    # Beak, with lines along its length darkening underneath.
    shape(beak_d, PAPER)
    add('<g clip-path="url(#beak)">')
    contour_hatch(beak_top, beak_bottom[::-1], 9, lambda t: max(0.0, (t - 0.3) * 2.6), taper=(0.02, 0.96))
    add("</g>")
    shape(beak_d, "none")

    # Eye, with the painted line running back from it.
    line(chain((150, 100), ((142, 100), (134, 103), (126, 109))), 2.8)
    add(f'<circle cx="153" cy="99" r="5.6" fill="{INK}"/>')
    add(f'<circle cx="151.4" cy="97.4" r="1.6" fill="{PAPER}"/>')
    # The beak's opening line.
    line(chain((170, 108), ((196, 120), (222, 160), (238, 200))), 1.1)

    # Broad collar: beads, stripes and a plain inner band.
    ccx, ccy = 140, 286
    for r_out, r_in, fill in ((104, 90, PAPER), (90, 76, OCHRE), (76, 64, PAPER)):
        d = (f"M{ccx-r_out} {ccy} A{r_out} {r_out} 0 0 1 {ccx+r_out} {ccy} "
             f"L{ccx+r_in} {ccy} A{r_in} {r_in} 0 0 0 {ccx-r_in} {ccy} Z")
        shape(d, fill, 2.6)
    for i in range(1, 24):
        a = math.pi + i * math.pi / 24
        x, y = ccx + math.cos(a) * 97, ccy + math.sin(a) * 97
        add(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="3.6" fill="{INK}"/>')
        x0, y0 = ccx + math.cos(a) * 77, ccy + math.sin(a) * 77
        x1, y1 = ccx + math.cos(a) * 89, ccy + math.sin(a) * 89
        line([(x0, y0), (x1, y1)], 1.6)
    for k in range(3):
        rr = 66 + k * 3.2
        add(f'<path d="M{ccx-rr} {ccy} A{rr} {rr} 0 0 1 {ccx+rr} {ccy}" fill="none" stroke="{INK}" stroke-width="0.7"/>')
    add("</g>")
    add("</svg>")


for name, rounded in (("icon.svg", True), ("icon-square.svg", False)):
    out.clear()
    build(rounded)
    path = ASSETS / name
    path.write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {path}")
