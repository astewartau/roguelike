//! Class abilities: blink, fireball, taming, life drain, fear, sprint,
//! barkskin, disengage, tumble, and crippling shot.

use hecs::Entity;
use crate::world::World;
use rand::Rng;

use crate::components::{
    AbilityType, BlocksMovement, Channel, ChannelEffect, ChaseAI, ClassAbility, Container,
    EffectType, Equipment, EquippedWeapon, Health, ItemType, LearnedAbilities,
    Player, Position, Projectile, ProjectileMarker,
    SecondaryAbility, Sprite, Follower, VisualPosition,
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
    (target_x, target_y): (i32, i32),
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

/// Cast a fireball at a tile: it flies there as a projectile (over anyone in
/// the way) and bursts on landing, in `detonate_fireball`. Damage is fixed at
/// launch from the caster's effective INT (gear counts).
pub fn apply_fireball(
    world: &mut World,
    grid: &Grid,
    caster: Entity,
    (target_x, target_y): (i32, i32),
    events: &mut EventQueue,
    rng: &mut impl Rng,
    current_time: f32,
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

    let damage = ((FIREBALL_DAMAGE as f32 * queries::int_power(world, caster)).round() as i32).max(1);

    let path = super::calculate_throw_path(caster_pos.0, caster_pos.1, target_x, target_y, FIREBALL_SPEED);
    if path.is_empty() {
        // Cast at your own feet: nothing to fly, it goes off where you stand.
        detonate_fireball(world, grid, caster, caster_pos, damage, events, rng);
        events.push(GameEvent::FireballExplosion {
            x: caster_pos.0,
            y: caster_pos.1,
            radius: FIREBALL_RADIUS,
        });
        return ActionResult::Completed;
    }

    let (dx, dy) = ((target_x - caster_pos.0) as f32, (target_y - caster_pos.1) as f32);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let pos = Position::new(caster_pos.0, caster_pos.1);
    let fireball = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::FIREBALL_FLIGHT),
        Projectile {
            source: caster,
            damage,
            path,
            path_index: 0,
            direction: (dx / len, dy / len),
            spawn_time: current_time,
            finished: None,
            potion_type: None,
            on_hit_effect: None,
            hit_enemy: false,
            incendiary: false,
            kind: crate::components::ProjectileKind::Fireball,
            speed: FIREBALL_SPEED,
        },
        ProjectileMarker,
    ));
    events.push(GameEvent::ProjectileSpawned { projectile: fireball, source: caster });

    ActionResult::Completed
}

