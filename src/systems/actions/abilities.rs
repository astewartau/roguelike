//! Class abilities: blink, fireball, taming, life drain, fear, sprint,
//! barkskin, disengage, tumble, and crippling shot.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    AbilityType, Attackable, BlocksMovement, ChaseAI, ClassAbility, Container, ContainerType,
    EffectType, Equipment, EquippedWeapon, Health, Inventory, ItemType, LearnedAbilities,
    LifeDrainInProgress, Player, Position, Projectile, ProjectileMarker, RaiseDeadInProgress,
    SecondaryAbility, Sprite, TamedBy, TamingInProgress, VisualPosition,
};
use crate::constants::*;
use crate::engine::EffectCtx;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::queries;
use crate::spatial_cache::SpatialCache;
use crate::systems::effects;
use crate::tile::tile_ids;

use super::{calculate_arrow_path, ActionResult};

/// Apply blink (teleport) action. The landing tile's effects (water, oil,
/// fire, traps) apply as for a step — see `tile_effects::on_enter_tile`.
#[allow(clippy::too_many_arguments)]
pub fn apply_blink(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    target_x: i32,
    target_y: i32,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Check range (INT-scaled: high effective INT blinks further)
    let dist = (target_x - current_pos.0).abs().max((target_y - current_pos.1).abs());
    if dist > scaled_blink_range(world, entity) {
        return ActionResult::Blocked;
    }

    // Check target is walkable
    if !grid.is_walkable(target_x, target_y) {
        return ActionResult::Blocked;
    }

    // Check no blocking entity at target
    if queries::is_position_blocked(spatial_cache, target_x, target_y, Some(entity)) {
        return ActionResult::Blocked;
    }

    // Teleport: update position
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        pos.x = target_x;
        pos.y = target_y;
    }
    spatial_cache.update_position(entity, current_pos, (target_x, target_y));

    // Snap visual position (instant teleport, no lerping)
    if let Ok(mut vis_pos) = world.get::<&mut VisualPosition>(entity) {
        vis_pos.x = target_x as f32;
        vis_pos.y = target_y as f32;
    }

    events.push(GameEvent::EntityMoved {
        entity,
        from: current_pos,
        to: (target_x, target_y),
    });

    crate::systems::tile_effects::on_enter_tile(
        world,
        grid,
        entity,
        (target_x, target_y),
        events,
        rng,
    );

    ActionResult::Completed
}

/// Blink range for a caster: `BLINK_RANGE` scaled by effective INT, never
/// below 1. Used by the range check in `apply_blink` and by the targeting
/// overlays (scroll and learned cast alike).
pub fn scaled_blink_range(world: &World, caster: Entity) -> i32 {
    ((BLINK_RANGE as f32 * queries::int_power(world, caster)).round() as i32).max(1)
}

