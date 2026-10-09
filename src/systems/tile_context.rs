//! "What is on this tile, and what can I do there?" — the logic behind the
//! hover info panel and the right-click context menu.
//!
//! Both are pure functions of the world, so they can be tested without egui
//! and the menu can be re-validated by the engine when an entry is picked.
//!
//! # Only what the player can know
//!
//! [`describe_tile`] mirrors what the renderer shows (`collect_renderables`):
//!
//! - a **visible** tile (in FOV, or magically revealed) lists creatures and
//!   everything else on it;
//! - an **explored** tile out of sight lists terrain and static features only
//!   (doors, containers, furniture, puddles, webs, revealed traps) — never
//!   creatures, and never transient fire;
//! - an **unexplored** tile describes nothing.
//!
//! Unrevealed dungeon traps (no sprite) and secret doors (drawn as wall) are
//! never mentioned, and neither are creatures that are invisible.
//!
//! # How the menu is built
//!
//! [`context_actions`] lists every action that applies to a tile. Actions that
//! apply but can't be done right now (an ability on cooldown, a door with
//! something in it, a target out of range) are listed **disabled with a
//! reason** rather than hidden, so the menu explains itself. The exception is
//! ground-targeted movement abilities (Tumble, Snare Trap, Blink), which only
//! appear for tiles inside their range; otherwise every floor tile on the map
//! would list them.
//!
//! Each entry carries a [`ContextCommand`] that goes through the same path as
//! the equivalent click or key: a left-click (walk / pursue / approach and
//! bump), or a [`PlayerIntent`] executed by the engine exactly as a keyboard
//! or targeting-click intent is. Ability entries reuse the same validators as
//! targeting clicks (`can_shield_bash`, `can_grave_bolt`, ...), so the menu
//! agrees with what a targeted click would do.

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{
    AbilityType, Actor, Asleep, Attackable, BarrelFuse, BlocksMovement, Brazier, BurningGrass,
    BurningOil, BurningWeb, CausesBurning, ClassAbility, ClassKit, Container, ContainerType,
    Dialogue, Door, DungeonTrap, DungeonTrapKind, EffectType, Equipment, EquippedWeapon,
    FriendlyNPC, Furniture, FurnitureKind, Health, Inventory, ItemType, LearnedAbilities, Name,
    OilBarrel, OilPuddle, PlacedFireTrap, PlacedTrap, Player, Position, RaisedUndead,
    SecondaryAbility, SecretDoor, StatusEffects, TamedBy, Tameable, Vendor, WetGrass, Web,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::systems::actions;
use crate::systems::player_input::{has_ranged_equipped, PlayerIntent};
use crate::tile::TileType;

// =============================================================================
// TILE INFO
// =============================================================================

/// How much the player knows about a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKnowledge {
    /// In view (or magically revealed): everything on it is known.
    Visible,
    /// Seen before but out of sight: terrain and static features only.
    Remembered,
    /// Never seen.
    Unexplored,
}

/// How a creature stands toward the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// The player themself.
    You,
    /// One of the player's tamed or raised companions.
    Companion,
    /// A friendly NPC (talks; trades if a merchant).
    Friendly,
    /// Anything else that can be attacked.
    Hostile,
}

/// A creature the player can see on a tile.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatureInfo {
    pub name: String,
    pub relation: Relation,
    /// (current, max) HP, for creatures that have health.
    pub hp: Option<(i32, i32)>,
    /// Active status effects, in the order they were applied.
    pub statuses: Vec<EffectType>,
    /// Asleep (not a status effect, but worth knowing before you walk up).
    pub asleep: bool,
}

/// Everything the player knows about one tile.
#[derive(Debug, Clone, PartialEq)]
pub struct TileInfo {
    pub tile: (i32, i32),
    pub knowledge: TileKnowledge,
    /// Terrain name ("Floor", "Closed door", "Stairs down"); `None` when
    /// unexplored.
    pub terrain: Option<String>,
    /// Visible creatures (empty unless `knowledge` is `Visible`).
    pub creatures: Vec<CreatureInfo>,
    /// Everything else worth naming: containers, items, furniture, oil,
    /// webs, fire, revealed traps.
    pub features: Vec<String>,
}

impl TileInfo {
    /// One-line description for the message log (the Examine action).
    pub fn summary(&self) -> String {
        let Some(terrain) = &self.terrain else {
            return "You haven't explored there.".to_string();
        };
        let mut parts = vec![if self.knowledge == TileKnowledge::Remembered {
            format!("{terrain} (remembered)")
        } else {
            terrain.clone()
        }];
        for c in &self.creatures {
            let mut bits = vec![relation_word(c.relation).to_string()];
            if let Some((cur, max)) = c.hp {
                bits.push(format!("{cur}/{max} HP"));
            }
            if c.asleep {
                bits.push("asleep".to_string());
            }
            bits.extend(c.statuses.iter().map(|s| effect_name(*s).to_lowercase()));
            parts.push(format!("{} ({})", c.name, bits.join(", ")));
        }
        parts.extend(self.features.iter().cloned());
        format!("{}.", parts.join(". "))
    }

    /// The panel / menu heading: the most interesting thing on the tile.
    pub fn title(&self) -> String {
        if let Some(c) = self.creatures.first() {
            return c.name.clone();
        }
        self.terrain.clone().unwrap_or_else(|| "Unexplored".to_string())
    }
}

/// Lower-case word for a relation, for log lines.
pub fn relation_word(relation: Relation) -> &'static str {
    match relation {
        Relation::You => "you",
        Relation::Companion => "companion",
        Relation::Friendly => "friendly",
        Relation::Hostile => "hostile",
    }
}

/// Full name of a status effect, for the info panel and log.
pub fn effect_name(effect: EffectType) -> &'static str {
    match effect {
        EffectType::Invisible => "Invisible",
        EffectType::SpeedBoost => "Hasted",
        EffectType::Regenerating => "Regenerating",
        EffectType::Strengthened => "Strengthened",
        EffectType::Protected => "Protected",
        EffectType::Barkskin => "Barkskin",
        EffectType::Confused => "Confused",
        EffectType::Feared => "Feared",
        EffectType::Slowed => "Slowed",
        EffectType::Burning => "Burning",
        EffectType::Rooted => "Rooted",
        EffectType::Invulnerable => "Invulnerable",
        EffectType::Stunned => "Stunned",
        EffectType::Guarding => "Guarding",
        EffectType::Thorns => "Thorns",
        EffectType::BoneWard => "Bone Ward",
        EffectType::Wet => "Wet",
        EffectType::Oiled => "Oiled",
        EffectType::Poisoned => "Poisoned",
        EffectType::Bleeding => "Bleeding",
        EffectType::Grabbed => "Grabbed",
    }
}

