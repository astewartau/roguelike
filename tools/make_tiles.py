#!/usr/bin/env python3
"""Hand-drawn 32x32 tiles in the 32rogues style."""
from PIL import Image

PAL = {
    # raise dead
    'B': '#d3d9b2',  # bone light
    'b': '#999f8e',  # bone mid
    'd': '#696a6a',  # bone dark
    'G': '#00ff8c',  # green bright
    'g': '#07bb79',  # green mid
    'e': '#14605d',  # green dark
    'E': '#453625',  # earth mid
    'm': '#8c6a56',  # earth light
    'D': '#1a130f',  # earth dark
    # life drain
    'R': '#ef3e6d',  # red bright
    'r': '#d41900',  # red mid
    'x': '#70162b',  # red dark
    'o': '#472138',  # dark maroon outline
    'h': '#fca89d',  # highlight
    'l': '#b598b2',  # purple light
    'P': '#76428a',  # purple mid
    'p': '#2f1d5a',  # purple dark
    # extended set for ability icons
    'w': '#874a00',  # wood
    'W': '#a15c52',  # leather
    'T': '#baa368',  # tan light
    'y': '#fae898',  # pale yellow
    'Y': '#fafa82',  # bright yellow
    'f': '#ff4d00',  # flame orange
    'A': '#a84111',  # flame dark
    'c': '#def9fc',  # ice white
    'C': '#748f93',  # gray blue
    'N': '#17394a',  # navy
    'S': '#3494cb',  # blue
    'L': '#82a368',  # green light
    'v': '#5c855b',  # green mid
    'V': '#244b3a',  # green dark
    '8': '#812611',  # dark red-brown
}

def hex2rgba(s):
    return (int(s[1:3],16), int(s[3:5],16), int(s[5:7],16), 255)

def new_grid():
    return [['.' for _ in range(32)] for _ in range(32)]

def px(g, x, y, c):
    if 0 <= x < 32 and 0 <= y < 32:
        g[y][x] = c

def vline(g, x, y0, y1, c):
    for y in range(y0, y1+1): px(g, x, y, c)

def hline(g, y, x0, x1, c):
    for x in range(x0, x1+1): px(g, x, y, c)

def rect(g, x0, y0, x1, y1, c):
    for y in range(y0, y1+1): hline(g, y, x0, x1, c)

def render(g, path, zoom=1):
    im = Image.new('RGBA', (32,32), (0,0,0,0))
    for y in range(32):
        for x in range(32):
            c = g[y][x]
            if c != '.':
                im.putpixel((x,y), hex2rgba(PAL[c]))
    if zoom > 1:
        im = im.resize((32*zoom, 32*zoom), Image.NEAREST)
    im.save(path)
    return im

# ---------------- RAISE DEAD ----------------
def raise_dead():
    g = new_grid()
    # fingers: (x, ytop, ybot) — light left col, mid right col, slanted tip
    for (fx, y0, y1) in [(11,7,10), (14,5,10), (17,6,10), (20,8,10)]:
        vline(g, fx, y0, y1, 'B')
        vline(g, fx+1, y0+1, y1, 'b')
    # palm — rounded corners, slight taper at bottom
    rect(g, 11, 11, 21, 15, 'b')
    hline(g, 16, 12, 20, 'b')
    hline(g, 11, 11, 21, 'B')
    vline(g, 11, 11, 15, 'B')
    vline(g, 21, 12, 15, 'd')
    hline(g, 16, 13, 20, 'd')
    px(g, 11, 11, '.'); px(g, 21, 11, '.')  # round top corners
    px(g, 12, 11, 'B')
    # carpal cracks
    px(g, 14, 13, 'd'); px(g, 17, 14, 'd')
    # thumb
    px(g, 10, 12, 'B'); px(g, 9, 13, 'B'); px(g, 8, 14, 'B'); px(g, 8, 15, 'b')
    # wrist
    hline(g, 17, 13, 19, 'b')
    px(g, 13, 17, 'B'); px(g, 19, 17, 'd')
    # forearm: two bones (radius/ulna)
    for (bx,) in [(13,), (17,)]:
        vline(g, bx, 18, 24, 'B')
        vline(g, bx+1, 18, 24, 'b')
    px(g, 12, 18, 'B'); px(g, 19, 18, 'b')  # bone heads widen
    # burial mound — rounded dome
    hline(g, 24, 11, 20, 'g')          # glow ring where bones break ground
    for x in (13, 14, 17, 18):
        px(g, x, 24, '.')              # bones pass through
    hline(g, 25, 9, 22, 'm')
    for x in range(11, 21):
        px(g, x, 25, 'G' if x in (12, 15, 16, 19) else 'g')
    hline(g, 26, 7, 24, 'E')
    px(g, 7, 26, 'm'); px(g, 8, 26, 'm'); px(g, 24, 26, 'm')
    hline(g, 26, 12, 19, 'D')
    px(g, 12, 26, 'e'); px(g, 19, 26, 'e')
    hline(g, 27, 5, 26, 'E')
    for x in (6, 9, 23): px(g, x, 27, 'm')
    hline(g, 27, 14, 17, 'D')
    hline(g, 28, 4, 27, 'E')
    px(g, 4, 28, 'D'); px(g, 27, 28, 'D')
    hline(g, 29, 5, 26, 'D')
    hline(g, 30, 8, 23, 'D')
    # drifting wisps
    px(g, 6, 9, 'g'); px(g, 6, 10, 'G'); px(g, 7, 12, 'e')
    px(g, 25, 7, 'g'); px(g, 25, 8, 'G'); px(g, 24, 11, 'e')
    px(g, 9, 5, 'e'); px(g, 23, 4, 'e')
    return g

