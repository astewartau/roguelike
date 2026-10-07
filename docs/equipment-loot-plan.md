# Equipment & Loot Depth — Implementation Plan

**Goal:** Make loot meaningful in a *pure roguelike* (permadeath, self-contained runs).
Depth comes from interesting in-run gear decisions, not number creep.

This document covers the **foundation milestone** only:

1. **`ItemInstance` migration** — items become instances that carry data.
2. **Armor + defense** — a real defensive stat that doesn't exist today, applied
   through a single damage chokepoint.

Affix tables, accessories, on-hit effects, and identification build on this
foundation and are deferred to follow-up docs.

---

## Why these two, in this order

- The flat `ItemType` enum + `Vec<ItemType>` inventory means two swords are
  byte-identical — an item cannot carry rolled stats. **Every** depth feature
  (affixes, rarity, identification) is blocked on item *instances*. Do it first.
- Defense does not exist anywhere in the codebase. Incoming damage is only ever
  reduced by the `Protected` / `Barkskin` status effects. So armor is the
  highest-impact feature: every armor drop is a genuine upgrade, never a
  duplicate. It also validates the instance system end to end.

---

## Phase 1 — `ItemInstance` migration

### New types (`components.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity { Common, Magic, Rare, Legendary }

/// A rolled modifier on a gear instance. Empty for consumables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Affix {
    Damage(i32),
    Defense(i32),
    Strength(i32),
    Agility(i32),
    Intelligence(i32),
    MaxHealth(i32),
    // Phase 2+: AttackSpeed(f32), OnHit(EffectType, f32), Resist(EffectType) ...
}

/// One concrete item. Consumables are trivial instances (no affixes, identified).
#[derive(Debug, Clone)]
pub struct ItemInstance {
    pub kind: ItemType,
    pub rarity: Rarity,
    pub affixes: Vec<Affix>,
    pub identified: bool,
}

impl ItemInstance {
    /// A plain, known item with no modifiers (consumables, vendor stock, arrows).
    pub fn plain(kind: ItemType) -> Self {
        Self { kind, rarity: Rarity::Common, affixes: Vec::new(), identified: true }
    }
}
```

**Stacking stays display-only.** Arrows are already stored as repeated entries in
the vec (see `starting_inventory` pushing `STARTING_ARROW_COUNT` arrows). Keep
that — each arrow is a `plain(Arrow)` instance, and the UI keeps grouping by
`kind` for the count badge. No `count` field needed.

### Field changes

| Location | Before | After |
|---|---|---|
| `Inventory.items` | `Vec<ItemType>` | `Vec<ItemInstance>` |
| `Container.items` | `Vec<ItemType>` | `Vec<ItemInstance>` |
| `Container::{chest,coffin,barrel,corpse,ground_pile}` ctors | take `Vec<ItemType>` | take `Vec<ItemInstance>` |
| `PlayerClass::starting_inventory` | returns `Vec<ItemType>` | returns `Vec<ItemInstance>` |
| `SavedFloor` item fields (`floor_transition.rs:40,47`) | `Vec<ItemType>` | `Vec<ItemInstance>` |
| `LootWindowState.items` (`ui/loot_window.rs:14`) | `Vec<ItemType>` | `Vec<ItemInstance>` |

### Call-site checklist (14 files touch `.items`, ~65 usages)

Most are mechanical: read `.kind` wherever code currently matched the enum
directly. Grouped by effort:

- **Trivial — display only:** `ui/inventory.rs`, `ui/hotbar.rs`,
  `ui/shop_window.rs`, `ui/status_bar.rs`, `ui/icons.rs`,
  `ui/loot_window.rs` — sprite/name lookups change from `item` to
  `item.kind`; add rarity color + affix lines to tooltips.
- **Core logic:** `systems/inventory.rs` (pickup / add / remove — the heart of
  the migration), `systems/items.rs` (consume: match on `.kind`),
  `engine/initialization.rs` (loot generation — now constructs `ItemInstance`,
  the hook for affix rolling in Phase 3).
- **Equip/drop:** `systems/actions.rs` — the equip path converts an
  `ItemInstance` ↔ `EquippedWeapon`; drop creates a `ground_pile` of instances.
- **Persistence/misc:** `engine/floor_transition.rs`, `engine/simulation.rs`,
  `systems/dev_tools.rs`.

### Decision: how equipped weapons carry affixes

`EquippedWeapon::Melee(Weapon)` / `Ranged(RangedWeapon)` already hold stats
(`base_damage`, `damage_bonus`) — they're *already* instances. The clean,
low-risk path for this milestone:

- Add `affixes: Vec<Affix>` (and `rarity`) to `Weapon` / `RangedWeapon`.
- On **equip**, move the `ItemInstance`'s affixes onto the constructed weapon.
- On **unequip**, reconstruct the `ItemInstance` from the weapon so nothing is
  lost. (Equip/unequip is the only place the two representations meet.)

This avoids reworking the `EquippedWeapon` enum, which is referenced across many
combat sites. (A later refactor can unify slots to store `ItemInstance`
directly, but it is not needed now.)

---

## Phase 2 — Armor & a central damage chokepoint

### Step 2a — Central damage function (prerequisite)

Damage is currently subtracted at **6 sites** with the protection check
duplicated:

- `systems/projectile.rs:150`
- `systems/actions.rs:195` (fire trap), `:342` (melee), `:1031` (fireball),
  `:1314` (cleave), `:1644` (life drain)

Introduce one function and route all six through it:

```rust
// systems/combat.rs
/// Apply `raw` incoming damage to `target`, accounting for invulnerability,
/// Protected/Barkskin, and equipped armor. Returns damage actually dealt.
pub fn apply_damage(world: &mut World, target: Entity, raw: i32) -> i32 {
    if queries::has_status_effect(world, target, EffectType::Invulnerable) {
        return 0;
    }
    let mut dmg = raw;

    // Flat armor reduction (Phase 2b)
    let defense = world.get::<&Equipment>(target).map(|e| e.total_defense()).unwrap_or(0);
    dmg -= defense;

    // Multiplicative damage reduction (existing behaviour)
    if queries::has_status_effect(world, target, EffectType::Protected)
        || queries::has_status_effect(world, target, EffectType::Barkskin) {
        dmg = (dmg as f32 * PROTECTION_DAMAGE_REDUCTION) as i32;
    }

    dmg = dmg.max(1);
    if let Ok(mut h) = world.get::<&mut Health>(target) { h.current -= dmg; }
    dmg
}
```

Each call site drops its bespoke protection/clamp block and calls
`apply_damage(world, target, base_damage_after_crit_and_strength)`. The
attacker-side multipliers (crit, `Strengthened`) stay on the attacker side; only
the defender-side reduction moves into `apply_damage`. **This is a behaviour-
preserving refactor** if done before armor exists (defense = 0), so it can land
and be verified on its own.

### Step 2b — Armor slots & defense

`Equipment` gains slots and a defense accessor:

```rust
pub struct Equipment {
    pub weapon: Option<EquippedWeapon>,
    pub enemy_ranged: Option<RangedWeapon>,
    pub body: Option<ItemInstance>,    // new
    pub head: Option<ItemInstance>,    // new
}