/// Apply fireball action - AoE damage at target location
pub fn apply_fireball(
    world: &mut World,
    caster: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    let caster_pos = match queries::get_entity_position(world, caster) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Check range
    let dist = (target_x - caster_pos.0).abs().max((target_y - caster_pos.1).abs());
    if dist > FIREBALL_RANGE {
        return ActionResult::Blocked;
    }

    // Fireball damage scales with the caster's effective INT (gear counts).
    let damage = ((FIREBALL_DAMAGE as f32 * queries::int_power(world, caster)).round() as i32).max(1);

    // Emit explosion VFX event
    events.push(GameEvent::FireballExplosion {
        x: target_x,
        y: target_y,
        radius: FIREBALL_RADIUS,
    });

    // A fireball is very loud — wake sleeping enemies over a wide area.
    crate::systems::ai::wake_enemies_in_radius(world, (target_x, target_y), EXPLOSION_NOISE_RADIUS);

    // Collect all attackable entities in radius
    let mut damaged: Vec<(Entity, i32, i32)> = Vec::new();
    for (id, (pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        let dx = (pos.x - target_x).abs();
        let dy = (pos.y - target_y).abs();
        if dx <= FIREBALL_RADIUS && dy <= FIREBALL_RADIUS {
            damaged.push((id, pos.x, pos.y));
        }
    }

    // Apply damage to all
    for (entity, x, y) in damaged {
        // Apply damage (handles invulnerability, armor defense, Protected/Barkskin)
        crate::systems::combat::apply_damage(world, entity, damage, rng, events);
        // Interrupt life drain if entity was channeling
        interrupt_life_drain_on_damage(world, entity, events);
        // Generate threat on fireball targets
        crate::systems::ai::generate_threat(world, entity, caster, damage as f32 * THREAT_PER_DAMAGE);
        crate::systems::ai::generate_companion_threat(world, entity, caster, damage as f32 * THREAT_PER_DAMAGE);
        events.push(GameEvent::AttackHit {
            attacker: caster,
            target: entity,
            target_pos: (x as f32 + 0.5, y as f32 + 0.5),
            damage,
            kind: crate::events::DamageKind::Fireball,
            crit: false,
            flanked: false,
            killed: crate::systems::combat::is_dead(world, entity),
        });
    }

    ActionResult::Completed
}

/// Apply sprint activation - applies speed boost effect to entity
pub fn apply_activate_sprint(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::SPRINT_DURATION;
    use crate::systems::effects::add_effect_to_entity;

    add_effect_to_entity(world, entity, EffectType::SpeedBoost, SPRINT_DURATION);

    events.push(GameEvent::AbilityActivated {
        entity,
        ability: AbilityType::Sprint,
    });

    ActionResult::Completed
}

/// Apply barkskin activation - applies damage reduction effect to entity
pub fn apply_activate_barkskin(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::BARKSKIN_DURATION;
    use crate::systems::effects::add_effect_to_entity;
    use crate::components::SecondaryAbility;

    // Barkskin duration scales with effective INT.
    let duration = BARKSKIN_DURATION * queries::int_power(world, entity);
    add_effect_to_entity(world, entity, EffectType::Barkskin, duration);

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut SecondaryAbility>(entity) {
        ability.start_cooldown();
    }

    // Emit event for VFX
    events.push(GameEvent::BarkskinActivated { entity });

    ActionResult::Completed
}

/// Start life drain channeling (Necromancer ability)
pub fn apply_start_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify target still exists and is alive
    let target_alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(false);
    if !target_alive {
        return ActionResult::Invalid;
    }

    // Check range
    let caster_pos = queries::get_entity_position(world, caster);
    let target_pos = queries::get_entity_position(world, target);
    match (caster_pos, target_pos) {
        (Some((cx, cy)), Some((tx, ty))) => {
            let dist = (cx - tx).abs().max((cy - ty).abs());
            if dist > LIFE_DRAIN_RANGE {
                return ActionResult::Invalid;
            }
        }
        _ => return ActionResult::Invalid,
    }

    // Add LifeDrainInProgress component to start channeling
    let _ = world.insert_one(caster, LifeDrainInProgress {
        target,
        tick_timer: 0.0, // Tick immediately on first wait
    });

    // Emit event to show VFX
    events.push(GameEvent::LifeDrainStarted { caster, target });

    ActionResult::Completed
}

/// Apply fear activation - causes nearby enemies to flee
pub fn apply_activate_fear(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::FEAR_ABILITY_DURATION;

    // Get entity position
    let entity_pos = crate::queries::get_entity_position(world, entity);
    if entity_pos.is_none() {
        return ActionResult::Invalid;
    }
    let (ex, ey) = entity_pos.unwrap();

    // Apply fear to all visible enemies in range
    let targets: Vec<hecs::Entity> = world
        .query::<(&Position, &Health)>()
        .without::<&Player>()
        .without::<&TamedBy>()
        .iter()
        .filter(|(_, (pos, health))| {
            let dx = (pos.x - ex).abs();
            let dy = (pos.y - ey).abs();
            dx <= crate::constants::FEAR_ABILITY_RADIUS
                && dy <= crate::constants::FEAR_ABILITY_RADIUS
                && health.current > 0
        })
        .map(|(e, _)| e)
        .collect();

    // Fear duration scales with the caster's effective INT.
    let duration = FEAR_ABILITY_DURATION * queries::int_power(world, entity);

    // Apply fear effect to each target
    for target in targets {
        crate::systems::effects::add_effect_to_entity(world, target, EffectType::Feared, duration);
    }

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut SecondaryAbility>(entity) {
        ability.start_cooldown();
    }

    // Emit event for VFX
    events.push(GameEvent::FearActivated {
        entity,
        position: (ex, ey),
    });

    ActionResult::Completed
}

