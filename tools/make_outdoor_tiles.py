#!/usr/bin/env python3
"""Outdoor terrain in the 32rogues style: grass, and autotiled paths.

Original art. No pixels are sampled from the 32rogues sheets; the colours
are the pack's own palette (32rogues-palette.png) plus a few in-between
shades for the ground, so these sit beside tiles.png without clashing. Like
the pack's floors, everything here is dark and low-contrast: the ground is a
backdrop for creatures, plants and items, not the subject.

Every tile is full-bleed (no transparency), because the renderer draws one
opaque terrain sprite per cell.

Grass
-----
All grass tiles tile with each other in any arrangement. Each has a band
round its border taken from one ground pattern they all share, and only its
middle is its own, so mixing them at random leaves no seams and no visible
repeat. Tufts, flowers and pebbles stay inside the tile for the same reason.

Paths
-----
Each path is a "blob" autotile set: 47 tiles, one for every way a path cell
can connect to its 8 neighbours. Look up a cell's tile from its neighbour
mask (bits below), after dropping any diagonal whose two neighbouring
orthogonals are not both path, since a corner only matters then:

    NW=128  N=1  NE=2
     W=64    .    E=4
    SW=32   S=16  SE=8

The path's edge lies inside the path cell, so the cell next to a path is
plain grass and needs no tile of its own. The slot of every mask is written
to outdoor.txt.

Usage
-----
    python3 tools/make_outdoor_tiles.py                  # writes assets/custom/outdoor.png + .txt
    python3 tools/make_outdoor_tiles.py --preview DIR    # also a magnified sheet and a demo scene
    python3 tools/make_outdoor_tiles.py --out DIR        # write the sheet somewhere else

The game reads the sheet from assets/custom/outdoor.png (optional: see
assets/README.md), and src/outdoor.rs mirrors where each tile sits, so a
change to the layout here (GRASS, PATHS, BLOCK_LAYOUT) needs the same change
there. In the game, up the screen is +y, so N is the cell at (x, y + 1).
"""
import math
import os
import sys

from PIL import Image

T = 32  # tile size


def hx(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16))


PAL = {
    # ground. #111f12 is the pack's "green bg" (15.a); the two either side
    # of it are in-between shades for the mottling, not palette entries.
    'g_dark': hx('#0d1810'),
    'g_base': hx('#111f12'),
    'g_lift': hx('#152618'),
    # grass blades, base to tip. #17394a is the pack's own tuft colour.
    'blade_1': hx('#17394a'),
    'blade_2': hx('#244b3a'),
    'blade_3': hx('#14605d'),
    'blade_4': hx('#5c855b'),
    # dry grass
    'dry_1': hx('#453625'),
    'dry_2': hx('#874a00'),
    'dry_3': hx('#baa368'),
    # flowers
    'pink': hx('#d77bba'),
    'pink_d': hx('#8f4975'),
    'yellow': hx('#fae898'),
    'yellow_d': hx('#baa368'),
    'white': hx('#def9fc'),
    'white_d': hx('#a5b5d4'),
    'blue': hx('#4c63e0'),
    'blue_d': hx('#42428f'),
    # leaves
    'leaf_1': hx('#812611'),
    'leaf_2': hx('#a84111'),
    'leaf_3': hx('#874a00'),
    'leaf_4': hx('#a15c52'),
    # earth. A path has to read against the grass, so the soil sits a step
    # lighter than the ground; #1a130f (the pack's dark earth) is its shadow.
    'soil_lip': hx('#1a130f'),
    'soil_dark': hx('#241b13'),
    'soil': hx('#2e2318'),
    'soil_2': hx('#382a1c'),
    'clod': hx('#453625'),
    'clod_hi': hx('#8c6a56'),
    'gravel': hx('#29231f'),
    'gravel_1': hx('#453625'),
    'gravel_2': hx('#675353'),
    'gravel_3': hx('#8c6a56'),
    'gravel_4': hx('#5a5875'),
    # stone
    'outline': hx('#0c1317'),
    'cob_dark': hx('#1c1b2b'),
    'cob_1': hx('#2a293f'),
    'cob_2': hx('#302f47'),
    'cob_hi': hx('#42405a'),
    'cob_glint': hx('#5a5875'),
    'peb_hi': hx('#748f93'),
    'peb': hx('#5a5875'),
    'peb_d': hx('#31314b'),
    'flag_1': hx('#323c39'),
    'flag_2': hx('#2c3431'),
    'flag_hi': hx('#46524d'),
    'flag_dark': hx('#1f2725'),
    'moss': hx('#244b3a'),
    'moss_d': hx('#17394a'),
}


# ---------------------------------------------------------------- noise

def h32(*args):
    """Deterministic integer hash of any number of ints."""
    h = 0x9E3779B9
    for a in args:
        h ^= (a & 0xFFFFFFFF) + 0x7F4A7C15 + ((h << 6) & 0xFFFFFFFF) + (h >> 2)
        h &= 0xFFFFFFFF
        h = (h ^ (h >> 15)) * 0x2C1B3C6D & 0xFFFFFFFF
        h = (h ^ (h >> 12)) * 0x297A2D39 & 0xFFFFFFFF
        h ^= h >> 15
    return h


def h01(*args):
    return (h32(*args) & 0xFFFFFF) / float(0x1000000)


class Rng:
    """Small deterministic generator, so the art is identical on every run."""

    def __init__(self, *seed):
        self.state = h32(*seed)

    def next(self):
        self.state = h32(self.state, 0x51ED)
        return (self.state & 0xFFFFFF) / float(0x1000000)

    def uniform(self, a, b):
        return a + (b - a) * self.next()

    def randint(self, a, b):
        return a + int(self.next() * (b - a + 1)) % (b - a + 1)

    def choice(self, seq):
        return seq[int(self.next() * len(seq)) % len(seq)]

    def chance(self, p):
        return self.next() < p


def _smooth(t):
    return t * t * (3 - 2 * t)


def pnoise(x, y, cells, seed):
    """Value noise in [0,1] that wraps every tile (period T in both axes),
    on a lattice of `cells` x `cells` across one tile."""
    s = T / cells
    fx, fy = x / s, y / s
    x0, y0 = math.floor(fx), math.floor(fy)
    tx, ty = _smooth(fx - x0), _smooth(fy - y0)

    def v(ix, iy):
        return h01(ix % cells, iy % cells, seed)

    a = v(x0, y0) + (v(x0 + 1, y0) - v(x0, y0)) * tx
    b = v(x0, y0 + 1) + (v(x0 + 1, y0 + 1) - v(x0, y0 + 1)) * tx
    return a + (b - a) * ty


def fbm(x, y, seed, octaves=((4, 0.6), (8, 0.3), (16, 0.1))):
    return sum(w * pnoise(x, y, c, seed + i * 101) for i, (c, w) in enumerate(octaves))


# ---------------------------------------------------------------- canvas

