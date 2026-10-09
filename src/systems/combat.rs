//! Combat system functions.

use crate::components::{
    Actor, Affix, Attackable, BlocksMovement, ChaseAI, CompanionAI, Container, Door, EffectType,
    Equipment, Experience, Health, ItemInstance, ItemType, Position, Sprite, Stats, VisualPosition,
    Weapon,
};
use crate::constants::*;
use crate::engine::{ActorCtx, EffectCtx};
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::systems::experience::{calculate_xp_value, grant_xp};
use crate::tile::tile_ids;
use hecs::{Entity, World};
use rand::Rng;

/// Calculate total damage for a weapon
pub fn weapon_damage(weapon: &Weapon) -> i32 {
    weapon.base_damage + weapon.damage_bonus
}

/// Amount healed by an `OnHitLifesteal` affix for a given hit.
/// Always at least 1 when any damage was dealt.
pub fn lifesteal_heal(damage: i32, fraction: f32) -> i32 {
    if damage <= 0 {
        return 0;
    }
    ((damage as f32 * fraction).round() as i32).max(1)
}

/// Attacker-side damage multiplier from conditional states:
/// - `LowHealthDamage` weapon affix: bonus damage while the attacker is below
///   the low-health threshold;
/// - Tired (player-only `Fatigue` meter above the threshold): -10% damage.
///
/// Multiply raw weapon damage by this before `apply_damage`.
pub fn attacker_conditional_damage_mult(world: &World, attacker: Entity) -> f32 {
    let mut mult = 1.0;

    // Tired attackers swing softer (player-only; enemies have no Fatigue).
    let tired = world
        .get::<&crate::components::Fatigue>(attacker)
        .map(|f| f.is_tired())
        .unwrap_or(false);
    if tired {
        mult *= TIRED_DAMAGE_MULT;
    }

    let low_health = world
        .get::<&Health>(attacker)
        .map(|h| (h.current as f32) < (h.max as f32 * LOW_HEALTH_DAMAGE_THRESHOLD))
        .unwrap_or(false);
    if low_health {
        if let Ok(equipment) = world.get::<&Equipment>(attacker) {
            if let Some(source) = equipment.weapon_source.as_ref() {
                for affix in &source.affixes {
                    if let Affix::LowHealthDamage(bonus) = affix {
                        mult += bonus;
                    }
                }
            }
        }
    }
    mult
}

/// Noise radius of an attack made by `attacker`: the base radius, doubled if
/// any worn gear carries the `CursedLoud` affix. Curses apply while equipped
/// whether or not the item has been identified.
pub fn attack_noise_radius(world: &World, attacker: Entity, base: i32) -> i32 {
    let loud = world
        .get::<&Equipment>(attacker)
        .map(|e| e.has_cursed_loud())
        .unwrap_or(false);
    if loud {
        base * 2
    } else {
        base
    }
}

