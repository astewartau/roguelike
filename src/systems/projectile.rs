//! Projectile system for arrows and other flying objects.
//!
//! Handles projectile movement, collision detection, and cleanup.
//!
//! The projectile lifecycle:
//! 1. Spawned when ShootBow action completes
//! 2. Game-time updates move the logical Position along the path
//! 3. When path ends or hits something, marked as "finished" (not despawned yet)
//! 4. Real-time visual lerp animates the arrow to its final position
//! 5. Once visual catches up, the arrow is despawned


use crate::components::{Health, 
    EffectType, ItemType, Position, Projectile, ProjectileKind, ProjectileMarker,
    VisualPosition,
};
use crate::events::GameEvent;
use crate::systems::actions::apply_potion_splash;
use crate::systems::effects;
use crate::engine::EffectCtx;
use hecs::Entity;
use crate::world::World;

/// Update all projectiles based on the current game time.
/// This should be called when game time advances.
/// Projectiles that finish their journey are marked as "finished" but NOT despawned yet.
pub fn update_projectiles(ctx: &mut EffectCtx, current_time: f32) {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // (projectile_entity, target_entity, position, damage, on_hit_effect, source_entity, potion_type, kind)
    type Hit = (
        Entity,
        Option<Entity>,
        (i32, i32),
        i32,
        Option<(EffectType, f32)>,
        Entity,
        Option<ItemType>,
        ProjectileKind,
    );
    let mut hits: Vec<Hit> = Vec::new();
    // (projectile_entity, x, y, potion_type, incendiary)
    let mut finished_projectiles: Vec<(Entity, i32, i32, Option<ItemType>, bool)> = Vec::new();

    // Get all attackable entities and their positions for collision checking
    let attackables: Vec<(Entity, i32, i32)> = world
        .query::<(&Position, &Health)>()
        .iter()
        .map(|(e, (pos, _))| (e, pos.x, pos.y))
        .collect();

    // Update positions and check for collisions (walls AND enemies)
    for (projectile_entity, (pos, projectile)) in
        world.query_mut::<(&mut Position, &mut Projectile)>()
    {
        // Skip already finished projectiles
        if projectile.finished.is_some() {
            continue;
        }

        let elapsed = current_time - projectile.spawn_time;

        // Find the current tile based on elapsed time
        let mut current_tile_index = projectile.path_index;
        while current_tile_index < projectile.path.len() {
            let (_, _, arrival_time) = projectile.path[current_tile_index];
            if elapsed >= arrival_time {
                // Arrow has reached or passed this tile
                current_tile_index += 1;
            } else {
                break;
            }
        }

        // Check tiles we've passed through for wall AND enemy hits
        let mut hit_something = false;
        for i in projectile.path_index..current_tile_index.min(projectile.path.len()) {
            let (tile_x, tile_y, _) = projectile.path[i];

            // Check for wall collision first. A fireball doesn't hit the
            // wall; it bursts in front of it (below).
            let tile = grid.get(tile_x, tile_y);
            let is_wall = tile.map(|t| !t.tile_type.is_walkable()).unwrap_or(true);
            let fireball = projectile.kind == ProjectileKind::Fireball;
            if is_wall && !fireball {
                hits.push((
                    projectile_entity,
                    None,
                    (tile_x, tile_y),
                    projectile.damage,
                    projectile.on_hit_effect,
                    projectile.source,
                    projectile.potion_type,
                    projectile.kind,
                ));
            }
            if is_wall {
                // Mark as finished at wall position (one tile before the wall)
                let final_pos = if i > 0 {
                    let (px, py, _) = projectile.path[i - 1];
                    (px, py)
                } else {
                    (pos.x, pos.y)
                };
                finished_projectiles.push((
                    projectile_entity,
                    final_pos.0,
                    final_pos.1,
                    projectile.potion_type,
                    projectile.incendiary,
                ));
                hit_something = true;
                break;
            }

            // Check for enemy collision at this tile (a fireball flies over
            // everyone to where it was aimed)
            for (target_entity, target_x, target_y) in attackables.iter().filter(|_| !fireball) {
                // Don't hit the shooter
                if *target_entity == projectile.source {
                    continue;
                }
                if tile_x == *target_x && tile_y == *target_y {
                    hits.push((
                        projectile_entity,
                        Some(*target_entity),
                        (tile_x, tile_y),
                        projectile.damage,
                        projectile.on_hit_effect,
                        projectile.source,
                        projectile.potion_type,
                        projectile.kind,
                    ));
                    finished_projectiles.push((
                        projectile_entity,
                        tile_x,
                        tile_y,
                        projectile.potion_type,
                        projectile.incendiary,
                    ));
                    hit_something = true;
                    break;
                }
            }

            if hit_something {
                break;
            }

            // Update logical position to this tile
            pos.x = tile_x;
            pos.y = tile_y;
        }

        if hit_something {
            continue;
        }

        // Update path index
        if current_tile_index < projectile.path.len() {
            projectile.path_index = current_tile_index;
        } else {
            // Projectile reached end of path without hitting anything - mark as finished
            if let Some((final_x, final_y, _)) = projectile.path.last() {
                finished_projectiles.push((
                    projectile_entity,
                    *final_x,
                    *final_y,
                    projectile.potion_type,
                    projectile.incendiary,
                ));
            }
        }
    }

    // Apply damage and emit events
    for (projectile_entity, target, position, damage, on_hit_effect, source, potion_type, kind) in hits {
        let mut actual_damage = damage;
        if let Some(target_entity) = target {
            // Mark that this projectile hit an enemy (for arrow recovery)
            if let Ok(mut projectile) = world.get::<&mut Projectile>(projectile_entity) {
                projectile.hit_enemy = true;
            }

            // Apply damage (handles invulnerability, armor defense, Protected/Barkskin)
            actual_damage =
                crate::systems::combat::apply_damage(world, grid, target_entity, actual_damage, rng, events);

            if actual_damage > 0 {

                // Generate threat on the target for the projectile source
                use crate::constants::THREAT_PER_DAMAGE;
                let threat_amount = actual_damage as f32 * THREAT_PER_DAMAGE;
                crate::systems::ai::generate_threat(world, target_entity, source, threat_amount);
                crate::systems::ai::generate_companion_threat(world, target_entity, source, threat_amount);
            }

            // Apply on_hit_effect (even if invulnerable, effects like Slowed still apply)
            if let Some((effect_type, duration)) = on_hit_effect {
                effects::add_effect_to_entity(world, target_entity, effect_type, duration);
            }

            // Resolve weapon on-hit affixes for ranged hits through the shared
            // chokepoint (potions and spells carry no weapon affixes).
            if potion_type.is_none() && kind == ProjectileKind::Missile {
                crate::systems::combat::resolve_weapon_on_hit(
                    &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
                    source,
                    target_entity,
                    actual_damage,
                );
            }
        }

        // Classify the projectile so the log can describe it specifically.
        let kind = if kind == ProjectileKind::GraveBolt {
            crate::events::DamageKind::GraveBolt
        } else if potion_type.is_some() {
            crate::events::DamageKind::Potion
        } else if matches!(on_hit_effect, Some((EffectType::Slowed, _))) {
            crate::events::DamageKind::CripplingShot
        } else {
            crate::events::DamageKind::Arrow
        };

        events.push(GameEvent::ProjectileHit {
            projectile: projectile_entity,
            source,
            target,
            position,
            damage: actual_damage,
            kind,
        });
    }

    // Mark projectiles as finished (don't despawn yet - wait for visual catch-up)
    // Also apply potion splash effects for potion projectiles
    for (entity, final_x, final_y, potion_type, incendiary) in finished_projectiles {
        let mut fireball = None;
        if let Ok(mut projectile) = world.get::<&mut Projectile>(entity) {
            projectile.finished = Some((final_x, final_y, current_time));
            if projectile.kind == ProjectileKind::Fireball {
                fireball = Some((projectile.source, projectile.damage));
            }
        }
        // Update logical position to final position
        if let Ok(mut pos) = world.get::<&mut Position>(entity) {
            pos.x = final_x;
            pos.y = final_y;
        }

        // A fireball bursts where it came down (and makes its own noise).
        if let Some((caster, damage)) = fireball {
            crate::systems::actions::detonate_fireball(world, grid, caster, (final_x, final_y), damage, events, rng);
            continue;
        }

        // Whatever lands makes a noise there: a bottle shattering, an arrow
        // clattering. Enemies who hear it go to look at the landing spot, not
        // at the thrower — which is what makes a throw a lure.
        let impact_noise = if potion_type.is_some() {
            crate::constants::THROWN_IMPACT_NOISE_RADIUS
        } else {
            crate::constants::ARROW_IMPACT_NOISE_RADIUS
        };
        crate::systems::noise::make_noise(world, grid, (final_x, final_y), impact_noise);

        // Fire arrows ignite the tile they land on (grass catches; the fire
        // system then handles spread and burnout).
        if incendiary {
            crate::systems::fire::ignite_tile(world, grid, final_x, final_y);
        }

        // If this is a potion projectile, apply splash effect and emit event
        if let Some(ptype) = potion_type {
            let thrower = world.get::<&Projectile>(entity).map(|p| p.source).ok();
            apply_potion_splash(world, grid, thrower, ptype, final_x, final_y, events);
            events.push(GameEvent::PotionSplash {
                x: final_x,
                y: final_y,
                potion_type: ptype,
            });
        }
    }
}

