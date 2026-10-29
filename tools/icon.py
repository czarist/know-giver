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
# Thoth as the scribe of the gods, in profile: ibis head under the striped
# headdress, broad collar, writing with a reed pen on a scribe's palette.
# Drawn facing right; the icon shows it mirrored.

# Torso, cut by the tile's lower edge.
torso_d = d_closed(
    chain((58, 262), ((56, 214), (58, 170), (68, 140)), ((76, 122), (96, 114), (116, 120))),
    chain((116, 120), ((128, 140), (126, 186), (120, 214)), ((118, 234), (118, 250), (120, 262))),
)

# Neck.
neck_d = d_closed(
    chain((84, 80), ((82, 96), (80, 108), (78, 122))),
    [(108, 122)],
    chain((108, 122), ((104, 108), (102, 94), (102, 80))),
)

# Head: the front of the skull; the headdress covers the rest.
head_top = chain((78, 70), ((84, 50), (106, 46), (114, 60)))
head_bottom = chain((114, 60), ((118, 70), (112, 82), (100, 84)), ((90, 86), (80, 82), (78, 70)))
head_d = d_closed(head_top, head_bottom)

# Beak: long and curved, pointing down at the palette.
beak_top = chain((110, 56), ((138, 58), (162, 80), (178, 114)))
beak_bottom = chain((178, 114), ((160, 88), (136, 72), (112, 72)))
beak_d = d_closed(beak_top, beak_bottom)

# Headdress: over the crown and down the back in a lappet.
nemes_top = chain((96, 44), ((82, 34), (58, 38), (50, 60)))
nemes_back = chain((50, 60), ((44, 90), (44, 126), (48, 162)))
nemes_front = chain((76, 166), ((72, 132), (74, 100), (82, 80)), ((86, 66), (90, 54), (96, 44)))
nemes_d = d_closed(nemes_top, nemes_back, [(48, 162)], nemes_front)

# Near arm: upper arm down to the elbow, forearm up to the pen.
upper_arm_d = d_closed(
    chain((96, 128), ((106, 148), (114, 170), (120, 196))),
    [(136, 190)],
    chain((136, 190), ((130, 164), (124, 142), (116, 124))),
)
forearm_top = chain((126, 186), ((144, 172), (164, 156), (180, 142)))
forearm_bottom = chain((188, 154), ((170, 168), (150, 184), (132, 200)))
forearm_d = d_closed(forearm_top, forearm_bottom)

# Scribe's palette: a long board with two ink wells and a slot for pens.
PAL = (202, 58, 22, 160)  # x, y, width, height