class Tile:
    def __init__(self, fill='g_base'):
        self.px = [[fill] * T for _ in range(T)]

    def get(self, x, y):
        if 0 <= x < T and 0 <= y < T:
            return self.px[y][x]
        return None

    def set(self, x, y, c):
        if 0 <= x < T and 0 <= y < T and c is not None:
            self.px[y][x] = c

    def image(self):
        im = Image.new('RGB', (T, T))
        im.putdata([PAL[c] for row in self.px for c in row])
        return im


# ---------------------------------------------------------------- ground

GROUND_SHARED_SEED = 7
GROUND_FLAT = 2.0    # px round the border taken purely from the shared pattern
GROUND_BLEND = 7.0   # px over which it hands over to the tile's own pattern


def ground_value(x, y, own_seed):
    """The mottling field: the shared pattern at the border, the tile's own
    in the middle. Points outside the tile count as border."""
    shared = fbm(x + 0.5, y + 0.5, GROUND_SHARED_SEED)
    if own_seed is None:
        return shared
    e = min(x + 0.5, T - 0.5 - x, y + 0.5, T - 0.5 - y)
    t = min(1.0, max(0.0, (e - GROUND_FLAT) / GROUND_BLEND))
    if t <= 0.0:
        return shared
    own = fbm(x + 0.5, y + 0.5, own_seed)
    return shared + (own - shared) * _smooth(t)


def ground_class(v, dark=0.36, lift=0.62):
    if v < dark:
        return 'g_dark'
    if v > lift:
        return 'g_lift'
    return 'g_base'


def ground(own_seed, dark=0.36, lift=0.62):
    """Grass ground: three close shades in soft blobs, with lone pixels
    removed (pixel art reads as noise otherwise)."""
    ext = {}
    for y in range(-1, T + 1):
        for x in range(-1, T + 1):
            ext[(x, y)] = ground_class(ground_value(x, y, own_seed), dark, lift)
    t = Tile()
    for y in range(T):
        for x in range(T):
            c = ext[(x, y)]
            ns = [ext[(x + 1, y)], ext[(x - 1, y)], ext[(x, y + 1)], ext[(x, y - 1)]]
            if c not in ns:
                c = max(set(ns), key=ns.count)
            t.set(x, y, c)
    return t


# ---------------------------------------------------------------- grass details

GREEN = ('blade_1', 'blade_2', 'blade_3', 'blade_4')
DRY = ('dry_1', 'dry_1', 'dry_2', 'dry_3')


def tuft_pixels(rng, size):
    """A clump of grass blades as {(dx, dy): shade}, rooted at (0, 0) and
    growing up (negative dy). Shade 0 is the blade's base, 2 its tip, 3 a
    highlight.

    Drawn the way the pack draws grass (15.b-d): blades one pixel wide with a
    pixel of gap between them, the outer ones bending away in diagonal steps,
    the inner ones rooted a little lower."""
    n = {1: rng.randint(2, 3), 2: rng.randint(3, 4), 3: rng.randint(4, 5)}[size]
    tall = {1: 4, 2: 7, 3: 10}[size]
    pts = {}
    mid = (n - 1) / 2.0
    for i in range(n):
        off = i - mid
        x = int(round(off * 2))
        centre = 1.0 - abs(off) / (mid + 1.0)
        h = max(2, int(round(tall * (0.55 + 0.45 * centre) * rng.uniform(0.8, 1.15))))
        root = 1 if abs(off) < 0.6 and n > 2 else 0
        lean = 0 if abs(off) < 0.6 else (1 if off > 0 else -1)
        if lean == 0 and rng.chance(0.3):
            lean = rng.choice((-1, 1))
        bend = int(h * rng.uniform(0.45, 0.65))   # height where it starts to lean
        step = rng.choice((1, 2))                 # rows per sideways pixel
        for k in range(h):
            if lean and k > bend and (k - bend) % step == 0:
                x += lean
            y = root - k
            shade = 0 if k < h * 0.5 else 1
            if k == h - 1 and h >= 4:
                shade = 2
            pts[(x, y)] = shade
        x = int(round(off * 2))
    return pts


def fits(pts, x, y, margin=1):
    return all(margin <= x + dx < T - margin and margin <= y + dy < T - margin for dx, dy in pts)


def stamp(tile, pts, x, y, colours, hi_chance=0.0, rng=None, shadow='g_dark'):
    for (dx, dy), s in pts.items():
        c = colours[s]
        if s == 2 and rng is not None and rng.chance(hi_chance):
            c = colours[3]
        tile.set(x + dx, y + dy, c)
    # a sliver of shadow under the roots grounds the clump
    if shadow:
        roots = [dx for (dx, dy) in pts if dy == 0]
        for dx in range(min(roots), max(roots) + 1):
            if tile.get(x + dx, y + 1) in ('g_base', 'g_lift'):
                tile.set(x + dx, y + 1, shadow)


def scatter(tile, rng, count, make, margin=1, spacing=5, taken=None, tries=60):
    """Place `count` details from `make(rng) -> (pts, draw)` without
    overlapping, each wholly inside the tile."""
    taken = taken if taken is not None else []
    placed = 0
    for _ in range(tries):
        if placed >= count:
            break
        pts, draw = make(rng)
        x, y = rng.randint(margin + 2, T - margin - 3), rng.randint(margin + 6, T - margin - 2)
        if not fits(pts, x, y, margin):
            continue
        if any((x - tx) ** 2 + (y - ty) ** 2 < spacing ** 2 for tx, ty in taken):
            continue
        draw(tile, pts, x, y)
        taken.append((x, y))
        placed += 1
    return taken


def grass_tufts(tile, rng, count, sizes, colours=GREEN, hi=0.0, spacing=6, taken=None):
    def make(r):
        pts = tuft_pixels(r, r.choice(sizes))
        return pts, lambda t, p, x, y: stamp(t, p, x, y, colours, hi, r)
    return scatter(tile, rng, count, make, spacing=spacing, taken=taken)


def flower_pixels(rng, kind):
    """A small flower on a stem: {(dx, dy): colour key}."""
    petal, petal_d = {'pink': ('pink', 'pink_d'), 'yellow': ('yellow', 'yellow_d'),
                      'white': ('white', 'white_d'), 'blue': ('blue', 'blue_d')}[kind]
    stem = rng.randint(2, 4)
    pts = {}
    for k in range(stem):
        pts[(0, -k)] = 'blade_1' if k == 0 else 'blade_2'
    if rng.chance(0.5):
        pts[(1 if rng.chance(0.5) else -1, -1)] = 'blade_2'   # a leaf
    top = -stem
    if rng.chance(0.55):
        # four petals round a centre
        pts[(0, top - 1)] = 'yellow' if kind != 'yellow' else 'leaf_3'
        pts[(-1, top - 1)] = petal
        pts[(1, top - 1)] = petal_d
        pts[(0, top - 2)] = petal
        pts[(0, top)] = petal_d
    else:
        # a bud seen side-on
        pts[(0, top)] = petal_d
        pts[(-1, top - 1)] = petal
        pts[(0, top - 1)] = petal
        pts[(1, top - 1)] = petal_d
    return pts