/// Apply wait action - handles taming and life drain progress if applicable
///
/// Takes the full effect context because completing a tame drops the target's
/// `BlocksMovement`, which has to be mirrored into the `SpatialCache`.
pub fn apply_wait(ctx: &mut EffectCtx, entity: Entity) -> ActionResult {
    let EffectCtx { world, grid: _grid, spatial: spatial_cache, events, rng } = ctx;
    let world = &mut **world;
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // Check if entity is taming something
    let taming_info = world.get::<&TamingInProgress>(entity)
        .ok()
        .map(|t| (t.target, t.progress, t.required));

    if let Some((target, progress, required)) = taming_info {
        // Check if target still exists and is in range
        let entity_pos = world.get::<&Position>(entity).ok().map(|p| (p.x, p.y));
        let target_pos = world.get::<&Position>(target).ok().map(|p| (p.x, p.y));

        match (entity_pos, target_pos) {
            (Some((ex, ey)), Some((tx, ty))) => {
                let dist = (ex - tx).abs().max((ey - ty).abs());
                if dist <= TAME_RANGE {
                    // Add progress (wait duration is 0.5s)
                    let new_progress = progress + ACTION_WAIT_DURATION;

                    if new_progress >= required {
                        // Taming complete!
                        complete_taming(world, spatial_cache, entity, target, events);
                    } else {
                        // Update progress
                        if let Ok(mut taming) = world.get::<&mut TamingInProgress>(entity) {
                            taming.progress = new_progress;
                        }
                        events.push(GameEvent::TamingProgress {
                            tamer: entity,
                            target,
                            progress: new_progress,
                            required,
                        });
                    }
                } else {
                    // Too far away - taming failed
                    let _ = world.remove_one::<TamingInProgress>(entity);
                    events.push(GameEvent::TamingFailed { tamer: entity, target });
                }
            }
            _ => {
                // Target no longer exists - remove taming state
                let _ = world.remove_one::<TamingInProgress>(entity);
            }
        }
    }

    // Check if entity is channeling Raise Dead (mirrors the taming channel)
    let raise_info = world.get::<&RaiseDeadInProgress>(entity)
        .ok()
        .map(|r| (r.target, r.progress, r.required));

    if let Some((target, progress, required)) = raise_info {
        let entity_pos = world.get::<&Position>(entity).ok().map(|p| (p.x, p.y));
        let target_pos = world.get::<&Position>(target).ok().map(|p| (p.x, p.y));
        let target_is_bones = world
            .get::<&Container>(target)
            .map(|c| matches!(c.container_type, ContainerType::Corpse))
            .unwrap_or(false);

        match (entity_pos, target_pos) {
            (Some((ex, ey)), Some((tx, ty))) if target_is_bones => {
                let dist = (ex - tx).abs().max((ey - ty).abs());
                if dist <= RAISE_DEAD_RANGE {
                    let new_progress = progress + ACTION_WAIT_DURATION;
                    if new_progress >= required {
                        complete_raise_dead(world, entity, target, events);
                    } else if let Ok(mut raise) = world.get::<&mut RaiseDeadInProgress>(entity) {
                        raise.progress = new_progress;
                    }
                } else {
                    // Wandered out of range - ritual fails
                    let _ = world.remove_one::<RaiseDeadInProgress>(entity);
                    events.push(GameEvent::RaiseDeadFailed { caster: entity });
                }
            }
            _ => {
                // Bones no longer exist (looted?) - ritual fails
                let _ = world.remove_one::<RaiseDeadInProgress>(entity);
                events.push(GameEvent::RaiseDeadFailed { caster: entity });
            }
        }
    }

    // Check if entity is channeling life drain
    let drain_info = world.get::<&LifeDrainInProgress>(entity)
        .ok()
        .map(|d| (d.target, d.tick_timer));

    if let Some((target, tick_timer)) = drain_info {
        tick_life_drain(world, entity, target, tick_timer, events, rng);
    }

    ActionResult::Completed
}

// =============================================================================
// LEARNED SPELLS (studied from scrolls)
// =============================================================================

/// Cast a learned (studied) spell. Routes targeted spells (Blink / Fireball)
/// through the same handlers scrolls use; untargeted spells apply their
/// status effect directly. All magnitudes scale with effective INT via
/// `queries::int_power`. Starts the spell's long cooldown on success.
#[allow(clippy::too_many_arguments)]
pub fn apply_cast_learned_spell(
    world: &mut World,
    grid: &Grid,
    caster: Entity,
    ability: AbilityType,
    target_x: i32,
    target_y: i32,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    use crate::systems::effects::{add_effect_to_entity, apply_effect_to_visible_enemies};

    let power = queries::int_power(world, caster);

    let result = match ability {
        AbilityType::LearnedBlink => {
            apply_blink(world, grid, caster, target_x, target_y, spatial_cache, events, rng)
        }
        AbilityType::LearnedFireball => {
            apply_fireball(world, caster, target_x, target_y, events, rng)
        }
        AbilityType::LearnedFear => match queries::get_entity_position(world, caster) {
            Some(pos) => {
                apply_effect_to_visible_enemies(
                    world, grid, pos, FOV_RADIUS, EffectType::Feared, FEAR_DURATION * power,
                );
                events.push(GameEvent::FearActivated { entity: caster, position: pos });
                ActionResult::Completed
            }
            None => ActionResult::Invalid,
        },
        AbilityType::LearnedSlow => match queries::get_entity_position(world, caster) {
            Some(pos) => {
                apply_effect_to_visible_enemies(
                    world, grid, pos, FOV_RADIUS, EffectType::Slowed, SLOW_DURATION * power,
                );
                ActionResult::Completed
            }
            None => ActionResult::Invalid,
        },
        AbilityType::LearnedProtection => {
            add_effect_to_entity(world, caster, EffectType::Protected, PROTECTION_DURATION * power);
            ActionResult::Completed
        }
        AbilityType::LearnedSpeed => {
            add_effect_to_entity(world, caster, EffectType::SpeedBoost, SPEED_BOOST_DURATION * power);
            ActionResult::Completed
        }
        AbilityType::LearnedInvisibility => {
            add_effect_to_entity(world, caster, EffectType::Invisible, INVISIBILITY_DURATION * power);
            ActionResult::Completed
        }
        _ => ActionResult::Invalid,
    };

    if result == ActionResult::Completed {
        if let Ok(mut learned) = world.get::<&mut LearnedAbilities>(caster) {
            learned.start_cooldown(ability);
        }
        events.push(GameEvent::AbilityActivated { entity: caster, ability });
    }

    result
}