/// What the player knows about `(x, y)`.
pub fn tile_knowledge(grid: &Grid, (x, y): (i32, i32)) -> TileKnowledge {
    match grid.get(x, y) {
        Some(t) if t.visible => TileKnowledge::Visible,
        Some(t) if t.explored => TileKnowledge::Remembered,
        _ => TileKnowledge::Unexplored,
    }
}

fn has<C: hecs::Component>(world: &World, e: Entity) -> bool {
    world.satisfies::<&C>(e).unwrap_or(false)
}

/// Entities standing on `tile`.
fn entities_at(world: &World, tile: (i32, i32)) -> Vec<Entity> {
    world
        .query::<&Position>()
        .iter()
        .filter(|(_, p)| (p.x, p.y) == tile)
        .map(|(e, _)| e)
        .collect()
}

/// Whether `e` is a creature (something that acts or talks, with a body).
fn is_creature(world: &World, e: Entity) -> bool {
    has::<Player>(world, e) || has::<Actor>(world, e) || has::<FriendlyNPC>(world, e)
}

/// A creature the player can actually see on a visible tile: alive and not
/// magically invisible (the player always sees themself).
fn is_seen_creature(world: &World, player: Entity, e: Entity) -> bool {
    if !is_creature(world, e) {
        return false;
    }
    if e == player {
        return true;
    }
    let alive = world.get::<&Health>(e).map(|h| !h.is_dead()).unwrap_or(true);
    alive && !crate::systems::effects::entity_has_effect(world, e, EffectType::Invisible)
}

fn relation_of(world: &World, player: Entity, e: Entity) -> Relation {
    if e == player {
        Relation::You
    } else if world.get::<&TamedBy>(e).map(|t| t.owner == player).unwrap_or(false) {
        Relation::Companion
    } else if has::<FriendlyNPC>(world, e) {
        Relation::Friendly
    } else {
        Relation::Hostile
    }
}

fn creature_name(world: &World, player: Entity, e: Entity) -> String {
    if e == player {
        return "You".to_string();
    }
    if let Ok(n) = world.get::<&Name>(e) {
        return n.0.clone();
    }
    if let Ok(d) = world.get::<&Dialogue>(e) {
        return d.name.clone();
    }
    "Creature".to_string()
}

fn creature_info(world: &World, player: Entity, e: Entity) -> CreatureInfo {
    CreatureInfo {
        name: creature_name(world, player, e),
        relation: relation_of(world, player, e),
        hp: world.get::<&Health>(e).ok().map(|h| (h.current.max(0), h.max)),
        statuses: world
            .get::<&StatusEffects>(e)
            .map(|s| {
                let mut v: Vec<EffectType> = Vec::new();
                for eff in &s.effects {
                    if eff.effect_type != EffectType::Invisible && !v.contains(&eff.effect_type) {
                        v.push(eff.effect_type);
                    }
                }
                v
            })
            .unwrap_or_default(),
        asleep: has::<Asleep>(world, e),
    }
}

fn terrain_name(tile_type: TileType) -> &'static str {
    match tile_type {
        TileType::Empty => "Solid rock",
        TileType::Floor => "Floor",
        TileType::Wall => "Wall",
        TileType::Water => "Water",
        TileType::Grass => "Grass",
        TileType::TallGrass => "Tall grass",
        TileType::Stone => "Stone",
        TileType::StairsDown => "Stairs down",
        TileType::StairsUp => "Stairs up",
    }
}

/// Up to `ITEM_PILE_NAMES_SHOWN` item names (stacks counted), then "+N more".
fn item_list(container: &Container) -> String {
    let mut names: Vec<(String, u32)> = Vec::new();
    for item in &container.items {
        let name = item.display_name();
        match names.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => names.push((name, 1)),
        }
    }
    let mut shown: Vec<String> = names
        .iter()
        .take(TILE_INFO_ITEM_NAMES_SHOWN)
        .map(|(n, c)| if *c > 1 { format!("{n} x{c}") } else { n.clone() })
        .collect();
    if container.gold > 0 {
        shown.push(format!("{} gold", container.gold));
    }
    let hidden = names.len().saturating_sub(TILE_INFO_ITEM_NAMES_SHOWN);
    if hidden > 0 {
        shown.push(format!("+{hidden} more"));
    }
    shown.join(", ")
}

fn container_feature(world: &World, e: Entity, c: &Container) -> Option<String> {
    let state = |closed: &str| -> String {
        if !c.is_open {
            closed.to_string()
        } else if c.is_empty() {
            "empty".to_string()
        } else {
            "open".to_string()
        }
    };
    Some(match c.container_type {
        ContainerType::Chest => format!("Chest ({})", state("closed")),
        ContainerType::Coffin => format!("Coffin ({})", state("sealed")),
        ContainerType::Barrel => format!("Barrel ({})", state("unopened")),
        ContainerType::Corpse => {
            let whose = world
                .get::<&Name>(e)
                .map(|n| format!("{} corpse", n.0))
                .unwrap_or_else(|_| "Corpse".to_string());
            if c.is_looted() {
                format!("{whose} (searched)")
            } else {
                whose
            }
        }
        ContainerType::GroundPile => {
            if c.is_empty() {
                return None;
            }
            format!("Items: {}", item_list(c))
        }
    })
}

fn trap_kind_name(kind: DungeonTrapKind) -> &'static str {
    match kind {
        DungeonTrapKind::Spike => "Spike trap",
        DungeonTrapKind::Fire => "Fire trap",
        DungeonTrapKind::Snare => "Snare trap",
        DungeonTrapKind::Alarm => "Alarm trap",
    }
}