def flowers(tile, rng, count, kinds, taken):
    def make(r):
        pts = flower_pixels(r, r.choice(kinds))

        def draw(t, p, x, y):
            for (dx, dy), c in p.items():
                t.set(x + dx, y + dy, c)
            t.set(x, y + 1, 'g_dark')
        return pts, draw
    return scatter(tile, rng, count, make, spacing=5, taken=taken)


CLOVER = [
    # a trefoil seen from above: two leaflets and one below, on a stem
    "LL.LL",
    "LhdhL",
    ".LdL.",
    "..s..",
]


def clover_pixels(rng):
    """A clover: three leaflets, lit from the top-left."""
    pts = {}
    flip = rng.chance(0.5)
    for y, row in enumerate(CLOVER):
        for x, ch in enumerate(row):
            if ch == '.':
                continue
            xx = (4 - x) if flip else x
            c = {'L': 'blade_2', 'h': 'blade_3', 'd': 'blade_1', 's': 'blade_1'}[ch]
            pts[(xx - 2, y - 3)] = c
    pts[(-2 if not flip else 1, -3)] = 'blade_4' if rng.chance(0.6) else 'blade_3'
    return pts


def clovers(tile, rng, count, taken):
    def make(r):
        pts = clover_pixels(r)

        def draw(t, p, x, y):
            for (dx, dy), c in p.items():
                t.set(x + dx, y + dy, c)
        return pts, draw
    return scatter(tile, rng, count, make, spacing=5, taken=taken)


def leaf_pixels(rng):
    col = rng.choice(('leaf_1', 'leaf_2', 'leaf_3', 'leaf_4'))
    vein = 'leaf_1' if col != 'leaf_1' else 'soil_2'
    shapes = [
        {(0, 0): col, (1, 0): col, (2, -1): col, (1, -1): vein, (0, -1): col},
        {(0, 0): col, (0, -1): vein, (1, -1): col, (1, -2): col, (-1, 0): col},
        {(0, 0): col, (1, 0): vein, (2, 0): col, (1, -1): col, (1, 1): col},
        {(0, 0): col, (1, -1): col},
    ]
    return rng.choice(shapes)


def leaves(tile, rng, count, taken):
    def make(r):
        pts = leaf_pixels(r)

        def draw(t, p, x, y):
            for (dx, dy), c in p.items():
                t.set(x + dx, y + dy, c)
            # a shadow on the ground below its lowest pixels
            for (dx, dy) in p:
                if (dx, dy + 1) not in p and t.get(x + dx, y + dy + 1) in ('g_base', 'g_lift'):
                    t.set(x + dx, y + dy + 1, 'g_dark')
        return pts, draw
    return scatter(tile, rng, count, make, spacing=4, taken=taken)


PEBBLES = [
    # h highlight, b body, d shade, o shadow cast on the grass
    [".hb.",
     "hbbd",
     "bbdd",
     ".ooo"],
    [".hh..",
     "hbbbd",
     "bbbdd",
     ".oooo"],
    ["hb.",
     "bd.",
     "oo."],
]


def pebble_pixels(rng):
    rows = rng.choice(PEBBLES)
    pts = {}
    h = len(rows)
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            if ch != '.':
                c = {'h': 'peb_hi', 'b': 'peb', 'd': 'peb_d', 'o': 'outline'}[ch]
                pts[(x, y - h + 1)] = c
    return pts


def pebbles(tile, rng, count, taken):
    def make(r):
        pts = pebble_pixels(r)

        def draw(t, p, x, y):
            for (dx, dy), c in p.items():
                t.set(x + dx, y + dy, c)
        return pts, draw
    return scatter(tile, rng, count, make, spacing=6, taken=taken)


def moss(tile, cx, cy, rx, ry, seed):
    """A cushion of moss: lit along its top-left, shadowed underneath."""
    def inside(x, y):
        d = math.hypot((x - cx) / rx, (y - cy) / ry)
        return d < 1.0 + (pnoise(x, y, 8, seed) - 0.5) * 0.6
    for y in range(T):
        for x in range(T):
            if not (2 <= x < T - 2 and 2 <= y < T - 3) or not inside(x, y):
                continue
            if not inside(x, y - 1) or not inside(x - 1, y):
                c = 'blade_3'
            elif not inside(x, y + 1) or not inside(x + 1, y):
                c = 'moss_d'
            else:
                c = 'moss'
            tile.set(x, y, c)
            if not inside(x, y + 1):
                tile.set(x, y + 1, 'g_dark')
    # a few bright sprigs on the lit side
    for k in range(3):
        x = int(cx - rx * 0.4 + k * rx * 0.4)
        y = int(cy - ry * 0.3 + (k % 2))
        if inside(x, y) and h01(x, y, seed) < 0.7:
            tile.set(x, y, 'blade_4')


def bare_patch(tile, rng, cx, cy, rx, ry, seed):
    """Worn ground where the grass has gone and soil shows through."""
    for y in range(T):
        for x in range(T):
            if not (3 <= x < T - 3 and 3 <= y < T - 3):
                continue
            d = math.hypot((x - cx) / rx, (y - cy) / ry)
            wob = (pnoise(x, y, 8, seed) - 0.5) * 0.55
            if d < 1.0 + wob:
                tile.set(x, y, 'soil' if pnoise(x, y, 16, seed + 1) > 0.4 else 'soil_2')
    # clods, like the pack's dirt (14.b)
    for _ in range(3):
        x = int(cx + rng.uniform(-rx * 0.6, rx * 0.6))
        y = int(cy + rng.uniform(-ry * 0.5, ry * 0.5))
        for dx, dy, c in ((0, 0, 'clod'), (1, 0, 'clod'), (0, -1, 'clod_hi'), (1, 1, 'soil_lip'), (0, 1, 'soil_lip')):
            if tile.get(x + dx, y + dy) in ('soil', 'soil_2'):
                tile.set(x + dx, y + dy, c)
    # the grass bank casts a shadow onto the soil along the patch's top
    for y in range(T):
        for x in range(T):
            if tile.get(x, y) in ('soil', 'soil_2') and tile.get(x, y - 1) in ('g_base', 'g_dark', 'g_lift'):
                tile.set(x, y, 'soil_lip')


# ---------------------------------------------------------------- grass tiles

def grass_short(v):
    rng = Rng(100, v)
    t = ground(1000 + v)
    taken = grass_tufts(t, rng, 1, ((2,), (1,), (2,), (1, 2))[v], spacing=12)
    if v % 2:
        grass_tufts(t, rng, 1, (1,), spacing=10, taken=taken)
    return t


def grass_lush(v):
    rng = Rng(200, v)
    t = ground(1100 + v, dark=0.40, lift=0.58)
    taken = grass_tufts(t, rng, 1, (3,), hi=0.4, spacing=11)
    grass_tufts(t, rng, 1 + v % 2, (1, 2), hi=0.2, spacing=9, taken=taken)
    return t


def grass_meadow(v):
    rng = Rng(300, v)
    t = ground(1200 + v)
    kinds = [('pink',), ('yellow',), ('white',), ('pink', 'white', 'blue')][v % 4]
    taken = grass_tufts(t, rng, 1, (2,), spacing=8)
    flowers(t, rng, 3 + v % 2, kinds, taken)
    return t