/// Resolve all weapon on-hit affix components after a successful hit.
///
/// This is the single chokepoint for on-hit triggers: the melee path
/// (`actions::apply_attack` / `apply_cleave`) and the projectile hit path
/// (`projectile::update_projectiles`) both route through here — do NOT
/// re-implement per-ability. No-op for attackers without a `weapon_source`
/// (enemy claws/bows are not item instances).
pub fn resolve_weapon_on_hit(
    ctx: &mut EffectCtx,
    attacker: Entity,
    target: Entity,
    damage: i32,
) {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    if damage <= 0 {
        return;
    }

    let affixes: Vec<Affix> = match world.get::<&Equipment>(attacker) {
        Ok(e) => match e.weapon_source.as_ref() {
            Some(source) => source.affixes.clone(),
            None => return,
        },
        Err(_) => return,
    };
    if affixes.is_empty() {
        return;
    }

    let target_died = world
        .get::<&Health>(target)
        .map(|h| h.current <= 0)
        .unwrap_or(true);


    for affix in &affixes {
        match affix {
            // Same Burning status the fire system applies (see
            // fire::try_combat_ignite); the fire system then handles
            // spread, grass ignition, and dousing.
            Affix::OnHitIgnite(chance) if !target_died && rng.gen::<f32>() < *chance => {
                // A Wet target refuses the fire (see effects::add_effect).
                let ignited = crate::systems::effects::add_effect_to_entity(
                    world,
                    target,
                    EffectType::Burning,
                    BURNING_DURATION,
                );
                if let Some(pos) = crate::queries::get_entity_position(world, target).filter(|_| ignited) {
                    events.push(GameEvent::CaughtFire { entity: target, position: pos });
                }
            }
            Affix::OnHitSlow(chance) if !target_died && rng.gen::<f32>() < *chance => {
                crate::systems::effects::add_effect_to_entity(
                    world,
                    target,
                    EffectType::Slowed,
                    ON_HIT_SLOW_DURATION,
                );
            }
            Affix::OnHitFear(chance) if !target_died && rng.gen::<f32>() < *chance => {
                crate::systems::effects::add_effect_to_entity(
                    world,
                    target,
                    EffectType::Feared,
                    ON_HIT_FEAR_DURATION,
                );
            }
            Affix::OnHitLifesteal(fraction) => {
                heal_entity(world, attacker, lifesteal_heal(damage, *fraction));
            }
            Affix::OnHitKnockback if !target_died => {
                try_knockback(world, grid, spatial_cache, attacker, target, events, rng);
            }
            Affix::KillHeal(amount) if target_died => {
                heal_entity(world, attacker, *amount);
            }
            // Attacker-side conditional, applied pre-damage via
            // attacker_conditional_damage_mult.
            Affix::LowHealthDamage(_) => {}
            // Stat / flat / curse affixes have no on-hit trigger.
            _ => {}
        }
    }
}

/// Heal an entity, clamped to its max health.
fn heal_entity(world: &mut World, entity: Entity, amount: i32) {
    if amount <= 0 {
        return;
    }
    if let Ok(mut health) = world.get::<&mut Health>(entity) {
        if health.current > 0 {
            health.current = (health.current + amount).min(health.max);
        }
    }
}

/// Push `target` one tile directly away from `attacker` if the destination
/// tile is walkable and unoccupied. Updates the spatial cache and emits an
/// `EntityMoved` event so downstream systems stay consistent.
fn try_knockback(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    attacker: Entity,
    target: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    let Some((ax, ay)) = crate::queries::get_entity_position(world, attacker) else {
        return;
    };
    let Some((tx, ty)) = crate::queries::get_entity_position(world, target) else {
        return;
    };

    let dx = (tx - ax).signum();
    let dy = (ty - ay).signum();
    if dx == 0 && dy == 0 {
        return;
    }

    let dest = (tx + dx, ty + dy);
    if !grid.is_walkable(dest.0, dest.1) {
        return;
    }
    if crate::queries::is_position_blocked(spatial_cache, dest.0, dest.1, Some(target)) {
        return;
    }

    if let Ok(mut pos) = world.get::<&mut Position>(target) {
        pos.x = dest.0;
        pos.y = dest.1;
    } else {
        return;
    }
    // Snap the visual so the shove reads as an impact rather than a stroll.
    if let Ok(mut vis) = world.get::<&mut VisualPosition>(target) {
        vis.x = dest.0 as f32;
        vis.y = dest.1 as f32;
    }
    spatial_cache.update_position(target, (tx, ty), dest);
    events.push(GameEvent::EntityMoved {
        entity: target,
        from: (tx, ty),
        to: dest,
    });

    // Whatever the victim lands in acts on them: water, oil, fire, traps.
    crate::systems::tile_effects::on_enter_tile(world, grid, target, dest, events, rng);

    // Knocked into a lit brazier? It topples onto the victim, spilling fire
    // over the tile they just landed on (see systems::fire::topple_brazier).
    let brazier_hit: Option<Entity> = world
        .query::<(&Position, &crate::components::Brazier)>()
        .iter()
        .find(|(_, (p, b))| b.lit && p.x == dest.0 && p.y == dest.1)
        .map(|(id, _)| id);
    if let Some(brazier) = brazier_hit {
        crate::systems::fire::topple_brazier(world, grid, brazier, events, rng);
    }
}