/// Non-creature features of `e`, as the player sees them. `visible` gates the
/// transient ones (fire, a lit fuse) that a remembered tile does not show.
fn features_of(world: &World, player: Entity, e: Entity, visible: bool, out: &mut Vec<String>) {
    if let Ok(c) = world.get::<&Container>(e) {
        if let Some(f) = container_feature(world, e, &c) {
            out.push(f);
        }
        return;
    }
    if has::<OilBarrel>(world, e) {
        let mut s = "Oil barrel".to_string();
        if visible {
            if let Ok(h) = world.get::<&Health>(e) {
                s.push_str(&format!(" ({}/{} HP)", h.current.max(0), h.max));
            }
            if has::<BarrelFuse>(world, e) {
                s.push_str(", fuse lit!");
            }
        }
        out.push(s);
        return;
    }
    if let Ok(f) = world.get::<&Furniture>(e) {
        let name = match f.kind {
            FurnitureKind::Fountain => "Fountain",
            FurnitureKind::Altar => "Altar",
            FurnitureKind::Shrine => "Shrine",
        };
        let one_use = !matches!(f.kind, FurnitureKind::Altar);
        out.push(if one_use && f.used { format!("{name} (used)") } else { name.to_string() });
        return;
    }
    if let Ok(b) = world.get::<&Brazier>(e) {
        out.push(if b.lit { "Brazier (lit)" } else { "Toppled brazier" }.to_string());
        return;
    }
    if has::<OilPuddle>(world, e) {
        out.push(
            if visible && has::<BurningOil>(world, e) { "Burning oil" } else { "Oil puddle" }
                .to_string(),
        );
        return;
    }
    if has::<Web>(world, e) {
        out.push(
            if visible && has::<BurningWeb>(world, e) { "Burning web" } else { "Web" }.to_string(),
        );
        return;
    }
    if let Ok(t) = world.get::<&DungeonTrap>(e) {
        // A hidden trap has no sprite: never mention it.
        if t.revealed {
            out.push(trap_kind_name(t.kind).to_string());
        }
        return;
    }
    if let Ok(t) = world.get::<&PlacedFireTrap>(e) {
        out.push(if t.owner == player { "Fire trap (yours)" } else { "Fire trap" }.to_string());
        return;
    }
    if let Ok(t) = world.get::<&PlacedTrap>(e) {
        let yours = if t.owner == player { " (yours)" } else { "" };
        out.push(format!("Snare trap{yours}"));
        return;
    }
    if has::<BurningGrass>(world, e) {
        if visible {
            out.push("Burning grass".to_string());
        }
        return;
    }
    if has::<CausesBurning>(world, e) {
        // Campfires (a fire pit that stays put): remembered like furniture.
        out.push("Campfire".to_string());
        return;
    }
    if has::<SecretDoor>(world, e) || has::<Door>(world, e) {
        return; // Reported as terrain.
    }
    // Named scenery: stalagmites, glowing mushrooms, crystal clusters.
    if let Ok(n) = world.get::<&Name>(e) {
        out.push(n.0.clone());
    }
}

/// Describe `tile` as far as `player` can know it (see the module docs).
pub fn describe_tile(world: &World, grid: &Grid, player: Entity, tile: (i32, i32)) -> TileInfo {
    let knowledge = tile_knowledge(grid, tile);
    let mut info = TileInfo {
        tile,
        knowledge,
        terrain: None,
        creatures: Vec::new(),
        features: Vec::new(),
    };
    let Some(grid_tile) = grid.get(tile.0, tile.1) else {
        return info;
    };
    if knowledge == TileKnowledge::Unexplored {
        return info;
    }
    let visible = knowledge == TileKnowledge::Visible;
    let here = entities_at(world, tile);

    let mut terrain = terrain_name(grid_tile.tile_type).to_string();
    if here.iter().any(|&e| has::<SecretDoor>(world, e)) {
        terrain = "Wall".to_string();
    } else if let Some(door) = here.iter().find_map(|&e| world.get::<&Door>(e).ok().map(|d| d.is_open)) {
        terrain = if door { "Open door" } else { "Closed door" }.to_string();
    } else if grid_tile.tile_type == TileType::Grass && here.iter().any(|&e| has::<WetGrass>(world, e))
    {
        terrain = "Grass (soaked)".to_string();
    }
    info.terrain = Some(terrain);

    for &e in &here {
        if is_creature(world, e) {
            if visible && is_seen_creature(world, player, e) {
                info.creatures.push(creature_info(world, player, e));
            }
            continue;
        }
        features_of(world, player, e, visible, &mut info.features);
    }
    // The player first in their own tile's list; then hostiles before others.
    info.creatures.sort_by_key(|c| match c.relation {
        Relation::Hostile => 0,
        Relation::Friendly => 1,
        Relation::Companion => 2,
        Relation::You => 3,
    });
    info
}

// =============================================================================
// CONTEXT ACTIONS
// =============================================================================

/// What picking a context-menu entry does. Each routes through an existing
/// input path rather than doing anything itself.
#[derive(Debug, Clone, PartialEq)]
pub enum ContextCommand {
    /// Exactly what a left-click on the tile does: walk there, pursue the
    /// creature on it, or walk up to a door/chest and bump it open.
    ClickTo { x: i32, y: i32 },
    /// Walk to a tile next to `(x, y)`, then bump into it (talking to an NPC
    /// that is not adjacent).
    Approach { x: i32, y: i32 },
    /// Execute this intent, as a key press or targeting click would.
    Intent(PlayerIntent),
    /// Write the tile's description to the message log.
    Examine { x: i32, y: i32 },
}

/// One entry in the right-click menu.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextAction {
    pub label: String,
    pub command: ContextCommand,
    /// False when the action applies to this tile but can't be done now.
    pub enabled: bool,
    /// Why it is disabled (shown as a tooltip).
    pub reason: Option<String>,
}

impl ContextAction {
    fn new(label: impl Into<String>, command: ContextCommand) -> Self {
        Self { label: label.into(), command, enabled: true, reason: None }
    }

    /// Disable with `reason` if `check` failed. The first failing check wins.
    fn gate(mut self, check: Result<(), String>) -> Self {
        if self.enabled {
            if let Err(reason) = check {
                self.enabled = false;
                self.reason = Some(reason);
            }
        }
        self
    }
}

fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn in_range(dist: i32, range: i32) -> Result<(), String> {
    if dist <= range {
        Ok(())
    } else {
        Err(format!("Out of range ({range} tiles)"))
    }
}

fn cooldown_reason(remaining: f32) -> Result<(), String> {
    if remaining <= 0.0 {
        Ok(())
    } else {
        Err(format!("On cooldown ({:.0}s)", remaining.ceil()))
    }
}