def grass_clover(v):
    rng = Rng(400, v)
    t = ground(1300 + v)
    taken = clovers(t, rng, 4 + v, [])
    grass_tufts(t, rng, 1, (1,), spacing=7, taken=taken)
    return t


def grass_moss(v):
    rng = Rng(500, v)
    t = ground(1400 + v)
    spots = [((12, 15, 6, 4), (23, 25, 3, 2)), ((19, 12, 5, 3), (9, 24, 4, 3))][v]
    for i, (cx, cy, rx, ry) in enumerate(spots):
        moss(t, cx, cy, rx, ry, 550 + v * 10 + i)
    grass_tufts(t, rng, 1, (1, 2), spacing=9, taken=[(cx, cy) for cx, cy, _, _ in spots])
    return t


def grass_dry(v):
    rng = Rng(600, v)
    t = ground(1500 + v)
    taken = grass_tufts(t, rng, 1, (2, 3), colours=DRY, hi=0.5, spacing=11)
    grass_tufts(t, rng, 1 + v % 2, (1,), colours=DRY, hi=0.3, spacing=9, taken=taken)
    return t


def grass_leaves(v):
    rng = Rng(700, v)
    t = ground(1600 + v)
    taken = leaves(t, rng, 5 + v, [])
    grass_tufts(t, rng, 1, (1,), spacing=6, taken=taken)
    return t


def grass_pebbles(v):
    rng = Rng(800, v)
    t = ground(1700 + v)
    taken = pebbles(t, rng, 2 + v, [])
    grass_tufts(t, rng, 1, (1, 2), spacing=8, taken=taken)
    return t


def grass_bare(v):
    rng = Rng(900, v)
    t = ground(1800 + v)
    bare_patch(t, rng, 15.5, 17.0, 9.0, 6.0, 909 + v)
    grass_tufts(t, rng, 2, (1,), spacing=8, taken=[(15, 17)])
    return t


GRASS = [
    # (name, fn, variants)
    ('short grass', grass_short, 4),
    ('lush grass', grass_lush, 4),
    ('meadow', grass_meadow, 4),
    ('clover', grass_clover, 2),
    ('moss', grass_moss, 2),
    ('dry grass', grass_dry, 3),
    ('fallen leaves', grass_leaves, 2),
    ('pebbles in grass', grass_pebbles, 2),
    ('worn patch', grass_bare, 1),
]


# ---------------------------------------------------------------- autotiling

N, NE, E, SE, S, SW, W, NW = 1, 2, 4, 8, 16, 32, 64, 128
DIRS = ((N, 0, -1), (NE, 1, -1), (E, 1, 0), (SE, 1, 1), (S, 0, 1), (SW, -1, 1), (W, -1, 0), (NW, -1, -1))
CORNERS = ((NE, N, E), (SE, S, E), (SW, S, W), (NW, N, W))


def reduce_mask(m):
    """Drop each diagonal unless both orthogonals beside it are set: a corner
    neighbour only changes the tile when it closes an inner corner."""
    for d, a, b in CORNERS:
        if not (m & a and m & b):
            m &= ~d
    return m


BLOB_MASKS = sorted({reduce_mask(m) for m in range(256)})
assert len(BLOB_MASKS) == 47


def path_distance(mask, k):
    """How deep a point (tile coords, may lie on the border) is inside the
    path, for a path cell whose neighbours are `mask`.

    A straight edge is the distance to the grass cell beside it. Outer
    corners are rounded off with radius k - w (k is where rounding starts),
    and an inner corner is the distance to the grass cell on the diagonal,
    which rounds it naturally. Capped at k, the depth of open path.

    Two connected path tiles agree on every point of the border between
    them: there the value only depends on cells both of them can see, which
    is what lets whole cobbles straddle tile borders."""
    sides = []
    if not mask & W:
        sides.append(lambda x, y: x)
    if not mask & E:
        sides.append(lambda x, y: T - x)
    if not mask & N:
        sides.append(lambda x, y: y)
    if not mask & S:
        sides.append(lambda x, y: T - y)
    notches = []
    for d, a, b in CORNERS:
        if mask & a and mask & b and not mask & d:
            cx = T if d in (NE, SE) else 0
            cy = 0 if d in (NE, NW) else T
            notches.append((cx, cy, d))

    def depth(x, y):
        acc = 0.0
        for f in sides:
            a = f(x, y)
            if a < k:
                acc += (k - a) ** 2
        best = k - math.sqrt(acc)
        for cx, cy, d in notches:
            dx = (cx - x) if cx == T else x
            dy = y if cy == 0 else (T - y)
            best = min(best, math.hypot(max(0.0, dx), max(0.0, dy)))
        return best
    return depth


def edge_noise(x, y, seed, amp):
    """A wobble in [-amp, amp] for the path's edge, wrapping every tile so
    an edge carries on unbroken into the next tile."""
    return (fbm(x, y, seed, ((4, 0.55), (8, 0.35), (16, 0.10))) - 0.5) * 2.0 * amp


def is_grass(c):
    return c in ('g_base', 'g_dark', 'g_lift')


def path_ground():
    """The grass a path tile's edges show: the shared ground on its own, so
    it meets any grass tile beside it without a seam."""
    return ground(None)


def mask_rng(style, mask, variant):
    return Rng(style, mask, variant)


def overhang(t, rng, inside, chance=0.35):
    """Grass along a path's lower edge, leaning up over it (in this view
    the bank south of a path is the one you see the top of)."""
    for x in range(1, T - 1):
        for y in range(2, T - 1):
            if inside(x, y) or not inside(x, y - 1):
                continue
            if h01(x, y, 4242) > chance:
                continue
            h = 2 + int(h01(x, y, 4343) * 3)
            lean = (-1, 0, 1)[int(h01(x, y, 4444) * 3)]
            ok = True
            pts = []
            xx = x
            for k in range(h):
                if k == h - 1 and lean:
                    xx += lean
                if not (0 <= xx < T and 0 <= y - k < T):
                    ok = False
                    break
                pts.append((xx, y - k, 'blade_1' if k == 0 else ('blade_3' if k == h - 1 else 'blade_2')))
            if ok:
                for px_, py_, c in pts:
                    t.set(px_, py_, c)
            break


def sprinkle_grass(t, rng, inside, count):
    """A few small tufts in the grass a path tile shows, kept off the path."""
    def make(r):
        pts = tuft_pixels(r, 1)
        return pts, lambda tt, p, x, y: stamp(tt, p, x, y, GREEN, 0.0, r)
    for _ in range(40):
        if count <= 0:
            break
        pts, draw = make(rng)
        x, y = rng.randint(3, T - 4), rng.randint(6, T - 3)
        if not fits(pts, x, y, 1):
            continue
        near = any(inside(x + dx + ox, y + dy + oy) for dx, dy in pts for ox in (-2, 0, 2) for oy in (-2, 0, 2))
        if near:
            continue
        draw(t, pts, x, y)
        count -= 1


# ---------------------------------------------------------------- dirt track