// =============================================================================
// RAISE DEAD (Necromancer)
// =============================================================================

/// Number of living raised skeletons in the world (they lose `Health` when
/// they die and turn into bones, so this counts only active ones).
pub fn raised_undead_count(world: &World) -> usize {
    world
        .query::<(&crate::components::RaisedUndead, &Health)>()
        .iter()
        .filter(|(_, (_, h))| h.current > 0)
        .count()
}

/// Start the Raise Dead channel on a bones container (Necromancer ability).
/// Mirrors `apply_start_taming`: the caster must keep Waiting in range until
/// the channel completes; moving interrupts it.
pub fn apply_start_raise_dead(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify the target is still a bones/corpse container
    let is_bones = world
        .get::<&Container>(target)
        .map(|c| matches!(c.container_type, ContainerType::Corpse))
        .unwrap_or(false);
    if !is_bones {
        return ActionResult::Invalid;
    }

    // Check range
    let caster_pos = queries::get_entity_position(world, caster);
    let target_pos = queries::get_entity_position(world, target);
    match (caster_pos, target_pos) {
        (Some((cx, cy)), Some((tx, ty))) => {
            let dist = (cx - tx).abs().max((cy - ty).abs());
            if dist > RAISE_DEAD_RANGE {
                return ActionResult::Invalid;
            }
        }
        _ => return ActionResult::Invalid,
    }

    // Control cap (also enforced at activation; re-checked defensively here)
    let cap = crate::constants::raise_dead_cap(queries::effective_stats(world, caster).intelligence);
    if raised_undead_count(world) >= cap {
        return ActionResult::Invalid;
    }

    let _ = world.insert_one(caster, RaiseDeadInProgress {
        target,
        progress: 0.0,
        required: RAISE_DEAD_CHANNEL_DURATION,
    });

    // Start the long cooldown (lives in the caster's spell list)
    if let Ok(mut learned) = world.get::<&mut LearnedAbilities>(caster) {
        learned.start_cooldown(AbilityType::RaiseDead);
    }

    events.push(GameEvent::RaiseDeadStarted { caster, target });

    ActionResult::Completed
}

/// Interrupt an in-progress Raise Dead channel (e.g. the caster moved).
pub fn interrupt_raise_dead(world: &mut World, entity: Entity, events: &mut EventQueue) {
    if world.get::<&RaiseDeadInProgress>(entity).is_ok() {
        let _ = world.remove_one::<RaiseDeadInProgress>(entity);
        events.push(GameEvent::RaiseDeadFailed { caster: entity });
    }
}

/// Complete Raise Dead: consume the bones container (its loot drops to a
/// ground pile on the same tile) and emit `SkeletonRaised` so the engine can
/// spawn and schedule the skeleton companion (it needs the action scheduler,
/// which action handlers don't have).
fn complete_raise_dead(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
) {
    let _ = world.remove_one::<RaiseDeadInProgress>(caster);

    let Some((x, y)) = queries::get_entity_position(world, target) else {
        events.push(GameEvent::RaiseDeadFailed { caster });
        return;
    };

    consume_corpse(world, target, (x, y));

    events.push(GameEvent::SkeletonRaised { owner: caster, position: (x, y) });
}

/// Consume a corpse (bones container) at `pos`: despawn it, and drop anything
/// it held to a ground pile on the same tile so no loot is destroyed. Shared
/// by Raise Dead and Corpse Explosion. Corpses never block movement, so there
/// is nothing to release in the spatial cache.
pub(super) fn consume_corpse(world: &mut World, corpse: Entity, pos: (i32, i32)) {
    let (x, y) = pos;
    // Take the loot out of the bones, then consume them.
    let (items, gold) = world
        .get::<&Container>(corpse)
        .map(|c| (c.items.clone(), c.gold))
        .unwrap_or((Vec::new(), 0));
    let _ = world.despawn(corpse);

    // Anything the corpse held drops to a ground pile on the same tile.
    if !items.is_empty() || gold > 0 {
        let sprite_ref = items
            .first()
            .map(|i| crate::systems::item_defs::get_def(i.kind).sprite)
            .unwrap_or(tile_ids::COINS);
        let mut pile = Container::ground_pile(items);
        pile.gold = gold;
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(sprite_ref),
            pile,
            crate::components::GroundItemPile,
        ));
    }
}

