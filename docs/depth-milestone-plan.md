# Depth Milestone — Master Plan

**Goal:** Give runs shape and depth: combinatorial items, a survival clock,
class/ability balance, a reactive fire ecosystem, dungeon discoveries, enemy
roles + bosses, and light meta (seeded runs, run history). Pure roguelike —
no cross-run power creep.

Builds on `docs/equipment-loot-plan.md` (ItemInstance, armor, `apply_damage`
chokepoint — all landed).

Each phase lands independently with a green build (`cargo build && cargo test`).

---

## Phase 0 — Code health (prerequisite)

1. **Split `systems/actions.rs` (2197 lines)** into a `systems/actions/`
   module directory: `movement.rs` (move/doors), `combat.rs`
   (attack/cleave/stun), `items.rs` (equip/drop/consume-adjacent),
   `abilities.rs` (blink/fireball/tame/life-drain/fear/sprint/disengage/
   tumble/crippling-shot), `traps.rs` (fire trap, snare, chest opening),
   `projectiles.rs` (shoot bow, throw, `calculate_arrow_path`).
   `mod.rs` re-exports everything so call sites don't change.
2. **Fix `engine/initialization.rs` unwraps** (lines ~73, 83, 110, 148,
   252-258): `.choose(rng).unwrap()` on loot pools → fall back to a safe
   default item (e.g. `ItemType::Bread`) instead of panicking.
3. **Delete legacy `src/tileset.rs`** if nothing references it; move any
   shared PNG-loading into `multi_tileset.rs`.

## Phase 1 — Component-based items (the loot plan's phase 3, expanded)

The core idea: affixes are **composable components**, and higher rarity =
more components = emergent combinations. A Legendary isn't a bigger number,
it's an unexpected *combination*.

### Affix expansion (`components.rs`)

```rust
pub enum Affix {
    // stat mods (existing style)
    Damage(i32), Defense(i32),
    Strength(i32), Agility(i32), Intelligence(i32), MaxHealth(i32),
    // on-hit components (weapons/ammo): chance in 0..1
    OnHitIgnite(f32),          // sets Burning; hooks fire.rs
    OnHitSlow(f32),
    OnHitFear(f32),
    OnHitLifesteal(f32),       // fraction of damage healed
    OnHitKnockback,            // push target 1 tile
    // conditional components
    KillHeal(i32),             // on kill: heal N
    LowHealthDamage(f32),      // below 30% HP: +X% damage
    // curses (negative, hidden until identified)
    CursedFragile(i32),        // -N defense
    CursedHeavy(f32),          // slower attack/move
    CursedLoud,                // attacks make double noise radius
}
```

- **Rarity → component count:** Common 0, Magic 1, Rare 2, **Legendary 3-4 +
  generated name** ("Emberfang, Sword of the Wolf") from its components.
  Add `Rarity::Legendary` (currently reserved).
- On-hit resolution goes through the existing `apply_damage`/melee path in
  one place (`systems/combat.rs`), NOT scattered per-ability.
- Roll tables live in `systems/item_defs.rs`, weighted by floor depth.

### Identification & curses

- `ItemInstance.identified` already exists. Magic+ drops spawn unidentified:
  UI shows rarity color but affixes as "???".
- Identify: automatic after carrying for a while, scaled by INT
  (`identify_seconds = BASE / (1 + (int - 10) * 0.1)`), or instantly at a
  shrine (Phase 5). Curses only revealed on identify — or the hard way.
- Cursed items are a downside *component* alongside good ones (gamble items),
  not pure trash.

### Accessories & ammo

- New slots on `Equipment`: `ring: Option<ItemInstance>`,
  `amulet: Option<ItemInstance>`. New ItemTypes `Ring`, `Amulet` — carriers
  for affix components (no base stats).
- **Fire Arrow**: new stackable `ItemType::FireArrow`; bow consumes the
  selected ammo; hit applies Burning + can ignite grass/oil via fire.rs.

## Phase 2 — Survival clock: hunger + fatigue/sleep

Two meters on the player, ticked in game-time (respect pause), UI in status bar.

- **Hunger** 0-100, drains ~1 per 12s of game time (tunable in
  `constants/gameplay.rs`). Food restores (Cheese 25, Bread 35, Apple 15 —
  heal values stay too, halved). Below 25: "Hungry" (no natural HP regen).
  At 0: "Starving" (1 damage / 5s). Resting drains hunger at 2x.
- **Fatigue** 0-100, grows ~1 per 20s awake, faster while Sprinting.
  Above 75: "Tired" (alertness gain vs you +25%, -10% damage). At 100:
  "Exhausted" (0.75x speed, no energy regen).
- **Sleep** (new action, replaces nothing — Rest stays for HP): sleep until
  fatigue 0 or interrupted; while asleep you're `Unaware` — enemies get the
  2x sneak multiplier on YOU and you wake on damage/noise. Sleeping is a
  real decision: safe room? door closed?

## Phase 3 — INT, learnable scrolls, Raise Dead

