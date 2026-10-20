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
    # HUD bar icons
    'k': '#c8743a',  # cooked meat
    'K': '#7e3a1c',  # meat shadow
    'j': '#e8a060',  # meat highlight
    'u': '#e0a526',  # gold shade
    # held torch: the stock torch's own handle colour (animated-tiles row 6)
    't': '#e17b50',  # torch handle; skin shade on badges
    # item badges
    'O': '#1c1429',  # badge outline
    's': '#fb9b72',  # skin
    'i': '#d77bba',  # pink
    # fire, from the rim of a flame (1) to its core (4): see FIRE_SHADES
    '1': '#ff9a1f',
    '2': '#ffc93c',
    '3': '#ffe680',
    '4': '#fffbe6',
    # arrow tips and rain
    'a': '#a5b5d4',
    # smoke, light to dark
    '5': '#5a5875',
    '6': '#31314b',
    '7': '#222034',
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


# ================= HUD BAR ICONS (status panel) =================
# Drawn big and bold: the HUD shows them at about half size, so fine detail
# turns to mush.

def _in_poly(x, y, pts):
    inside = False
    j = len(pts) - 1
    for i in range(len(pts)):
        xi, yi = pts[i]; xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside

# ---------------- HEALTH (heart) ----------------
def health_heart():
    g = new_grid()
    heart = set()
    for y in range(32):
        for x in range(32):
            # classic implicit heart, centred, ~26px wide
            u = (x + 0.5 - 16) / 12.5
            v = (15.5 - (y + 0.5)) / 12.5 + 0.12
            if (u*u + v*v - 1) ** 3 - u*u * v**3 <= 0:
                heart.add((x, y))
    def fill(x, y):
        # light from the upper left: bright -> mid -> dark in diagonal bands
        d = (x - 6) + (y - 6) * 1.2
        if d < 9: return 'R'
        if d < 27: return 'r'
        return 'x'
    blob(g, heart, fill, 'o')
    # small square gleam on the left lobe
    rect(g, 8, 9, 9, 10, 'h'); px(g, 10, 9, 'h')
    return g

# ---------------- EXPERIENCE (star) ----------------
def xp_star():
    import math
    g = new_grid()
    cx, cy, ro, ri = 16, 16.8, 14.5, 6.8
    pts = []
    for k in range(10):
        a = -math.pi / 2 + k * math.pi / 5
        r = ro if k % 2 == 0 else ri
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    star = {(x, y) for y in range(32) for x in range(32) if _in_poly(x + 0.5, y + 0.5, pts)}
    def fill(x, y):
        if (x - 13.5) ** 2 + (y - 13.5) ** 2 <= 4: return 'c'  # glint
        if (x - cx) + (y - cy) > 2: return 'u'                  # shaded lower-right
        return 'Y'
    blob(g, star, fill, 'A')
    return g

# ---------------- HUNGER (meat drumstick) ----------------
def hunger_drumstick():
    g = new_grid()
    # bone shaft running down-right from the meat, ending in two knobs
    bone = set()
    for t in range(0, 8):
        b0 = 17 + t
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (1, 2), (2, 2)]:
            bone.add((b0 + dx, b0 + dy))
    bone |= ellipse_set(27.0, 24.0, 2.8, 2.8) | ellipse_set(24.0, 27.0, 2.8, 2.8)
    blob(g, bone, lambda x, y: 'B' if x + y < 52 else 'b', 'd')
    # meat: a fat oval tipped toward the bone
    meat = set()
    for y in range(32):
        for x in range(32):
            u, v = x + 0.5 - 12.5, y + 0.5 - 12.5
            a, b = (u + v) / 1.414, (v - u) / 1.414
            if (a / 11.0) ** 2 + (b / 8.2) ** 2 <= 1:
                meat.add((x, y))
    def fill(x, y):
        if (x - 8.5) ** 2 + (y - 8.5) ** 2 <= 6: return 'j'   # glossy highlight
        if x + y >= 29: return 'K'                             # underside shadow
        return 'k'
    blob(g, meat, fill, '8')
    return g

