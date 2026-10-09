//! Animation systems for visual effects.

use crate::components::{HitFlash, LungeAnimation, Position, VisualPosition};
use crate::constants::*;
use crate::ease;
use crate::events::GameEvent;
use hecs::{Entity, World};

/// Smoothly interpolate visual positions toward logical positions.
///
/// A hostile winding up a melee attack is aimed slightly off its tile, toward
/// its target, by an amount that grows with the wind-up's progress at game
/// time `game_time` (see `systems::telegraph::lean_offsets`). The lean is
/// approached through the same lerp, so the jump in progress after each
/// player action reads as a smooth lean rather than a snap.
pub fn visual_lerp(world: &mut World, dt: f32, game_time: f32) {
    let lerp_speed = dt * VISUAL_LERP_SPEED;
    let leans: std::collections::HashMap<Entity, (f32, f32)> =
        crate::systems::telegraph::lean_offsets(world, game_time).into_iter().collect();
    for (id, (pos, vis_pos, lunge)) in
        world.query_mut::<(&Position, &mut VisualPosition, Option<&LungeAnimation>)>()
    {
        // If lunging, offset visual position toward target
        if let Some(lunge) = lunge {
            let base_x = pos.x as f32;
            let base_y = pos.y as f32;

            // Calculate lunge offset (move 0.5 tiles toward target at peak)
            // Use ease-out for punch, ease-in for return
            let lunge_amount = if lunge.returning {
                let t = lunge.progress;
                t * t // Ease-in (slow start, fast end)
            } else {
                let t = lunge.progress;
                1.0 - (1.0 - t) * (1.0 - t) // Ease-out (fast start, slow end)
            };
            let lunge_distance =
                LUNGE_DISTANCE * if lunge.returning { 1.0 - lunge_amount } else { lunge_amount };

            let dx = lunge.target_x - base_x;
            let dy = lunge.target_y - base_y;
            let dist = (dx * dx + dy * dy).sqrt().max(0.001);
            let dir_x = dx / dist;
            let dir_y = dy / dist;

            vis_pos.x = base_x + dir_x * lunge_distance;
            vis_pos.y = base_y + dir_y * lunge_distance;
        } else {
            // Normal interpolation (toward the wind-up lean, if any)
            let (lean_x, lean_y) = leans.get(&id).copied().unwrap_or((0.0, 0.0));
            let tx = pos.x as f32 + lean_x;
            let ty = pos.y as f32 + lean_y;
            let dx = tx - vis_pos.x;
            let dy = ty - vis_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 0.01 {
                vis_pos.x = tx;
                vis_pos.y = ty;
            } else {
                let t = lerp_speed.min(1.0);
                vis_pos.x += dx * t;
                vis_pos.y += dy * t;
            }
        }
    }
}

/// Update lunge animations
pub fn update_lunge_animations(world: &mut World, dt: f32) {
    let lunge_speed = LUNGE_ANIMATION_SPEED;
    let mut to_remove = Vec::new();

    for (id, lunge) in world.query_mut::<&mut LungeAnimation>() {
        lunge.progress += dt * lunge_speed;

        if lunge.progress >= 1.0 {
            if lunge.returning {
                // Animation complete
                to_remove.push(id);
            } else {
                // Start return
                lunge.returning = true;
                lunge.progress = 0.0;
            }
        }
    }

    for id in to_remove {
        let _ = world.remove_one::<LungeAnimation>(id);
    }
}

// =============================================================================
// HIT FLASH
// =============================================================================

/// Flash whatever just took damage, driven straight off the damage events.
///
/// Called for every event in `process_events`; events that are not damage are
/// ignored, and an entity that has already been despawned (killed by the very
/// hit being reported) simply does not get a flash.
pub fn flash_on_damage(world: &mut World, event: &GameEvent) {
    let victim = match event {
        GameEvent::AttackHit { target, damage, .. } if *damage > 0 => *target,
        GameEvent::ProjectileHit { target: Some(target), damage, .. } if *damage > 0 => *target,
        GameEvent::BurnDamage { entity, .. } => *entity,
        GameEvent::DotDamage { entity, .. } => *entity,
        GameEvent::StarvationDamage { entity, .. } => *entity,
        GameEvent::DungeonTrapTriggered { victim, damage, .. } if *damage > 0 => *victim,
        _ => return,
    };

    // Overwriting an existing flash is the point: a second hit re-arms it
    // rather than being swallowed by the first one still fading.
    let _ = world.insert_one(victim, HitFlash { remaining: HIT_FLASH_DURATION });
}