/// Whether the player has `ability` in any slot, and if so whether it is off
/// cooldown (`Err` names the wait). `None` when the player lacks it.
fn ability_ready(world: &World, player: Entity, ability: AbilityType) -> Option<Result<(), String>> {
    if let Ok(a) = world.get::<&ClassAbility>(player) {
        if a.ability_type == ability {
            return Some(cooldown_reason(a.cooldown_remaining));
        }
    }
    if let Ok(a) = world.get::<&SecondaryAbility>(player) {
        if a.ability_type == ability {
            return Some(cooldown_reason(a.cooldown_remaining));
        }
    }
    if let Ok(kit) = world.get::<&ClassKit>(player) {
        if let Some(k) = kit.position(ability).and_then(|i| kit.get(i)) {
            return Some(cooldown_reason(k.cooldown_remaining));
        }
    }
    if let Ok(la) = world.get::<&LearnedAbilities>(player) {
        if let Some(s) = la.get(ability) {
            return Some(cooldown_reason(s.cooldown_remaining));
        }
    }
    None
}

/// Line of sight from `from` to `tile` within `range`, with vision-blocking
/// entities (closed doors) in the way: the check targeted Crippling Shot uses.
fn line_of_sight(world: &World, grid: &Grid, from: (i32, i32), tile: (i32, i32), range: i32) -> bool {
    let blockers: HashSet<(i32, i32)> = world
        .query::<(&Position, &crate::components::BlocksVision)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    crate::fov::Fov::calculate(
        grid,
        from.0,
        from.1,
        range,
        Some(|x: i32, y: i32| blockers.contains(&(x, y))),
    )
    .contains(&tile)
}

/// What the player calls their melee attack: "Attack (Sword)", or fists when
/// no melee weapon is in hand (a bow user swings unarmed).
fn attack_label(world: &World, player: Entity) -> String {
    let weapon = world
        .get::<&Equipment>(player)
        .ok()
        .and_then(|e| match &e.weapon {
            Some(EquippedWeapon::Melee(w)) => Some(
                e.weapon_source.as_ref().map(|s| s.display_name()).unwrap_or_else(|| w.name.clone()),
            ),
            _ => None,
        })
        .unwrap_or_else(|| "fists".to_string());
    format!("Attack ({weapon})")
}

fn bow_name(world: &World, player: Entity) -> String {
    world
        .get::<&Equipment>(player)
        .ok()
        .and_then(|e| match &e.weapon {
            Some(EquippedWeapon::Ranged(b)) => Some(
                e.weapon_source.as_ref().map(|s| s.display_name()).unwrap_or_else(|| b.name.clone()),
            ),
            _ => None,
        })
        .unwrap_or_else(|| "Bow".to_string())
}

fn has_ammo(world: &World, player: Entity) -> bool {
    world
        .get::<&Inventory>(player)
        .map(|inv| inv.items.iter().any(|i| matches!(i.kind, ItemType::Arrow | ItemType::FireArrow)))
        .unwrap_or(false)
}

/// Movement blockers the player knows about: everything that blocks, except
/// creatures on tiles they cannot see (so reachability doesn't give away an
/// unseen monster in a corridor).
fn known_blockers(world: &World, grid: &Grid, player: Entity) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .filter(|(e, (p, _))| {
            *e != player
                && (!is_creature(world, *e)
                    || tile_knowledge(grid, (p.x, p.y)) == TileKnowledge::Visible)
        })
        .map(|(_, (p, _))| (p.x, p.y))
        .collect()
}

fn reachable(
    world: &World,
    grid: &Grid,
    player: Entity,
    from: (i32, i32),
    to: (i32, i32),
) -> Result<(), String> {
    let blocked = known_blockers(world, grid, player);
    if crate::pathfinding::find_path(grid, from, to, &blocked).is_some() {
        Ok(())
    } else {
        Err("No path".to_string())
    }
}

