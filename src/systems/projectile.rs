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


use crate::components::{Attackable, EffectType, ItemType, Position, Projectile, ProjectileMarker, VisualPosition};
use crate::events::GameEvent;
use crate::grid::Grid;
use crate::systems::actions::apply_potion_splash;
use crate::systems::effects;
use crate::engine::EffectCtx;
use hecs::{Entity, World};

/// Update all projectiles based on the current game time.
/// This should be called when game time advances.
/// Projectiles that finish their journey are marked as "finished" but NOT despawned yet.
pub fn update_projectiles(ctx: &mut EffectCtx, current_time: f32) {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // (projectile_entity, target_entity, position, damage, on_hit_effect, source_entity, potion_type)
    type Hit = (Entity, Option<Entity>, (i32, i32), i32, Option<(EffectType, f32)>, Entity, Option<ItemType>);
    let mut hits: Vec<Hit> = Vec::new();
    // (projectile_entity, x, y, potion_type, incendiary)
    let mut finished_projectiles: Vec<(Entity, i32, i32, Option<ItemType>, bool)> = Vec::new();

    // Get all attackable entities and their positions for collision checking
    let attackables: Vec<(Entity, i32, i32)> = world
        .query::<(&Position, &Attackable)>()
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

            // Check for wall collision first
            let tile = grid.get(tile_x, tile_y);
            let is_wall = tile.map(|t| !t.tile_type.is_walkable()).unwrap_or(true);
            if is_wall {
                hits.push((projectile_entity, None, (tile_x, tile_y), projectile.damage, projectile.on_hit_effect, projectile.source, projectile.potion_type));
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

            // Check for enemy collision at this tile
            for (target_entity, target_x, target_y) in &attackables {
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
    for (projectile_entity, target, position, damage, on_hit_effect, source, potion_type) in hits {
        let mut actual_damage = damage;
        if let Some(target_entity) = target {
            // Mark that this projectile hit an enemy (for arrow recovery)
            if let Ok(mut projectile) = world.get::<&mut Projectile>(projectile_entity) {
                projectile.hit_enemy = true;
            }

            // Apply damage (handles invulnerability, armor defense, Protected/Barkskin)
            actual_damage =
                crate::systems::combat::apply_damage(world, target_entity, actual_damage, rng, events);

            if actual_damage > 0 {
                // Interrupt life drain if target was channeling
                crate::systems::actions::interrupt_life_drain_on_damage(world, target_entity, events);

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
            // chokepoint (potions carry no weapon affixes).
            if potion_type.is_none() {
                crate::systems::combat::resolve_weapon_on_hit(
                    &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
                    source,
                    target_entity,
                    actual_damage,
                );
            }
        }

        // Classify the projectile so the log can describe it specifically.
        let kind = if potion_type.is_some() {
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
        if let Ok(mut projectile) = world.get::<&mut Projectile>(entity) {
            projectile.finished = Some((final_x, final_y, current_time));
        }
        // Update logical position to final position
        if let Ok(mut pos) = world.get::<&mut Position>(entity) {
            pos.x = final_x;
            pos.y = final_y;
        }

        // Fire arrows ignite the tile they land on (grass catches; the fire
        // system then handles spread and burnout).
        if incendiary {
            ignite_tile(world, grid, final_x, final_y);
        }

        // If this is a potion projectile, apply splash effect and emit event
        if let Some(ptype) = potion_type {
            let thrower = world.get::<&Projectile>(entity).map(|p| p.source).ok();
            apply_potion_splash(world, grid, thrower, ptype, final_x, final_y);
            events.push(GameEvent::PotionSplash {
                x: final_x,
                y: final_y,
                potion_type: ptype,
            });
        }
    }
}

/// Ignite a tile hit by an incendiary projectile: flammable tiles (tall grass)
/// catch fire via the same burning-grass entities the fire-spread system uses,
/// and any oil puddle on the tile lights up. No-op for wet grass or tiles
/// already burning.
fn ignite_tile(world: &mut World, grid: &Grid, x: i32, y: i32) {
    // Oil on the tile catches regardless of the terrain underneath.
    let puddles: Vec<hecs::Entity> = world
        .query::<(&Position, &crate::components::OilPuddle)>()
        .iter()
        .filter(|(_, (pos, _))| pos.x == x && pos.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in puddles {
        crate::systems::fire::ignite_oil_puddle(world, id);
    }

    let flammable = grid
        .get(x, y)
        .map(|t| t.tile_type.is_flammable())
        .unwrap_or(false);
    if !flammable {
        return;
    }

    // Soaked grass can't catch.
    let wet = world
        .query::<(&Position, &crate::components::WetGrass)>()
        .iter()
        .any(|(_, (pos, _))| pos.x == x && pos.y == y);
    if wet {
        return;
    }

    let already_burning = world
        .query::<(&Position, &crate::components::BurningGrass)>()
        .iter()
        .any(|(_, (pos, _))| pos.x == x && pos.y == y);
    if already_burning {
        return;
    }

    crate::spawning::spawn_burning_grass(world, x, y);
}

/// Update visual positions of projectiles for smooth interpolation.
/// This runs every frame in real-time to animate arrows along their path.
/// Uses real_time_elapsed to interpolate based on actual frame time.
pub fn lerp_projectiles_realtime(world: &mut World, real_time_elapsed: f32, arrow_speed: f32) {
    for (_, (pos, vis_pos, _projectile)) in
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
            // Move at arrow_speed tiles per second
            let move_dist = arrow_speed * real_time_elapsed;
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

/// Clean up finished projectiles whose visuals have caught up.
/// Returns (entities to despawn, arrow recovery info).
/// Arrows that missed are always recoverable; arrows that hit have a chance
/// (rolled by the caller, see `ARROW_RECOVERY_CHANCE_ON_HIT`).
pub fn cleanup_finished_projectiles(world: &World) -> (Vec<Entity>, Vec<ArrowRecovery>) {
    let mut to_despawn = Vec::new();
    let mut arrow_recovery_info = Vec::new();

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

            // If this was an arrow (not a potion), it may be recoverable.
            // Fire arrows burn up on impact and are never recovered.
            if projectile.potion_type.is_none() && !projectile.incendiary {
                arrow_recovery_info.push(((pos.x, pos.y), projectile.hit_enemy));
            }
        }
    }

    (to_despawn, arrow_recovery_info)
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