# ---------------- LIFE DRAIN ----------------
def life_drain():
    g = new_grid()
    # heart silhouette spans per row: y -> list of (x0,x1)
    spans = {
        8:  [(9,12),(17,21)],
        9:  [(8,13),(16,22)],
        10: [(7,23)],
        11: [(7,23)],
        12: [(7,23)],
        13: [(7,23)],
        14: [(8,22)],
        15: [(8,22)],
        16: [(9,21)],
        17: [(10,20)],
        18: [(11,19)],
        19: [(12,18)],
        20: [(13,17)],
        21: [(14,16)],
        22: [(15,15)],
    }
    filled = {(x,y) for y,ss in spans.items() for (x0,x1) in ss for x in range(x0,x1+1)}
    for (x,y) in filled:
        # mostly mid red; thin dark shade on right/bottom, bright rounded upper-left lobe
        c = 'r'
        if x >= 20 or y >= 18: c = 'x'
        if (x-10.5)**2 + (y-11)**2 <= 12: c = 'R'
        px(g, x, y, c)
    # outline: border pixels of silhouette
    for (x,y) in filled:
        for (nx,ny) in [(x-1,y),(x+1,y),(x,y-1),(x,y+1)]:
            if (nx,ny) not in filled:
                px(g, x, y, 'o')
                break
    # top dip between lobes
    px(g, 14, 9, 'o'); px(g, 15, 9, 'o')
    px(g, 14, 10, 'o'); px(g, 15, 10, 'o')
    # highlight on left lobe
    px(g, 9, 10, 'h'); px(g, 10, 10, 'h'); px(g, 9, 11, 'h')
    # shadow claw entering from top-right: slim 2px diagonal arm
    arm = [(28,0),(27,1),(26,2),(25,3),(24,4)]
    for (ax,ay) in arm:
        px(g, ax, ay, 'P'); px(g, ax+1, ay, 'p')
    px(g, 29, 0, 'p')
    # small palm above the right lobe
    rect(g, 20, 5, 24, 6, 'P')
    px(g, 20, 5, 'l'); px(g, 21, 5, 'l')
    px(g, 24, 6, 'p'); px(g, 25, 5, 'p')
    # three short claws hooking down into the heart
    for (cx, y0, y1) in [(18, 6, 9), (21, 7, 10), (24, 7, 9)]:
        px(g, cx, y0, 'l')        # lit knuckle
        vline(g, cx, y0+1, y1, 'P')
        px(g, cx, y1+1, 'p')      # dark point
    # gripped lobe falls into shadow
    px(g, 19, 8, 'x'); px(g, 20, 8, 'x'); px(g, 19, 9, 'x')
    px(g, 22, 9, 'x'); px(g, 23, 9, 'x'); px(g, 22, 10, 'x')
    # blood drops below the point
    px(g, 15, 24, 'r')
    rect(g, 14, 25, 15, 26, 'r')
    px(g, 14, 25, 'R'); px(g, 15, 26, 'x')
    px(g, 11, 24, 'r'); px(g, 11, 25, 'x')
    px(g, 19, 26, 'r'); px(g, 19, 27, 'x')
    return g

