//! What the Player and World tabs do to a run: the player's numbers and
//! statuses, and the world's creatures, map and contents.

use hecs::Entity;
use crate::world::World;

use crate::components::{
    AbilityType, ChaseAI, ClassAbility, ClassKit, CompanionAI, DevFrozen, EffectType, Experience,
    Fatigue, Health, Hunger, Inventory, LearnedAbilities, Player, Position, SecondaryAbility, Stats,
    StatusEffects, VisualPosition,
};
use crate::engine::GameState;
use crate::events::{EventQueue, GameEvent};

/// Every status effect, for the Player tab.
pub const ALL_EFFECTS: [EffectType; 21] = [
    EffectType::Invisible,
    EffectType::SpeedBoost,
    EffectType::Regenerating,
    EffectType::Strengthened,
    EffectType::Protected,
    EffectType::Barkskin,
    EffectType::Confused,
    EffectType::Feared,
    EffectType::Slowed,
    EffectType::Burning,
    EffectType::Rooted,
    EffectType::Invulnerable,
    EffectType::Stunned,
    EffectType::Guarding,
    EffectType::Thorns,
    EffectType::BoneWard,
    EffectType::Wet,
    EffectType::Oiled,
    EffectType::Poisoned,
    EffectType::Bleeding,
    EffectType::Grabbed,
];

/// The spells that can be learned (what scrolls teach, and Raise Dead).
pub const LEARNABLE: [AbilityType; 8] = [
    AbilityType::LearnedBlink,
    AbilityType::LearnedFireball,
    AbilityType::LearnedFear,
    AbilityType::LearnedSlow,
    AbilityType::LearnedProtection,
    AbilityType::LearnedSpeed,
    AbilityType::LearnedInvisibility,
    AbilityType::RaiseDead,
];

/// How long god mode's Invulnerable lasts each time it is topped up: far
/// longer than anything else runs, so it never lapses between frames.
const GOD_MODE_SECONDS: f32 = 1.0e6;

/// Keep god mode and full meters going (called every frame).
pub fn maintain(state: &mut GameState, god_mode: bool, keep_fed: bool) {
    let player = state.player_entity;
    if god_mode
        && !crate::systems::effects::entity_has_effect(&state.world, player, EffectType::Invulnerable)
    {
        crate::systems::effects::add_effect_to_entity(&mut state.world, player, EffectType::Invulnerable, GOD_MODE_SECONDS);
    }
    if keep_fed {
        if let Ok(mut hunger) = state.world.get::<&mut Hunger>(player) {
            hunger.value = crate::constants::HUNGER_MAX;
            hunger.starvation_timer = 0.0;
        }
        if let Ok(mut fatigue) = state.world.get::<&mut Fatigue>(player) {
            fatigue.value = 0.0;
        }
    }
}

/// God mode switched off: take its Invulnerable away again.
pub fn end_god_mode(state: &mut GameState) {
    crate::systems::effects::remove_effect_from_entity(&mut state.world, state.player_entity, EffectType::Invulnerable);
}

pub fn set_health(world: &mut World, player: Entity, current: i32, max: i32) {
    if let Ok(mut health) = world.get::<&mut Health>(player) {
        health.max = max.max(1);
        health.current = current.clamp(0, health.max);
    }
}

pub fn set_stats(world: &mut World, player: Entity, stats: Stats) {
    if let Ok(mut s) = world.get::<&mut Stats>(player) {
        *s = stats;
    }
}

/// Give XP, announcing any level reached the usual way.
pub fn add_xp(world: &mut World, player: Entity, amount: u32, events: &mut EventQueue) {
    let levelled = world.get::<&mut Experience>(player).ok().map(|mut exp| {
        let up = crate::systems::experience::grant_xp(&mut exp, amount);
        (up, exp.level)
    });
    if let Some((true, new_level)) = levelled {
        events.push(GameEvent::LevelUp { new_level });
    }
}

pub fn add_gold(world: &mut World, player: Entity, amount: u32) {
    if let Ok(mut inventory) = world.get::<&mut Inventory>(player) {
        inventory.gold = inventory.gold.saturating_add(amount);
    }
}

pub fn set_meters(world: &mut World, player: Entity, hunger: f32, fatigue: f32) {
    if let Ok(mut h) = world.get::<&mut Hunger>(player) {
        h.value = hunger.clamp(0.0, crate::constants::HUNGER_MAX);
    }
    if let Ok(mut f) = world.get::<&mut Fatigue>(player) {
        f.value = fatigue.max(0.0);
    }
}

/// Learn a spell. Returns whether it was new (so it can go on the hotbar).
pub fn learn(world: &mut World, player: Entity, ability: AbilityType) -> bool {
    if world.get::<&LearnedAbilities>(player).is_err() {
        let _ = world.insert_one(player, LearnedAbilities::default());
    }
    world.get::<&mut LearnedAbilities>(player).map(|mut l| l.learn(ability)).unwrap_or(false)
}

