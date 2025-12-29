//! Combat system functions.

use crate::components::{
    Actor, Attackable, BlocksMovement, ChaseAI, Container, Door, Experience, Health, Position,
    Sprite, Stats, Weapon,
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

        // Change sprite to bones (corpse)
        if let Ok(mut sprite) = world.get::<&mut Sprite>(id) {
            let bones_ref = tile_ids::BONES_4;
            sprite.sheet = bones_ref.0;
            sprite.tile_id = bones_ref.1;
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
        use crate::tile::SpriteSheet;
        let weapon = Weapon {
            name: "Test Sword".to_string(),
            sprite: (SpriteSheet::Items, 0),
            base_damage: 5,
            damage_bonus: 2,
        };
        assert_eq!(weapon_damage(&weapon), 7);
    }
}