# ---------------- LIGHT (sun) ----------------
def light_sun():
    import math
    g = new_grid()
    cx, cy = 15.5, 15.5
    # eight tapered rays, drawn first so the disc sits over their roots
    rays = set()
    for k in range(8):
        a = k * math.pi / 4
        for step in range(10, 15):
            w = 1.6 if step < 13 else 0.9
            px_, py_ = cx + step * math.cos(a), cy + step * math.sin(a)
            for y in range(32):
                for x in range(32):
                    if (x + 0.5 - px_) ** 2 + (y + 0.5 - py_) ** 2 <= w * w:
                        rays.add((x, y))
    blob(g, rays, 'Y', 'A')
    disc = ellipse_set(cx, cy, 8.6, 8.6)
    def fill(x, y):
        if (x - 12.5) ** 2 + (y - 12.5) ** 2 <= 6: return 'c'   # glint
        if (x - cx) + (y - cy) > 4: return 'u'                   # shaded lower-right
        return 'Y'
    blob(g, disc, fill, 'A')
    return g

# ================= HELD TORCH (drawn over its carrier) =================
# The stock torch (animated-tiles row 6) is a floor sprite as long as a person
# is tall: laid over a goblin or the player it hid their face and read as a
# torch lying across them. This is a short one held up in the off hand.
#
# The renderer shifts the whole cell so HELD_TORCH_GRIP lands on the
# carrier's hand (`tile_ids::off_hand_anchor` in src/tile.rs, which keeps a
# copy of this point), so the grip is set at a typical hand position and the
# art leans up and out from it.
HELD_TORCH_GRIP = (23, 21)

# Flame frames, bottom row resting on the torch head, drawn as a silhouette
# ('Y') and coloured by `flame_shades` like every other flame. The sway and
# the odd spark are what make it read as burning.
TORCH_FLAMES = [
    ["..Y..",
     "..YY.",
     ".YYY.",
     ".YYYY",
     "YYYYY",
     ".YYY."],
    ["...Y.",
     "..YY.",
     ".YYYY",
     ".YYYY",
     "YYYYY",
     ".YYY."],
    ["..Y..",
     "..Y..",
     ".YYY.",
     ".YYY.",
     "YYYYY",
     "YYYYY",
     ".YYY."],
    [".Y...",
     ".YY..",
     ".YYY.",
     "YYYY.",
     "YYYYY",
     ".YYY."],
    ["Y....",
     ".....",
     "..Y..",
     ".YYY.",
     "YYYYY",
     ".YYY."],
    ["..Y..",
     ".YY..",
     ".YYY.",
     "YYYY.",
     "YYYYY",
     ".YYY."],
]

def _held_torch_body(g):
    """Handle and head. The handle leans out one pixel every three rows and
    skips the two rows at the grip, so the carrier's own fist shows across it
    instead of being painted over."""
    gx, gy = HELD_TORCH_GRIP
    for y in range(gy - 7, gy + 3):
        if y in (gy, gy - 1):
            continue
        px(g, gx + round((gy - y) / 3), y, 't')
    # head: pitch-soaked rag, the stock unlit torch's gray and navy
    hline(g, gy - 8, gx + 1, gx + 3, 'd')
    px(g, gx + 3, gy - 8, 'N')
    return gx + 2, gy - 9  # where the flame's bottom-centre sits

def held_torch(frame):
    g = new_grid()
    fx, fy = _held_torch_body(g)
    rows = TORCH_FLAMES[frame]
    flame = set()
    for i, row in enumerate(rows):
        y = fy - (len(rows) - 1) + i
        for j, c in enumerate(row):
            if c != '.':
                flame.add((fx - 2 + j, y))
    for (x, y), c in flame_shades(flame).items():
        px(g, x, y, c)
    return g

def held_torch_unlit():
    g = new_grid()
    fx, fy = _held_torch_body(g)
    # charred tip where the flame was
    hline(g, fy, fx - 1, fx, 'N')
    return g


# ================= WORLD SPRITES =================
# Things lying in the dungeon that the pack has no sprite for. They're drawn
# over the floor, so they keep it showing through.

def line(g, x0, y0, x1, y1, c):
    """One-pixel line, by sampling (fine at this size)."""
    n = max(abs(x1 - x0), abs(y1 - y0), 1)
    for k in range(n + 1):
        t = k / n
        px(g, round(x0 + (x1 - x0) * t), round(y0 + (y1 - y0) * t), c)

def sagging(g, x0, y0, x1, y1, toward, sag, c):
    """A thread from (x0, y0) to (x1, y1) that sags `sag` pixels toward the
    point `toward`, the way a web's spiral droops between its spokes."""
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    dx, dy = toward[0] - mx, toward[1] - my
    d = max((dx * dx + dy * dy) ** 0.5, 1e-6)
    qx, qy = mx + dx / d * sag, my + dy / d * sag
    n = int(max(abs(x1 - x0), abs(y1 - y0))) * 2 + 2
    for k in range(n + 1):
        t = k / n
        x = (1 - t) ** 2 * x0 + 2 * (1 - t) * t * qx + t * t * x1
        y = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * qy + t * t * y1
        px(g, round(x), round(y), c)