/// Apply `raw` incoming damage to `target`, accounting for invulnerability,
/// equipped armor defense, and Protected/Barkskin reduction. Returns the
/// damage actually dealt (0 if the target was invulnerable).
///
/// This is the single chokepoint for all damage so defense applies uniformly
/// regardless of the damage source (melee, ranged, traps, spells). Attacker-side
/// modifiers (crit, variance, Strengthened) are applied by the caller before
/// passing `raw` in.
pub fn apply_damage(
    world: &mut World,
    target: Entity,
    raw: i32,
    rng: &mut impl Rng,
    events: &mut crate::events::EventQueue,
) -> i32 {
    apply_damage_inner(world, target, raw, false, rng, events)
}

/// Apply one damage-over-time tick (Poisoned, Bleeding) to `target`.
///
/// DoTs are not blows, so compared with [`apply_damage`] they **bypass armor**
/// (venom and open wounds are not stopped by a breastplate), get no sneak
/// multiplier, do not spend a Bone Ward charge (the ward absorbs *hits*), and
/// make no combat noise. Invulnerability and Protected/Barkskin still apply,
/// as do waking an unaware victim and the morale check.
pub fn apply_damage_dot(
    world: &mut World,
    target: Entity,
    raw: i32,
    rng: &mut impl Rng,
    events: &mut crate::events::EventQueue,
) -> i32 {
    apply_damage_inner(world, target, raw, true, rng, events)
}

fn apply_damage_inner(
    world: &mut World,
    target: Entity,
    raw: i32,
    dot: bool,
    rng: &mut impl Rng,
    events: &mut crate::events::EventQueue,
) -> i32 {
    // Invulnerable negates all damage.
    if crate::queries::has_status_effect(world, target, EffectType::Invulnerable) {
        return 0;
    }

    // A Bone Ward swallows the whole hit and spends a charge.
    if !dot && absorb_with_bone_ward(world, target, events) {
        return 0;
    }

    // Sneak attack: an unaware target takes extra damage from this hit.
    // Symmetric for the player: while asleep (the `Asleep` marker the sleep
    // action adds) the player counts as unaware and eats the same multiplier.
    let unaware = !dot
        && (world
            .get::<&ChaseAI>(target)
            .map(|ai| ai.state == crate::components::AIState::Unaware)
            .unwrap_or(false)
            || world.get::<&crate::components::Asleep>(target).is_ok());
    let mut dmg = if unaware {
        (raw as f32 * SNEAK_ATTACK_MULT) as i32
    } else {
        raw
    };

    // Flat armor reduction (0 for entities without armor). DoTs bypass it.
    if !dot {
        let defense = world
            .get::<&Equipment>(target)
            .map(|e| e.total_defense())
            .unwrap_or(0);
        dmg -= defense;
    }

    // Multiplicative damage reduction from Protected / Barkskin.
    if crate::queries::has_status_effect(world, target, EffectType::Protected)
        || crate::queries::has_status_effect(world, target, EffectType::Barkskin)
    {
        dmg = (dmg as f32 * PROTECTION_DAMAGE_REDUCTION) as i32;
    }

    dmg = dmg.max(1);

    let pos = world.get::<&Position>(target).map(|p| (p.x, p.y)).ok();
    let mut hp_after = (0, 1);
    if let Ok(mut health) = world.get::<&mut Health>(target) {
        health.current -= dmg;
        hp_after = (health.current, health.max.max(1));
    }

    // Taking a hit cancels any alarm shout and wakes an unaware victim.
    crate::systems::ai::interrupt_shout_on_damage(world, target);
    crate::systems::ai::wake_on_attacked(world, target);

    // Morale: a surviving enemy that drops below the HP threshold may panic.
    // Bosses (FearImmune) never break.
    if hp_after.0 > 0
        && world.get::<&ChaseAI>(target).is_ok()
        && world.get::<&crate::components::FearImmune>(target).is_err()
    {
        let frac = hp_after.0 as f32 / hp_after.1 as f32;
        if frac < MORALE_HP_THRESHOLD && rng.gen_bool(MORALE_FLEE_CHANCE) {
            crate::systems::effects::add_effect_to_entity(
                world,
                target,
                EffectType::Feared,
                MORALE_FEAR_DURATION,
            );
        }
    }

    // Combat is loud: wake nearby sleeping enemies that "hear" the impact.
    // A DoT tick is silent.
    if let Some(pos) = pos.filter(|_| !dot) {
        crate::systems::ai::wake_enemies_in_radius(world, pos, MELEE_NOISE_RADIUS);
    }

    dmg
}