/// Tick life drain channeling - applies damage and healing
fn tick_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    tick_timer: f32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    // Check if target still exists and is alive
    let target_alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(false);
    if !target_alive {
        // Target died - end drain and start cooldown
        end_life_drain(world, caster, target, events, false);
        return;
    }

    // Check range and get positions for VFX
    let caster_pos = queries::get_entity_position(world, caster);
    let target_pos = queries::get_entity_position(world, target);
    let (cx, cy, tx, ty) = match (caster_pos, target_pos) {
        (Some((cx, cy)), Some((tx, ty))) => {
            let dist = (cx - tx).abs().max((cy - ty).abs());
            if dist > LIFE_DRAIN_RANGE {
                // Out of range - end drain and start cooldown
                end_life_drain(world, caster, target, events, false);
                return;
            }
            (cx, cy, tx, ty)
        }
        _ => {
            // Invalid positions - end drain
            end_life_drain(world, caster, target, events, false);
            return;
        }
    };

    // Update tick timer
    let new_timer = tick_timer + ACTION_WAIT_DURATION;
    if new_timer >= LIFE_DRAIN_TICK_INTERVAL {
        // Time to tick! Per-tick damage (and thus the heal derived from it)
        // scales with the caster's effective INT — gear counts.
        let damage = ((LIFE_DRAIN_DAMAGE_PER_TICK as f32 * queries::int_power(world, caster))
            .round() as i32)
            .max(1);

        // Apply damage to target (handles invulnerability, armor defense, Protected/Barkskin)
        crate::systems::combat::apply_damage(world, target, damage, rng, events);
        let target_died = world
            .get::<&Health>(target)
            .map(|h| h.current <= 0)
            .unwrap_or(false);

        // Generate threat on life drain target
        crate::systems::ai::generate_threat(world, target, caster, damage as f32 * THREAT_PER_DAMAGE);
        crate::systems::ai::generate_companion_threat(world, target, caster, damage as f32 * THREAT_PER_DAMAGE);

        // Heal caster (percentage of damage dealt)
        let heal_amount = (damage as f32 * LIFE_DRAIN_HEAL_PERCENT) as i32;
        if let Ok(mut health) = world.get::<&mut Health>(caster) {
            health.current = (health.current + heal_amount).min(health.max);
        }

        // Emit tick event with positions for damage numbers
        events.push(GameEvent::LifeDrainTick {
            caster,
            target,
            caster_pos: (cx as f32 + 0.5, cy as f32 + 0.5),
            target_pos: (tx as f32 + 0.5, ty as f32 + 0.5),
            damage,
            healed: heal_amount,
        });

        // Reset timer (or end if target died)
        if target_died {
            end_life_drain(world, caster, target, events, false);
        } else {
            // Reset tick timer
            if let Ok(mut drain) = world.get::<&mut LifeDrainInProgress>(caster) {
                drain.tick_timer = 0.0;
            }
        }
    } else {
        // Just update timer
        if let Ok(mut drain) = world.get::<&mut LifeDrainInProgress>(caster) {
            drain.tick_timer = new_timer;
        }
    }
}

/// Interrupt life drain if the caster takes damage
/// Call this when an entity takes damage to check if they should stop channeling
pub fn interrupt_life_drain_on_damage(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) {
    // Check if this entity is channeling life drain (extract target to avoid borrow conflict)
    let target = world.get::<&LifeDrainInProgress>(entity).ok().map(|d| d.target);
    if let Some(target) = target {
        end_life_drain(world, entity, target, events, true);
    }
}

/// End life drain channeling and start cooldown
fn end_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
    was_interrupted: bool,
) {
    // Remove the channeling component
    let _ = world.remove_one::<LifeDrainInProgress>(caster);

    // Start cooldown
    if let Ok(mut ability) = world.get::<&mut ClassAbility>(caster) {
        ability.start_cooldown();
    }

    // Emit appropriate event
    if was_interrupted {
        events.push(GameEvent::LifeDrainInterrupted { caster, target });
    } else {
        events.push(GameEvent::LifeDrainEnded { caster, target });
    }
}

/// Interrupt an in-progress taming channel (e.g. the tamer moved). Removes the
/// channeling state and emits TamingFailed so the visual stops. The druid must
/// stand still (keep waiting) to tame.
pub fn interrupt_taming(world: &mut World, entity: Entity, events: &mut EventQueue) {
    let target = world.get::<&TamingInProgress>(entity).ok().map(|t| t.target);
    if let Some(target) = target {
        let _ = world.remove_one::<TamingInProgress>(entity);
        events.push(GameEvent::TamingFailed { tamer: entity, target });
    }
}