# ---------------- WEB ----------------
# Was a white-tinted wheat sheaf. Spokes out to the edges of the tile and a
# spiral sagging between them; thin enough that the floor shows through.
def web():
    cx, cy = 16, 15
    # spoke ends, a little irregular so it reads as spun rather than ruled
    ends = [(16, 0), (29, 2), (31, 15), (28, 29), (15, 31), (3, 28), (0, 14), (4, 2)]
    g = new_grid()
    for frac, c in ((0.84, 'b'), (0.6, 'b'), (0.36, 'b')):
        pts = [(cx + (ex - cx) * frac, cy + (ey - cy) * frac) for ex, ey in ends]
        for i in range(len(pts)):
            (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % len(pts)]
            sagging(g, x0, y0, x1, y1, (cx, cy), 1.5 * frac + 0.5, c)
    for ex, ey in ends:
        line(g, cx, cy, ex, ey, 'B')
    return g

# ---------------- FOUNTAIN ----------------
# Was the pack's pit, tinted blue, which read as a hole in the floor. A stone
# basin of water, seen from the front like the fire pit, with a spout.
def fountain():
    g = new_grid()
    cx, cy = 15.5, 19.5
    outer = ellipse_set(cx, cy, 14.2, 8.2)
    inner = ellipse_set(cx, cy - 0.5, 11.2, 5.6)
    # front face of the basin wall, below the rim
    for (x, y) in list(outer):
        if y >= cy:
            for k in range(1, 4):
                if (x, y + k) not in outer:
                    px(g, x, y + k, 'd' if k < 3 else 'N')
    # rim: lit from above, darker toward the front
    for (x, y) in outer - inner:
        px(g, x, y, 'B' if y < cy - 3 else ('b' if y < cy + 4 else 'C'))
    # mortar joints
    for (x, y) in [(3, 18), (28, 18), (8, 25), (16, 27), (23, 25), (9, 13), (22, 13)]:
        if (x, y) in outer - inner:
            px(g, x, y, 'd')
    # water: deeper under the far rim, ripples toward the front
    for (x, y) in inner:
        px(g, x, y, 'N' if y < cy - 4 else 'S')
    for (x, y) in [(7, 20), (8, 20), (12, 22), (13, 22), (20, 21), (21, 21), (23, 19), (10, 18)]:
        px(g, x, y, 'c')
    # spout: a stone post in the middle, water falling either side of it
    rect(g, 15, 9, 16, 18, 'b')
    vline(g, 16, 9, 18, 'C')
    hline(g, 8, 14, 17, 'B')
    hline(g, 9, 14, 17, 'b')
    for (x, y) in [(13, 9), (12, 10), (12, 11), (11, 12), (11, 13), (11, 15),
                   (18, 9), (19, 10), (19, 11), (20, 12), (20, 13), (20, 15)]:
        px(g, x, y, 'c')
    px(g, 15, 6, 'c'); px(g, 16, 5, 'c'); px(g, 15, 7, 'S'); px(g, 16, 7, 'S')
    return g


# ================= FIRE COLOURS =================
# The pack draws every flame (fires, braziers, fire pits, torches, lanterns)
# in one flat #fafa82, a pale yellow that leans green. They're repainted in
# amber: a flame is coloured by how deep inside it each pixel sits, deepest
# being the shortest run of flame to its left, right or above. Not below, so
# a flame is hottest at its base, and its tips and edges are the rim colour.

STOCK_FLAME = (250, 250, 130)  # #fafa82
FIRE_SHADES = '1234'           # PAL keys, rim to core

def flame_shades(flame):
    """Palette key for every pixel of a flame, a set of (x, y)."""
    def run(x, y, dx, dy):
        n = 0
        while (x, y) in flame:
            n += 1
            x += dx
            y += dy
        return n
    return {
        (x, y): FIRE_SHADES[min(run(x, y, -1, 0), run(x, y, 1, 0), run(x, y, 0, -1), len(FIRE_SHADES)) - 1]
        for (x, y) in flame
    }