def blob(g, filled, fill_fn, outline_char):
    """Fill a silhouette (set of (x,y)) and add an inner outline on border pixels."""
    for (x, y) in filled:
        px(g, x, y, fill_fn(x, y) if callable(fill_fn) else fill_fn)
    for (x, y) in filled:
        for (nx, ny) in [(x-1,y),(x+1,y),(x,y-1),(x,y+1)]:
            if (nx, ny) not in filled:
                px(g, x, y, outline_char)
                break

def ellipse_set(cx, cy, rx, ry):
    s = set()
    for y in range(32):
        for x in range(32):
            if ((x-cx)/rx)**2 + ((y-cy)/ry)**2 <= 1.0:
                s.add((x, y))
    return s

# ---------------- TAME (paw print + heart) ----------------
def tame():
    g = new_grid()
    # main pad
    pad = ellipse_set(13.5, 20, 6.2, 4.4)
    # three toe pads
    toes = (ellipse_set(7.5, 13, 2.4, 3.0) | ellipse_set(13.5, 10.5, 2.4, 3.0)
            | ellipse_set(19.5, 13, 2.4, 3.0))
    def pad_fill(x, y):
        return 'T' if (x + y) < 30 and y < 20 else 'm'
    blob(g, pad, pad_fill, 'E')
    blob(g, toes, lambda x, y: 'T' if y < 12 else 'm', 'E')
    # small heart, top-right
    hs = set()
    for y, (a, b) in {5:(22,23), 6:(21,24), 7:(21,27), 8:(21,27), 9:(22,26),
                      10:(23,25), 11:(24,24)}.items():
        for x in range(a, b+1):
            hs.add((x, y))
    for y in (5, 6):
        hs.add((26, y)); hs.add((27, y))
    hs.discard((24, 5)); hs.discard((25, 5))
    blob(g, hs, lambda x, y: 'R' if x < 25 else 'r', 'x')
    px(g, 22, 6, 'h')
    return g

# ---------------- REST (campfire) ----------------
def rest():
    g = new_grid()
    # flame: teardrop, wide at base
    spans = {7:(15,15), 8:(14,16), 9:(14,16), 10:(13,17), 11:(13,17), 12:(12,18),
             13:(12,18), 14:(11,19), 15:(11,19), 16:(10,20), 17:(10,20), 18:(10,20),
             19:(9,21), 20:(9,21), 21:(9,21), 22:(10,20), 23:(11,20), 24:(12,19)}
    fs = {(x,y) for y,(a,b) in spans.items() for x in range(a,b+1)}
    def flame_fill(x, y):
        dx = abs(x - 15.2)
        if y >= 15 and dx < (y - 12) * 0.32: return 'Y'   # bright core
        if y >= 12 and dx < (y - 9) * 0.45: return 'y'
        return 'f'
    blob(g, fs, flame_fill, 'A')
    # log
    rect(g, 7, 25, 24, 27, 'w')
    hline(g, 25, 7, 24, 'm')
    hline(g, 27, 7, 24, 'E')
    # end caps
    vline(g, 6, 25, 27, 'E'); px(g, 7, 26, 'T')
    vline(g, 25, 25, 27, 'E'); px(g, 24, 26, 'T')
    # embers / sparks
    px(g, 8, 12, 'f'); px(g, 23, 9, 'f'); px(g, 21, 14, 'Y')
    return g

# ---------------- SLEEP (crescent moon + Zzz) ----------------
def sleep():
    g = new_grid()
    moon = ellipse_set(12, 18, 8.2, 8.2) - ellipse_set(16.5, 14.5, 7.4, 7.4)
    blob(g, moon, lambda x, y: 'T' if ((x-16.5)/8.6)**2 + ((y-14.5)/8.6)**2 <= 1 else 'y', 'w')
    def zee(x, y, s, c):
        hline(g, y, x, x+s-1, c)
        hline(g, y+s-1, x, x+s-1, c)
        for i in range(1, s-1):
            px(g, x+s-1-i, y+i, c)
    zee(18, 17, 3, 'c')
    zee(21, 10, 4, 'c')
    zee(25, 3, 5, 'c')
    return g

