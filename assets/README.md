# `assets/` — what goes here

**The game will not start without this directory populated.** `Renderer::new`
loads the 32rogues sprite sheets with `.expect("Failed to load tileset")`
([src/render/mod.rs](../src/render/mod.rs)), so a missing or misnamed sheet is a
panic on launch, not a degraded run.

The contents of `assets/` are **not in git**. The binding reason is the
32rogues license, which allows commercial use but states plainly:

> "You may not redistribute or resell it."

The audio and the spare tilesets are CC0 and could legally be committed; they
stay out because 41 MB of binary art has no business in a source history. So
`.gitignore` ignores `assets/*` and makes an exception only for this file. You
supply the packs yourself, and this document is the contract between what you
put here and what the code looks for.

## Required: `assets/32rogues/`

Five sheets, loaded by `SHEET_SPECS` in
[src/multi_tileset.rs](../src/multi_tileset.rs). Tiles are 32×32 throughout.
Filenames and column counts are not negotiable — the column count is how a flat
tile ID is turned into a row/column, so a sheet of the wrong width renders the
wrong sprite for everything rather than failing loudly.

| File | Columns | Expected size | Contents |
| --- | --- | --- | --- |
| `tiles.png` | 17 | 544×832 (17×26) | terrain, walls, doors, stairs, traps |
| `rogues.png` | 7 | 224×224 (7×7) | player characters |
| `monsters.png` | 12 | 384×416 (12×13) | enemies |
| `items.png` | 11 | 352×864 (11×27) | weapons, armour, potions, food, ability icons |
| `animated-tiles.png` | 11 | 352×384 (11×12) | fire pits, torches, braziers |

The `.txt` files that ship alongside each sheet are 32rogues' own row/column
documentation. The code does not read them, but the tile-ID comments in
[src/tile.rs](../src/tile.rs) use their `row.letter` notation (`10.b stone floor
1`), so keeping them is worth it when you go looking for a sprite.

**Source:** 32rogues by Seth Boyles — <https://sethbb.itch.io/32rogues>
(free to download). Commercial use is allowed; redistribution is not, which is
why the sheets are not committed here.

### `items.png` must be extended past stock

This is the one file a fresh clone gets wrong. The sheet must be **27 rows
tall (864 px)**, because the ability icons live in rows the stock pack does not
have. [src/tile.rs](../src/tile.rs) addresses them through `rc(row, col, cols)`,
where **`row` is 1-based** and `col` is 0-based:

- row 26, columns f–g — `raise dead`, `life drain`
- row 27, columns a–i — `tame`, `rest`, `sleep`, `barkskin`, `sprint`,
  `disengage`, `tumble`, `crippling shot`, `stun`

[`tools/make_tiles.py`](../tools/make_tiles.py) draws these eleven icons in the
32rogues palette. It is **tracked in the repository** — unlike everything else
under `assets/` — and needs nothing but Pillow, so the icons are reproducible
from a fresh clone rather than something you have to obtain from the author.

It writes individual 32×32 PNGs to `$OUTDIR` (default `/tmp`) plus a magnified
contact sheet at `/tmp/contact.png`. It does **not** patch `items.png` for you:
run it, then paste the icons into the sheet at the positions above, growing the
canvas to 27 rows.

```bash
pip install pillow
OUTDIR=assets/custom python3 tools/make_tiles.py
```

If you skip this, the UV lookup runs off the bottom of the texture. Because the
textures use `CLAMP_TO_EDGE`, that does not error — the affected abilities just
draw a smear of the sheet's last row. Silent, so it is worth checking the
hotbar icons after a first build.

## Required for sound: `assets/sounds/`

[src/audio.rs](../src/audio.rs) reads **only** `assets/sounds/`, in five
subdirectories, and looks for these exact filenames. Everything here is
optional in practice: `find_sounds` filters on `Path::exists`, and a sound type
with no files is silently skipped. Audio degrades to silence; it never panics.

| Directory | Filenames the code asks for |
| --- | --- |
| `battle/` | `swing.wav`, `swing2.wav`, `swing3.wav`, `spell.wav`, `magic1.wav`, `sword-unsheathe.wav`, `sword-unsheathe2.wav`, `sword-unsheathe3.wav` |
| `inventory/` | `coin.wav`, `coin2.wav`, `coin3.wav`, `metal-small1.wav`, `metal-small2.wav`, `metal-small3.wav`, `bubble.wav`, `bubble3.wav`, `bottle.wav` |
| `interface/` | `interface1.wav` |
| `world/` | `door.wav` |
| `enemies/` | `mnstr1.wav`–`mnstr4.wav`, `slime1.wav`–`slime3.wav`, `shade1.wav`–`shade3.wav` |

Where several files are listed for one effect, the game picks one at random per
play, so partial sets work fine — they just vary less.

**These are the project's own filenames, not any pack's.** `assets/sounds/` is
a hand-curated, renamed subset, and nothing on disk records which clip came
from where. The `essential_retro_video_game_sound_effects_collection_juhani_junkala/`
directory (28 MB — "The Essential Retro Video Game Sound Effects Collection
[512 sounds]" by Juhani Junkala, released CC0, per its own `INFO.txt`) sits
alongside as the likely source library, but its filenames are `sfx_*` under
Title-Case directories and none of them match, so you **cannot** just unzip it
into `assets/sounds/`.

Treat `assets/sounds/` as the authoritative set and copy it from an existing
checkout. Rebuilding it from the pack means re-choosing and re-naming every
clip by hand — and since the mapping was never recorded, only the author can
say which original each one was.

## Optional / unused

Present in the author's tree but **not read by any code** — safe to omit:

- `hexanys_roguelike_tiles_0.3.0/` — Hexany's Roguelike Tiles, a 16×16
  monochrome 1-bit tileset by Hexany Ives, CC0.
  <https://hexany-ives.itch.io/hexanys-roguelike-tiles>
- `urizen_onebit_tileset__v2d0.png` — a 1-bit tileset. No license or source
  file travels with it in this tree, so its terms are unrecorded; check before
  using it for anything.
- `minirogue-all.{png,tsj,tsx}`, `water-example.tmx`, `watertiles-auto.tsx` —
  Tiled scratch files, provenance unrecorded.
- `32rogues/animals.png`, `autotiles.png`, `items-palette-swaps.png`,
  `32rogues-palette.png` — part of the 32rogues pack, but not in
  `SHEET_SPECS`.

## Expected layout

Only the **bold** entries are required.

```
assets/
├── README.md                     (this file — the only tracked file here)
├── 32rogues/
│   ├── tiles.png                 ** 544×832
│   ├── rogues.png                ** 224×224
│   ├── monsters.png              ** 384×416
│   ├── items.png                 ** 352×864, extended past stock
│   ├── animated-tiles.png        ** 352×384
│   ├── *.txt                     (sheet documentation, unread)
│   └── LICENSE.txt
├── custom/
│   └── *.png                     (output of tools/make_tiles.py)
├── sounds/
│   ├── battle/                   ** see table above
│   ├── enemies/                  **
│   ├── interface/                **
│   ├── inventory/                **
│   └── world/                    **
└── (optional unused packs)
```

## Checking your work

Paths are resolved relative to the **process working directory**, not the
executable, so run from the repo root:

```bash
cargo run --release
```

A panic mentioning `Failed to load tileset` means a sheet under
`assets/32rogues/` is missing or misnamed. Silence means `assets/sounds/` is
absent or renamed — the game plays on regardless.