/// Complete taming - convert enemy to companion
fn complete_taming(
    world: &mut World,
    spatial_cache: &mut SpatialCache,
    tamer: Entity,
    target: Entity,
    events: &mut EventQueue,
) {
    use crate::components::{CompanionAI, TamedBy};

    // Remove hostile AI (but keep Attackable so enemies can still attack the companion)
    let _ = world.remove_one::<ChaseAI>(target);

    // Remove BlocksMovement so player can walk through their companion, and
    // mirror that into the SpatialCache. Without this the cache keeps the
    // companion's tile blocked, and because the entity is still *tracked*,
    // `update_position` drags that phantom blocker along every step it takes —
    // the companion trails an invisible wall that enemies path around.
    let _ = world.remove_one::<BlocksMovement>(target);
    spatial_cache.clear_blocking_flags(target);

    // Add companion components
    let _ = world.insert_one(target, TamedBy { owner: tamer });
    let _ = world.insert_one(target, CompanionAI {
        owner: tamer,
        follow_distance: 2,
        threat_table: Vec::new(),
    });

    // Remove taming state from player
    let _ = world.remove_one::<TamingInProgress>(tamer);

    // Emit event
    events.push(GameEvent::TamingCompleted { tamer, target });
}

/// Start taming an animal (Druid ability)
pub fn apply_start_taming(
    world: &mut World,
    tamer: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify target still exists and is tameable
    if world.get::<&crate::components::Tameable>(target).is_err() {
        return ActionResult::Invalid;
    }

    // Add TamingInProgress component to the tamer
    let _ = world.insert_one(tamer, TamingInProgress {
        target,
        progress: 0.0,
        required: TAME_DURATION,
    });

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut ClassAbility>(tamer) {
        ability.start_cooldown();
    }

    // Emit event to show message and VFX
    events.push(GameEvent::TamingStarted { tamer, target });

    ActionResult::Completed
}

/// Ranger ability: Disengage - leap away from the nearest enemy. The landing
/// tile's effects apply (see `tile_effects::on_enter_tile`).
pub fn apply_disengage(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    use crate::fov::Fov;
    use crate::constants::{DISENGAGE_DISTANCE, FOV_RADIUS};

    // Get entity position
    let pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Blocked,
    };

    events.push(GameEvent::AbilityActivated {
        entity,
        ability: AbilityType::Disengage,
    });

    // Find visible enemies
    let visible_tiles = Fov::calculate(grid, pos.0, pos.1, FOV_RADIUS, None::<fn(i32, i32) -> bool>);
    let visible_set: std::collections::HashSet<(i32, i32)> = visible_tiles.into_iter().collect();

    // Find nearest enemy
    let mut nearest_enemy: Option<(i32, i32, i32)> = None; // (x, y, distance_squared)
    for (_, (enemy_pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if (enemy_pos.x, enemy_pos.y) == pos {
            continue; // Skip self
        }
        if !visible_set.contains(&(enemy_pos.x, enemy_pos.y)) {
            continue;
        }
        let dx = enemy_pos.x - pos.0;
        let dy = enemy_pos.y - pos.1;
        let dist_sq = dx * dx + dy * dy;
        if nearest_enemy.is_none() || dist_sq < nearest_enemy.unwrap().2 {
            nearest_enemy = Some((enemy_pos.x, enemy_pos.y, dist_sq));
        }
    }

    // If no enemy found, just stay in place
    let (enemy_x, enemy_y) = match nearest_enemy {
        Some((x, y, _)) => (x, y),
        None => return ActionResult::Completed, // No enemies, ability still goes on cooldown
    };

    // Calculate direction away from enemy
    let dx = pos.0 - enemy_x;
    let dy = pos.1 - enemy_y;
    let len = ((dx * dx + dy * dy) as f32).sqrt().max(0.001);
    let dir_x = (dx as f32 / len).round() as i32;
    let dir_y = (dy as f32 / len).round() as i32;

    // Try to find a valid landing spot
    let try_offsets = [
        (dir_x, dir_y),
        (dir_y, -dir_x),  // Perpendicular
        (-dir_y, dir_x),  // Other perpendicular
    ];

    for (off_x, off_y) in try_offsets {
        if off_x == 0 && off_y == 0 {
            continue;
        }
        let target_x = pos.0 + off_x * DISENGAGE_DISTANCE;
        let target_y = pos.1 + off_y * DISENGAGE_DISTANCE;

        // Check if target is walkable and unoccupied
        if grid.get(target_x, target_y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
            let blocked = world.query::<(&Position, &BlocksMovement)>()
                .iter()
                .any(|(_, (p, _))| p.x == target_x && p.y == target_y);

            if !blocked {
                // Teleport to target
                if let Ok(mut p) = world.get::<&mut Position>(entity) {
                    p.x = target_x;
                    p.y = target_y;
                }
                spatial_cache.update_position(entity, pos, (target_x, target_y));
                if let Ok(mut vpos) = world.get::<&mut VisualPosition>(entity) {
                    vpos.x = target_x as f32;
                    vpos.y = target_y as f32;
                }
                events.push(GameEvent::EntityMoved {
                    entity,
                    from: pos,
                    to: (target_x, target_y),
                });
                crate::systems::tile_effects::on_enter_tile(
                    world,
                    grid,
                    entity,
                    (target_x, target_y),
                    events,
                    rng,
                );
                return ActionResult::Completed;
            }
        }
    }

    // All spots blocked, ability still goes on cooldown but no movement
    ActionResult::Completed
}