# ---------------- BARKSKIN (wooden shield + leaf) ----------------
def barkskin():
    g = new_grid()
    spans = {5:(10,21), 6:(9,22), 7:(9,22), 8:(9,22), 9:(9,22), 10:(9,22), 11:(9,22),
             12:(9,22), 13:(9,22), 14:(9,22), 15:(10,21), 16:(10,21), 17:(11,20),
             18:(12,19), 19:(13,18), 20:(14,17), 21:(15,16)}
    ss = {(x,y) for y,(a,b) in spans.items() for x in range(a,b+1)}
    def wood_fill(x, y):
        if x in (13, 18): return 'E'      # plank seams
        return 'w'
    blob(g, ss, wood_fill, 'E')
    hline(g, 6, 10, 21, 'T')              # top rim light
    px(g, 10, 7, 'T'); px(g, 10, 8, 'T')
    # leaf overlay — bigger and brighter so it pops off the wood
    ls = set()
    for y, (a, b) in {7:(16,16), 8:(15,17), 9:(14,18), 10:(13,19), 11:(13,19),
                      12:(12,18), 13:(13,17), 14:(14,16), 15:(15,15)}.items():
        for x in range(a, b+1):
            ls.add((x, y))
    blob(g, ls, 'g', 'V')
    for (vx, vy) in [(16,9),(15,10),(15,11),(15,12)]:                 # vein
        px(g, vx, vy, 'v')
    px(g, 14, 10, 'L'); px(g, 14, 11, 'L')                            # lit edge
    px(g, 16, 16, 'V'); px(g, 17, 17, 'V')                            # stem
    return g

# ---------------- SPRINT (boot + speed lines) ----------------
def sprint():
    g = new_grid()
    # boot shaft
    ss = {(x,y) for y in range(9, 19) for x in range(14, 21)}
    # foot extending right
    ss |= {(x,y) for y,(a,b) in {19:(14,23), 20:(14,25), 21:(14,26)}.items()
           for x in range(a,b+1)}
    def boot_fill(x, y):
        return 'w' if x >= 19 and y < 19 else 'W'
    blob(g, ss, boot_fill, '8')
    vline(g, 15, 10, 17, 'T')             # thin shaft highlight
    hline(g, 8, 13, 21, 'T')              # cuff
    hline(g, 9, 13, 21, 'm')
    px(g, 24, 19, '8'); px(g, 25, 19, '8')  # round the toe
    px(g, 25, 20, '8'); px(g, 26, 20, '8')
    hline(g, 22, 13, 26, 'E')             # sole
    hline(g, 23, 14, 25, 'D')
    # speed lines
    hline(g, 11, 4, 10, 'c'); hline(g, 14, 2, 8, 'c'); hline(g, 17, 4, 10, 'c')
    # dust puff at heel
    px(g, 10, 21, 'C'); px(g, 11, 22, 'c'); px(g, 9, 23, 'C')
    return g

# ---------------- DISENGAGE (leap-back arrow) ----------------
def disengage():
    g = new_grid()
    # bold arc from bottom-right up to top-left
    arc = [(26,24),(25,22),(24,20),(23,18),(22,16),(21,15),(20,14),(19,13),
           (18,12),(17,11),(16,11),(15,10),(14,10),(13,10),(12,10),(11,10)]
    for (x, y) in arc:
        px(g, x, y-1, 'L'); px(g, x, y, 'L'); px(g, x, y+1, 'v')
    # solid arrowhead pointing left, joined to the arc end
    for i in range(4):
        vline(g, 5+i, 10-i, 10+i, 'L')
    px(g, 4, 10, 'v'); vline(g, 8, 7, 13, 'v')
    # dust puffs at the take-off point
    px(g, 28, 25, 'C'); px(g, 27, 27, 'c'); px(g, 25, 26, 'C')
    # motion dashes trailing the arc
    px(g, 28, 19, 'c'); px(g, 27, 15, 'c'); px(g, 25, 11, 'c')
    return g

# ---------------- TUMBLE (circular roll arrow) ----------------
def tumble():
    g = new_grid()
    import math
    ring = set()
    for y in range(32):
        for x in range(32):
            d = math.hypot(x - 15.5, y - 15.5)
            if 7.0 <= d <= 10.0:
                ang = math.degrees(math.atan2(15.5 - y, x - 15.5)) % 360
                if not (20 <= ang <= 105):   # gap at top-right
                    ring.add((x, y))
    blob(g, ring, lambda x, y: 'L' if y <= 16 else 'v', 'V')
    # solid arrowhead at the gap's end, pointing clockwise (rightward)
    for i in range(4):
        vline(g, 22+i, 7+i, 13-i, 'L')
    px(g, 26, 10, 'v')
    # tucked figure: ball in the middle
    ball = ellipse_set(15.5, 15.5, 3.4, 3.4)
    blob(g, ball, lambda x, y: 'T' if x+y < 30 else 'm', 'E')
    return g

