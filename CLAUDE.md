# Working in this repository

Conventions for grid-roguelike. These were previously buried at the bottom of
[docs/depth-milestone-plan.md](docs/depth-milestone-plan.md); this file is now
the canonical copy.

See [README.md](README.md) for what the game is and how to build it, and
[assets/README.md](assets/README.md) for the asset contract.

## Commands

```bash
cargo build --all-targets          # compile, tests included
cargo test                         # 158 tests, no window needed
cargo clippy --all-targets         # has a standing backlog; don't add to it
cargo run --release                # play (must be run from the repo root)
cargo run --release --features profiling   # puffin on 127.0.0.1:8585
```

Run from the repo root: asset paths resolve against the process working
directory, not the executable.

Tests never open an audio stream (`GameEngine::new()` skips `AudioManager` under
`cfg(test)`), so they can run in parallel without touching the sound card.
Setting `ROGUELIKE_NO_AUDIO` does the same for a real run.

### Don't open the game on the user's screen

`cargo run` is for the user. An agent that needs to see the game must not
launch it on the real display. The user is often working at the same machine,
and a window that keeps popping up, stealing focus and playing sound
interrupts them. Check behaviour with a unit test first. If you really need
pixels, use the headless driver in
[.claude/skills/run-game](.claude/skills/run-game/SKILL.md), which runs the
game under Xvfb with audio off and saves screenshots for you to read.

## Conventions

### Constants live in `src/constants/`

No magic numbers in systems. `src/constants/` is split by domain — `combat`,
`time`, `dungeon`, `items`, `abilities`, `effects`, `enemies`, `animation`,
`camera`, `ui`, `gameplay` — and `mod.rs` re-exports all of them flat, so
`use crate::constants::*;` is the normal import and a new constant needs no
import changes at the use site.

Put the number in the domain module it belongs to, with a doc comment saying
what it means and what tuning it up or down does. A literal in a system is a
tuning value nobody can find.

### Anything time-based uses the game-time accumulator pattern

Systems that advance on an interval take the game-time delta for the frame plus
a mutable accumulator, and carry the remainder between calls. Two forms are in
use, both legitimate — pick by whether a step has to be a fixed size.

Fixed-size steps, for anything where a step is a discrete event that must not
be scaled (fire spreading to a neighbour either happens or it does not).
[src/systems/fire.rs](src/systems/fire.rs) (`tick_fire`) is the reference:

```rust
*accumulator += game_dt;
while *accumulator >= FIRE_STEP_INTERVAL {
    *accumulator -= FIRE_STEP_INTERVAL;
    // one discrete step
}
```

Threshold-and-drain, for anything that scales linearly with elapsed time and
only needs rate-limiting (hunger drain, identification progress). See
`tick_survival` in [src/systems/survival.rs](src/systems/survival.rs) and
`tick_identification` in [src/systems/identify.rs](src/systems/identify.rs):

```rust
*accumulator += game_dt;
if *accumulator < SURVIVAL_TICK_INTERVAL {
    return;
}
let step = *accumulator;   // the whole elapsed span, not a fixed slice
*accumulator = 0.0;
```

This matters because the game clock is not wall time. Driving these systems
from game time rather than frame time means they freeze while the game is
paused or waiting on input, and correctly race ahead during rest/sleep
fast-forward. Never tick game state off real time or off frame count.

### Actions flow through the action/event system

Gameplay belongs in an action handler or a system, not in the engine tick. The
tick dispatches; it does not decide outcomes.

The shape of a new action — a new `ActionType` variant, its energy cost, its
duration, where it is detected and where its effects are applied — is laid out
in [.claude/agents/feature-builder.md](.claude/agents/feature-builder.md).
(That file predates the move to `src/engine/`, `src/constants/` and `src/ui/`
as directories, so trust its *architecture* and check its file paths against
the tree.)

Systems communicate through `GameEvent` ([src/events.rs](src/events.rs)), not
by calling each other. Emit an event and let audio, VFX and UI react; that is
why [src/audio.rs](src/audio.rs) and [src/vfx.rs](src/vfx.rs) have no
knowledge of combat.