def build(rounded):
    add('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">')
    add("<!-- Generated by tools/icon.py: Thoth the scribe, engraved, on a jade tile. -->")
    add("<defs>")
    clip("tile", "M0 0 H256 V256 H0 Z" if not rounded else "M56 0 H200 A56 56 0 0 1 256 56 V200 A56 56 0 0 1 200 256 H56 A56 56 0 0 1 0 200 V56 A56 56 0 0 1 56 0 Z")
    clip("nemes", nemes_d)
    clip("torso", torso_d)
    clip("head", head_d)
    clip("beak", beak_d)
    clip("upper", upper_arm_d)
    clip("fore", forearm_d)
    px, py, pw, ph = PAL
    clip("palette", f"M{px} {py} h{pw} v{ph} h{-pw} Z")
    add("</defs>")
    add('<g clip-path="url(#tile)">')
    add(f'<rect width="256" height="256" fill="{TILE}"/>')
    # Drawn facing right, shown facing left.
    add('<g transform="translate(256 0) scale(-1 1)">')

    # Lunar disc and crescent above the head, with engraved rays.
    cx, cy, r = 84, 22, 11
    for i in range(-5, 6):
        a = math.radians(-90 + i * 16)
        line([(cx + math.cos(a) * 18, cy + math.sin(a) * 18),
               (cx + math.cos(a) * 40, cy + math.sin(a) * 40)], 2.0 - abs(i) * 0.14, TILE_DARK)
    add(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{OCHRE}" stroke="{INK}" stroke-width="2.6"/>')
    shape(f"M{cx-17} {cy+4} A17 17 0 0 0 {cx+17} {cy+4} A17 10 0 0 1 {cx-17} {cy+4} Z", OCHRE, 2.4)

    # Torso, shaded along the back.
    shape(torso_d, PAPER)
    add('<g clip-path="url(#torso)">')
    parallel_hatch((40, 110, 140, 262), 75, 3.2, lambda x, y: max(0.0, 1.6 - (x - 56) * 0.045))
    # Broad collar across the chest.
    ccx, ccy = 92, 116
    for r_out, r_in, fill in ((38, 31, PAPER), (31, 24, OCHRE), (24, 17, PAPER)):
        d = (f"M{ccx-r_out} {ccy} A{r_out} {r_out} 0 0 0 {ccx+r_out} {ccy} "
             f"L{ccx+r_in} {ccy} A{r_in} {r_in} 0 0 1 {ccx-r_in} {ccy} Z")
        shape(d, fill, 2.2)
    for i in range(1, 12):
        a = i * math.pi / 12
        add(f'<circle cx="{ccx + math.cos(a) * 34.5:.1f}" cy="{ccy + math.sin(a) * 34.5:.1f}" r="2.2" fill="{INK}"/>')
        line([(ccx + math.cos(a) * 25, ccy + math.sin(a) * 25),
              (ccx + math.cos(a) * 30, ccy + math.sin(a) * 30)], 1.3)
    add("</g>")
    shape(torso_d, "none")

    # Neck.
    shape(neck_d, PAPER)

    # Headdress, striped.
    shape(nemes_d, PAPER)
    add('<g clip-path="url(#nemes)">')
    y, band = 30, 0
    while y < 170:
        if band % 2 == 0:
            for j in range(5):
                yy = y + 0.8 + j * 1.6
                line([(30, yy - 4), (110, yy + 4)], 1.3)
        else:
            line([(30, y + 4), (110, y + 12)], 0.45)
        y += 8.5
        band += 1
    add("</g>")
    shape(nemes_d, "none")

    # Head.
    shape(head_d, PAPER)
    add('<g clip-path="url(#head)">')
    contour_hatch(chain((76, 56), ((88, 40), (110, 42), (118, 58))),
                  chain((76, 76), ((84, 92), (106, 90), (118, 68))), 8,
                  lambda t: max(0.0, (t - 0.55) * 3.0), taper=(0.1, 0.9))
    add("</g>")
    shape(head_d, "none")

    # Beak.
    shape(beak_d, PAPER)
    add('<g clip-path="url(#beak)">')
    contour_hatch(beak_top, beak_bottom[::-1], 7, lambda t: max(0.0, (t - 0.3) * 2.4), taper=(0.04, 0.96))
    add("</g>")
    shape(beak_d, "none")
    line(chain((116, 64), ((138, 68), (158, 86), (174, 110))), 0.9)

    # Eye with its painted line.
    line(chain((100, 62), ((94, 62), (88, 64), (82, 68))), 2.4)
    add(f'<circle cx="103" cy="61" r="4.4" fill="{INK}"/>')
    add(f'<circle cx="101.8" cy="59.8" r="1.3" fill="{PAPER}"/>')

    # Palette, upright, with its ink wells and pen slot.
    shape(f"M{px} {py} h{pw} v{ph} h{-pw} Z", PAPER, 2.6)
    add('<g clip-path="url(#palette)">')
    parallel_hatch((px, py, px + pw, py + ph), 90, 2.6,
                   lambda x, y: max(0.0, (x - (px + pw * 0.55)) * 0.16))
    add("</g>")
    add(f'<circle cx="{px + pw / 2}" cy="{py + 12}" r="4.6" fill="{INK}"/>')
    add(f'<circle cx="{px + pw / 2}" cy="{py + 26}" r="4.6" fill="{OCHRE}" stroke="{INK}" stroke-width="1.6"/>')
    add(f'<rect x="{px + pw / 2 - 2.5}" y="{py + 40}" width="5" height="{ph - 56}" rx="2.5" fill="none" stroke="{INK}" stroke-width="1.6"/>')
    for k in range(3):
        line([(px + pw / 2 - 1 + k, py + 46), (px + pw / 2 - 1 + k, py + ph - 22)], 0.7)

    # Near arm, with the reed pen touching the palette.
    shape(upper_arm_d, PAPER)
    add('<g clip-path="url(#upper)">')
    parallel_hatch((90, 120, 140, 200), 60, 3.0, lambda x, y: max(0.0, (y - 150) * 0.03))
    add("</g>")
    shape(upper_arm_d, "none")
    # Armlet.
    line(chain((100, 140), ((106, 140), (114, 136), (120, 132))), 4.2, OCHRE)
    line(chain((100, 137), ((106, 137), (114, 133), (120, 129))), 1.0)
    line(chain((100, 143), ((106, 143), (114, 139), (121, 135))), 1.0)
    shape(forearm_d, PAPER)
    add('<g clip-path="url(#fore)">')
    contour_hatch(forearm_top, forearm_bottom[::-1], 6, lambda t: max(0.0, (t - 0.4) * 2.6), taper=(0.05, 0.95))
    add("</g>")
    shape(forearm_d, "none")
    line([(180, 158), (204, 112)], 2.6)
    # Fist around the pen: knuckles and a thumb on top.
    shape("M174 142 C178 134 190 132 196 140 C200 146 198 156 190 160 C184 162 176 158 174 152 Z", PAPER, 2.4)
    for k in range(3):
        y0 = 145 + k * 4.5
        line(chain((186, y0), ((191, y0 - 1), (195, y0 + 1), (196, y0 + 4))), 1.1)
    shape("M180 140 C184 134 192 132 198 134 C196 138 190 140 184 142 Z", PAPER, 1.8)

    add("</g>")
    add("</g>")
    add("</svg>")


for name, rounded in (("icon.svg", True), ("icon-square.svg", False)):
    out.clear()
    build(rounded)
    path = ASSETS / name
    path.write_text("\n".join(out) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {path}")