/// Update visual positions of projectiles for smooth interpolation.
/// This runs every frame in real-time to animate arrows along their path,
/// each at its own `speed`, so a second of flight in game time takes a second
/// on screen.
pub fn lerp_projectiles_realtime(world: &mut World, real_time_elapsed: f32) {
    for (_, (pos, vis_pos, projectile)) in
        world.query_mut::<(&Position, &mut VisualPosition, &Projectile)>()
    {
        // Calculate target position
        let target_x = pos.x as f32;
        let target_y = pos.y as f32;

        // Move visual position toward target at arrow speed
        let dx = target_x - vis_pos.x;
        let dy = target_y - vis_pos.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < 0.01 {
            // Close enough, snap to target
            vis_pos.x = target_x;
            vis_pos.y = target_y;
        } else {
            // Move at the projectile's own speed, in tiles per second
            let move_dist = projectile.speed * real_time_elapsed;
            if move_dist >= dist {
                vis_pos.x = target_x;
                vis_pos.y = target_y;
            } else {
                let t = move_dist / dist;
                vis_pos.x += dx * t;
                vis_pos.y += dy * t;
            }
        }
    }
}

/// Where a spent arrow came to rest, and whether it hit an enemy on the way.
pub type ArrowRecovery = ((i32, i32), bool);

