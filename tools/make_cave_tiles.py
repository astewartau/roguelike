#!/usr/bin/env python3
"""Hand-drawn 32x32 cave tiles in the 32rogues style.

Original art — no pixels are sampled from or derived from the 32rogues sheets.
Only the palette hexes are shared, so these sit alongside tiles.png without
clashing. Mirrors the conventions in make_tiles.py.

Decal tiles (stalagmites, mushrooms, crystals) draw on a transparent
background so the floor beneath shows through, like 19.a rocks / 21.a
mushrooms. Terrain tiles (ore wall, rubble floor) are full-bleed.
"""
from PIL import Image

PAL = {
    # stone / rock
    'K': '#def9fc',  # stone highlight
    'C': '#748f93',  # gray blue
    'd': '#696a6a',  # stone mid
    'N': '#17394a',  # navy shadow
    'o': '#0c1317',  # near-black outline
    'J': '#323c39',  # dark slate
    # earth / dirt
    'E': '#453625',  # earth mid
    'm': '#8c6a56',  # earth light
    'D': '#1a130f',  # earth dark
    'w': '#874a00',  # wood / ochre
    # fungal glow (cyan-green)
    'G': '#00ff8c',  # glow bright
    'g': '#07bb79',  # glow mid
    'e': '#14605d',  # glow dark
    'q': '#00edff',  # cyan bright
    'Q': '#3494cb',  # cyan mid
    # crystal
    'c': '#def9fc',  # crystal white
    'S': '#3494cb',  # crystal blue
    'n': '#12274d',  # crystal deep
    'P': '#76428a',  # purple mid
    # ore / gold
    'Y': '#fafa82',  # gold bright
    'y': '#fae898',  # gold pale
    'T': '#baa368',  # gold mid
    'A': '#a84111',  # copper dark
}


def hex2rgba(s):
    return (int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16), 255)


def new_grid():
    return [['.' for _ in range(32)] for _ in range(32)]


def px(g, x, y, c):
    if 0 <= x < 32 and 0 <= y < 32:
        g[y][x] = c


def hline(g, y, x0, x1, c):
    for x in range(x0, x1 + 1):
        px(g, x, y, c)


def rect(g, x0, y0, x1, y1, c):
    for y in range(y0, y1 + 1):
        hline(g, y, x0, x1, c)