def dirt_texture(x, y):
    """The track's soil, wrapping every tile: mottled, with clods."""
    v = fbm(x + 0.5, y + 0.5, 3100)
    c = 'soil_dark' if v < 0.38 else ('soil_2' if v > 0.63 else 'soil')
    return c


DIRT_CLODS = [(5, 9), (19, 4), (26, 15), (12, 21), (22, 27), (3, 26), (15, 13), (29, 6)]


def dirt_tile(mask, variant=0):
    rng = mask_rng(31, mask, variant)
    w, k, amp = 6.0, 12.5, 1.8
    depth = path_distance(mask, k)

    def inside(x, y):
        return depth(x + 0.5, y + 0.5) > w + edge_noise(x + 0.5, y + 0.5, 3000, amp)

    t = path_ground()
    for y in range(T):
        for x in range(T):
            if inside(x, y):
                t.set(x, y, dirt_texture(x, y))
    # clods and stones, on a pattern that wraps so it runs on across tiles
    clods = list(DIRT_CLODS)
    if variant:
        clods = [((cx * 7 + variant * 11) % T, (cy * 5 + variant * 3) % T) for cx, cy in DIRT_CLODS[:6]]
    for i, (cx, cy) in enumerate(clods):
        shape = ((0, 0, 'clod'), (1, 0, 'clod'), (0, -1, 'clod_hi' if i % 3 == 0 else 'clod'),
                 (0, 1, 'soil_lip'), (1, 1, 'soil_lip'))
        if i % 2:
            shape = ((0, 0, 'clod'), (0, -1, 'clod'), (0, 1, 'soil_lip'))
        for dx, dy, c in shape:
            x, y = (cx + dx) % T, (cy + dy) % T
            if inside(x, y) and inside(cx % T, cy % T):
                t.set(x, y, c)
    # the bank's shadow on the soil below a grass edge, and a dark rim
    # where soil gives way to grass on the left
    for y in range(T):
        for x in range(T):
            if not inside(x, y):
                continue
            if not inside(x, y - 1) or (not inside(x, y - 2) and h01(x, y, 3) < 0.5):
                t.set(x, y, 'soil_lip')
            elif not inside(x - 1, y) or not inside(x + 1, y):
                t.set(x, y, 'soil_dark')
    overhang(t, rng, inside)
    sprinkle_grass(t, rng, inside, 1)
    if variant == 1:
        # a puddle-dark rut
        for x in range(10, 22):
            for y in (15, 16):
                if inside(x, y) and h01(x, y, 9) < 0.8:
                    t.set(x, y, 'soil_lip' if y == 15 else 'soil_dark')
    return t


# ---------------------------------------------------------------- gravel

def gravel_tile(mask, variant=0):
    rng = mask_rng(32, mask, variant)
    w, k, amp = 5.0, 11.0, 1.4
    depth = path_distance(mask, k)

    def margin(x, y):
        return depth(x + 0.5, y + 0.5) - (w + edge_noise(x + 0.5, y + 0.5, 3200, amp))

    def inside(x, y):
        return margin(x, y) > 0

    t = path_ground()
    for y in range(T):
        for x in range(T):
            if inside(x, y):
                t.set(x, y, 'gravel')
    # stones on a jittered 4px lattice that wraps, so they're evenly spread
    # and run on across tiles; thinning out past the edge so a few lie
    # spilled in the grass
    for gy in range(0, T, 3):
        for gx in range(0, T, 3):
            if h01(gx, gy, 3300 + variant) < 0.25:
                continue
            x = (gx + int(h01(gx, gy, 3301) * 3)) % T
            y = (gy + int(h01(gx, gy, 3302) * 3)) % T
            m = margin(x, y)
            if m <= 0 and (m < -2.5 or h01(gx, gy, 3303) < 0.75):
                continue
            r = h01(gx, gy, 3400)
            c = 'gravel_1' if r < 0.55 else ('gravel_2' if r < 0.8 else ('gravel_4' if r < 0.92 else 'gravel_3'))
            t.set(x, y, c)
            if r > 0.7 and x + 1 < T and inside(x + 1, y) == inside(x, y):
                t.set(x + 1, y, 'gravel_1')
            if y + 1 < T:
                below = t.get(x, y + 1)
                if below == 'gravel':
                    t.set(x, y + 1, 'soil_lip')
                elif is_grass(below):
                    t.set(x, y + 1, 'g_dark')
    for y in range(T):
        for x in range(T):
            if inside(x, y) and not inside(x, y - 1) and t.get(x, y) == 'gravel':
                t.set(x, y, 'soil_lip')
    sprinkle_grass(t, rng, inside, 1)
    if variant == 1:
        # a weed pushing up through the gravel
        pts = tuft_pixels(Rng(77), 1)
        stamp(t, pts, 15, 18, GREEN, 0.0, rng, shadow=None)
    return t


# ---------------------------------------------------------------- stones on a torus

def voronoi(seeds, metric):
    """Assign every pixel of the wrapping tile to its nearest seed.
    Returns owner[y][x], edge[y][x] (gap to the second-nearest) and each
    seed's extent as (min dx, max dx, min dy, max dy) from the seed."""
    owner = [[0] * T for _ in range(T)]
    edge = [[0.0] * T for _ in range(T)]
    ext = [[99, -99, 99, -99] for _ in seeds]
    for y in range(T):
        for x in range(T):
            best = []
            for i, (sx, sy) in enumerate(seeds):
                dx = (x + 0.5 - sx + T / 2) % T - T / 2
                dy = (y + 0.5 - sy + T / 2) % T - T / 2
                best.append((metric(dx, dy, i), i, dx, dy))
            best.sort()
            d1, i, dx, dy = best[0]
            owner[y][x] = i
            edge[y][x] = best[1][0] - d1
            e = ext[i]
            ix, iy = math.floor(dx + 0.5), math.floor(dy + 0.5)   # pixel offset from the seed's pixel
            e[0], e[1], e[2], e[3] = min(e[0], ix), max(e[1], ix), min(e[2], iy), max(e[3], iy)
    return owner, edge, ext


def stone_instance(seeds, i, x, y):
    """Which copy of seed i (they repeat every tile) pixel (x, y) belongs to,
    as that copy's position in this tile's coordinates."""
    sx, sy = seeds[i]
    dx = (x + 0.5 - sx + T / 2) % T - T / 2
    dy = (y + 0.5 - sy + T / 2) % T - T / 2
    return (x + 0.5 - dx, y + 0.5 - dy)


def stone_eval_point(seeds, ext, i, inst):
    """The point whose path depth decides whether a stone is laid. A stone
    across a tile border is judged on that border and one across a corner on
    the corner, so both tiles it touches judge it on the same point and
    make the same call."""
    ix, iy = inst
    sxp, syp = math.floor(ix), math.floor(iy)
    e = ext[i]
    x0, x1, y0, y1 = sxp + e[0], sxp + e[1], syp + e[2], syp + e[3]
    bx = 0 if x0 < 0 <= x1 else (T if x0 < T <= x1 else None)
    by = 0 if y0 < 0 <= y1 else (T if y0 < T <= y1 else None)
    return (ix if bx is None else bx, iy if by is None else by)


