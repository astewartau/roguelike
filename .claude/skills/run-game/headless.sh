#!/usr/bin/env bash
# Run a drive script against the real game on a private virtual X display, so
# nothing appears on the user's screen and nothing plays through their speakers.
#
#   .claude/skills/run-game/headless.sh path/to/drive.sh [out-dir]
#
# The drive script is sourced inside the virtual display with these helpers:
#   launch_game          start the release binary, wait for its window
#   press KEY...         hold each key briefly (taps are dropped; see SKILL.md)
#   click X Y [BUTTON]   click at window-relative pixel coords (default left)
#   wait_s SECONDS       let the game run (real seconds)
#   shot NAME            save $OUT_DIR/NAME.png of the game window
#   quit_game            kill the game (also done automatically on exit)
#
# Run from the repo root (asset paths resolve against the working directory).
set -euo pipefail

if [[ "${1:-}" == "--inner" ]]; then
    shift
    DRIVE="$1"; OUT_DIR="$2"
    BIN="${ROGUELIKE_BIN:-./target/release/grid-roguelike}"
    GAME_PID=""; WID=""

    launch_game() {
        "$BIN" >"$OUT_DIR/game.log" 2>&1 &
        GAME_PID=$!
        for _ in $(seq 1 120); do
            WID=$(xdotool search --name '^Roguelike$' 2>/dev/null | head -1 || true)
            [[ -n "$WID" ]] && break
            if ! kill -0 "$GAME_PID" 2>/dev/null; then
                echo "game exited during startup; see $OUT_DIR/game.log" >&2
                tail -20 "$OUT_DIR/game.log" >&2
                exit 1
            fi
            sleep 0.5
        done
        [[ -n "$WID" ]] || { echo "game window never appeared" >&2; exit 1; }
        sleep 1.5   # first frames: tileset upload, start screen layout
        xdotool windowfocus --sync "$WID" 2>/dev/null || true
    }
    press() {
        for k in "$@"; do
            xdotool keydown "$k"; sleep 0.1; xdotool keyup "$k"; sleep 0.25
        done
    }
    click() {
        xdotool mousemove --window "$WID" "$1" "$2"; sleep 0.1
        xdotool mousedown "${3:-1}"; sleep 0.1; xdotool mouseup "${3:-1}"; sleep 0.25
    }
    wait_s() { sleep "$1"; }
    shot() { import -window "$WID" "$OUT_DIR/$1.png"; echo "$OUT_DIR/$1.png"; }
    quit_game() { [[ -n "$GAME_PID" ]] && kill "$GAME_PID" 2>/dev/null || true; GAME_PID=""; }
    trap quit_game EXIT

    # shellcheck source=/dev/null
    source "$DRIVE"
    exit 0
fi

DRIVE="${1:?usage: headless.sh drive.sh [out-dir]}"
OUT_DIR="${2:-$(mktemp -d)}"
mkdir -p "$OUT_DIR"

for tool in xvfb-run xdotool import; do
    command -v "$tool" >/dev/null || {
        echo "missing '$tool' (Debian/Ubuntu: apt install xvfb xdotool imagemagick)" >&2
        exit 1
    }
done
[[ -f assets/32rogues/tiles.png ]] || {
    echo "assets/32rogues is missing: the game panics on launch without it (see assets/README.md)" >&2
    exit 1
}

# Unsetting WAYLAND_DISPLAY is what keeps the window off the user's desktop:
# winit prefers Wayland whenever it is set, and would ignore the Xvfb DISPLAY.
env -u WAYLAND_DISPLAY ROGUELIKE_NO_AUDIO=1 \
    xvfb-run -a -s "-screen 0 1280x720x24" \
    bash "$0" --inner "$(realpath "$DRIVE")" "$(realpath "$OUT_DIR")" \
    2> >(grep -v 'XGetInputFocus returned' >&2)
echo "screenshots in $OUT_DIR"