def repaint_flames(sheet):
    """Repaint every stock-flame pixel of a sheet, a cell at a time (no
    flame crosses a cell), leaving everything else as it was."""
    out = sheet.copy()
    pix = out.load()
    for cy in range(0, sheet.height, 32):
        for cx in range(0, sheet.width, 32):
            flame = {(x, y) for y in range(cy, cy + 32) for x in range(cx, cx + 32)
                     if pix[x, y][3] and pix[x, y][:3] == STOCK_FLAME}
            for (x, y), c in flame_shades(flame).items():
                pix[x, y] = hex2rgba(PAL[c])
    return out

def repaint_animated_tiles(sheet_dir):
    """Write animated-tiles.png with its flames repainted, always from the
    stock sheet, which is kept beside it as animated-tiles.stock.png (saved
    the first time). So re-running redoes it from scratch, and putting the
    stock file back undoes it."""
    import os, shutil
    path = os.path.join(sheet_dir, 'animated-tiles.png')
    stock = os.path.join(sheet_dir, 'animated-tiles.stock.png')
    if not os.path.exists(stock):
        shutil.copyfile(path, stock)
    repaint_flames(Image.open(stock).convert('RGBA')).save(path)
    print(f'repainted the flames in {path} (stock sheet kept as {stock})')


# ================= PROJECTILES IN FLIGHT =================
# One cell per direction of travel, FLIGHT_DIRECTIONS of them going
# anticlockwise from due east (screen right), so the renderer can turn a
# projectile to face where it's going. Drawn by code along the direction
# rather than by hand, so all sixteen stay alike.
import math

FLIGHT_DIRECTIONS = 16

def _along(k):
    """Unit vector of direction k in sheet pixels (y down), and its
    perpendicular."""
    a = 2 * math.pi * k / FLIGHT_DIRECTIONS
    u = (math.cos(a), -math.sin(a))
    return u, (-u[1], u[0])

def _at(c, u, v, t, w=0.0):
    """The pixel t along u and w along v from the cell centre c."""
    return round(c[0] + u[0] * t + v[0] * w), round(c[1] + u[1] * t + v[1] * w)

def _stroke(g, c, u, v, t0, t1, w, col, step=0.25):
    t = t0
    while t <= t1 + 1e-9:
        px(g, *_at(c, u, v, t, w), col)
        t += step

def arrow_flight(k):
    """The stock arrow (items 24.a), at any heading: a thin shaft, a steel
    tip, green fletching."""
    g = new_grid()
    u, v = _along(k)
    c = (15.5, 15.5)
    _stroke(g, c, u, v, -8, 6, 0, 't')
    for side in (-1, 1):
        _stroke(g, c, u, v, -9, -6.5, side * 1.0, 'g' if side > 0 else 'V')
    px(g, *_at(c, u, v, 7), 'a')
    px(g, *_at(c, u, v, 8), 'c')
    px(g, *_at(c, u, v, 7, 0.9), 'a')
    return g

def grave_bolt_flight(k):
    """A shard of bone with a wisp of grave-green trailing behind it."""
    g = new_grid()
    u, v = _along(k)
    c = (15.5, 15.5)
    # the wisp: a wavering line that fades out behind
    t = -3.0
    while t >= -13:
        w = 1.4 * math.sin(t * 0.9)
        col = 'G' if t > -6 else ('g' if t > -9.5 else 'e')
        px(g, *_at(c, u, v, t, w), col)
        t -= 0.35
    for t, w in [(-7, -1.6), (-10.5, 1.8), (-12.5, -1.0)]:
        px(g, *_at(c, u, v, t, w), 'g')
    # the shard: a long sliver, widest behind its point
    for t10 in range(-40, 71):
        t = t10 / 10
        half = 1.1 if -2.5 <= t <= 3.5 else (0.6 if t < 5.5 else 0.0)
        _stroke(g, c, u, v, t, t, 0, 'B')
        if half > 0.9:
            px(g, *_at(c, u, v, t, half), 'b')
            px(g, *_at(c, u, v, t, -half), 'B')
    px(g, *_at(c, u, v, 7), 'c')
    return g