/// Everything the player could do at `tile`, in menu order. See the module
/// docs for which entries are hidden vs disabled.
pub fn context_actions(
    world: &World,
    grid: &Grid,
    spatial: &SpatialCache,
    player: Entity,
    tile: (i32, i32),
) -> Vec<ContextAction> {
    let mut out = Vec::new();
    let examine = ContextAction::new("Examine", ContextCommand::Examine { x: tile.0, y: tile.1 });
    let Some(from) = crate::queries::get_entity_position(world, player) else {
        return vec![examine];
    };
    let knowledge = tile_knowledge(grid, tile);
    let Some(grid_tile) = grid.get(tile.0, tile.1) else {
        return vec![examine];
    };
    if knowledge == TileKnowledge::Unexplored || tile == from {
        return vec![examine];
    }
    let visible = knowledge == TileKnowledge::Visible;
    let dist = chebyshev(from, tile);
    let (dx, dy) = (tile.0 - from.0, tile.1 - from.1);
    let (x, y) = tile;
    let idle = crate::queries::can_entity_act(world, player);
    let busy: Result<(), String> = if idle { Ok(()) } else { Err("You are busy".to_string()) };
    let bump = ContextCommand::Intent(PlayerIntent::Move { dx, dy });
    let click = ContextCommand::ClickTo { x, y };

    let here = entities_at(world, tile);
    let seen: Vec<Entity> =
        if visible { here.iter().copied().filter(|&e| is_seen_creature(world, player, e)).collect() } else { Vec::new() };
    let hostile = seen
        .iter()
        .copied()
        .find(|&e| relation_of(world, player, e) == Relation::Hostile && has::<Attackable>(world, e));
    let npc = seen.iter().copied().find(|&e| has::<FriendlyNPC>(world, e));
    let companion = seen.iter().copied().find(|&e| relation_of(world, player, e) == Relation::Companion);
    let oil_barrel = if visible { here.iter().copied().find(|&e| has::<OilBarrel>(world, e)) } else { None };
    let pushable = if visible { crate::systems::push::pushable_at(world, x, y) } else { None };
    let corpse = if visible {
        here.iter().copied().find(|&e| {
            world.get::<&Container>(e).map(|c| c.container_type == ContainerType::Corpse).unwrap_or(false)
        })
    } else {
        None
    };
    // Something the attack/shoot/bolt entries can aim at.
    let attack_target = hostile.or(oil_barrel);

    // --- Attacks ----------------------------------------------------------
    if attack_target.is_some() {
        let command = if dist == 1 { bump.clone() } else { click.clone() };
        let gate = if dist == 1 { busy.clone() } else { Ok(()) };
        out.push(ContextAction::new(attack_label(world, player), command).gate(gate));
    }
    if visible && has_ranged_equipped(world, player) && attack_target.is_some() {
        let entry = ContextAction::new(
            format!("Shoot ({})", bow_name(world, player)),
            ContextCommand::Intent(PlayerIntent::ShootRanged { target_x: x, target_y: y }),
        );
        out.push(
            entry
                .gate(busy.clone())
                .gate(if has_ammo(world, player) { Ok(()) } else { Err("No arrows".to_string()) })
                .gate(in_range(dist, BOW_RANGE)),
        );
    }

    // --- Talking ----------------------------------------------------------
    if let Some(npc) = npc {
        let label = if has::<Vendor>(world, npc) { "Trade" } else { "Speak" };
        let command = if dist == 1 { bump.clone() } else { ContextCommand::Approach { x, y } };
        let gate = if dist == 1 { busy.clone() } else { Ok(()) };
        out.push(ContextAction::new(label, command).gate(gate));
    }

    // --- Doors ------------------------------------------------------------
    if let Some(door) = actions::door_at(world, x, y) {
        let open = world.get::<&Door>(door).map(|d| d.is_open).unwrap_or(false);
        if open {
            let check = if dist > 1 {
                Err("Too far away".to_string())
            } else if actions::can_close_door(world, player, door) {
                Ok(())
            } else {
                Err("Something is in the doorway".to_string())
            };
            out.push(
                ContextAction::new("Close door", ContextCommand::Intent(PlayerIntent::CloseDoor { door }))
                    .gate(busy.clone())
                    .gate(check),
            );
        } else {
            let entry = if dist == 1 {
                ContextAction::new("Open door", bump.clone()).gate(busy.clone())
            } else {
                ContextAction::new("Open door", click.clone())
            };
            out.push(entry);
        }
    }

    // --- Containers -------------------------------------------------------
    let mut offers_loot = false;
    for &e in &here {
        let Ok(c) = world.get::<&Container>(e) else { continue };
        let blocks = has::<BlocksMovement>(world, e);
        let label = match c.container_type {
            ContainerType::Chest | ContainerType::Coffin | ContainerType::Barrel => {
                if !blocks || c.is_looted() || (c.is_open && c.is_empty()) {
                    continue;
                }
                let noun = match c.container_type {
                    ContainerType::Chest => "chest",
                    ContainerType::Coffin => "coffin",
                    _ => "barrel",
                };
                if c.is_open { format!("Loot {noun}") } else { format!("Open {noun}") }
            }
            ContainerType::Corpse => {
                if c.is_looted() {
                    continue;
                }
                "Search corpse".to_string()
            }
            ContainerType::GroundPile => {
                if c.is_empty() {
                    continue;
                }
                "Pick up items".to_string()
            }
        };
        offers_loot = true;
        let entry = if blocks && dist == 1 {
            ContextAction::new(label, bump.clone()).gate(busy.clone())
        } else {
            ContextAction::new(label, click.clone())
                .gate(reachable(world, grid, player, from, tile))
        };
        out.push(entry);
        if blocks {
            break; // One blocking container per tile.
        }
    }

    // --- Push -------------------------------------------------------------
    if pushable.is_some() {
        let check = if dist > 1 {
            Err("Too far away".to_string())
        } else if crate::systems::push::can_push(world, grid, spatial, player, tile) {
            Ok(())
        } else {
            Err("It won't budge that way".to_string())
        };
        out.push(
            ContextAction::new("Push", ContextCommand::Intent(PlayerIntent::Push { dx, dy }))
                .gate(busy.clone())
                .gate(check),
        );
    }

    // --- Stairs -----------------------------------------------------------
    let stairs = matches!(grid_tile.tile_type, TileType::StairsDown | TileType::StairsUp);
    let blocked_by_known = here.iter().any(|&e| {
        has::<BlocksMovement>(world, e) && (!is_creature(world, e) || seen.contains(&e))
    });
    if stairs && !blocked_by_known {
        let label = if grid_tile.tile_type == TileType::StairsDown { "Descend stairs" } else { "Ascend stairs" };
        let entry = if dist == 1 {
            ContextAction::new(label, bump.clone()).gate(busy.clone())
        } else {
            ContextAction::new(label, click.clone()).gate(reachable(world, grid, player, from, tile))
        };
        out.push(entry);
    }

    // --- Walk -------------------------------------------------------------
    if grid_tile.tile_type.is_walkable() && !stairs && !blocked_by_known && seen.is_empty() && !offers_loot {
        out.push(ContextAction::new("Walk here", click.clone()).gate(reachable(world, grid, player, from, tile)));
    }

    // --- Abilities --------------------------------------------------------
    let ability = |ability: AbilityType, intent: PlayerIntent, check: Result<(), String>| {
        ability_ready(world, player, ability).map(|ready| {
            ContextAction::new(ability.name(), ContextCommand::Intent(intent))
                .gate(busy.clone())
                .gate(ready)
                .gate(check)
        })
    };
    let open_ground = grid_tile.tile_type.is_walkable() && !blocked_by_known && seen.is_empty();
    let mut abilities: Vec<Option<ContextAction>> = Vec::new();

    if hostile.is_some() || pushable.is_some() {
        let check = if dist > SHIELD_BASH_RANGE {
            Err("Too far away".to_string())
        } else if actions::can_shield_bash(world, player, tile) {
            Ok(())
        } else {
            Err("Nothing to bash".to_string())
        };
        abilities.push(ability(
            AbilityType::ShieldBash,
            PlayerIntent::ShieldBash { target_x: x, target_y: y },
            check,
        ));
    }
    if let Some(target) = attack_target {
        let check = if dist > GRAVE_BOLT_RANGE {
            in_range(dist, GRAVE_BOLT_RANGE)
        } else if actions::can_grave_bolt(world, grid, player, tile) {
            Ok(())
        } else {
            Err("No line of sight".to_string())
        };
        abilities.push(ability(
            AbilityType::GraveBolt,
            PlayerIntent::GraveBolt { target_x: x, target_y: y },
            check,
        ));
        let check = if dist > BOW_RANGE {
            in_range(dist, BOW_RANGE)
        } else if line_of_sight(world, grid, from, tile, BOW_RANGE) {
            Ok(())
        } else {
            Err("No line of sight".to_string())
        };
        abilities.push(ability(
            AbilityType::CripplingShot,
            PlayerIntent::ShootCripplingShot { target_x: x, target_y: y },
            check,
        ));
        if hostile.is_some() {
            abilities.push(ability(
                AbilityType::Entangle,
                PlayerIntent::Entangle { target_x: x, target_y: y },
                in_range(dist, ENTANGLE_RANGE),
            ));
            abilities.push(ability(
                AbilityType::LearnedFireball,
                PlayerIntent::CastLearnedSpell { ability: AbilityType::LearnedFireball, target_x: x, target_y: y },
                in_range(dist, FIREBALL_RANGE),
            ));
            let drainable = world.get::<&Health>(target).is_ok() && !has::<TamedBy>(world, target);
            if drainable {
                abilities.push(ability(
                    AbilityType::LifeDrain,
                    PlayerIntent::StartLifeDrain { target },
                    in_range(dist, LIFE_DRAIN_RANGE),
                ));
            }
        }
    }
    let on_fire = visible
        && here.iter().any(|&e| {
            has::<CausesBurning>(world, e)
                || crate::systems::effects::entity_has_effect(world, e, EffectType::Burning)
        });
    if !seen.is_empty() || on_fire {
        abilities.push(ability(
            AbilityType::CallRain,
            PlayerIntent::CallRain { target_x: x, target_y: y },
            in_range(dist, CALL_RAIN_RANGE),
        ));
    }
    if let Some(target) = seen.iter().copied().find(|&e| has::<Tameable>(world, e) && !has::<TamedBy>(world, e)) {
        abilities.push(ability(AbilityType::Tame, PlayerIntent::StartTaming { target }, in_range(dist, TAME_RANGE)));
    }
    if let Some(corpse) = corpse {
        abilities.push(ability(
            AbilityType::CorpseExplosion,
            PlayerIntent::CorpseExplosion { corpse },
            in_range(dist, CORPSE_EXPLOSION_RANGE),
        ));
        let int = crate::queries::effective_stats(world, player).intelligence;
        let cap = raise_dead_cap(int);
        let check = in_range(dist, RAISE_DEAD_RANGE).and_then(|_| {
            if actions::raised_undead_count(world) >= cap {
                Err(format!("You can control only {cap} skeleton{}", if cap == 1 { "" } else { "s" }))
            } else {
                Ok(())
            }
        });
        abilities.push(ability(AbilityType::RaiseDead, PlayerIntent::StartRaiseDead { target: corpse }, check));
    }
    if let Some(skeleton) = companion.filter(|&e| has::<RaisedUndead>(world, e)) {
        let check = if actions::is_valid_sacrifice_target(world, player, skeleton) {
            Ok(())
        } else {
            in_range(dist, SACRIFICE_RANGE)
        };
        abilities.push(ability(AbilityType::Sacrifice, PlayerIntent::Sacrifice { skeleton }, check));
    }
    if open_ground {
        if (1..=TUMBLE_DISTANCE).contains(&dist) {
            abilities.push(ability(
                AbilityType::Tumble,
                PlayerIntent::Tumble { target_x: x, target_y: y },
                Ok(()),
            ));
        }
        if (1..=SNARE_TRAP_RANGE).contains(&dist) {
            abilities.push(ability(
                AbilityType::SnareTrap,
                PlayerIntent::PlaceSnareTrap { target_x: x, target_y: y },
                Ok(()),
            ));
        }
        if dist <= actions::scaled_blink_range(world, player) {
            abilities.push(ability(
                AbilityType::LearnedBlink,
                PlayerIntent::CastLearnedSpell { ability: AbilityType::LearnedBlink, target_x: x, target_y: y },
                Ok(()),
            ));
        }
    }
    out.extend(abilities.into_iter().flatten());

    out.push(examine);
    out
}