def cone(g, cx, base_y, h, halfw, lit, mid, dark, outline):
    """A tapered spire: lit on the upper-left, shadowed right, dark outline."""
    for i in range(h):
        y = base_y - i
        t = i / max(1, h - 1)
        w = max(0, int(round(halfw * (1.0 - t * 0.92))))
        x0, x1 = cx - w, cx + w
        for x in range(x0, x1 + 1):
            if x == x0 or x == x1:
                px(g, x, y, outline)
            elif x <= cx - max(1, w // 2):
                px(g, x, y, lit)
            elif x >= cx + max(1, w // 2):
                px(g, x, y, dark)
            else:
                px(g, x, y, mid)
        if w == 0:
            px(g, cx, y, lit)
    hline(g, base_y + 1, cx - halfw, cx + halfw, outline)


def shard(g, tip, base, halfw, bright, mid, deep, outline):
    """An angular crystal shard from `base` to `tip` (both (x, y))."""
    tx, ty = tip
    bx, by = base
    h = by - ty
    for i in range(h + 1):
        y = ty + i
        t = i / max(1, h)
        x = int(round(tx + (bx - tx) * t))
        w = max(0, int(round(halfw * t)))
        for dx in range(-w, w + 1):
            xx = x + dx
            if dx == -w or dx == w:
                px(g, xx, y, outline)
            elif dx < 0:
                px(g, xx, y, bright)
            elif dx == 0:
                px(g, xx, y, mid)
            else:
                px(g, xx, y, deep)
        if w == 0:
            px(g, x, y, bright)


def speckle(g, cells, c):
    for (x, y) in cells:
        px(g, x, y, c)


# ---------------------------------------------------------------- tiles

def stalagmites():
    """Floor obstacle: three stone spires. Blocks movement, not vision."""
    g = new_grid()
    # chunkier than a literal cone — 32rogues silhouettes are bold, and thin
    # spires disappear against the floor at actual size
    cone(g, 11, 28, 18, 7, 'C', 'd', 'N', 'o')
    cone(g, 22, 29, 13, 6, 'C', 'd', 'N', 'o')
    cone(g, 16, 30, 8, 4, 'd', 'N', 'J', 'o')
    # rim light down the lit edge of the tallest spire
    for y, x in ((12, 9), (13, 9), (14, 8), (15, 8), (16, 7), (17, 7), (18, 7)):
        px(g, x, y, 'K')
    for y, x in ((19, 19), (20, 19), (21, 18)):
        px(g, x, y, 'K')
    # scree pooling at the bases
    speckle(g, [(5, 29), (6, 30), (27, 30), (28, 29), (13, 31), (19, 31),
                (8, 31), (25, 31)], 'J')
    return g


def stalactites():
    """Ceiling spikes, drawn hanging from the top edge (wall-top overlay)."""
    g = new_grid()
    for cx, h, hw in ((7, 13, 4), (15, 9, 3), (23, 15, 4), (28, 7, 2)):
        for i in range(h):
            y = i
            t = i / max(1, h - 1)
            w = max(0, int(round(hw * (1.0 - t * 0.9))))
            for x in range(cx - w, cx + w + 1):
                if x == cx - w or x == cx + w:
                    px(g, x, y, 'o')
                elif x < cx:
                    px(g, x, y, 'd')
                else:
                    px(g, x, y, 'N')
            if w == 0:
                px(g, cx, y, 'C')
    hline(g, 0, 0, 31, 'J')
    return g


def glow_mushrooms():
    """Fungal light source: cyan-green caps with a faint bloom."""
    g = new_grid()
    # bloom halo (sparse, so it reads as light not noise)
    halo = [(9, 9), (14, 7), (20, 8), (24, 12), (7, 15), (26, 18),
            (12, 5), (18, 5), (22, 22), (6, 21)]
    speckle(g, halo, 'e')

    def cap(cx, cy, rx, ry):
        for dy in range(-ry, 1):
            for dx in range(-rx, rx + 1):
                if (dx * dx) / (rx * rx + 0.01) + (dy * dy) / (ry * ry + 0.01) <= 1.0:
                    x, y = cx + dx, cy + dy
                    if dy == -ry or abs(dx) == rx:
                        px(g, x, y, 'e')
                    elif dy <= -ry + 1 or dx <= -rx + 1:
                        px(g, x, y, 'q')      # lit upper-left
                    elif dx >= rx - 1:
                        px(g, x, y, 'e')
                    else:
                        px(g, x, y, 'G')
        # underside gills
        hline(g, cy + 1, cx - rx + 1, cx + rx - 1, 'g')

    def stem(cx, top, bot):
        for y in range(top, bot + 1):
            px(g, cx - 1, y, 'g')
            px(g, cx, y, 'G')
            px(g, cx + 1, y, 'e')
        px(g, cx, bot + 1, 'e')

    stem(12, 17, 26); cap(12, 16, 6, 5)
    stem(22, 20, 27); cap(22, 19, 4, 4)
    stem(17, 24, 29); cap(17, 23, 3, 2)
    # brightest specular on each cap
    speckle(g, [(9, 13), (10, 12), (20, 17), (16, 22)], 'c')
    return g


def crystal_cluster():
    """Blue crystal formation — light source and a mining target."""
    g = new_grid()
    shard(g, (13, 4), (14, 29), 7, 'c', 'S', 'n', 'o')
    shard(g, (22, 10), (21, 30), 6, 'c', 'S', 'n', 'o')
    shard(g, (7, 14), (9, 30), 5, 'S', 'n', 'n', 'o')
    shard(g, (26, 17), (25, 30), 4, 'S', 'n', 'n', 'o')
    # internal facet lines
    for y in range(9, 26, 3):
        px(g, 14, y, 'c')
    for y in range(15, 27, 4):
        px(g, 20, y, 'c')
    # cold bloom at the base
    speckle(g, [(6, 29), (28, 29), (17, 31), (11, 31), (23, 31)], 'n')
    # purple refraction, keeps it from reading as plain ice
    speckle(g, [(15, 12), (15, 13), (21, 20), (13, 21)], 'P')
    return g


def _noise(x, y, salt=0):
    """Deterministic hash in [0,1). Modular arithmetic bands into visible
    diagonal lattices at this scale, so mix the bits properly instead."""
    h = (x * 374761393 + y * 668265263 + salt * 2246822519) & 0xFFFFFFFF
    h = (h ^ (h >> 13)) * 1274126177 & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65536.0


def _blob(g, cx, cy, r, core, edge, jitter=0.35, salt=0):
    """An irregular lump — the shape 32rogues uses for rocks and ore."""
    for dy in range(-r - 1, r + 2):
        for dx in range(-r - 1, r + 2):
            dist = (dx * dx + dy * dy) ** 0.5
            wobble = 1.0 + (_noise(cx + dx, cy + dy, salt) - 0.5) * jitter * 2
            if dist <= r * wobble:
                px(g, cx + dx, cy + dy, core if dist <= r * wobble - 1.2 else edge)


def ore_wall():
    """Full-bleed dark rock wall with gold ore embedded in clustered nuggets.

    Deliberately NOT a continuous seam — a single line of gold reads as a
    stick lying on top of the rock rather than metal held inside it.
    """
    g = new_grid()
    # Earth-toned base, NOT slate: this has to read as "this dirt wall holds
    # ore", so it must sit in the same colour family as the surrounding wall.
    rect(g, 0, 0, 31, 31, 'E')
    for y in range(32):
        for x in range(32):
            n = _noise(x, y, 3)
            if n > 0.88:
                px(g, x, y, 'm')
            elif n < 0.14:
                px(g, x, y, 'D')
    # shadowed hollows for depth
    for (cx, cy, r) in ((7, 9, 3), (24, 21, 4), (15, 27, 2)):
        _blob(g, cx, cy, r, 'D', 'o', jitter=0.5, salt=cx)

    # ore: discrete nugget clusters following an implied fault line
    for (cx, cy, r) in ((9, 20, 2), (14, 16, 3), (20, 12, 2), (25, 8, 2), (11, 24, 1)):
        _blob(g, cx, cy, r, 'T', 'w', jitter=0.45, salt=cy)
        # lit face on the upper-left of each nugget
        px(g, cx - 1, cy - 1, 'y')
        px(g, cx, cy - 1, 'Y')
        px(g, cx - 1, cy, 'Y')
        # contact shadow underneath
        px(g, cx + 1, cy + r, 'o')
    # scattered loose flecks tie the clusters together
    speckle(g, [(12, 19), (17, 14), (22, 10), (7, 23), (27, 11), (19, 17)], 'T')
    speckle(g, [(10, 18), (23, 13), (16, 25)], 'A')
    return g


def rubble_floor():
    """Full-bleed cave floor: dark earth with scattered scree."""
    g = new_grid()
    rect(g, 0, 0, 31, 31, 'D')
    # Hashed grain (not modular, which bands into diagonals). Kept SPARSE and
    # low-contrast on purpose: this tile covers every walkable cave cell, so a
    # busy floor turns the whole room to visual noise and buries entities.
    for y in range(32):
        for x in range(32):
            n = _noise(x, y, 11)
            if n > 0.955:
                px(g, x, y, 'E')
            elif n < 0.035:
                px(g, x, y, 'o')
    # a couple of broad earth patches for large-scale variation
    for (cx, cy, r) in ((8, 11, 5), (24, 24, 6)):
        for dy in range(-r, r + 1):
            for dx in range(-r, r + 1):
                if dx * dx + dy * dy <= r * r and _noise(cx + dx, cy + dy, 7) > 0.72:
                    px(g, cx + dx, cy + dy, 'E')

    def pebble(x, y, big=False):
        if big:
            _blob(g, x, y, 2, 'd', 'N', jitter=0.3, salt=x + y)
            px(g, x - 1, y - 1, 'C')
            px(g, x, y - 1, 'K')
        else:
            px(g, x, y, 'C')
            px(g, x + 1, y, 'd')
            px(g, x, y + 1, 'N')
            px(g, x + 1, y + 1, 'o')

    # fewer, larger pebbles read better than many small ones
    for (x, y, b) in ((6, 8, True), (26, 18, True), (18, 27, False),
                      (13, 14, False), (29, 6, False)):
        pebble(x, y, b)
    return g


TILES = [
    ('cave_stalagmites', stalagmites),
    ('cave_stalactites', stalactites),
    ('cave_glow_mushrooms', glow_mushrooms),
    ('cave_crystal_cluster', crystal_cluster),
    ('cave_ore_wall', ore_wall),
    ('cave_rubble_floor', rubble_floor),
]


def render(g):
    im = Image.new('RGBA', (32, 32), (0, 0, 0, 0))
    for y in range(32):
        for x in range(32):
            c = g[y][x]
            if c != '.':
                im.putpixel((x, y), hex2rgba(PAL[c]))
    return im


def install_into_sheet(sheet='../assets/32rogues/tiles.png', cols=17):
    """Append the cave tiles as a new row of tiles.png.

    `MultiTileset::load_sheet` derives row count from image height
    (`rows = height / TILE_SIZE`) and only the column count is declared, so
    growing the sheet downward needs no Rust change. Same trick that put the
    ability icons into items.png rows 26-27.

    Always composites from `tiles.png.orig-backup` so re-running is idempotent
    rather than stacking a new row every time.
    """
    import os
    here = os.path.dirname(os.path.abspath(__file__))
    target = os.path.normpath(os.path.join(here, sheet))
    backup = target + '.orig-backup'
    if not os.path.exists(backup):
        Image.open(target).save(backup)
        print('created backup', backup)
    base = Image.open(backup).convert('RGBA')
    w, h = base.size
    rows = h // 32
    out = Image.new('RGBA', (w, (rows + 1) * 32), (0, 0, 0, 0))
    out.paste(base, (0, 0))
    for i, (name, fn) in enumerate(TILES):
        out.paste(render(fn()), (i * 32, rows * 32))
    out.save(target)
    print(f'installed {len(TILES)} tiles into {target} at row {rows + 1}')
    for i, (name, _) in enumerate(TILES):
        print(f'  {rows + 1}.{chr(ord("a") + i)} = {name}')


if __name__ == '__main__':
    import sys
    args = [a for a in sys.argv[1:] if not a.startswith('-')]
    outdir = args[0] if args else '.'
    for name, fn in TILES:
        render(fn()).save(f'{outdir}/{name}.png')
        print('wrote', name + '.png')
    if '--install' in sys.argv:
        install_into_sheet()