def fireball_flight(k):
    """A ball of fire with its flames streaming out behind."""
    g = new_grid()
    u, v = _along(k)
    c = (15.5, 15.5)
    head = (c[0] + u[0] * 4, c[1] + u[1] * 4)
    # tail: tapering, cooling from yellow to red as it trails off
    for t10 in range(-10, -140, -2):
        t = t10 / 10
        half = max(0.0, 3.0 + t * 0.25)
        heat = '3' if t > -3 else ('2' if t > -5.5 else ('1' if t > -8 else ('f' if t > -10.5 else 'r')))
        w = -half
        while w <= half + 1e-9:
            px(g, *_at(head, u, v, t, w + 0.6 * math.sin(t * 1.3)), heat)
            w += 0.5
    for t, w, col in [(-9, 3.2, 'f'), (-12, -2.4, 'r'), (-6.5, -3.4, '1'), (-14, 0.8, 'r')]:
        px(g, *_at(head, u, v, t, w), col)
    # the ball, hottest toward the front
    for y in range(32):
        for x in range(32):
            dx, dy = x + 0.5 - head[0], y + 0.5 - head[1]
            d = math.hypot(dx, dy)
            if d <= 3.6:
                front = (dx * u[0] + dy * u[1]) / 3.6
                heat = 3.6 - d + front * 1.2
                px(g, x, y, '4' if heat > 2.6 else ('3' if heat > 1.6 else ('2' if heat > 0.7 else '1')))
    return g


# ================= EXPLOSION =================
# A blast, one tile of it: the renderer plays it on every tile the blast
# covers, later the further out, so it rolls outward from the centre. A flash,
# a ball of fire that swells and breaks up, then smoke.
EXPLOSION_FRAMES = 8
# Differently-shaped blasts, so neighbouring tiles don't match: cells run
# variant by variant, EXPLOSION_FRAMES each.
EXPLOSION_VARIANTS = 3

def _noise(x, y, seed):
    """Cheap deterministic value noise in [0, 1)."""
    n = (x * 374761393 + y * 668265263 + seed * 1442695041) & 0xffffffff
    n = ((n ^ (n >> 13)) * 1274126177) & 0xffffffff
    return (n & 0xffff) / 65536

def _smooth_noise(x, y, seed, scale):
    x0, y0 = math.floor(x / scale), math.floor(y / scale)
    fx, fy = x / scale - x0, y / scale - y0
    def n(i, j): return _noise(x0 + i, y0 + j, seed)
    top = n(0, 0) * (1 - fx) + n(1, 0) * fx
    bot = n(0, 1) * (1 - fx) + n(1, 1) * fx
    return top * (1 - fy) + bot * fy

# fire colours by heat, coldest first (below the first is smoke)
EXPLOSION_RAMP = [(0.48, 'r'), (0.58, 'f'), (0.68, '1'), (0.78, '2'), (0.88, '3'), (9.0, '4')]
FIRE_FLOOR = 0.38

def explosion(frame, variant=0):
    g = new_grid()
    v = variant * 101                         # reseeds every noise below
    p = frame / (EXPLOSION_FRAMES - 1)        # 0 .. 1 through the blast
    radius = 4 + 11 * min(1.0, p * 2.2)       # swells fast, then holds
    cool = max(0.0, p - 0.3) * 1.6            # how far it has burnt out
    rise = max(0.0, p - 0.45) * 10            # the smoke drifts up
    for y in range(32):
        for x in range(32):
            dx, dy = x + 0.5 - 16, y + 0.5 - 16 + rise
            wobble = (_smooth_noise(x, y, 7 + v, 5) - 0.5) * 7 * min(1.0, p * 3)
            d = math.hypot(dx, dy) + wobble
            if d > radius:
                continue
            heat = (1.15 - d / radius * 0.7) - cool + (_smooth_noise(x, y, 11 + v + frame, 3) - 0.5) * 0.35
            if heat >= FIRE_FLOOR:
                px(g, x, y, next(col for top, col in EXPLOSION_RAMP if heat < top))
                continue
            # smoke, thinning into tatters as it ages
            if p < 0.4:
                continue
            puff = _smooth_noise(x, y, 23 + v + frame, 4)
            if puff < 0.2 + p * 0.5:
                continue
            px(g, x, y, '5' if puff > 0.78 else ('6' if puff > 0.6 else '7'))
    return g


# ================= RAIN =================
# Call Rain: streaks falling across a tile, and drops splashing where they
# land. RAIN_FRAMES loop; streaks wrap top to bottom, so tiles side by side
# make one sheet of rain.
RAIN_FRAMES = 4