/// The entry in `context_actions(tile)` that matches `command`, if it is
/// there and enabled right now. The engine uses this to re-validate a menu
/// choice at the moment it is executed (the world may have moved on since
/// the menu was drawn).
pub fn validate_choice(
    world: &World,
    grid: &Grid,
    spatial: &SpatialCache,
    player: Entity,
    tile: (i32, i32),
    command: &ContextCommand,
) -> Result<(), String> {
    match context_actions(world, grid, spatial, player, tile)
        .into_iter()
        .find(|a| &a.command == command)
    {
        Some(a) if a.enabled => Ok(()),
        Some(a) => Err(a.reason.unwrap_or_else(|| "You can't do that now.".to_string())),
        None => Err("You can't do that now.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BlocksVision, Sprite, VisualPosition};
    use crate::systems::actions::TestArena as Arena;
    use crate::tile::{tile_ids, Tile};

    /// Make every tile visible and explored (the arena has no FOV pass).
    fn see_all(arena: &mut Arena) {
        for t in arena.grid.tiles.iter_mut() {
            t.visible = true;
            t.explored = true;
        }
    }

    fn set_tile(arena: &mut Arena, (x, y): (i32, i32), f: impl FnOnce(&mut Tile)) {
        f(arena.grid.get_mut(x, y).expect("in bounds"));
    }

    fn actions_at(arena: &Arena, tile: (i32, i32)) -> Vec<ContextAction> {
        context_actions(&arena.world, &arena.grid, &arena.cache, arena.player, tile)
    }

    fn labels(actions: &[ContextAction]) -> Vec<String> {
        actions.iter().map(|a| a.label.clone()).collect()
    }

    fn find<'a>(actions: &'a [ContextAction], label: &str) -> &'a ContextAction {
        actions
            .iter()
            .find(|a| a.label == label)
            .unwrap_or_else(|| panic!("no {label:?} in {:?}", labels(actions)))
    }

    fn give_kit(arena: &mut Arena, class: crate::components::PlayerClass) {
        let _ = arena.world.insert_one(arena.player, ClassKit::for_class(class));
    }

    fn door(arena: &mut Arena, (x, y): (i32, i32), open: bool) -> Entity {
        let pos = Position::new(x, y);
        let mut d = Door::new();
        d.is_open = open;
        let e = if open {
            arena.world.spawn((pos, VisualPosition::from_position(&pos), d))
        } else {
            arena.world.spawn((pos, VisualPosition::from_position(&pos), d, BlocksMovement, BlocksVision))
        };
        arena.cache.rebuild_in_place(&arena.world);
        e
    }

    // --- Info ---------------------------------------------------------------

    #[test]
    fn a_visible_tile_lists_the_creature_with_hp() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        arena.rat(7, 5, 1.0);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (7, 5));
        assert_eq!(info.knowledge, TileKnowledge::Visible);
        assert_eq!(info.terrain.as_deref(), Some("Floor"));
        assert_eq!(info.creatures.len(), 1);
        let rat = &info.creatures[0];
        assert_eq!(rat.name, "Rat");
        assert_eq!(rat.relation, Relation::Hostile);
        assert!(rat.hp.is_some());
        assert!(info.summary().contains("Rat (hostile"), "{}", info.summary());
    }

    #[test]
    fn statuses_show_on_visible_creatures() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        let rat = arena.rat(7, 5, 1.0);
        crate::systems::effects::add_effect_to_entity(&mut arena.world, rat, EffectType::Oiled, 5.0);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (7, 5));
        assert_eq!(info.creatures[0].statuses, vec![EffectType::Oiled]);
        assert!(info.summary().contains("oiled"), "{}", info.summary());
    }

    #[test]
    fn a_remembered_tile_shows_terrain_and_features_but_no_creatures() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        arena.rat(7, 5, 1.0);
        crate::spawning::spawn_oil_puddle(&mut arena.world, 7, 5);
        set_tile(&mut arena, (7, 5), |t| t.visible = false);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (7, 5));
        assert_eq!(info.knowledge, TileKnowledge::Remembered);
        assert_eq!(info.terrain.as_deref(), Some("Floor"));
        assert!(info.creatures.is_empty(), "no creature leaks into the fog");
        assert_eq!(info.features, vec!["Oil puddle".to_string()]);
        assert!(!info.summary().contains("Rat"));
        // And the menu offers nothing that targets the unseen rat.
        let acts = actions_at(&arena, (7, 5));
        assert!(!labels(&acts).iter().any(|l| l.starts_with("Attack")), "{:?}", labels(&acts));
    }

    #[test]
    fn an_unexplored_tile_describes_nothing() {
        let mut arena = Arena::new((5, 5));
        arena.rat(7, 5, 1.0);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (7, 5));
        assert_eq!(info.knowledge, TileKnowledge::Unexplored);
        assert_eq!(info.terrain, None);
        assert!(info.creatures.is_empty() && info.features.is_empty());
        assert_eq!(info.title(), "Unexplored");
        assert_eq!(labels(&actions_at(&arena, (7, 5))), vec!["Examine"]);
    }

    #[test]
    fn an_unrevealed_trap_is_not_listed_but_a_revealed_one_is() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        let trap = crate::spawning::spawn_dungeon_trap(&mut arena.world, 6, 6, DungeonTrapKind::Spike);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (6, 6));
        assert!(info.features.is_empty(), "{:?}", info.features);
        assert_eq!(info.summary(), "Floor.");

        arena.world.get::<&mut DungeonTrap>(trap).unwrap().revealed = true;
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (6, 6));
        assert_eq!(info.features, vec!["Spike trap".to_string()]);
    }

    #[test]
    fn a_secret_door_reads_as_wall() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        crate::spawning::spawn_secret_door(&mut arena.world, 6, 5, tile_ids::WALL);
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (6, 5));
        assert_eq!(info.terrain.as_deref(), Some("Wall"));
        assert!(info.features.is_empty());
    }

    // --- Actions ------------------------------------------------------------

    #[test]
    fn an_adjacent_enemy_offers_attack_with_the_weapon_name_and_shield_bash_for_a_fighter() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        give_kit(&mut arena, crate::components::PlayerClass::Fighter);
        arena.rat(6, 5, 1.0);
        let acts = actions_at(&arena, (6, 5));
        let attack = find(&acts, "Attack (Claws)");
        assert!(attack.enabled);
        assert_eq!(attack.command, ContextCommand::Intent(PlayerIntent::Move { dx: 1, dy: 0 }));
        let bash = find(&acts, "Shield Bash");
        assert!(bash.enabled, "{:?}", bash.reason);
        assert_eq!(
            bash.command,
            ContextCommand::Intent(PlayerIntent::ShieldBash { target_x: 6, target_y: 5 })
        );
        assert!(!labels(&acts).contains(&"Walk here".to_string()));
    }

    #[test]
    fn a_ranger_gets_no_shield_bash() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        give_kit(&mut arena, crate::components::PlayerClass::Ranger);
        arena.rat(6, 5, 1.0);
        let acts = actions_at(&arena, (6, 5));
        assert!(!labels(&acts).contains(&"Shield Bash".to_string()), "{:?}", labels(&acts));
        assert!(labels(&acts).contains(&"Crippling Shot".to_string()));
    }

    #[test]
    fn an_adjacent_barrel_offers_attack_and_push_and_push_greys_out_against_a_wall() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        crate::spawning::spawn_oil_barrel(&mut arena.world, 6, 5);
        arena.cache.rebuild_in_place(&arena.world);
        let acts = actions_at(&arena, (6, 5));
        assert!(find(&acts, "Attack (Claws)").enabled);
        let push = find(&acts, "Push");
        assert!(push.enabled);
        assert_eq!(push.command, ContextCommand::Intent(PlayerIntent::Push { dx: 1, dy: 0 }));

        set_tile(&mut arena, (7, 5), |t| *t = Tile { visible: true, explored: true, ..Tile::new(TileType::Wall) });
        let acts = actions_at(&arena, (6, 5));
        let push = find(&acts, "Push");
        assert!(!push.enabled);
        assert!(push.reason.is_some());
    }

    #[test]
    fn an_open_door_offers_close_unless_something_stands_in_it() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        let d = door(&mut arena, (6, 5), true);
        let acts = actions_at(&arena, (6, 5));
        let close = find(&acts, "Close door");
        assert!(close.enabled, "{:?}", close.reason);
        assert_eq!(close.command, ContextCommand::Intent(PlayerIntent::CloseDoor { door: d }));

        arena.rat(6, 5, 1.0);
        let acts = actions_at(&arena, (6, 5));
        let close = find(&acts, "Close door");
        assert!(!close.enabled);
        assert_eq!(close.reason.as_deref(), Some("Something is in the doorway"));
    }

    #[test]
    fn a_closed_door_offers_open() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        door(&mut arena, (6, 5), false);
        let acts = actions_at(&arena, (6, 5));
        let open = find(&acts, "Open door");
        assert!(open.enabled);
        assert_eq!(open.command, ContextCommand::Intent(PlayerIntent::Move { dx: 1, dy: 0 }));
        assert!(!labels(&acts).contains(&"Close door".to_string()));

        // From afar it walks up and bumps it, like a left-click.
        let acts = actions_at(&arena, (9, 5));
        assert!(!labels(&acts).contains(&"Open door".to_string()));
        door(&mut arena, (9, 9), false);
        let open = actions_at(&arena, (9, 9));
        assert_eq!(find(&open, "Open door").command, ContextCommand::ClickTo { x: 9, y: 9 });
    }

    #[test]
    fn npcs_offer_speak_and_merchants_trade() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        crate::spawning::npcs::WIZARD.spawn(&mut arena.world, 6, 5);
        crate::spawning::vendors::MERCHANT.spawn(&mut arena.world, 5, 6, 1);
        arena.cache.rebuild_in_place(&arena.world);
        let wiz = actions_at(&arena, (6, 5));
        assert_eq!(find(&wiz, "Speak").command, ContextCommand::Intent(PlayerIntent::Move { dx: 1, dy: 0 }));
        assert!(!labels(&wiz).iter().any(|l| l.starts_with("Attack")));
        let shop = actions_at(&arena, (5, 6));
        assert!(find(&shop, "Trade").enabled);
        assert!(!labels(&shop).contains(&"Speak".to_string()));
        assert_eq!(
            describe_tile(&arena.world, &arena.grid, arena.player, (6, 5)).creatures[0].name,
            "Old Wizard"
        );
    }

    #[test]
    fn a_far_floor_tile_offers_only_walk_here() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        let acts = actions_at(&arena, (11, 9));
        assert_eq!(labels(&acts), vec!["Walk here", "Examine"]);
        assert_eq!(acts[0].command, ContextCommand::ClickTo { x: 11, y: 9 });
        assert!(acts[0].enabled);
    }

    #[test]
    fn walk_here_greys_out_without_a_path() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        for (x, y) in [(10, 9), (12, 9), (11, 8), (11, 10)] {
            set_tile(&mut arena, (x, y), |t| *t = Tile { visible: true, explored: true, ..Tile::new(TileType::Wall) });
        }
        let acts = actions_at(&arena, (11, 9));
        let walk = find(&acts, "Walk here");
        assert!(!walk.enabled);
        assert_eq!(walk.reason.as_deref(), Some("No path"));
    }

    fn necromancer_vs_rat(rat_at: (i32, i32)) -> Arena {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        give_kit(&mut arena, crate::components::PlayerClass::Necromancer);
        arena.rat(rat_at.0, rat_at.1, 1.0);
        arena
    }

    #[test]
    fn grave_bolt_is_enabled_in_range_with_line_of_sight() {
        let arena = necromancer_vs_rat((9, 5));
        let acts = actions_at(&arena, (9, 5));
        let bolt = find(&acts, "Grave Bolt");
        assert!(bolt.enabled, "{:?}", bolt.reason);
        assert_eq!(bolt.command, ContextCommand::Intent(PlayerIntent::GraveBolt { target_x: 9, target_y: 5 }));
        assert!(validate_choice(&arena.world, &arena.grid, &arena.cache, arena.player, (9, 5), &bolt.command).is_ok());
        // Not adjacent: attacking means walking up (pursuit), as a left-click does.
        assert_eq!(find(&acts, "Attack (Claws)").command, ContextCommand::ClickTo { x: 9, y: 5 });
    }

    #[test]
    fn grave_bolt_is_disabled_on_cooldown() {
        let arena = necromancer_vs_rat((9, 5));
        arena.world.get::<&mut ClassKit>(arena.player).unwrap().start_cooldown_for(AbilityType::GraveBolt);
        let acts = actions_at(&arena, (9, 5));
        let bolt = find(&acts, "Grave Bolt");
        assert!(!bolt.enabled);
        assert!(bolt.reason.as_deref().unwrap().starts_with("On cooldown"), "{:?}", bolt.reason);
        assert!(validate_choice(&arena.world, &arena.grid, &arena.cache, arena.player, (9, 5), &bolt.command).is_err());
    }

    #[test]
    fn grave_bolt_is_disabled_out_of_range_or_out_of_sight() {
        let arena = necromancer_vs_rat((5 + GRAVE_BOLT_RANGE + 1, 5));
        let acts = actions_at(&arena, (5 + GRAVE_BOLT_RANGE + 1, 5));
        let bolt = find(&acts, "Grave Bolt");
        assert!(!bolt.enabled);
        assert!(bolt.reason.as_deref().unwrap().starts_with("Out of range"));

        let mut arena = necromancer_vs_rat((9, 5));
        set_tile(&mut arena, (7, 5), |t| *t = Tile { visible: true, explored: true, ..Tile::new(TileType::Wall) });
        let acts = actions_at(&arena, (9, 5));
        assert_eq!(find(&acts, "Grave Bolt").reason.as_deref(), Some("No line of sight"));
    }

    #[test]
    fn a_closed_chest_offers_open_and_its_own_tile_offers_examine_only() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        let pos = Position::new(6, 6);
        arena.world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::CHEST_CLOSED),
            Container::chest(Vec::new(), 5),
            BlocksMovement,
        ));
        arena.cache.rebuild_in_place(&arena.world);
        let acts = actions_at(&arena, (6, 6));
        assert_eq!(find(&acts, "Open chest").command, ContextCommand::Intent(PlayerIntent::Move { dx: 1, dy: 1 }));
        let info = describe_tile(&arena.world, &arena.grid, arena.player, (6, 6));
        assert_eq!(info.features, vec!["Chest (closed)".to_string()]);
        assert_eq!(labels(&actions_at(&arena, (5, 5))), vec!["Examine"]);
    }

    #[test]
    fn intents_are_disabled_while_the_player_is_busy() {
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        arena.rat(6, 5, 1.0);
        let player = arena.player;
        arena.start(player, crate::components::ActionType::Wait);
        let acts = actions_at(&arena, (6, 5));
        let attack = find(&acts, "Attack (Claws)");
        assert!(!attack.enabled);
        assert_eq!(attack.reason.as_deref(), Some("You are busy"));
    }

    #[test]
    fn a_bow_user_can_shoot_a_visible_enemy_in_range_with_arrows() {
        use crate::components::{ItemInstance, RangedWeapon};
        let mut arena = Arena::new((5, 5));
        see_all(&mut arena);
        arena.world.get::<&mut Equipment>(arena.player).unwrap().weapon =
            Some(EquippedWeapon::Ranged(RangedWeapon::bow()));
        arena.rat(9, 5, 1.0);
        let shoot = ContextCommand::Intent(PlayerIntent::ShootRanged { target_x: 9, target_y: 5 });

        let acts = actions_at(&arena, (9, 5));
        let entry = find(&acts, "Shoot (Bow)");
        assert_eq!(entry.command, shoot);
        assert_eq!(entry.reason.as_deref(), Some("No arrows"));
        // A bow in hand means a melee swing is unarmed.
        assert!(labels(&acts).contains(&"Attack (fists)".to_string()));

        let mut inv = Inventory::new();
        inv.items.push(ItemInstance::plain(ItemType::Arrow));
        let _ = arena.world.insert_one(arena.player, inv);
        assert!(find(&actions_at(&arena, (9, 5)), "Shoot (Bow)").enabled);
    }
}
