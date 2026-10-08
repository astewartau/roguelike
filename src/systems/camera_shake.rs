//! How hard each game event shakes the view.
//!
//! This module only maps events to [`ShakeRequest`]s. The camera owns the
//! oscillation, the decay and the distance falloff
//! ([`Camera::apply_shake_request`](crate::camera::Camera::apply_shake_request)),
//! and the engine tick hands the queued requests over once a frame — so no
//! combat or ability code ever has to know the camera exists.

use crate::camera::ShakeRequest;
use crate::components::{Health, Position};
use crate::constants::*;
use crate::events::GameEvent;
use glam::Vec2;
use hecs::{Entity, World};

/// Queue a shake for `event` if it is the sort of thing that shakes the view,
/// and nothing otherwise.
///
/// Every lookup degrades to "no shake" rather than panicking: an event can
/// name an entity that has already been despawned, or one that never had the
/// components this needs.
pub fn queue_for_event(
    world: &World,
    player: Entity,
    event: &GameEvent,
    out: &mut Vec<ShakeRequest>,
) {
    match event {
        // Taking a hit is the big one, scaled by how much of the player's max
        // HP it just cost them.
        GameEvent::AttackHit { target, damage, .. } if *target == player && *damage > 0 => {
            out.extend(player_hit_shake(world, player, *damage));
        }
        GameEvent::ProjectileHit {
            target: Some(target),
            damage,
            ..
        } if *target == player && *damage > 0 => {
            out.extend(player_hit_shake(world, player, *damage));
        }

        // Explosions shake by proximity to what the player is looking at.
        // Oil barrels emit a FireballExplosion of their own alongside
        // BarrelExploded, so they are covered here and must not be handled
        // again below, or they would shake twice.
        GameEvent::FireballExplosion { x, y, .. } => out.push(ShakeRequest {
            amplitude: CAMERA_SHAKE_EXPLOSION,
            origin: Some(tile_centre(*x, *y)),
            direction: None,
        }),

        // A boss throwing its weight around.
        GameEvent::BossAbilityUsed { position, .. } => out.push(ShakeRequest {
            amplitude: CAMERA_SHAKE_BOSS_ABILITY,
            origin: Some(tile_centre(position.0, position.1)),
            direction: None,
        }),

        // The player's own heavy swing: a small kick along the swing rather
        // than a rattle, so their attacks have follow-through without ever
        // fighting their aim.
        GameEvent::CleavePerformed { center } => {
            let direction = player_centre(world, player)
                .map(|from| tile_centre(center.0, center.1) - from);
            out.push(ShakeRequest {
                amplitude: CAMERA_SHAKE_PLAYER_SWING,
                origin: None,
                direction,
            });
        }

        _ => {}
    }
}

/// A player hit scaled by the fraction of max HP it took, with a floor so
/// that even a scratch registers. `None` if the player has no readable Health.
fn player_hit_shake(world: &World, player: Entity, damage: i32) -> Option<ShakeRequest> {
    let max_health = world.get::<&Health>(player).ok().map(|h| h.max)?;
    if max_health <= 0 {
        return None;
    }

    let fraction = (damage as f32 / max_health as f32).clamp(0.0, 1.0);
    let amplitude = (CAMERA_SHAKE_PLAYER_HIT * fraction).max(CAMERA_SHAKE_PLAYER_HIT_MIN);

    Some(ShakeRequest {
        amplitude,
        origin: None,
        direction: None,
    })
}

/// Centre of a tile in world units, matching how the camera tracks the player.
fn tile_centre(x: i32, y: i32) -> Vec2 {
    Vec2::new(x as f32 + 0.5, y as f32 + 0.5)
}