def stones_layout(seed, rows, cols, jitter, stagger):
    rng = Rng(seed)
    seeds = []
    for r in range(rows):
        for c in range(cols):
            off = (0.5 if (stagger and r % 2) else 0.0)
            sx = ((c + 0.5 + off) * T / cols + rng.uniform(-jitter, jitter)) % T
            sy = ((r + 0.5) * T / rows + rng.uniform(-jitter, jitter)) % T
            seeds.append((sx, sy))
    return seeds


COBBLE_SEEDS = stones_layout(5100, 4, 4, 1.3, True)
COBBLE_WEIGHT = [0.85 + 0.3 * h01(i, 5150) for i in range(len(COBBLE_SEEDS))]
COBBLES = voronoi(COBBLE_SEEDS, lambda dx, dy, i: math.hypot(dx * 1.0, dy * 1.12) * COBBLE_WEIGHT[i])



def laid_stones(mask, layout, seeds, w, k, amp, noise_seed, variant_drop=()):
    """Which stones are laid in this tile: {(stone, instance)} -> bool."""
    owner, edge, ext = layout
    depth = path_distance(mask, k)
    laid = {}
    for y in range(T):
        for x in range(T):
            i = owner[y][x]
            inst = stone_instance(seeds, i, x, y)
            key = (i, round(inst[0]), round(inst[1]))
            if key in laid:
                continue
            ex, ey = stone_eval_point(seeds, ext, i, inst)
            wob = (h01(i, noise_seed) - 0.5) * 2.0 * amp
            laid[key] = depth(ex, ey) > w + wob and i not in variant_drop
    return laid


def stone_key(seeds, owner, x, y):
    i = owner[y][x]
    inst = stone_instance(seeds, i, x, y)
    return (i, round(inst[0]), round(inst[1]))


# ---------------------------------------------------------------- cobbles

COB_W, COB_K, COB_AMP = 7.0, 12.0, 1.5


def cobble_tile(mask, variant=0):
    rng = mask_rng(33, mask, variant)
    owner, edge, ext = COBBLES
    drop = ()
    if variant == 1:
        drop = (5,)    # a stone gone, a weed in the gap
    laid = laid_stones(mask, COBBLES, COBBLE_SEEDS, COB_W, COB_K, COB_AMP, 5300, drop)

    def stone_at(x, y):
        xx, yy = x % T, y % T
        if not (0 <= x < T and 0 <= y < T):
            # beyond the tile: judge by this tile's view of the stone
            i = owner[yy][xx]
            inst = stone_instance(COBBLE_SEEDS, i, x, y)
            key = (i, round(inst[0]), round(inst[1]))
            if key not in laid:
                ex, ey = stone_eval_point(COBBLE_SEEDS, ext, i, inst)
                d = path_distance(mask, COB_K)(ex, ey)
                laid[key] = d > COB_W + (h01(i, 5300) - 0.5) * 2.0 * COB_AMP and i not in drop
            return laid[key] and edge[yy][xx] >= 1.0
        return laid[stone_key(COBBLE_SEEDS, owner, x, y)] and edge[y][x] >= 1.0

    def laid_near(x, y):
        """Is (x, y) a joint between laid stones?"""
        xx, yy = x % T, y % T
        return laid.get(stone_key(COBBLE_SEEDS, owner, xx, yy), False) if (0 <= x < T and 0 <= y < T) else False

    t = path_ground()
    for y in range(T):
        for x in range(T):
            i = owner[y][x]
            if stone_at(x, y):
                body = 'cob_2' if h01(i, 5400) < 0.4 else 'cob_1'
                c = body
                # lit along the top, shaded along the bottom: flat-topped
                # setts, not pillows
                if not stone_at(x, y - 1):
                    c = 'cob_hi'
                elif not stone_at(x, y + 1):
                    c = 'cob_dark'
                elif not stone_at(x + 1, y) and h01(x, y, 5401) < 0.6:
                    c = 'cob_dark'
                t.set(x, y, c)
            else:
                # a joint: soil between stones that are laid, grass elsewhere
                neigh = [stone_at(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))]
                if laid_near(x, y) or sum(neigh) >= 2:
                    t.set(x, y, 'soil_lip')
                elif neigh[3]:
                    t.set(x, y, 'outline')      # shadow under a stone at the road's edge
                elif any(neigh):
                    t.set(x, y, 'g_dark')
                elif stone_at(x, y - 2) and is_grass(t.get(x, y)):
                    t.set(x, y, 'g_dark')
    # a glint on the top-left of a few stones
    for y in range(1, T):
        for x in range(1, T):
            if t.get(x, y) == 'cob_hi' and t.get(x - 1, y - 1) not in ('cob_hi', 'cob_1', 'cob_2', 'cob_dark') \
                    and h01(owner[y][x], 5500) < 0.45 and h01(x, y, 5501) < 0.5:
                t.set(x, y, 'cob_glint')
    if variant == 1:
        for i in drop:
            sx, sy = COBBLE_SEEDS[i]
            stamp(t, tuft_pixels(Rng(i, 5600), 1), int(sx), int(sy) + 2, GREEN, 0.0, rng, shadow=None)

    def inside(x, y):
        return stone_at(x, y) or laid_near(x, y)
    sprinkle_grass(t, rng, inside, 1)
    return t


# ---------------------------------------------------------------- flagstones

# Paving laid in courses, each course a row of rectangular slabs with its
# joints staggered from the next. (top, height, x of each joint): the
# pattern wraps every tile, so the courses carry on across tiles.
FLAG_COURSES = [
    (0, 16, (2, 17)),
    (16, 16, (9, 23)),
]


def flag_slab(x, y):
    """Which slab pixel (x, y) of the wrapping pattern is in, and whether it
    sits on a joint: (slab id, joint, top row, bottom row, left, right)."""
    x, y = x % T, y % T
    for row, (top, h, joints) in enumerate(FLAG_COURSES):
        if top <= y < top + h:
            js = sorted(joints)
            for k, j in enumerate(js):
                nxt = js[(k + 1) % len(js)]
                span = (nxt - j) % T
                if (x - j) % T < span:
                    rx = (x - j) % T
                    joint = rx == 0 or y == top
                    return (row * 8 + k, joint, y == top + 1, y == top + h - 1, rx == 1, rx == span - 1)
    return (0, True, False, False, False, False)