def rain(frame):
    g = new_grid()
    for i in range(18):
        x0 = int(_noise(i, 1, 3) * 32)
        y0 = int(_noise(i, 2, 3) * 32) + frame * 8
        length = 4 + int(_noise(i, 3, 3) * 3)
        for j in range(length):
            # slanting a pixel left every other row, as if blown
            px(g, (x0 - (y0 + j) // 2) % 32, (y0 + j) % 32, 'c' if j >= length - 2 else 'a')
    for i in range(4):
        sx = 3 + int(_noise(i, frame, 9) * 26)
        sy = 4 + int(_noise(i, frame + 7, 9) * 24)
        px(g, sx - 1, sy, 'a'); px(g, sx + 1, sy, 'a'); px(g, sx, sy - 1, 'c')
        px(g, sx - 2, sy + 1, 'S'); px(g, sx + 2, sy + 1, 'S')
    return g


# ================= ITEM BADGES (what a potion or scroll does) =================
# Every scroll shares one sprite and the potions differ only in colour, so
# each gets a small glyph drawn over its icon. A badge is a whole 32x32 cell
# with the glyph in the top-right corner (the hotbar's key label is top-left
# and stack counts are bottom-right), so it lines up with the item sprite
# under it at any size: the inventory, hotbar and the ground all draw it over
# the item's own cell. A dark outline keeps it legible over the item.

def badge(rows):
    """Stamp `rows` (palette chars, '.' clear) into the top-right corner of
    a fresh cell and ring it with a one-pixel outline."""
    g = new_grid()
    w = max(len(r) for r in rows)
    x0, y0 = 31 - w, 1
    filled = set()
    for j, row in enumerate(rows):
        for i, c in enumerate(row):
            if c != '.':
                px(g, x0 + i, y0 + j, c)
                filled.add((x0 + i, y0 + j))
    for (x, y) in filled:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                if (x + dx, y + dy) not in filled:
                    px(g, x + dx, y + dy, 'O')
    return g

def badge_heal():          # Health Potion: a heart
    return badge([".RRR.RRR.",
                  "RRhRRRRRr",
                  "RhRRRRRRr",
                  "RRRRRRRRr",
                  ".RRRRRRr.",
                  "..RRRRr..",
                  "...RRr...",
                  "....r...."])

def badge_regen():         # Regeneration Potion: a green cross
    return badge(["...GGG...",
                  "...GGg...",
                  "...GGg...",
                  "GGGGGGGGG",
                  "GGGGGGGGg",
                  "GgggGgggg",
                  "...GGg...",
                  "...GGg...",
                  "...ggg..."])

def badge_strength():      # Strength Potion: a flexed arm
    return badge(["......ss.",
                  ".....ssss",
                  ".....ssss",
                  "......sss",
                  "..ss..sss",
                  ".shss.sss",
                  "sssssssss",
                  "ssssssstt",
                  ".ttttttt."])

def badge_confusion():     # Confusion Potion: a swirl
    return badge(["..iiiii..",
                  ".i.....i.",
                  "i..iii..i",
                  "i.i...i.i",
                  "i.i.i.i.i",
                  "i.i..i..i",
                  "i..i...i.",
                  ".i..iii..",
                  "..i......"])

def badge_invisibility():  # Scroll of Invisibility: an eye, struck out
    # the Reveal eye, faded and struck through; the strike is cut into the
    # eye with the outline colour so it stays a line, not a smear
    eye = ["..........",
           "..ccccc...",
           ".ccCCCcc..",
           "ccCNNNCcc.",
           ".ccCCCcc..",
           "..ccccc...",
           ".........."]
    g = [list(r) for r in eye]
    for k in range(7):
        x, y = 8 - k, k
        g[y][x] = 'R'
        if x + 1 < 10 and g[y][x + 1] != '.':
            g[y][x + 1] = 'O'
    return badge([''.join(r) for r in g])

def badge_speed():         # Scroll of Speed: a lightning bolt
    return badge(["...YYYY",
                  "..YYYY.",
                  ".YYYY..",
                  "YYYYYYY",
                  "..YYYY.",
                  ".YYYu..",
                  ".YYu...",
                  "YYu....",
                  "Yu....."])

def badge_protection():    # Scroll of Protection: a shield
    return badge(["ccccccccC",
                  "cSSSSSSSC",
                  "cSSyySSSC",
                  "cSSyySSSC",
                  ".cSSSSSC.",
                  ".cSSSSSC.",
                  "..cSSSC..",
                  "...cSC...",
                  "....C...."])

def badge_blink():         # Scroll of Blink: a dotted leap, landing
    # a sparkle where you land, two motes trailing back to where you were
    return badge(["......l...",
                  "......P...",
                  ".....lPl..",
                  "...lPPcPPl",
                  ".....lPl..",
                  "..l...P...",
                  "......l...",
                  "l........."])

def badge_fear():          # Scroll of Fear: a skull
    return badge([".BBBBBBB.",
                  "BBBBBBBBb",
                  "BBDDBDDBb",
                  "BBDDBDDBb",
                  "BBBBDBBBb",
                  ".BBBBBBb.",
                  "..BbBbB..",
                  "..B.B.B.."])

def badge_fireball():      # Scroll of Fireball: a flame
    return badge(["....f....",
                  "...ff..f.",
                  "..fff.ff.",
                  "..fYfff..",
                  ".ffYYff.f",
                  ".fYYyYff.",
                  "ffYyyyYff",
                  ".fYyyyYf.",
                  "..fffff.."])

def badge_reveal():        # Scroll of Reveal: an open eye
    return badge(["..ccccc..",
                  ".ccSSScc.",
                  "ccSNNNScc",
                  ".ccSSScc.",
                  "..ccccc.."])

def badge_mapping():       # Scroll of Mapping: a folded map, X marks the spot
    return badge(["TyyyTyyyT",
                  "TywyTyyyT",
                  "TyywTRyRT",
                  "TyyyTwRyT",
                  "TyyyTRyRT",
                  "TyyyTyyyT",
                  "TTTTTTTTT"])

def badge_slow():          # Scroll of Slow: an hourglass
    return badge(["wwwwwwwww",
                  ".cyyyyyC.",
                  "..cyyyC..",
                  "...cyC...",
                  "....y....",
                  "...c.C...",
                  "..c.y.C..",
                  ".cyyyyyC.",
                  "wwwwwwwww"])

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
    ('health_heart', health_heart),
    ('xp_star', xp_star),
    ('hunger_drumstick', hunger_drumstick),
    ('light_sun', light_sun),
] + [
    (f'held_torch_{i}', (lambda i=i: held_torch(i))) for i in range(len(TORCH_FLAMES))
] + [
    ('held_torch_unlit', held_torch_unlit),
    ('badge_heal', badge_heal),
    ('badge_regen', badge_regen),
    ('badge_strength', badge_strength),
    ('badge_confusion', badge_confusion),
    ('badge_invisibility', badge_invisibility),
    ('badge_speed', badge_speed),
    ('badge_protection', badge_protection),
    ('badge_blink', badge_blink),
    ('badge_fear', badge_fear),
    ('badge_fireball', badge_fireball),
    ('badge_reveal', badge_reveal),
    ('badge_mapping', badge_mapping),
    ('badge_slow', badge_slow),
    ('web', web),
    ('fountain', fountain),
] + [
    (f'{name}_{k}', (lambda fn=fn, k=k: fn(k)))
    for name, fn, n in [('arrow_flight', arrow_flight, FLIGHT_DIRECTIONS),
                        ('grave_bolt_flight', grave_bolt_flight, FLIGHT_DIRECTIONS),
                        ('fireball_flight', fireball_flight, FLIGHT_DIRECTIONS),
                        ('explosion', lambda k: explosion(k % EXPLOSION_FRAMES, k // EXPLOSION_FRAMES),
                         EXPLOSION_VARIANTS * EXPLOSION_FRAMES),
                        ('rain', rain, RAIN_FRAMES)]
    for k in range(n)
]

# Where each icon lives in assets/32rogues/items.png, as (1-based row, 0-based
# column): the same convention as `rc()` in src/tile.rs, and items.txt's
# "row.letter" notation (26.h is row 26, column 7). Stock items.png is 26 rows;
# these fill the free end of row 26 and rows added after it.
SLOTS = {
    'raise_dead': (26, 5),        # 26.f
    'life_drain': (26, 6),        # 26.g
    'health_heart': (26, 7),      # 26.h
    'xp_star': (26, 8),           # 26.i
    'hunger_drumstick': (26, 9),  # 26.j
    'light_sun': (26, 10),        # 26.k
    'tame': (27, 0),              # 27.a
    'rest': (27, 1),              # 27.b
    'sleep': (27, 2),             # 27.c
    'barkskin': (27, 3),          # 27.d
    'sprint': (27, 4),            # 27.e
    'disengage': (27, 5),         # 27.f
    'tumble': (27, 6),            # 27.g
    'crippling_shot': (27, 7),    # 27.h
    'stun': (27, 8),              # 27.i
    # held torch: six lit frames in a row (tile_ids::HELD_TORCH), then unlit
    **{f'held_torch_{i}': (28, i) for i in range(len(TORCH_FLAMES))},  # 28.a-f
    'held_torch_unlit': (28, 6),  # 28.g
    'badge_heal': (28, 7),        # 28.h
    'badge_regen': (28, 8),       # 28.i
    'badge_strength': (28, 9),    # 28.j
    'badge_confusion': (28, 10),  # 28.k
    'badge_invisibility': (29, 0),  # 29.a
    'badge_speed': (29, 1),       # 29.b
    'badge_protection': (29, 2),  # 29.c
    'badge_blink': (29, 3),       # 29.d
    'badge_fear': (29, 4),        # 29.e
    'badge_fireball': (29, 5),    # 29.f
    'badge_reveal': (29, 6),      # 29.g
    'badge_mapping': (29, 7),     # 29.h
    'badge_slow': (29, 8),        # 29.i
    'web': (29, 9),               # 29.j
    'fountain': (29, 10),         # 29.k
}

def _run(name, first_row, n):
    """Slots for an n-cell run starting at a row's first column, wrapping
    onto the rows below: the renderer adds the frame or direction to the
    first cell's id, so a run's cells have to be consecutive."""
    return {f'{name}_{k}': (first_row + k // 11, k % 11) for k in range(n)}

SLOTS.update(_run('arrow_flight', 31, FLIGHT_DIRECTIONS))       # 31.a-32.e
SLOTS.update(_run('grave_bolt_flight', 33, FLIGHT_DIRECTIONS))  # 33.a-34.e
SLOTS.update(_run('fireball_flight', 35, FLIGHT_DIRECTIONS))    # 35.a-36.e
SLOTS.update(_run('explosion', 37, EXPLOSION_VARIANTS * EXPLOSION_FRAMES))  # 37.a-39.b
SLOTS.update(_run('rain', 40, RAIN_FRAMES))                     # 40.a-d

# Stock sprites copied into items.png rather than drawn: the inventory and HUD
# draw from the items sheet only, so a sprite that lives on another sheet has
# to be copied in to be shown there. Slot -> (source sheet, 1-based row,
# 0-based column), all in the same 32rogues directory as items.png.
COPIES = {
    (27, 9): ('animated-tiles.png', 6, 0),   # 27.j torch, lit (first frame)
    (27, 10): ('animated-tiles.png', 5, 0),  # 27.k torch, unlit
    (30, 0): ('tiles.png', 17, 9),           # 30.a pressure plate (the fire trap item)
}

def to_image(g):
    im = Image.new('RGBA', (32, 32), (0, 0, 0, 0))
    for y in range(32):
        for x in range(32):
            if g[y][x] != '.':
                im.putpixel((x, y), hex2rgba(PAL[g[y][x]]))
    return im

def paste_into(sheet_path):
    """Write every icon into its slot in items.png, growing the sheet (stock
    is 26 rows) to as many rows as SLOTS and COPIES reach, after repainting
    the flames in animated-tiles.png beside it. Each slot is cleared first,
    so re-running is harmless and a redrawn icon replaces the old one."""
    import os
    sheet_dir = os.path.dirname(sheet_path)
    # First, so the torch copied into COPIES below is already amber.
    repaint_animated_tiles(sheet_dir)
    sheet = Image.open(sheet_path).convert('RGBA')
    rows = max(r for r, _ in list(SLOTS.values()) + list(COPIES))
    if sheet.height < rows * 32:
        grown = Image.new('RGBA', (sheet.width, rows * 32), (0, 0, 0, 0))
        grown.paste(sheet, (0, 0))
        sheet = grown
    for name, fn in ICONS:
        row, col = SLOTS[name]
        box = (col * 32, (row - 1) * 32, col * 32 + 32, row * 32)
        sheet.paste(to_image(fn()), box)  # plain paste: replaces the slot, alpha and all
    for (row, col), (source, src_row, src_col) in COPIES.items():
        src = Image.open(os.path.join(sheet_dir, source)).convert('RGBA')
        tile = src.crop((src_col * 32, (src_row - 1) * 32, src_col * 32 + 32, src_row * 32))
        sheet.paste(tile, (col * 32, (row - 1) * 32, col * 32 + 32, row * 32))
    sheet.save(sheet_path)
    print(f'pasted {len(ICONS)} icons and copied {len(COPIES)} sprites into {sheet_path}')

if __name__ == '__main__':
    import os, sys
    # `make_tiles.py --paste assets/32rogues/items.png` writes every icon into
    # the sheet; with no arguments it renders previews into $OUTDIR instead.
    if len(sys.argv) == 3 and sys.argv[1] == '--paste':
        paste_into(sys.argv[2])
        sys.exit(0)
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
    sheet.save(f'{outdir}/contact.png')
    print('done')