- **INT scaling for magic** (`constants/abilities.rs` helpers): fireball
  damage, life-drain per-tick, fear/barkskin/confusion durations, scroll
  effect magnitudes scale by `1 + (int - 10) * 0.05`. Staff *melee* damage
  unchanged.
- **Learnable scrolls:** each scroll has `min_learn_int`. With enough INT, a
  "Study" option (inventory context) consumes the scroll and permanently adds
  it as a repeatable ability with energy cost + long cooldown (e.g. Blink:
  INT 14, 1 energy, 45s cd). Learned abilities join the class-ability hotbar
  system. Single-use stays the low-INT path.
- **Raise Dead (Necromancer):** targets a bones container within 5 tiles;
  channel 3s; consumes the bones; spawns a Skeleton companion on the
  existing companion-AI path (tamed-animal infrastructure). Cap: INT-scaled
  (1 at 10, +1 per 6 INT). 2 energy, 30s cooldown.
- Fighter gets its Stun formally on the hotbar if not already; balance pass
  on costs/cooldowns in Phase 8.

## Phase 4 — Fire ecosystem

- **Oil puddle**: new tile overlay entity (dark-tinted water sprite or "🛢"
  emoji fallback). Walkable; ignites at 90% spread chance; burns 8s with
  taller flames; entities standing in burning oil ignite at high chance.
- **Oil barrel**: barrel sprite (tiles.png 18.e, red-tinted). Blocks
  movement. When Burning reaches it: 3s fuse, then explodes — 15 damage
  radius 1, spawns oil puddles radius 1-2, ignites them. Spawns in Storage
  rooms + occasionally corridors.
- **Braziers can be toppled**: interact (or knockback into one) → spills
  fire onto its tile + one adjacent, igniting grass/oil.
- **Water flask**: `ItemType::WaterFlask { filled }` — fill by using on/
  adjacent to water tile; splash (throwable arc, radius 1) douses burning
  tiles/entities and wets grass (temporarily unignitable).

## Phase 5 — Traps, discoveries, hidden rooms

- **Floor traps** placed by dungeon gen (1-3 per floor, more deeper):
  Spike (8 dmg), Fire (ignite + burst), Snare (root 5s), Alarm (wakes
  radius 10). Hidden by default; detection roll when adjacent, scaled by
  Agility; revealed traps render (trap-door tile 17.n or emoji); can be
  stepped around or triggered by enemies too.
- **Room furniture** (interactables, one per ~3 rooms):
  - **Fountain**: drink → random (heal full / restore hunger / random buff /
    poison-ish debuff). One use, then dries up.
  - **Altar**: sacrifice an inventory item → chance of blessing (permanent
    +1 stat or identify-all) vs curse (random debuff). Better items = better
    odds.
  - **Shrine**: instantly identifies carried items + short buff. One use.
- **Hidden rooms**: BSP occasionally seals a small treasure room behind a
  `SecretDoor` (renders as wall); passive discovery when adjacent scaled by
  Agility; contains a chest with boosted rarity roll.

## Phase 6 — Enemy roles + bosses

- **Goblin Shaman**: goblin stats but 5 INT; keeps 3-5 tile distance; heals
  lowest-HP visible ally (8 HP, 4s cd) or hastes an attacker. Priority
  target — appears floor 1+.
- **Giant Spider / Lesser Giant Spider** (sprites monsters.png 7.i/7.j):
  spider lays **web tiles** (concealment-like entity: roots non-spiders 2s
  on entry, highly flammable — fire + webs = beautiful). Giant version
  floor 3+, applies Slowed on hit.
- **Bosses** every 3rd floor (3, 6, 9...): a named, scaled-up variant
  (e.g. "Gnash, Orc Warlord" — 2.5x HP, unique ability like ground-slam
  stun) in the largest room, guarding a guaranteed Rare/Legendary chest.
  Boss kill = floor milestone message + XP bonus.

## Phase 7 — Seeded runs + run history

- **Seeded RNG**: `GameState` owns a `StdRng` + the `u64` seed. Dungeon
  gen, spawning, and loot rolls draw from it (AI jitter/VFX can stay on
  thread_rng). Start screen: shows seed for the new run, allows entering
  one. Death/win screen shows the seed.
- **Run history**: append JSON lines to `runs_history.jsonl` next to the
  binary (or `dirs`-less: `./runs_history.jsonl`): timestamp, seed, class,
  floor reached, game-time survived, kills, cause of death. Game-over
  screen shows this run's stats; start screen shows a "Past Runs" panel
  (last 10).

## Phase 8 — Tests + balance

- Integration tests: melee attack → apply_damage → death → bones loot;
  hunger/fatigue tick thresholds; affix roll count per rarity; fire spread
  into oil; trap trigger; raise-dead cap.
- Balance sweep over new constants; clippy + warning cleanup pass.

---

## Conventions for all phases

- Constants in `src/constants/` (no magic numbers in systems).
- Game-time accumulator pattern for anything time-based (see fire.rs).
- Actions flow through the action/event system — no logic in engine tick
  beyond dispatch.
- Sprites: prefer 32rogues sheets; tint existing sprites for variants;
  emoji text overlay as last resort.
- No `.unwrap()` on data-driven lookups; degrade gracefully.