/// Spend one Bone Ward charge on an incoming hit, if `target` has a live ward.
/// Returns true if the hit was absorbed. The ward needs both the charge
/// component and its (timed) status effect: when the effect expires the
/// leftover charges are discarded here rather than lingering.
fn absorb_with_bone_ward(
    world: &mut World,
    target: Entity,
    events: &mut crate::events::EventQueue,
) -> bool {
    let Some(charges) = world.get::<&crate::components::BoneWard>(target).ok().map(|w| w.charges)
    else {
        return false;
    };
    if charges == 0 || !crate::queries::has_status_effect(world, target, EffectType::BoneWard) {
        let _ = world.remove_one::<crate::components::BoneWard>(target);
        return false;
    }
    let charges_left = charges - 1;
    if charges_left == 0 {
        let _ = world.remove_one::<crate::components::BoneWard>(target);
        crate::systems::effects::remove_effect_from_entity(world, target, EffectType::BoneWard);
    } else if let Ok(mut ward) = world.get::<&mut crate::components::BoneWard>(target) {
        ward.charges = charges_left;
    }
    let position = world
        .get::<&Position>(target)
        .map(|p| (p.x as f32 + 0.5, p.y as f32 + 0.5))
        .unwrap_or((0.0, 0.0));
    events.push(crate::events::GameEvent::BoneWardAbsorbed {
        entity: target,
        charges_left,
        position,
    });
    true
}

/// Handle a ContainerOpened event - update sprite for containers
pub fn handle_container_opened(world: &mut World, container_id: Entity) {
    if let Ok(mut sprite) = world.get::<&mut Sprite>(container_id) {
        let current = (sprite.sheet, sprite.tile_id);

        // Handle chest opening
        if current == tile_ids::CHEST_CLOSED {
            sprite.sheet = tile_ids::CHEST_OPEN.0;
            sprite.tile_id = tile_ids::CHEST_OPEN.1;
        }
        // Handle coffin opening
        else if current == tile_ids::COFFIN_CLOSED {
            sprite.sheet = tile_ids::COFFIN_OPEN.0;
            sprite.tile_id = tile_ids::COFFIN_OPEN.1;
        }
        // Barrels don't change sprite when opened (they stay as barrel sprite)
    }
}

/// Handle a DoorOpened event - update sprite to the door's open sprite
pub fn handle_door_opened(world: &mut World, door_id: Entity) {
    // Get the door's open_sprite first
    let open_sprite = if let Ok(door) = world.get::<&Door>(door_id) {
        Some(door.open_sprite)
    } else {
        None
    };

    // Then update the sprite
    if let Some((sheet, tile_id)) = open_sprite {
        if let Ok(mut sprite) = world.get::<&mut Sprite>(door_id) {
            sprite.sheet = sheet;
            sprite.tile_id = tile_id;
        }
    }
}

/// Handle a DoorClosed event - update sprite to the door's closed sprite
pub fn handle_door_closed(world: &mut World, door_id: Entity) {
    // Get the door's closed_sprite first
    let closed_sprite = if let Ok(door) = world.get::<&Door>(door_id) {
        Some(door.closed_sprite)
    } else {
        None
    };

    // Then update the sprite
    if let Some((sheet, tile_id)) = closed_sprite {
        if let Ok(mut sprite) = world.get::<&mut Sprite>(door_id) {
            sprite.sheet = sheet;
            sprite.tile_id = tile_id;
        }
    }
}

