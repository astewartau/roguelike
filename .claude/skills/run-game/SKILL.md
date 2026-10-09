---
name: run-game
description: How to see the game running — screenshots, checking a visual or UI change, driving the real binary with keys and clicks. Use before any `cargo run` of grid-roguelike. Runs the game on a private virtual display so no window opens on the user's desktop and no sound plays.
---

# Running the game without disturbing the user

The user is often working at the same machine. A bare `cargo run` opens a
window on their desktop, steals focus and plays sound, and an agent iterating
on screenshots does that over and over. **Never launch the game on the user's
real display.** Work down this list and stop at the first step that answers
your question:

1. **A unit test.** Most gameplay changes can be checked without pixels:
   `GameEngine::new()` plus `start_game(class, seed, &mut camera)` gives a live,
   seeded run with no window and no audio (see `engine_with_run()` in the
   tests of `src/engine/mod.rs`). Assert on world state and
   `ui_state.message_log`. This also works in cloud sessions, which have no
   art.
2. **The headless driver below**, when you genuinely need to see the screen
   (rendering, VFX, egui layout).
3. **Ask the user to look**, if neither works on this machine. Don't fall back
   to a visible window on your own.

## The headless driver

`.claude/skills/run-game/headless.sh` runs the real release binary inside
`xvfb-run` with `WAYLAND_DISPLAY` unset (otherwise winit picks Wayland and the
window lands on the user's screen regardless of `DISPLAY`) and
`ROGUELIKE_NO_AUDIO=1` (skips opening the sound device). You write a short
drive script; the driver sources it with helpers defined.

```bash
cargo build --release
cat > "$SCRATCH/drive.sh" <<'DRIVE'
launch_game
shot start            # class-select screen
click 640 477         # "Start Game" (Fighter is preselected)
wait_s 1.5
press d d d           # walk right three tiles
wait_s 1
shot moved
press i
shot inventory
DRIVE
.claude/skills/run-game/headless.sh "$SCRATCH/drive.sh" "$SCRATCH/shots"
```

Then read the PNGs with the Read tool. Run from the repo root: asset paths
resolve against the working directory. The game's stdout/stderr goes to
`<out-dir>/game.log`.

| Helper | Does |
| --- | --- |
| `launch_game` | start the binary (`$ROGUELIKE_BIN`, default `./target/release/grid-roguelike`), wait for the window |
| `press KEY...` | hold each key ~0.1 s, then release (xdotool key names: `d`, `Return`, `Escape`, `shift`, `grave`) |
| `click X Y [BUTTON]` | click at window-relative pixels (1 = left, 3 = right) |
| `wait_s S` | let real time pass |
| `shot NAME` | write `<out-dir>/NAME.png` of the game window |
| `quit_game` | kill the game (done automatically when the script ends) |

### Gotchas

- **Quick key taps get dropped.** The engine inserts a key into
  `InputState.keys_pressed` on press and removes it on release, so a
  press and release that both land between two frames never reach
  `process_keyboard`. `xdotool key d` does exactly that. Use `press`, which
  holds each key.
- **The window is 1280×720** (`WINDOW_DEFAULT_WIDTH/HEIGHT`), and click
  coordinates are relative to it. To find a button, take a `shot`, read it,
  then click.
- **Runs use a random seed** unless you pick "Custom" on the start screen and
  type one. Don't expect two runs to look the same otherwise.
- **Needs `assets/32rogues/`.** The driver refuses to start without it, since
  the game panics on a missing tileset. Cloud sessions have no art (it is
  gitignored, see `assets/README.md`), so there you are limited to step 1.
  Placeholder sheets generated with ImageMagick at the sizes in
  `assets/README.md` are enough to check layout and input flow, though not
  what a sprite looks like.
- **Needs `xvfb-run`, `xdotool` and ImageMagick's `import`.** On
  Debian/Ubuntu: `apt install xvfb xdotool imagemagick`. If they're missing on
  the user's machine, say so rather than installing system packages there
  unasked.
- Xvfb renders with Mesa's software GL (llvmpipe), so it's slow and colours
  can differ slightly from the user's GPU. Fine for layout and behaviour; not a
  reference for exact pixels.