/// A fireball bursts at (x, y): `damage` to everything attackable within
/// `FIREBALL_RADIUS`, and a noise that wakes the area. The blast you see and
/// hear (`GameEvent::FireballExplosion`) is sent once the fireball visibly
/// gets there, not from here (see `systems::projectile`).
pub fn detonate_fireball(
    world: &mut World,
    grid: &Grid,
    caster: Entity,
    (target_x, target_y): (i32, i32),
    damage: i32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    // A fireball is very loud — wake sleeping enemies over a wide area.
    crate::systems::noise::make_noise(world, grid, (target_x, target_y), EXPLOSION_NOISE_RADIUS);

    // Collect all attackable entities in radius
    let mut damaged: Vec<(Entity, i32, i32)> = Vec::new();
    for (id, (pos, _)) in world.query::<(&Position, &Health)>().iter() {
        let dx = (pos.x - target_x).abs();
        let dy = (pos.y - target_y).abs();
        if dx <= FIREBALL_RADIUS && dy <= FIREBALL_RADIUS {
            damaged.push((id, pos.x, pos.y));
        }
    }

    // Apply damage to all
    for (entity, x, y) in damaged {
        // Apply damage (handles invulnerability, armor defense, Protected/Barkskin)
        crate::systems::combat::apply_damage(world, grid, entity, damage, rng, events);
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
    if !crate::systems::targets::can_be_drained(world, caster, target) {
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

    // Ticks on the first Wait (the interval is one Wait long).
    let _ = world.insert_one(caster, Channel {
        effect: ChannelEffect::LifeDrain { since_tick: 0.0 },
        target: Some(target),
        elapsed: 0.0,
        required: f32::INFINITY,
        range: LIFE_DRAIN_RANGE,
        breaks_on_move: false,
        breaks_on_damage: true,
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
        .without::<&Follower>()
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

/// Apply wait action: a Wait is also what keeps a channel going.
///
/// Takes the full effect context because completing a tame drops the target's
/// `BlocksMovement`, which has to be mirrored into the `SpatialCache`.
pub fn apply_wait(ctx: &mut EffectCtx, entity: Entity) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &**grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    advance_channel(world, grid, spatial_cache, entity, ACTION_WAIT_DURATION, events, rng);

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
    current_time: f32,
) -> ActionResult {
    use crate::systems::effects::{add_effect_to_entity, apply_effect_to_visible_enemies};

    let power = queries::int_power(world, caster);

    let result = match ability {
        AbilityType::LearnedBlink => {
            apply_blink(world, grid, caster, (target_x, target_y), spatial_cache, events, rng)
        }
        AbilityType::LearnedFireball => {
            apply_fireball(world, grid, caster, (target_x, target_y), events, rng, current_time)
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

/// How many living things `by` has summoned (raised skeletons lose `Health`
/// when they die and turn into bones, so this counts only active ones).
pub fn summoned_count(world: &World, by: Entity) -> usize {
    world
        .query::<(&crate::components::Summoned, &Health)>()
        .iter()
        .filter(|(_, (s, h))| s.by == by && h.current > 0)
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
    if !crate::systems::targets::is_remains(world, target) {
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
    if summoned_count(world, caster) >= cap {
        return ActionResult::Invalid;
    }

    let _ = world.insert_one(caster, Channel {
        effect: ChannelEffect::RaiseDead,
        target: Some(target),
        elapsed: 0.0,
        required: RAISE_DEAD_CHANNEL_DURATION,
        range: RAISE_DEAD_RANGE,
        breaks_on_move: true,
        breaks_on_damage: false,
    });

    // Start the long cooldown (lives in the caster's spell list)
    if let Ok(mut learned) = world.get::<&mut LearnedAbilities>(caster) {
        learned.start_cooldown(AbilityType::RaiseDead);
    }

    events.push(GameEvent::RaiseDeadStarted { caster, target });

    ActionResult::Completed
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
    let Some((x, y)) = queries::get_entity_position(world, target) else {
        events.push(GameEvent::RaiseDeadFailed { caster });
        return;
    };

    consume_corpse(world, target, (x, y));

    events.push(GameEvent::SkeletonRaised { owner: caster, position: (x, y) });
}

/// Consume a corpse (bones container) at `pos`: despawn it, and drop anything
/// it held onto the same tile so no loot is destroyed. Shared by Raise Dead
/// and Corpse Explosion. Corpses never block movement, so there is nothing to
/// release in the spatial cache.
pub(crate) fn consume_corpse(world: &mut World, corpse: Entity, pos: (i32, i32)) {
    let (x, y) = pos;
    // Take the loot out of the bones (so it isn't despawned with them), then
    // consume them.
    let (items, gold) = world
        .get::<&mut Container>(corpse)
        .map(|mut c| (std::mem::take(&mut c.items), std::mem::take(&mut c.gold)))
        .unwrap_or((Vec::new(), 0));
    let _ = world.despawn(corpse);

    // Anything the corpse held drops onto the same tile.
    for item in items {
        crate::systems::held::put_down(world, item, x, y);
    }
    crate::systems::inventory::drop_gold(world, x, y, gold);
}

/// One pulse of Life Drain: damage the target and heal by part of it.
/// Returns whether the target died of it.
fn drain_tick(
    world: &mut World,
    grid: &Grid,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> bool {
    let (Some((cx, cy)), Some((tx, ty))) =
        (queries::get_entity_position(world, caster), queries::get_entity_position(world, target))
    else {
        return false;
    };
    {
        // Time to tick! Per-tick damage (and thus the heal derived from it)
        // scales with the caster's effective INT — gear counts.
        let damage = ((LIFE_DRAIN_DAMAGE_PER_TICK as f32 * queries::int_power(world, caster))
            .round() as i32)
            .max(1);

        // Apply damage to target (handles invulnerability, armor defense, Protected/Barkskin)
        crate::systems::combat::apply_damage(world, grid, target, damage, rng, events);
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

        target_died
    }
}

/// Keep a channel going for `step` seconds of focus: check its target is
/// still there and in reach, then advance it, finishing it if it's done.
fn advance_channel(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    entity: Entity,
    step: f32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    let Some(channel) = world.get::<&Channel>(entity).ok().map(|c| *c) else {
        return;
    };
    if let Some(target) = channel.target {
        let in_reach = match (queries::get_entity_position(world, entity), queries::get_entity_position(world, target)) {
            (Some((ex, ey)), Some((tx, ty))) => (ex - tx).abs().max((ey - ty).abs()) <= channel.range,
            _ => false,
        };
        if !in_reach || !channel_target_still_fits(world, &channel, target) {
            end_channel(world, entity, events, false);
            return;
        }
    }

    let elapsed = channel.elapsed + step;
    match channel.effect {
        ChannelEffect::LifeDrain { since_tick } => {
            let Some(target) = channel.target else {
                end_channel(world, entity, events, false);
                return;
            };
            let mut since_tick = since_tick + step;
            if since_tick >= LIFE_DRAIN_TICK_INTERVAL {
                since_tick = 0.0;
                if drain_tick(world, grid, entity, target, events, rng) {
                    end_channel(world, entity, events, false);
                    return;
                }
            }
            if let Ok(mut c) = world.get::<&mut Channel>(entity) {
                c.elapsed = elapsed;
                c.effect = ChannelEffect::LifeDrain { since_tick };
            }
        }
        _ if elapsed >= channel.required => {
            let _ = world.remove_one::<Channel>(entity);
            finish_channel(world, grid, spatial_cache, entity, &channel, events);
        }
        effect => {
            if let Ok(mut c) = world.get::<&mut Channel>(entity) {
                c.elapsed = elapsed;
            }
            if let (ChannelEffect::Tame, Some(target)) = (effect, channel.target) {
                events.push(GameEvent::TamingProgress {
                    tamer: entity,
                    target,
                    progress: elapsed,
                    required: channel.required,
                });
            }
        }
    }
}

/// Whether the target is still the kind of thing the channel works on: a
/// creature for a tame or a drain, remains for raising the dead.
fn channel_target_still_fits(world: &World, channel: &Channel, target: Entity) -> bool {
    match channel.effect {
        // A creature that died mid-channel isn't tamed as a corpse.
        ChannelEffect::Tame => {
            crate::systems::targets::can_be_tamed(world, target)
        }
        ChannelEffect::RaiseDead => crate::systems::targets::is_remains(world, target),
        ChannelEffect::LifeDrain { .. } => world.get::<&Health>(target).is_ok_and(|h| h.current > 0),
        ChannelEffect::Shout { .. } => true,
    }
}

/// A channel reached its full time: do what it was for.
fn finish_channel(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    entity: Entity,
    channel: &Channel,
    events: &mut EventQueue,
) {
    match (channel.effect, channel.target) {
        (ChannelEffect::Tame, Some(target)) => complete_taming(world, spatial_cache, entity, target, events),
        (ChannelEffect::RaiseDead, Some(target)) => complete_raise_dead(world, entity, target, events),
        (ChannelEffect::Shout { radius }, _) => {
            if let Some(pos) = queries::get_entity_position(world, entity) {
                crate::systems::noise::make_noise(world, grid, pos, radius);
            }
        }
        _ => {}
    }
}

/// Stop a channel short. `broken` is for being interrupted (moving, a hit)
/// rather than running out (target gone, out of reach, drained dry).
fn end_channel(world: &mut World, entity: Entity, events: &mut EventQueue, broken: bool) {
    let Ok(channel) = world.remove_one::<Channel>(entity) else {
        return;
    };
    match (channel.effect, channel.target) {
        (ChannelEffect::Tame, Some(target)) => events.push(GameEvent::TamingFailed { tamer: entity, target }),
        (ChannelEffect::RaiseDead, _) => events.push(GameEvent::RaiseDeadFailed { caster: entity }),
        (ChannelEffect::LifeDrain { .. }, Some(target)) => {
            if let Ok(mut ability) = world.get::<&mut ClassAbility>(entity) {
                ability.start_cooldown();
            }
            events.push(if broken {
                GameEvent::LifeDrainInterrupted { caster: entity, target }
            } else {
                GameEvent::LifeDrainEnded { caster: entity, target }
            });
        }
        _ => {}
    }
}

/// `entity` stepped away: a channel that needs it to stand still is broken.
pub fn break_channel_on_move(world: &mut World, entity: Entity, events: &mut EventQueue) {
    if world.get::<&Channel>(entity).is_ok_and(|c| c.breaks_on_move) {
        end_channel(world, entity, events, true);
    }
}

/// `entity` took damage: a channel a hit breaks is broken. Called from the
/// damage chokepoint, so every source of harm counts.
pub fn break_channel_on_damage(world: &mut World, entity: Entity, events: &mut EventQueue) {
    if world.get::<&Channel>(entity).is_ok_and(|c| c.breaks_on_damage) {
        end_channel(world, entity, events, true);
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
    use crate::components::{CompanionAI, Follower};

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
    let _ = world.insert_one(target, Follower { leader: tamer });
    let _ = world.insert_one(target, CompanionAI::new());

    // It's yours now: no longer something to tame, awake (it was asleep for
    // good otherwise: noise and blows only wake hostile minds), and whatever
    // it was in the middle of (a swing at you) is dropped, so its first act
    // as a companion is its own choice.
    let _ = world.remove_one::<crate::components::Tameable>(target);
    let _ = world.remove_one::<crate::components::Asleep>(target);
    let _ = world.insert_one(target, crate::components::PendingInterrupt);

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
    if !crate::systems::targets::can_be_tamed(world, target) {
        return ActionResult::Invalid;
    }

    let _ = world.insert_one(tamer, Channel {
        effect: ChannelEffect::Tame,
        target: Some(target),
        elapsed: 0.0,
        required: TAME_DURATION,
        range: TAME_RANGE,
        breaks_on_move: true,
        breaks_on_damage: false,
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
    for (_, (enemy_pos, _)) in world.query::<(&Position, &Health)>().iter() {
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
    (target_x, target_y): (i32, i32),
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

    // Apply invulnerability effect (before landing: the roll protects from
    // damage, not from what the tile itself does to you)
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
    if crate::systems::inventory::use_up_first_of(world, shooter, &[ItemType::Arrow]).is_none() {
        // No arrows! Can't shoot
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
            kind: crate::components::ProjectileKind::Missile,
            speed: arrow_speed,
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
    use crate::components::{Stats, Summoned};
    use crate::events::GameEvent;

    /// Blinking onto a web roots you, the same as walking into it would.
    #[test]
    fn test_blink_landing_triggers_the_tile() {
        use rand::SeedableRng;
        let mut world = World::new();
        let grid = Grid::filled(12, 12, crate::tile::TileType::Floor);
        let caster = spawn_caster(&mut world, 10);
        world.insert_one(caster, crate::components::StatusEffects::new()).unwrap();
        crate::spawning::spawn_web(&mut world, 7, 5, None);
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);

        let result = apply_blink(&mut world, &grid, caster, (7, 5), &mut cache, &mut events, &mut rng);
        assert_eq!(result, ActionResult::Completed);
        assert!(
            effects::entity_has_effect(&world, caster, EffectType::Rooted),
            "blinking into a web still gets you stuck"
        );
    }

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
        let items = crate::systems::held::spawn_items(world, items);
        let mut container = Container::corpse(items, gold);
        container.is_open = true;
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::BONES_4),
            container,
            crate::components::Raisable,
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
        let undead = world.spawn((Summoned { by: caster }, Health::new(8)));
        assert_eq!(summoned_count(&world, caster), 1);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, bones, &mut events),
            ActionResult::Invalid
        );
        assert!(world.get::<&Channel>(caster).is_err());

        // The skeleton falls -> a slot frees up and the channel starts.
        world.get::<&mut Health>(undead).unwrap().current = 0;
        assert_eq!(summoned_count(&world, caster), 0);
        assert_eq!(
            apply_start_raise_dead(&mut world, caster, bones, &mut events),
            ActionResult::Completed
        );
        assert!(world.get::<&Channel>(caster).is_ok_and(|c| c.effect == ChannelEffect::RaiseDead));
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

        // Keep waiting until the channel completes.
        let grid = Grid::filled(12, 12, crate::tile::TileType::Floor);
        let mut spatial = SpatialCache::rebuild_from_world(&world);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        let waits = (RAISE_DEAD_CHANNEL_DURATION / ACTION_WAIT_DURATION).ceil() as usize;
        for _ in 0..waits {
            advance_channel(&mut world, &grid, &mut spatial, caster, ACTION_WAIT_DURATION, &mut events, &mut rng);
        }

        // Bones consumed; channel state cleared.
        assert!(!world.contains(bones), "bones are consumed by the raise");
        assert!(world.get::<&Channel>(caster).is_err());

        // The corpse's loot survives on the same tile: its item lying
        // there, its gold as coins.
        assert_eq!(crate::systems::held::on_ground_at(&world, (6, 5)).len(), 1, "loot drops where the bones were");
        let coins: Vec<(i32, i32, u32)> = world
            .query::<(&Position, &Container)>()
            .iter()
            .map(|(_, (p, c))| (p.x, p.y, c.gold))
            .collect();
        assert_eq!(coins, vec![(6, 5, 13)], "and its gold");

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

    #[test]
    fn a_hit_breaks_only_the_channels_a_hit_breaks() {
        use crate::components::{StatusEffects};
        let mut world = World::new();
        let mut events = EventQueue::new();
        let grid = Grid::filled(8, 8, crate::tile::TileType::Floor);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(5);
        let channel = |effect, breaks_on_damage| Channel {
            effect,
            target: None,
            elapsed: 0.0,
            required: 3.0,
            range: 0,
            breaks_on_move: true,
            breaks_on_damage,
        };
        let body = || (Position::new(2, 2), Health::new(50), StatusEffects::new());
        let shouter = world.spawn(body());
        world.insert_one(shouter, channel(ChannelEffect::Shout { radius: 5 }, true)).unwrap();
        let tamer = world.spawn(body());
        world.insert_one(tamer, channel(ChannelEffect::Tame, false)).unwrap();

        for who in [shouter, tamer] {
            crate::systems::combat::apply_damage(&mut world, &grid, who, 3, &mut rng, &mut events);
        }
        assert!(world.get::<&Channel>(shouter).is_err(), "a hit stops a shout");
        assert!(world.get::<&Channel>(tamer).is_ok(), "taming shrugs off a hit");

        break_channel_on_move(&mut world, tamer, &mut events);
        assert!(world.get::<&Channel>(tamer).is_err(), "but not a step");
    }

    /// A tamed creature wakes, stops being a tame target, and drops whatever
    /// it was doing; and one that dies mid-channel isn't tamed as a corpse.
    #[test]
    fn taming_wakes_it_and_a_corpse_cannot_be_tamed() {
        use crate::components::{Asleep, PendingInterrupt, StatusEffects, Tameable};
        let grid = Grid::filled(10, 10, crate::tile::TileType::Floor);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(9);
        let tame = |world: &mut World, events: &mut EventQueue, rng: &mut rand::rngs::StdRng, tamer, rat| {
            assert_eq!(apply_start_taming(world, tamer, rat, events), ActionResult::Completed);
            let mut spatial = SpatialCache::rebuild_from_world(world);
            let waits = (TAME_DURATION / ACTION_WAIT_DURATION).ceil() as usize;
            for _ in 0..waits {
                advance_channel(world, &grid, &mut spatial, tamer, ACTION_WAIT_DURATION, events, rng);
            }
        };

        let mut world = World::new();
        let mut events = EventQueue::new();
        let druid = world.spawn((Position::new(2, 2), Health::new(20)));
        let rat = world.spawn((Position::new(3, 2), Health::new(5), StatusEffects::new(), Tameable, Asleep));
        tame(&mut world, &mut events, &mut rng, druid, rat);
        assert!(world.get::<&Follower>(rat).is_ok_and(|f| f.leader == druid), "tamed");
        assert!(world.get::<&Asleep>(rat).is_err(), "awake");
        assert!(world.get::<&Tameable>(rat).is_err(), "no longer a tame target");
        assert!(world.get::<&PendingInterrupt>(rat).is_ok(), "drops what it was doing");

        // Dies mid-channel: the tame fails.
        let other = world.spawn((Position::new(2, 3), Health::new(5), ChaseAI::new(4), Tameable));
        assert_eq!(apply_start_taming(&mut world, druid, other, &mut events), ActionResult::Completed);
        world.get::<&mut Health>(other).unwrap().current = 0;
        let mut spatial = SpatialCache::rebuild_from_world(&world);
        advance_channel(&mut world, &grid, &mut spatial, druid, ACTION_WAIT_DURATION, &mut events, &mut rng);
        assert!(world.get::<&Follower>(other).is_err(), "a corpse isn't tamed");
        assert!(world.get::<&Channel>(druid).is_err(), "the channel ends");
    }
}