def flag_tile(mask, variant=0):
    rng = mask_rng(34, mask, variant)
    w, k = 4.0, 10.0
    depth = path_distance(mask, k)

    def inside(x, y):
        return depth(x + 0.5, y + 0.5) > w

    t = path_ground()
    for y in range(T):
        for x in range(T):
            if not inside(x, y):
                continue
            slab, joint, top, bottom, left, right = flag_slab(x, y)
            if joint:
                # grass and moss creep into a few of the joints
                c = 'moss' if pnoise(x, y, 16, 5700) > 0.68 else 'flag_dark'
            elif top:
                c = 'flag_hi'
            elif bottom or (right and h01(slab, 5801) < 0.5):
                c = 'flag_2'
            else:
                c = 'flag_1' if h01(slab, 5800) < 0.6 else 'flag_2'
                if c == 'flag_2' and left:
                    c = 'flag_1'
            t.set(x, y, c)
    # a hairline crack across a few slabs
    for slab_seed in range(3):
        if h01(slab_seed, 5900 + variant) < 0.5:
            continue
        x0 = int(h01(slab_seed, 5901) * T)
        y0 = int(h01(slab_seed, 5902) * T)
        for kk in range(5):
            xx, yy = (x0 + kk) % T, (y0 + (kk * 2) // 3) % T
            if inside(xx, yy) and t.get(xx, yy) in ('flag_1', 'flag_2'):
                t.set(xx, yy, 'flag_dark')
    # the laid edge: a dark kerb line, then the grass bank's shadow below
    for y in range(T):
        for x in range(T):
            if inside(x, y) and not (inside(x - 1, y) and inside(x + 1, y) and inside(x, y - 1) and inside(x, y + 1)):
                t.set(x, y, 'outline')
            elif not inside(x, y) and inside(x, y - 1) and is_grass(t.get(x, y)):
                t.set(x, y, 'g_dark')
    if variant == 1:
        # moss spreading over one slab
        for y in range(T):
            for x in range(T):
                if inside(x, y) and flag_slab(x, y)[0] == 8 and t.get(x, y) in ('flag_1', 'flag_2', 'flag_hi') \
                        and pnoise(x, y, 8, 6000) > 0.45:
                    t.set(x, y, 'moss' if pnoise(x, y, 16, 6001) > 0.4 else 'moss_d')
    overhang(t, rng, inside, chance=0.25)
    sprinkle_grass(t, rng, inside, 1)
    return t


PATHS = [
    ('dirt track', dirt_tile),
    ('cobbles', cobble_tile),
    ('flagstones', flag_tile),
    ('gravel', gravel_tile),
]


# ---------------------------------------------------------------- the sheet

COLS = 16
GRASS_ROWS = 2
PATH_ROWS = 3

# Where each path tile sits in its 3-row block, so the sheet reads as a
# picture: a filled square (corners, edges, middle), a one-wide path down
# and across, the four inner corners round a single missing cell, the lone
# cell, a spare middle, then every junction.
FULL = 255
BLOCK_LAYOUT = {
    (0, 0): E | S | SE,              (1, 0): E | W | S | SE | SW,             (2, 0): W | S | SW,
    (0, 1): N | S | E | NE | SE,     (1, 1): FULL,                            (2, 1): N | S | W | NW | SW,
    (0, 2): N | E | NE,              (1, 2): N | E | W | NE | NW,             (2, 2): N | W | NW,
    (3, 0): S, (3, 1): N | S, (3, 2): N,
    (4, 0): E, (5, 0): E | W, (6, 0): W,
    (4, 1): FULL & ~SE, (5, 1): FULL & ~SW,
    (4, 2): FULL & ~NE, (5, 2): FULL & ~NW,
    (6, 1): 0,
    # (6, 2) is the spare middle
}
SPARE_SLOT = (6, 2)


def block_slots():
    """[(col, row_in_block, mask, variant)] for one path's 48 slots."""
    used = set(BLOCK_LAYOUT.values())
    rest = [m for m in BLOB_MASKS if m not in used]
    slots = [(c, r, m, 0) for (c, r), m in BLOCK_LAYOUT.items()]
    slots.append((SPARE_SLOT[0], SPARE_SLOT[1], FULL, 1))
    free = [(c, r) for r in range(PATH_ROWS) for c in range(7, COLS)]
    assert len(free) == len(rest)
    slots += [(c, r, m, 0) for (c, r), m in zip(free, rest)]
    return slots


def letter(c):
    return chr(ord('a') + c)


def build_sheet():
    """Returns the sheet image and the doc lines for outdoor.txt."""
    rows = GRASS_ROWS + PATH_ROWS * len(PATHS)
    sheet = Image.new('RGBA', (COLS * T, rows * T), (0, 0, 0, 0))
    doc = []
    slot = 0
    for name, fn, n in GRASS:
        for v in range(n):
            c, r = slot % COLS, slot // COLS
            sheet.paste(fn(v).image(), (c * T, r * T))
            doc.append(f'{r + 1}.{letter(c)}. {name} {v + 1}')
            slot += 1
    doc.append('')
    lookup = {}
    for p, (name, fn) in enumerate(PATHS):
        top = GRASS_ROWS + p * PATH_ROWS
        entries = []
        for c, r, mask, variant in block_slots():
            sheet.paste(fn(mask, variant).image(), (c * T, (top + r) * T))
            entries.append((r, c, mask, variant))
        entries.sort()
        for r, c, mask, variant in entries:
            label = describe(mask) + (' (variant)' if variant else '')
            doc.append(f'{top + r + 1}.{letter(c)}. {name}: {label}  [mask {mask}]')
            if not variant:
                lookup[(name, mask)] = (top + r + 1, c)
        doc.append('')
    return sheet, doc, lookup


def describe(mask):
    names = [('N', N), ('NE', NE), ('E', E), ('SE', SE), ('S', S), ('SW', SW), ('W', W), ('NW', NW)]
    if mask == FULL:
        return 'middle'
    if mask == 0:
        return 'lone cell'
    return 'joins ' + ' '.join(n for n, b in names if mask & b)


# ---------------------------------------------------------------- preview

def demo_scene():
    """A small field laid out to show every kind of edge: returns
    (cells, grass) where cells[y][x] is a path name or None."""
    W_, H_ = 22, 15
    cells = [[None] * W_ for _ in range(H_)]

    def put(name, pts):
        for x, y in pts:
            if 0 <= x < W_ and 0 <= y < H_:
                cells[y][x] = name
    # a dirt track wandering left to right
    track = [(0, 9), (1, 9), (2, 9), (2, 8), (3, 8), (4, 8), (5, 8), (5, 7), (6, 7), (7, 7), (8, 7),
             (9, 7), (9, 8), (10, 8), (11, 8)]
    put('dirt track', track)
    # a cobbled road down the middle, widening into a square
    put('cobbles', [(12, y) for y in range(0, 15)] + [(13, y) for y in range(0, 15)])
    put('cobbles', [(x, y) for x in range(14, 18) for y in range(6, 10)])
    put('cobbles', [(11, 8), (10, 8)])
    # a flagstone yard with a planter left out of the middle
    put('flagstones', [(x, y) for x in range(15, 21) for y in range(1, 5)])
    cells[2][17] = None
    cells[2][18] = None
    put('flagstones', [(16, 5)])
    # gravel: a loop, one wide, and a spur
    loop = [(x, 11) for x in range(2, 8)] + [(x, 14) for x in range(2, 8)] + \
           [(2, y) for y in range(11, 15)] + [(7, y) for y in range(11, 15)]
    put('gravel', loop + [(8, 12), (9, 12), (10, 12), (11, 12)])
    put('gravel', [(4, 1), (5, 1), (4, 2), (5, 2), (6, 2), (4, 3)])
    put('dirt track', [(1, 2), (1, 3), (2, 3), (3, 3), (1, 1), (1, 4), (1, 5), (1, 6)])
    put('dirt track', [(18, 11), (19, 12)])
    # grass: patches of each kind
    grass = [[None] * W_ for _ in range(H_)]
    for y in range(H_):
        for x in range(W_):
            v = pnoise(x * 4.0, y * 4.0, 8, 61)
            u = h01(x, y, 62)
            if v > 0.68:
                kind = 'lush grass'
            elif v < 0.30:
                kind = 'dry grass'
            elif u < 0.12:
                kind = 'meadow'
            elif u < 0.17:
                kind = 'clover'
            elif u < 0.20:
                kind = 'pebbles in grass'
            elif u < 0.22:
                kind = 'moss'
            else:
                kind = 'short grass'
            grass[y][x] = kind
    for x, y in ((8, 2), (9, 2), (8, 3), (10, 3), (9, 4)):
        grass[y][x] = 'fallen leaves'
    grass[10][19] = 'worn patch'
    return cells, grass


def cell_mask(cells, x, y):
    name = cells[y][x]
    m = 0
    for bit, dx, dy in DIRS:
        xx, yy = x + dx, y + dy
        if 0 <= yy < len(cells) and 0 <= xx < len(cells[0]):
            if cells[yy][xx] == name:
                m |= bit
        else:
            m |= bit    # off the map counts as more of the same
    return reduce_mask(m)


def render_preview(sheet, lookup, out_dir, stock_dir):
    cells, grass = demo_scene()
    H_, W_ = len(cells), len(cells[0])
    scene = Image.new('RGBA', (W_ * T, H_ * T))
    grass_slots = {}
    slot = 0
    for name, fn, n in GRASS:
        grass_slots[name] = [(slot // COLS, slot % COLS) for slot in range(slot, slot + n)]
        slot += n

    def tile_at(r, c):
        return sheet.crop((c * T, r * T, c * T + T, r * T + T))
    for y in range(H_):
        for x in range(W_):
            if cells[y][x]:
                m = cell_mask(cells, x, y)
                r, c = lookup[(cells[y][x], m)]
                if m == FULL and h01(x, y, 70) < 0.15:
                    # the spare middle, now and then
                    top = GRASS_ROWS + [p[0] for p in PATHS].index(cells[y][x]) * PATH_ROWS
                    r, c = top + SPARE_SLOT[1] + 1, SPARE_SLOT[0]
                scene.paste(tile_at(r - 1, c), (x * T, y * T))
            else:
                opts = grass_slots[grass[y][x]]
                r, c = opts[h32(x, y, 71) % len(opts)]
                scene.paste(tile_at(r, c), (x * T, y * T))
    # some of the pack's own sprites, to check the ground sits under them
    stock = os.path.join(stock_dir, 'tiles.png')
    if os.path.exists(stock):
        tiles = Image.open(stock).convert('RGBA')

        def sprite(row, col):
            return tiles.crop((col * T, (row - 1) * T, col * T + T, row * T))
        props = [((3, 5), (26, 2)), ((8, 1), (26, 1)), ((21, 6), (26, 2)), ((20, 13), (26, 0)),
                 ((10, 1), (20, 0)), ((14, 12), (20, 4)), ((3, 6), (20, 7)), ((9, 10), (19, 0)),
                 ((17, 2), (18, 4)), ((18, 2), (20, 14)), ((0, 13), (21, 0)), ((10, 5), (20, 9)),
                 ((19, 9), (26, 1)), ((6, 4), (20, 2))]
        for (x, y), (r, c) in props:
            if 0 <= x < W_ and 0 <= y < H_:
                scene.alpha_composite(sprite(r, c), (x * T, y * T))
    rogues = os.path.join(stock_dir, 'rogues.png')
    if os.path.exists(rogues):
        rg = Image.open(rogues).convert('RGBA')
        for (x, y), (r, c) in (((7, 7), (1, 0)), ((13, 4), (2, 1))):
            scene.alpha_composite(rg.crop((c * T, r * T, c * T + T, r * T + T)), (x * T, y * T))
    scene.convert('RGB').resize((W_ * T * 2, H_ * T * 2), Image.NEAREST).save(os.path.join(out_dir, 'outdoor-demo.png'))
    # the sheet itself, magnified, on a neutral backdrop with a grid
    z = 3
    big = Image.new('RGB', (sheet.width * z + COLS, sheet.height * z + sheet.height // T), (60, 0, 60))
    for r in range(sheet.height // T):
        for c in range(COLS):
            t = sheet.crop((c * T, r * T, c * T + T, r * T + T)).resize((T * z, T * z), Image.NEAREST)
            big.paste(t, (c * (T * z + 1), r * (T * z + 1)), t)
    big.save(os.path.join(out_dir, 'outdoor-sheet-x3.png'))


def check_against_game(root):
    """src/outdoor.rs mirrors where each path tile sits (LAYOUT). Read it
    back and stop if it no longer matches, rather than write a sheet the
    game would draw the wrong corners from."""
    import re
    src = os.path.join(root, 'src', 'outdoor.rs')
    if not os.path.exists(src):
        return
    text = open(src).read()
    m = re.search(r'const LAYOUT:[^=]*=\s*\[(.*?)\];', text, re.S)
    if not m:
        sys.exit(f'{src}: no LAYOUT table found to check against')
    rows = [[int(v) for v in re.findall(r'\d+', r)] for r in re.findall(r'\[([^\[\]]*)\]', m.group(1))]
    ours = [[None] * COLS for _ in range(PATH_ROWS)]
    for c, r, mask, _ in block_slots():
        ours[r][c] = mask
    if rows != ours:
        sys.exit(f'{src}: LAYOUT no longer matches this script\'s BLOCK_LAYOUT; change both together.\n'
                 f'script: {ours}\ngame:   {rows}')


def main():
    args = [a for a in sys.argv[1:]]
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.dirname(here)
    out = os.path.join(root, 'assets', 'custom')
    preview = None
    if '--preview' in args:
        preview = args[args.index('--preview') + 1]
    if '--out' in args:
        out = args[args.index('--out') + 1]
    check_against_game(root)
    os.makedirs(out, exist_ok=True)
    sheet, doc, lookup = build_sheet()
    sheet.save(os.path.join(out, 'outdoor.png'))
    header = [
        'Outdoor terrain, drawn by tools/make_outdoor_tiles.py.',
        f'{COLS} columns, {sheet.height // T} rows of 32x32. Row.letter as in the 32rogues .txt files.',
        'Path masks: N=1 NE=2 E=4 SE=8 S=16 SW=32 W=64 NW=128; a diagonal counts only',
        'when both orthogonals beside it are path.',
        '',
    ]
    with open(os.path.join(out, 'outdoor.txt'), 'w') as f:
        f.write('\n'.join(header + doc) + '\n')
    print(f'wrote {os.path.join(out, "outdoor.png")} ({sheet.width}x{sheet.height}) and outdoor.txt')
    if preview:
        os.makedirs(preview, exist_ok=True)
        render_preview(sheet, lookup, preview, os.path.join(root, 'assets', '32rogues'))
        print(f'previews in {preview}')


if __name__ == '__main__':
    main()