/// The player's tile centre, or `None` if they have no position.
fn player_centre(world: &World, player: Entity) -> Option<Vec2> {
    world
        .get::<&Position>(player)
        .ok()
        .map(|p| tile_centre(p.x, p.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::DamageKind;

    const MAX_HEALTH: i32 = 50;

    /// A world holding just a player with health and a position.
    fn world_with_player() -> (World, Entity) {
        let mut world = World::new();
        let player = world.spawn((Position { x: 5, y: 5 }, Health::new(MAX_HEALTH)));
        (world, player)
    }

    fn attack_on(target: Entity, damage: i32) -> GameEvent {
        GameEvent::AttackHit {
            attacker: target,
            target,
            target_pos: (5.0, 5.0),
            damage,
            kind: DamageKind::Melee,
            crit: false,
        }
    }

    #[test]
    fn a_bigger_hit_shakes_harder() {
        let (world, player) = world_with_player();

        let mut light = Vec::new();
        queue_for_event(&world, player, &attack_on(player, MAX_HEALTH / 10), &mut light);
        let mut heavy = Vec::new();
        queue_for_event(&world, player, &attack_on(player, MAX_HEALTH), &mut heavy);

        assert_eq!(light.len(), 1, "a hit on the player queued no shake");
        assert_eq!(heavy.len(), 1);
        assert!(
            heavy[0].amplitude > light[0].amplitude,
            "shake did not scale with the fraction of max HP lost"
        );
    }

    #[test]
    fn even_the_smallest_scratch_registers() {
        let (world, player) = world_with_player();

        let mut shakes = Vec::new();
        queue_for_event(&world, player, &attack_on(player, 1), &mut shakes);

        assert_eq!(shakes.len(), 1);
        assert!(shakes[0].amplitude >= CAMERA_SHAKE_PLAYER_HIT_MIN);
    }

    #[test]
    fn hits_on_other_entities_do_not_shake_the_view() {
        let (mut world, player) = world_with_player();
        let enemy = world.spawn((Position { x: 6, y: 5 }, Health::new(10)));

        let mut shakes = Vec::new();
        queue_for_event(&world, player, &attack_on(enemy, 10), &mut shakes);

        assert!(shakes.is_empty(), "an enemy taking a hit shook the camera");
    }

    #[test]
    fn an_explosion_carries_its_origin_for_falloff() {
        let (world, player) = world_with_player();

        let mut shakes = Vec::new();
        queue_for_event(
            &world,
            player,
            &GameEvent::FireballExplosion { x: 9, y: 3, radius: 2 },
            &mut shakes,
        );

        assert_eq!(shakes.len(), 1);
        assert_eq!(shakes[0].origin, Some(Vec2::new(9.5, 3.5)));
        assert!(shakes[0].direction.is_none(), "an explosion should rattle, not kick");
    }

    #[test]
    fn a_cleave_kicks_toward_the_swing() {
        let (world, player) = world_with_player();

        let mut shakes = Vec::new();
        // Player is at (5,5); cleaving to the east.
        queue_for_event(
            &world,
            player,
            &GameEvent::CleavePerformed { center: (7, 5) },
            &mut shakes,
        );

        assert_eq!(shakes.len(), 1);
        let direction = shakes[0].direction.expect("a cleave should kick directionally");
        assert!(direction.x > 0.0, "the kick did not point at the swing");
        assert_eq!(direction.y, 0.0);
    }

    #[test]
    fn a_player_with_no_health_component_is_simply_not_shaken() {
        let mut world = World::new();
        let player = world.spawn((Position { x: 0, y: 0 },));

        let mut shakes = Vec::new();
        queue_for_event(&world, player, &attack_on(player, 5), &mut shakes);

        assert!(shakes.is_empty());
    }

    #[test]
    fn unremarkable_events_do_not_shake() {
        let (world, player) = world_with_player();

        let mut shakes = Vec::new();
        queue_for_event(
            &world,
            player,
            &GameEvent::GoldPickedUp { entity: player, amount: 10 },
            &mut shakes,
        );

        assert!(shakes.is_empty());
    }
}