/// Ranger ability: Tumble - roll to target position with brief invulnerability.
/// The landing tile's effects apply (see `tile_effects::on_enter_tile`); the
/// tiles rolled over on the way do not.
#[allow(clippy::too_many_arguments)]
pub fn apply_tumble(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    target_x: i32,
    target_y: i32,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    use crate::constants::TUMBLE_INVULN_DURATION;

    // Get entity position
    let from = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Check target is walkable terrain (ignore blocking entities - we roll through)
    if !grid.get(target_x, target_y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
        return ActionResult::Blocked;
    }

    events.push(GameEvent::AbilityActivated {
        entity,
        ability: AbilityType::Tumble,
    });

    // Teleport to target
    if let Ok(mut p) = world.get::<&mut Position>(entity) {
        p.x = target_x;
        p.y = target_y;
    }
    spatial_cache.update_position(entity, from, (target_x, target_y));
    if let Ok(mut vpos) = world.get::<&mut VisualPosition>(entity) {
        vpos.x = target_x as f32;
        vpos.y = target_y as f32;
    }

    // Apply invulnerability effect
    effects::add_effect_to_entity(world, entity, EffectType::Invulnerable, TUMBLE_INVULN_DURATION);

    events.push(GameEvent::EntityMoved { entity, from, to: (target_x, target_y) });
    crate::systems::tile_effects::on_enter_tile(
        world,
        grid,
        entity,
        (target_x, target_y),
        events,
        rng,
    );

    ActionResult::Completed
}