/// Count hit flashes down and drop the component once it expires.
///
/// Takes the *real* frame delta, not the game-time delta, for the same reason
/// camera shake does (see the call site in `engine::tick`): this is
/// presentation, not simulation state. The game clock only advances while an
/// action resolves, so a flash paced by game time would freeze at full
/// brightness the moment the clock settled down to wait for input — which is
/// precisely when the player is looking at it — and then vanish in a single
/// jump on their next keypress. [`update_lunge_animations`] above and
/// `VfxManager::update` take the real delta for the same reason.
pub fn update_hit_flashes(world: &mut World, dt: f32) {
    let mut to_remove = Vec::new();

    for (id, flash) in world.query_mut::<&mut HitFlash>() {
        flash.remaining -= dt;
        if flash.remaining <= 0.0 {
            to_remove.push(id);
        }
    }

    for id in to_remove {
        let _ = world.remove_one::<HitFlash>(id);
    }
}

/// The tint to render `entity` with: its own `base` tint, lifted toward the
/// hit-flash colour for as long as its flash lasts.
///
/// An entity with no `HitFlash` gets `base` back untouched, so this is safe to
/// call for every renderable every frame.
pub fn hit_flash_tint(
    world: &World,
    entity: Entity,
    is_player: bool,
    base: (f32, f32, f32),
) -> (f32, f32, f32) {
    let Ok(flash) = world.get::<&HitFlash>(entity) else {
        return base;
    };

    // Eased so the flash holds near full brightness and then drops away,
    // rather than fading evenly and reading as a glow.
    let strength = ease::out_cubic((flash.remaining / HIT_FLASH_DURATION).clamp(0.0, 1.0));
    let target = if is_player {
        HIT_FLASH_PLAYER_TINT
    } else {
        HIT_FLASH_TINT
    };

    (
        base.0 + (target.0 - base.0) * strength,
        base.1 + (target.1 - base.1) * strength,
        base.2 + (target.2 - base.2) * strength,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::DamageKind;

    /// A world with one entity, standing in for an enemy.
    fn world_with_one_entity() -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn((Position { x: 0, y: 0 },));
        (world, entity)
    }

    #[test]
    fn damage_arms_a_flash_and_it_expires() {
        let (mut world, victim) = world_with_one_entity();

        flash_on_damage(
            &mut world,
            &GameEvent::AttackHit {
                attacker: victim,
                target: victim,
                target_pos: (0.0, 0.0),
                damage: 3,
                kind: DamageKind::Melee,
                crit: false,
                flanked: false,
                killed: false,
            },
        );
        assert!(world.get::<&HitFlash>(victim).is_ok(), "no flash was armed");

        // Part way through, the flash is still there and still tinting.
        update_hit_flashes(&mut world, HIT_FLASH_DURATION / 2.0);
        assert!(world.get::<&HitFlash>(victim).is_ok(), "flash ended early");
        let tint = hit_flash_tint(&world, victim, false, (1.0, 1.0, 1.0));
        assert!(tint.0 > 1.0, "a live flash did not brighten the tint");

        // Past its duration the component is gone and the tint is back to base.
        update_hit_flashes(&mut world, HIT_FLASH_DURATION);
        assert!(world.get::<&HitFlash>(victim).is_err(), "flash outlived its duration");
        assert_eq!(hit_flash_tint(&world, victim, false, (1.0, 1.0, 1.0)), (1.0, 1.0, 1.0));
    }

    #[test]
    fn a_zero_damage_event_does_not_flash() {
        let (mut world, victim) = world_with_one_entity();

        flash_on_damage(
            &mut world,
            &GameEvent::AttackHit {
                attacker: victim,
                target: victim,
                target_pos: (0.0, 0.0),
                damage: 0,
                kind: DamageKind::Melee,
                crit: false,
                flanked: false,
                killed: false,
            },
        );

        assert!(world.get::<&HitFlash>(victim).is_err(), "a miss flashed anyway");
    }

    #[test]
    fn the_player_flashes_red_rather_than_white() {
        let (mut world, player) = world_with_one_entity();
        let _ = world.insert_one(player, HitFlash { remaining: HIT_FLASH_DURATION });

        let (r, g, _) = hit_flash_tint(&world, player, true, (1.0, 1.0, 1.0));
        assert!(r > g, "the player's flash was not red-dominant");
    }

    #[test]
    fn an_entity_with_no_flash_keeps_its_own_tint() {
        let (world, entity) = world_with_one_entity();
        // Fire-arrow style base tint must survive untouched.
        let base = (1.0, 0.5, 0.2);
        assert_eq!(hit_flash_tint(&world, entity, false, base), base);
    }
}