/// Turn dead entities into bones (health <= 0) and grant XP to player.
/// Also cancels any pending actions for dead entities in the scheduler.
/// Returns the number of hostile enemies (ChaseAI) that died, so the engine
/// can keep the run's kill counter (companion deaths don't count).
/// `floor` scales corpse gold with depth (see `ENEMY_GOLD_PER_FLOOR`).
pub fn remove_dead_entities(ctx: &mut ActorCtx, floor: u32) -> u32 {
    let ActorCtx { world, player: player_entity, rng, events, scheduler, spatial, tracker, .. } = ctx;
    let (world, player_entity) = (&mut **world, *player_entity);
    let (rng, events) = (&mut **rng, &mut **events);
    let mut scheduler = Some(&mut **scheduler);
    let (spatial_cache, active_ai_tracker) = (&mut **spatial, &mut **tracker);

    let mut to_convert = Vec::new();
    let mut hostile_kills: u32 = 0;

    for (id, (pos, health, stats)) in world.query::<(&Position, &Health, Option<&Stats>)>().iter()
    {
        // Never convert the player into a corpse - the player keeps its Health
        // component (at <=0) so the engine can detect death and show the retry
        // screen. The dead player is handled separately by the game over flow.
        // Oil barrels are also skipped: a destroyed barrel detonates in the
        // fire system (systems::fire::tick_fire) instead of leaving bones.
        if world.entity(id).map(|e| e.has::<crate::components::OilBarrel>()).unwrap_or(false) {
            continue;
        }
        if id != player_entity && health.current <= 0 {
            // Bosses grant a bonus multiple of their (already stat-inflated) XP.
            let is_boss = world
                .entity(id)
                .map(|e| e.has::<crate::components::Boss>())
                .unwrap_or(false);
            let mut xp = calculate_xp_value(stats) * if is_boss { BOSS_XP_MULT } else { 1 };
            // Split slimes pay half per generation, so the two halves of a
            // split together are worth what the original was.
            let split_generation = world
                .get::<&crate::components::Splits>(id)
                .map(|s| s.generation)
                .unwrap_or(0);
            if split_generation > 0 {
                xp = (xp >> split_generation.min(31)).max(1);
            }
            // Kill counter: hostile enemies only (companions carry
            // CompanionAI instead of ChaseAI and don't count).
            if world.entity(id).map(|e| e.has::<ChaseAI>()).unwrap_or(false) {
                hostile_kills += 1;
            }
            to_convert.push((id, (pos.x as f32 + 0.5, pos.y as f32 + 0.5), xp));
        }
    }

    // Grant XP to player
    let total_xp: u32 = to_convert.iter().map(|(_, _, xp)| xp).sum();
    if total_xp > 0 {
        if let Ok(mut exp) = world.get::<&mut Experience>(player_entity) {
            let leveled_up = grant_xp(&mut exp, total_xp);
            if leveled_up {
                events.push(GameEvent::LevelUp {
                    new_level: exp.level,
                });
            }
        }
    }

    // Remember where enemies fell, for the ally-death morale check below.
    let death_positions: Vec<(i32, i32)> = to_convert
        .iter()
        .map(|(_, p, _)| (p.0 as i32, p.1 as i32))
        .collect();

    for (id, position, _xp) in to_convert {
        // Cancel any pending actions for this entity
        if let Some(ref mut sched) = scheduler {
            sched.cancel_for_entity(id);
        }

        // Boss deaths are floor milestones: flourish message via a dedicated
        // event (fired before EntityDied so the log reads slain-then-eulogy).
        let boss_name: Option<String> = if world
            .entity(id)
            .map(|e| e.has::<crate::components::Boss>())
            .unwrap_or(false)
        {
            world
                .get::<&crate::components::Name>(id)
                .map(|n| n.0.clone())
                .ok()
        } else {
            None
        };

        // Emit death event
        events.push(GameEvent::EntityDied {
            entity: id,
            position,
        });
        if let Some(name) = boss_name {
            events.push(GameEvent::BossDefeated { name });
        }

        // Remove from spatial cache before removing components
        spatial_cache.remove_entity(id);

        // Death is where an entity leaves the AI sets: ChaseAI/CompanionAI are
        // stripped just below, so update_on_player_move will never re-add it.
        // Without this the id lingers in active_entities/dormant_entities until
        // the next rebuild — and once the corpse is despawned (bones consumed by
        // Raise Dead) it is a stale id in both sets.
        active_ai_tracker.remove_entity(id);

        // Remove AI, Actor, Attackable, Stats components - turn into decoration
        let _ = world.remove_one::<Actor>(id);
        let _ = world.remove_one::<ChaseAI>(id);
        let _ = world.remove_one::<CompanionAI>(id);
        let _ = world.remove_one::<Attackable>(id);
        let _ = world.remove_one::<Health>(id);
        let _ = world.remove_one::<BlocksMovement>(id); // Bones are walkable
        let _ = world.remove_one::<Stats>(id);

        // Clean dead entity from all threat tables
        for (_, ai) in world.query_mut::<&mut ChaseAI>() {
            ai.remove_target(id);
        }
        for (_, ai) in world.query_mut::<&mut CompanionAI>() {
            ai.remove_target(id);
        }

        // Change sprite to bones (corpse)
        if let Ok(mut sprite) = world.get::<&mut Sprite>(id) {
            let bones_ref = tile_ids::BONES_4;
            sprite.sheet = bones_ref.0;
            sprite.tile_id = bones_ref.1;
        }

        // Add loot container with random gold, scaled by depth (bosses hoard
        // a fat purse on top)
        let mut gold =
            rng.gen_range(ENEMY_GOLD_DROP_MIN..=ENEMY_GOLD_DROP_MAX) + floor * ENEMY_GOLD_PER_FLOOR;
        if world.get::<&crate::components::Boss>(id).is_ok() {
            gold *= BOSS_GOLD_MULT;
        }

        // Check if enemy had a bow - 50% chance to drop arrows
        let mut loot_items = Vec::new();
        if let Ok(equipment) = world.get::<&Equipment>(id) {
            if equipment.get_bow().is_some() {
                // 50% chance to drop 1-3 arrows
                if rng.gen::<f32>() < 0.5 {
                    let arrow_count = rng.gen_range(1..=3);
                    for _ in 0..arrow_count {
                        loot_items.push(ItemInstance::plain(ItemType::Arrow));
                    }
                }
            }
        }

        let _ = world.insert_one(id, Container::corpse(loot_items, gold));
    }

    // Ally-death morale: living enemies near a fresh corpse may panic and flee.
    // Bosses (FearImmune) are unshakable.
    for (dx, dy) in death_positions {
        let nearby: Vec<Entity> = world
            .query::<(&Position, &ChaseAI, &Health)>()
            .without::<&crate::components::FearImmune>()
            .iter()
            .filter(|(_, (pos, _, h))| {
                h.current > 0 && (pos.x - dx).abs().max((pos.y - dy).abs()) <= ALLY_DEATH_MORALE_RADIUS
            })
            .map(|(id, _)| id)
            .collect();
        for e in nearby {
            if rng.gen_bool(ALLY_DEATH_MORALE_CHANCE) {
                crate::systems::effects::add_effect_to_entity(
                    world,
                    e,
                    EffectType::Feared,
                    MORALE_FEAR_DURATION,
                );
            }
        }
    }

    hostile_kills
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::StatusEffects;

    /// A knockback shove lands its victim on a tile like any other arrival:
    /// shoved into an unlit oil puddle, it comes up Oiled (and slippery);
    /// shoved into water, it comes up Wet and no longer burning.
    #[test]
    fn knockback_landing_runs_tile_effects() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        let mut grid = crate::grid::Grid::new_floor(20, 20, 0, &mut rng);
        grid.water_positions.clear();
        for x in 1..10 {
            for y in 1..4 {
                if let Some(t) = grid.get_mut(x, y) {
                    t.tile_type = crate::tile::TileType::Floor;
                }
            }
        }
        grid.water_positions.push((4, 3));

        let mut world = World::new();
        let attacker = world.spawn((Position::new(1, 1),));
        let victim = world.spawn((Position::new(2, 1), StatusEffects::new()));
        let wet_victim = world.spawn((Position::new(2, 3), StatusEffects::new()));
        let shover = world.spawn((Position::new(1, 3),));
        crate::spawning::spawn_oil_puddle(&mut world, 3, 1);
        crate::systems::effects::add_effect_to_entity(
            &mut world,
            wet_victim,
            EffectType::Burning,
            BURNING_DURATION,
        );
        let mut cache = SpatialCache::rebuild_from_world(&world);
        let mut events = crate::events::EventQueue::new();

        try_knockback(&mut world, &grid, &mut cache, attacker, victim, &mut events, &mut rng);
        assert_eq!(crate::queries::get_entity_position(&world, victim), Some((3, 1)));
        assert!(crate::queries::has_status_effect(&world, victim, EffectType::Oiled));
        assert!(crate::queries::is_slippery(&world, victim));

        // Shove the burning victim twice, (2,3) -> (3,3) -> (4,3): into the water.
        try_knockback(&mut world, &grid, &mut cache, shover, wet_victim, &mut events, &mut rng);
        if let Ok(mut p) = world.get::<&mut Position>(shover) {
            p.x = 2;
        }
        try_knockback(&mut world, &grid, &mut cache, shover, wet_victim, &mut events, &mut rng);
        assert_eq!(crate::queries::get_entity_position(&world, wet_victim), Some((4, 3)));
        assert!(crate::queries::has_status_effect(&world, wet_victim, EffectType::Wet));
        assert!(!crate::queries::has_status_effect(&world, wet_victim, EffectType::Burning));
    }

    /// DoT damage bypasses armor; a normal hit does not.
    #[test]
    fn dot_damage_ignores_armor() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let mut events = crate::events::EventQueue::new();
        let mut world = World::new();
        let mut armor = crate::components::Equipment::with_weapon(Weapon::claws(1));
        armor.body = Some(crate::components::ItemInstance::plain(crate::components::ItemType::ChainMail));
        let defense = armor.total_defense();
        assert!(defense >= 1, "the test needs real armor");
        let e = world.spawn((Health::new(50), armor, StatusEffects::new()));

        let hit = apply_damage(&mut world, e, defense, &mut rng, &mut events);
        assert_eq!(hit, 1, "armor soaks a hit down to the minimum");
        let dot = apply_damage_dot(&mut world, e, 2, &mut rng, &mut events);
        assert_eq!(dot, 2, "a DoT tick ignores armor");
    }

    #[test]
    fn test_weapon_damage() {
        use crate::tile::SpriteSheet;
        let weapon = Weapon {
            name: "Test Sword".to_string(),
            sprite: (SpriteSheet::Items, 0),
            base_damage: 5,
            damage_bonus: 2,
        };
        assert_eq!(weapon_damage(&weapon), 7);
    }

    #[test]
    fn test_dead_entity_is_removed_from_active_ai_tracker() {
        // ActiveAITracker::remove_entity was never called, so dead entities
        // lingered in active_entities/dormant_entities. It self-healed because
        // update_on_player_move rebuilds both sets from a world query, but the
        // stale id survives until then — and once the corpse is despawned (bones
        // consumed by Raise Dead) the id is permanently invalid.
        use crate::active_ai_tracker::ActiveAITracker;
        use rand::{rngs::StdRng, SeedableRng};

        let mut world = World::new();
        let player = world.spawn((
            Position::new(0, 0),
            crate::components::Player,
            Health::new(20),
        ));

        let mut rng = StdRng::seed_from_u64(11);
        let rat = crate::spawning::enemies::RAT.spawn(&mut world, 3, 3, &mut rng);
        world.get::<&mut Health>(rat).unwrap().current = 0;

        let mut tracker = ActiveAITracker::new();
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();

        // An in-range enemy is active; this is the state death must clean up.
        tracker.update_on_player_move(&world, (3, 3));
        assert!(tracker.is_tracked(rat), "live enemy should be tracked");

        let mut clock = crate::time_system::GameClock::new();
        let mut scheduler = crate::time_system::ActionScheduler::new();
        let mut grid = crate::grid::Grid::new_floor(10, 10, 0, &mut rng);
        remove_dead_entities(&mut crate::engine::ActorCtx {
                world: &mut world,
                grid: &mut grid,
                player,
                clock: &mut clock,
                scheduler: &mut scheduler,
                tracker: &mut tracker,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            }, 0);

        assert!(
            !tracker.is_tracked(rat),
            "dead entity must be dropped from the AI tracker at the death site, \
             not left for the next update_on_player_move rebuild"
        );
        assert!(
            !tracker.get_active_entities().contains(&rat),
            "specifically not in the active set"
        );

        // Despawning the corpse (Raise Dead) must not resurrect a stale id.
        let _ = world.despawn(rat);
        assert!(!tracker.is_tracked(rat), "still untracked after the corpse is consumed");
    }

    #[test]
    fn test_lifesteal_heal_math() {
        // 25% of 12 damage = 3
        assert_eq!(lifesteal_heal(12, 0.25), 3);
        // Rounds to nearest: 10% of 14 = 1.4 -> 1
        assert_eq!(lifesteal_heal(14, 0.10), 1);
        // Always at least 1 when damage was dealt
        assert_eq!(lifesteal_heal(1, 0.10), 1);
        // No heal without damage
        assert_eq!(lifesteal_heal(0, 0.25), 0);
        assert_eq!(lifesteal_heal(-5, 0.25), 0);
    }
}