/// What a projectile left behind once it visibly arrived.
#[derive(Default)]
pub struct Landings {
    /// Spent arrows that may be picked up again (see `ArrowRecovery`).
    pub arrows: Vec<ArrowRecovery>,
    /// Where fireballs came down. Their blast was dealt when they landed in
    /// game time; this is when it is seen and heard.
    pub fireballs: Vec<(i32, i32)>,
}

/// Clean up finished projectiles whose visuals have caught up.
/// Returns the entities to despawn and what they left behind.
/// Arrows that missed are always recoverable; arrows that hit have a chance
/// (rolled by the caller, see `ARROW_RECOVERY_CHANCE_ON_HIT`).
pub fn cleanup_finished_projectiles(world: &World) -> (Vec<Entity>, Landings) {
    let mut to_despawn = Vec::new();
    let mut landings = Landings::default();

    for (entity, (pos, vis_pos, projectile)) in
        world.query::<(&Position, &VisualPosition, &Projectile)>().iter()
    {
        // Only check finished projectiles
        if projectile.finished.is_none() {
            continue;
        }

        // Check if visual has caught up to logical position
        let dx = pos.x as f32 - vis_pos.x;
        let dy = pos.y as f32 - vis_pos.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < 0.1 {
            // Visual has caught up, safe to despawn
            to_despawn.push(entity);

            // If this was an arrow (not a potion or a spell), it may be
            // recoverable. Fire arrows burn up on impact and are never recovered.
            if projectile.potion_type.is_none()
                && !projectile.incendiary
                && projectile.kind == ProjectileKind::Missile
            {
                landings.arrows.push(((pos.x, pos.y), projectile.hit_enemy));
            }
            if projectile.kind == ProjectileKind::Fireball {
                landings.fireballs.push((pos.x, pos.y));
            }
        }
    }

    (to_despawn, landings)
}

/// Seconds until a projectile's sprite reaches where it came down. Its
/// landing is settled in game time, which jumps ahead in an instant, while
/// the sprite flies there at its own speed in real time.
fn visual_arrival(projectile: &Projectile, vis: &VisualPosition, (x, y): (i32, i32)) -> f32 {
    (x as f32 - vis.x).hypot(y as f32 - vis.y) / projectile.speed.max(0.1)
}

/// Whether `shooter` has something it fired still in flight (in game time).
/// The shooter waits it out (`time_system`), so nothing hangs in the air.
pub fn has_projectile_in_flight(world: &World, shooter: Entity) -> bool {
    world
        .query::<&Projectile>()
        .iter()
        .any(|(_, p)| p.source == shooter && p.finished.is_none())
}

/// How long to hold back what `event` shows (numbers, splashes) so it
/// appears when the projectile behind it visibly lands rather than while it
/// is still in the air. Zero for anything else.
pub fn impact_delay(world: &World, event: &GameEvent) -> f32 {
    let at = match event {
        GameEvent::ProjectileHit { projectile, .. } => {
            let (Ok(vis), Ok(p)) =
                (world.get::<&VisualPosition>(*projectile), world.get::<&Projectile>(*projectile))
            else {
                return 0.0;
            };
            return p.finished.map_or(0.0, |(x, y, _)| visual_arrival(&p, &vis, (x, y)));
        }
        GameEvent::PotionSplash { x, y, .. } => (*x, *y),
        GameEvent::AttackHit { kind: crate::events::DamageKind::Fireball, target_pos, .. } => {
            (target_pos.0.floor() as i32, target_pos.1.floor() as i32)
        }
        _ => return 0.0,
    };
    // The projectile that came down over `at`, still flying on screen.
    world
        .query::<(&Projectile, &VisualPosition)>()
        .iter()
        .filter_map(|(_, (p, vis))| {
            let (x, y, _) = p.finished?;
            let reach = if p.kind == ProjectileKind::Fireball { crate::constants::FIREBALL_RADIUS } else { 0 };
            ((at.0 - x).abs().max((at.1 - y).abs()) <= reach).then(|| visual_arrival(p, vis, (x, y)))
        })
        .fold(0.0, f32::max)
}

/// Despawn projectile entities
pub fn despawn_projectiles(world: &mut World, to_despawn: Vec<Entity>) {
    for entity in to_despawn {
        let _ = world.despawn(entity);
    }
}

/// Check if there are any active projectiles in the world
#[allow(dead_code)] // Public API for blocking input during projectile flight
pub fn has_active_projectiles(world: &World) -> bool {
    world.query::<&ProjectileMarker>().iter().next().is_some()
}