/// Every cooldown the player has back to ready.
pub fn reset_cooldowns(world: &mut World, player: Entity) {
    if let Ok(mut a) = world.get::<&mut ClassAbility>(player) {
        a.cooldown_remaining = 0.0;
    }
    if let Ok(mut a) = world.get::<&mut SecondaryAbility>(player) {
        a.cooldown_remaining = 0.0;
    }
    if let Ok(mut kit) = world.get::<&mut ClassKit>(player) {
        for a in &mut kit.abilities {
            a.cooldown_remaining = 0.0;
        }
    }
    if let Ok(mut learned) = world.get::<&mut LearnedAbilities>(player) {
        for spell in &mut learned.spells {
            spell.cooldown_remaining = 0.0;
        }
    }
}

pub fn clear_effects(world: &mut World, player: Entity) {
    if let Ok(mut status) = world.get::<&mut StatusEffects>(player) {
        status.effects.clear();
    }
}

/// Every hostile (or every one you can see) dies, and is turned into bones,
/// loot and XP by the usual end-of-frame pass.
pub fn kill_hostiles(state: &mut GameState, visible_only: bool) -> usize {
    let grid = &state.grid;
    let doomed: Vec<Entity> = state
        .world
        .query::<(&Position, &Health)>()
        .with::<&ChaseAI>()
        .iter()
        .filter(|(_, (p, h))| h.current > 0 && (!visible_only || grid.get(p.x, p.y).is_some_and(|t| t.visible)))
        .map(|(id, _)| id)
        .collect();
    for &id in &doomed {
        if let Ok(mut health) = state.world.get::<&mut Health>(id) {
            health.current = 0;
        }
    }
    doomed.len()
}

/// Hold every creature still (or let them go again).
pub fn freeze_ai(world: &mut World, frozen: bool) {
    let creatures: Vec<Entity> = world
        .query::<()>()
        .with::<&crate::components::Actor>()
        .without::<&Player>()
        .iter()
        .map(|(id, _)| id)
        .filter(|&id| world.get::<&ChaseAI>(id).is_ok() || world.get::<&CompanionAI>(id).is_ok())
        .collect();
    for id in creatures {
        if frozen {
            let _ = world.insert_one(id, DevFrozen);
        } else {
            let _ = world.remove_one::<DevFrozen>(id);
        }
    }
}

pub fn wake_all(world: &mut World) -> usize {
    let sleepers: Vec<Entity> =
        world.query::<&crate::components::Asleep>().iter().map(|(id, _)| id).collect();
    for &id in &sleepers {
        let _ = world.remove_one::<crate::components::Asleep>(id);
    }
    sleepers.len()
}

/// Move the player to a free floor tile, telling the spatial cache.
pub fn teleport(state: &mut GameState, to: (i32, i32)) -> Result<(), &'static str> {
    if !state.grid.is_walkable(to.0, to.1) {
        return Err("not a floor tile");
    }
    if state.spatial_cache.is_blocked(to) {
        return Err("something is standing there");
    }
    let player = state.player_entity;
    let Some(from) = state.world.get::<&Position>(player).ok().map(|p| (p.x, p.y)) else {
        return Err("no player");
    };
    if let Ok(mut pos) = state.world.get::<&mut Position>(player) {
        *pos = Position::new(to.0, to.1);
    }
    if let Ok(mut vis) = state.world.get::<&mut VisualPosition>(player) {
        *vis = VisualPosition { x: to.0 as f32, y: to.1 as f32 };
    }
    state.spatial_cache.update_position(player, from, to);
    state.fov_dirty = true;
    Ok(())
}

/// Remove everything on a tile but the player and the water's ripples,
/// unhooking it from the spatial cache, the AI and the scheduler.
pub fn delete_at(state: &mut GameState, at: (i32, i32)) -> usize {
    let player = state.player_entity;
    let doomed: Vec<Entity> = state
        .world
        .query::<&Position>()
        .iter()
        .filter(|(id, p)| *id != player && (p.x, p.y) == at)
        .map(|(id, _)| id)
        .filter(|&id| {
            !state
                .world
                .get::<&crate::components::AnimatedSprite>(id)
                .is_ok_and(|a| a.base_tile_id == crate::tile::tile_ids::WATER_ANIMATED.1)
        })
        .collect();
    for &id in &doomed {
        state.spatial_cache.remove_entity(id);
        state.active_ai_tracker.remove_entity(id);
        state.action_scheduler.cancel_for_entity(id);
        let _ = state.world.despawn(id);
    }
    if !doomed.is_empty() {
        state.fov_dirty = true;
    }
    doomed.len()
}

/// Whatever on a tile has health drops to none (and dies, smashes or blows,
/// as it would).
pub fn kill_at(state: &mut GameState, at: (i32, i32)) -> usize {
    let player = state.player_entity;
    let mut n = 0;
    for (id, (p, health)) in state.world.query_mut::<(&Position, &mut Health)>() {
        if id != player && (p.x, p.y) == at && health.current > 0 {
            health.current = 0;
            n += 1;
        }
    }
    n
}