### Never `.unwrap()` a data-driven lookup — degrade gracefully

Anything that depends on content — an asset file, a loot table entry, a
history record, a component that may have been removed — must handle absence
instead of panicking. A missing sound should mean silence, not a crash.

Examples to copy:

- [src/audio.rs](src/audio.rs) filters sound paths on `exists()` and silently
  skips sound types with no files.
- [src/run_history.rs](src/run_history.rs) treats all of its IO as best-effort:
  corrupt lines are skipped, write failures are logged and ignored.

`expect()` is acceptable for genuine programmer invariants that cannot depend
on external content — a statically-known sprite sheet key, for instance.

### Sprites

In order of preference:

1. **Use a 32rogues sheet sprite.** Add the tile ID to `tile_ids` in
   [src/tile.rs](src/tile.rs), with the sheet's own `row.letter` notation in a
   trailing comment to match the pack's documentation. Note that `rc()` takes a
   **1-based row** and a 0-based column.
2. **Tint an existing sprite** for a variant. Tints multiply with the texture
   (`1.0, 1.0, 1.0` is untinted) — fire arrows are tinted normal arrows,
   revealed traps are tinted trap doors.
3. **Emoji text overlay as a last resort**, for transient state that is not
   really a sprite (the sneaking indicator in [src/ui/vfx.rs](src/ui/vfx.rs)).

Custom pixel art is a real cost: it has to be hand-drawn into
[tools/make_tiles.py](tools/make_tiles.py) and then pasted into `items.png` by
hand, and since `items.png` itself cannot be committed, every checkout has to
redo the paste. The script is tracked so the pixels are at least reproducible —
keep new custom art in it rather than editing a PNG directly, or the next clone
loses the sprite. Exhaust options 1 and 2 first.

### Assets are not in the repository

`assets/` is gitignored. Do not commit art or audio, and do not add code that
requires a new asset file without saying so and updating
[assets/README.md](assets/README.md) — that file is the only thing standing
between a fresh clone and a panic on launch.

## Module layout

| Path | Responsibility |
| --- | --- |
| `src/app.rs` | window, GL context, event loop. No gameplay. |
| `src/engine/` | owns simulation state; runs the tick, floor transitions, initialisation |
| `src/components.rs` | the ECS vocabulary: components and item/affix data |
| `src/systems/` | game logic by domain |
| `src/systems/actions/` | what an action does once its duration elapses |
| `src/time_system.rs` | game clock and action scheduler |
| `src/events.rs` | `GameEvent` definitions |
| `src/constants/` | all tuning numbers, by domain |
| `src/ui/` | egui panels |
| `src/render/`, `src/renderer.rs`, `src/multi_tileset.rs` | GPU rendering and sheet UVs |
| `src/dungeon_gen.rs`, `src/grid.rs`, `src/spawning.rs` | generation, terrain, spawn tables |
| `src/fov.rs`, `src/pathfinding.rs`, `src/spatial_cache.rs`, `src/queries.rs` | visibility, A*, blocking cache, shared queries |

New gameplay goes in `src/systems/`. New tuning goes in `src/constants/`. Never
in `src/main.rs` or `src/app.rs`.

## In flux

**How simulation state is threaded through function signatures is being
reworked right now** on the `claude/refactor-state-threading` branch, which is
also splitting up `src/engine/mod.rs` and `src/components.rs`. Check that
branch before writing anything that depends on the current parameter lists, the
layout of those two files, or where a given component is declared. The
conventions above are unaffected.

Also deliberately deferred, so don't treat either as a quick win:

- The clippy and rustc warning backlog. A large share of it is
  `too_many_arguments` and dead-code lints in exactly the files the refactor is
  rewriting, so it waits for that to land.
- `runs_history.jsonl` is written to the working directory. A user-data
  directory via the `dirs` crate would be better, but it adds a dependency and
  orphans existing history files.