impl Equipment {
    pub fn total_defense(&self) -> i32 {
        [&self.body, &self.head].iter()
            .filter_map(|s| s.as_ref())
            .map(armor_defense)   // base-by-kind + Affix::Defense sum
            .sum()
    }
}
```

- Add armor `ItemType` variants (e.g. `LeatherArmor`, `ChainMail`, `Helmet`) with
  `ItemCategory::Armor`, base defense, weight, sprite, and `base_price` in
  `item_defs.rs`. Sprites exist in `assets/32rogues/items.png`.
- `UseEffect::Equip` already covers armor; the equip handler in `actions.rs`
  routes to the `body`/`head` slot by category instead of the weapon slot.
- **Flat reduction** (`dmg - defense`, floored at 1) is chosen over a % rating:
  it reads clearly to the player and composes cleanly with the existing
  multiplicative `Protected` effect. Keep enemy defense modest so the
  `dmg.max(1)` floor doesn't trivialize fast/weak attackers.
- Inventory UI: add the two equipment slots; show defense in the status bar
  next to health.

---

## Suggested landing order (each independently shippable & verifiable)

1. **`ItemInstance` type + migration**, consumables behaving exactly as before
   (no affixes rolled yet). Verify: pick up, use, drop, shop, floor transition,
   save/restore all still work.
2. **Central `apply_damage` refactor** with defense = 0. Verify: combat numbers
   unchanged.
3. **Armor slots + defense stat.** Verify: equipping leather visibly reduces
   incoming damage; status bar shows defense.

After this milestone, the affix/rarity roll in `generate_chest_contents`,
accessories, on-hit weapon effects, and identification/curses all slot onto the
foundation without further structural change.

---

## Risks / watch-items

- **Migration breadth:** 14 files, ~65 `.items` sites. Mostly mechanical, but
  `systems/inventory.rs` (add/remove) and `systems/actions.rs` (equip/drop) hold
  the real logic — review those carefully.
- **Equip/unequip round-trip:** the one place `ItemInstance` and `Weapon` meet.
  Ensure affixes survive equip→unequip without loss or duplication.
- **Stacking assumptions:** confirm every UI count badge groups by `.kind`, not
  by vec identity, after the migration.
- **Do the `apply_damage` refactor as its own commit** (defense = 0) so it's a
  pure no-op refactor and any combat regression is isolated from the armor work.
