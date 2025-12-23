//! Combat system functions.

use crate::components::{
    Actor, Attackable, BlocksMovement, ChaseAI, Container, Door, Equipment, Experience, Health,
    Position, Sprite, Stats, Weapon,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::systems::experience::{calculate_xp_value, grant_xp};
use crate::tile::tile_ids;
use crate::time_system::ActionScheduler;
use hecs::{Entity, World};
use rand::Rng;

/// Calculate total damage for a weapon
pub fn weapon_damage(weapon: &Weapon) -> i32 {
    weapon.base_damage + weapon.damage_bonus
}

/// Get the damage an entity deals (from equipped weapon or unarmed)
pub fn get_attack_damage(world: &World, attacker: Entity) -> i32 {
    if let Ok(equipment) = world.get::<&Equipment>(attacker) {
        equipment
            .weapon
            .as_ref()
            .map(|w| weapon_damage(w))
            .unwrap_or(UNARMED_DAMAGE)
    } else {
        UNARMED_DAMAGE
    }
}

/// Open a door - remove blocking components (sprite stays the same but renders darker)
pub fn open_door(world: &mut World, door_id: Entity) {
    // Mark as open
    if let Ok(mut door) = world.get::<&mut Door>(door_id) {
        door.is_open = true;
    }

    // Remove blocking components
    let _ = world.remove_one::<crate::components::BlocksVision>(door_id);
    let _ = world.remove_one::<BlocksMovement>(door_id);
}

/// Open a chest - mark as open (sprite change handled by event system)
pub fn open_chest(world: &mut World, chest_id: Entity) {
    // Mark as open
    if let Ok(mut container) = world.get::<&mut Container>(chest_id) {
        container.is_open = true;
    }
}

/// Handle a ContainerOpened event - update sprite for chests only
pub fn handle_container_opened(world: &mut World, container_id: Entity) {
    if let Ok(mut sprite) = world.get::<&mut Sprite>(container_id) {
        // Only change sprite for actual chests, not bones
        if sprite.tile_id == tile_ids::CHEST_CLOSED {
            sprite.tile_id = tile_ids::CHEST_OPEN;
        }
    }
}

/// Turn dead entities into bones (health <= 0) and grant XP to player
/// Also cancels any pending actions for dead entities in the scheduler
pub fn remove_dead_entities(
    world: &mut World,
    player_entity: Entity,
    rng: &mut impl Rng,
    events: &mut EventQueue,
    mut scheduler: Option<&mut ActionScheduler>,
) {
    let mut to_convert = Vec::new();

    for (id, (pos, health, stats)) in world.query::<(&Position, &Health, Option<&Stats>)>().iter()
    {
        if health.current <= 0 {
            let xp = calculate_xp_value(stats);
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

    for (id, position, _xp) in to_convert {
        // Cancel any pending actions for this entity
        if let Some(ref mut sched) = scheduler {
            sched.cancel_for_entity(id);
        }

        // Emit death event
        events.push(GameEvent::EntityDied {
            entity: id,
            position,
        });

        // Remove AI, Actor, Attackable, Stats components - turn into decoration
        let _ = world.remove_one::<Actor>(id);
        let _ = world.remove_one::<ChaseAI>(id);
        let _ = world.remove_one::<Attackable>(id);
        let _ = world.remove_one::<Health>(id);
        let _ = world.remove_one::<BlocksMovement>(id); // Bones are walkable
        let _ = world.remove_one::<Stats>(id);

        // Change sprite to bones
        if let Ok(mut sprite) = world.get::<&mut Sprite>(id) {
            sprite.tile_id = tile_ids::BONES;
        }

        // Add loot container with random gold
        let gold = rng.gen_range(ENEMY_GOLD_DROP_MIN..=ENEMY_GOLD_DROP_MAX);
        let _ = world.insert_one(id, Container::with_gold(vec![], gold));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weapon_damage() {
        let weapon = Weapon {
            name: "Test Sword".to_string(),
            tile_id: 0,
            base_damage: 5,
            damage_bonus: 2,
        };
        assert_eq!(weapon_damage(&weapon), 7);
    }

    #[test]
    fn test_get_attack_damage_unarmed() {
        let mut world = World::new();
        let entity = world.spawn(());
        assert_eq!(get_attack_damage(&world, entity), UNARMED_DAMAGE);
    }

    #[test]
    fn test_get_attack_damage_with_weapon() {
        let mut world = World::new();
        let weapon = Weapon {
            name: "Test Sword".to_string(),
            tile_id: 0,
            base_damage: 10,
            damage_bonus: 3,
        };
        let entity = world.spawn((Equipment { weapon: Some(weapon), ranged: None },));
        assert_eq!(get_attack_damage(&world, entity), 13);
    }
}