/// Ranger ability: Shoot a crippling arrow that slows the target
pub fn apply_shoot_crippling_shot(
    world: &mut World,
    grid: &Grid,
    shooter: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    use crate::constants::CRIPPLING_SHOT_SLOW_DURATION;

    // Get shooter position and stats
    let (start_x, start_y) = match world.get::<&Position>(shooter) {
        Ok(pos) => (pos.x, pos.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Check for and consume arrow from inventory
    if let Ok(mut inventory) = world.get::<&mut Inventory>(shooter) {
        if let Some(idx) = inventory.items.iter().position(|i| i.kind == ItemType::Arrow) {
            inventory.items.remove(idx);
        } else {
            // No arrows! Can't shoot
            return ActionResult::Blocked;
        }
    } else {
        return ActionResult::Blocked;
    }

    // Get bow stats
    let (base_damage, arrow_speed) = if let Ok(equip) = world.get::<&Equipment>(shooter) {
        match &equip.weapon {
            Some(EquippedWeapon::Ranged(bow)) => (bow.base_damage, bow.arrow_speed),
            _ => return ActionResult::Blocked, // No bow equipped
        }
    } else {
        return ActionResult::Blocked;
    };

    // Calculate damage with stats (same as regular bow shot; includes stat affixes)
    let agility = crate::queries::effective_stats(world, shooter).agility;
    let damage = base_damage + (agility - 10) / 2;

    // Calculate arrow path using Bresenham
    let path = calculate_arrow_path(start_x, start_y, target_x, target_y, arrow_speed, grid);

    // Calculate direction for sprite rotation
    let direction = {
        let dx = (target_x - start_x) as f32;
        let dy = (target_y - start_y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        (dx / len, dy / len)
    };

    // Spawn arrow projectile with on_hit_effect
    let pos = Position::new(start_x, start_y);
    let arrow = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::ARROW),
        Projectile {
            source: shooter,
            damage,
            path,
            path_index: 0,
            direction,
            spawn_time: current_time,
            finished: None,
            potion_type: None,
            on_hit_effect: Some((EffectType::Slowed, CRIPPLING_SHOT_SLOW_DURATION)),
            hit_enemy: false,
            incendiary: false,
        },
        ProjectileMarker,
    ));

    events.push(GameEvent::ProjectileSpawned {
        projectile: arrow,
        source: shooter,
    });

    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{RaisedUndead, Stats};
    use crate::events::GameEvent;

    fn spawn_caster(world: &mut World, int: i32) -> Entity {
        let pos = Position::new(5, 5);
        let mut learned = LearnedAbilities::default();
        learned.learn(AbilityType::RaiseDead);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Stats::new(10, int, 10),
            learned,
        ))
    }

    fn spawn_bones(world: &mut World, x: i32, y: i32, items: Vec<crate::components::ItemInstance>, gold: u32) -> Entity {
        let pos = Position::new(x, y);
        let mut container = Container::corpse(items, gold);
        container.is_open = true;
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::BONES_4),
            container,
        ))
    }

    /// Raise Dead respects the INT-scaled control cap: at INT 10 the cap is 1,
    /// so a second raise is refused until the first skeleton falls.
    #[test]
    fn test_raise_dead_cap_blocks_when_full() {
        let mut world = World::new();
        let mut events = EventQueue::new();

        let caster = spawn_caster(&mut world, 10);
        assert_eq!(crate::constants::raise_dead_cap(10), 1);
        let bones = spawn_bones(&mut world, 6, 5, vec![], 0);

        // One raised skeleton already up: at the cap, the channel refuses.
        let undead = world.spawn((RaisedUndead, Health::new(8)));
        assert_eq!(raised_undead_count(&world), 1);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, bones, &mut events),
            ActionResult::Invalid
        );
        assert!(world.get::<&RaiseDeadInProgress>(caster).is_err());

        // The skeleton falls -> a slot frees up and the channel starts.
        world.get::<&mut Health>(undead).unwrap().current = 0;
        assert_eq!(raised_undead_count(&world), 0);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, bones, &mut events),
            ActionResult::Completed
        );
        assert!(world.get::<&RaiseDeadInProgress>(caster).is_ok());
    }

    /// Out-of-range bones can't be raised (chebyshev range check).
    #[test]
    fn test_raise_dead_requires_range() {
        let mut world = World::new();
        let mut events = EventQueue::new();
        let caster = spawn_caster(&mut world, 16);
        let far_bones = spawn_bones(&mut world, 5 + RAISE_DEAD_RANGE + 1, 5, vec![], 0);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, far_bones, &mut events),
            ActionResult::Invalid
        );
    }

    /// Completing the channel consumes the bones, drops their loot as a
    /// ground pile on the tile, and emits SkeletonRaised so the engine spawns
    /// the companion there.
    #[test]
    fn test_complete_raise_dead_consumes_bones_and_drops_loot() {
        let mut world = World::new();
        let mut events = EventQueue::new();

        let caster = spawn_caster(&mut world, 12);
        let loot = vec![crate::components::ItemInstance::plain(ItemType::HealthPotion)];
        let bones = spawn_bones(&mut world, 6, 5, loot, 13);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, bones, &mut events),
            ActionResult::Completed
        );

        complete_raise_dead(&mut world, caster, bones, &mut events);

        // Bones consumed; channel state cleared.
        assert!(!world.contains(bones), "bones are consumed by the raise");
        assert!(world.get::<&RaiseDeadInProgress>(caster).is_err());

        // The corpse's loot survives as a ground pile on the same tile.
        let pile: Vec<(i32, i32, u32, usize)> = world
            .query::<(&Position, &Container, &crate::components::GroundItemPile)>()
            .iter()
            .map(|(_, (p, c, _))| (p.x, p.y, c.gold, c.items.len()))
            .collect();
        assert_eq!(pile, vec![(6, 5, 13, 1)], "loot drops where the bones were");

        // The engine is told to spawn the companion at the bones' tile.
        let raised = events.drain().any(|e| {
            matches!(e, GameEvent::SkeletonRaised { owner, position } if owner == caster && position == (6, 5))
        });
        assert!(raised, "SkeletonRaised event emitted for the engine");
    }

    /// Taming drops the target's `BlocksMovement` so you can walk through your
    /// own companion. That has to reach the SpatialCache too: the cache is the
    /// single source of truth for "is this tile blocked", and a tracked entity
    /// whose stale flags say it blocks will drag a phantom blocker along behind
    /// it via `update_position` on every step.
    #[test]
    fn taming_clears_the_companion_from_the_spatial_cache() {
        use crate::components::VisualPosition;
        use crate::spatial_cache::SpatialCache;

        let mut world = World::new();
        let pos = Position::new(3, 3);
        let rat = world.spawn((pos, VisualPosition::from_position(&pos), BlocksMovement));
        let player = world.spawn((Position::new(2, 3),));

        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((3, 3)), "sanity: rat blocks before taming");

        let mut events = EventQueue::new();
        complete_taming(&mut world, &mut cache, player, rat, &mut events);

        assert!(
            world.get::<&BlocksMovement>(rat).is_err(),
            "taming should drop BlocksMovement"
        );
        cache.assert_coherent_with_world(&world, "after taming");
        assert!(
            !cache.is_blocked((3, 3)),
            "the companion's tile must no longer be blocked"
        );

        // And the phantom must not follow the companion as it walks away.
        if let Ok(mut p) = world.get::<&mut Position>(rat) {
            p.x = 5;
            p.y = 5;
        }
        cache.update_position(rat, (3, 3), (5, 5));
        cache.assert_coherent_with_world(
            &world,
            "a tamed companion must not drag a blocked tile around with it",
        );
    }
}