# ---------------- CRIPPLING SHOT (arrow piercing leg) ----------------
def crippling_shot():
    g = new_grid()
    # leg: thigh -> knee -> calf -> boot
    leg = {(x,y) for y in range(5, 13) for x in range(13, 19)}          # thigh
    leg |= {(x,y) for y in range(13, 20) for x in range(14, 19)}        # calf
    leg |= {(x,y) for y,(a,b) in {20:(14,22), 21:(14,23), 22:(14,23)}.items()
            for x in range(a,b+1)}                                       # boot
    def leg_fill(x, y):
        if y >= 20: return 'w'            # boot
        return 'C' if x <= 15 else 'N'    # trousers
    blob(g, leg, leg_fill, 'N')
    hline(g, 23, 13, 23, 'D')             # sole
    # arrow shaft from top-left, piercing through the thigh (drawn over it)
    shaft = [(3,3),(4,4),(5,4),(6,5),(7,5),(8,6),(9,6),(10,7),(11,7),(12,8),
             (13,8),(14,9),(15,9)]
    for (x, y) in shaft:
        px(g, x, y, 'T'); px(g, x, y+1, 'w')
    # fletching: two red slashes across the nock
    for (fx, fy) in [(3,1),(2,2),(1,3)]:
        px(g, fx, fy, 'r'); px(g, fx+1, fy, 'r')
    for (fx, fy) in [(5,2),(4,3),(3,4)]:
        px(g, fx, fy, 'x'); px(g, fx+1, fy+1, 'x')
    # arrowhead exiting the other side, with blood
    px(g, 19, 11, 'B'); px(g, 20, 12, 'B'); px(g, 21, 13, 'B')
    px(g, 20, 11, 'b'); px(g, 21, 12, 'b'); px(g, 22, 14, 'b')
    px(g, 20, 15, 'r'); px(g, 21, 17, 'r'); px(g, 22, 19, 'x')
    # entry wound + pain sparks
    px(g, 15, 10, 'r'); px(g, 16, 10, 'r')
    px(g, 11, 4, 'Y'); px(g, 14, 5, 'Y')
    return g

# ---------------- STUN (starburst) ----------------
def stun():
    g = new_grid()
    cx, cy = 15, 14
    # 4-point star built from two tapered spikes
    star = set()
    for dy in range(-9, 10):
        w = max(0, 3 - abs(dy) // 2)
        for dx in range(-w, w+1):
            star.add((cx+dx, cy+dy))
    for dx in range(-9, 10):
        w = max(0, 3 - abs(dx) // 2)
        for dy in range(-w, w+1):
            star.add((cx+dx, cy+dy))
    blob(g, star, lambda x, y: 'c' if abs(x-cx)+abs(y-cy) <= 2 else 'Y', 'w')
    # small satellite stars (plus signs)
    for (sx, sy, c) in [(5, 6, 'y'), (26, 8, 'y'), (24, 23, 'y')]:
        px(g, sx, sy, c); px(g, sx-1, sy, c); px(g, sx+1, sy, c)
        px(g, sx, sy-1, c); px(g, sx, sy+1, c)
    return g

ICONS = [
    ('raise_dead', raise_dead),
    ('life_drain', life_drain),
    ('tame', tame),
    ('rest', rest),
    ('sleep', sleep),
    ('barkskin', barkskin),
    ('sprint', sprint),
    ('disengage', disengage),
    ('tumble', tumble),
    ('crippling_shot', crippling_shot),
    ('stun', stun),
]

if __name__ == '__main__':
    import os
    outdir = os.environ.get('OUTDIR', '/tmp')
    z = 6
    cols = 6
    rows = (len(ICONS) + cols - 1) // cols
    sheet = Image.new('RGBA', (cols*(32*z+12)+12, rows*(32*z+12)+12), (24,20,28,255))
    for i, (name, fn) in enumerate(ICONS):
        im = render(fn(), f'{outdir}/{name}.png')
        big = im.resize((32*z, 32*z), Image.NEAREST)
        cx, cy = i % cols, i // cols
        sheet.paste(big, (12+cx*(32*z+12), 12+cy*(32*z+12)), big)
    sheet.save('/tmp/contact.png')
    print('done')
