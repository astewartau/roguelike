//! Rendering-related systems and data structures.

use crate::components::{Actor, BlocksVision, Door, Position, Sprite, VisualPosition};
use crate::fov::FOV;
use crate::grid::Grid;
use hecs::{Entity, World};
use std::collections::HashSet;

/// Visual effect flags (bitfield) - reserved for future effects
pub mod effects {
    pub const NONE: u32 = 0;
    // Future effects:
    // pub const POISONED: u32 = 1 << 0;
    // pub const BURNING: u32 = 1 << 1;
    // pub const FROZEN: u32 = 1 << 2;
    // pub const SHIELDED: u32 = 1 << 3;
}

/// Entity ready for rendering with all visual state
pub struct RenderEntity {
    pub x: f32,
    pub y: f32,
    pub sprite: Sprite,
    pub brightness: f32,
    pub effects: u32, // Bitfield of active effects
}

/// Update field of view from player position
pub fn update_fov(world: &World, grid: &mut Grid, player_entity: Entity, radius: i32) {
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };

    // Clear visibility
    for tile in &mut grid.tiles {
        tile.visible = false;
    }

    // Collect positions of entities that block vision
    let blocking_positions: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate and apply FOV
    let visible_tiles = FOV::calculate(
        grid,
        player_pos.x,
        player_pos.y,
        radius,
        Some(|x: i32, y: i32| blocking_positions.contains(&(x, y))),
    );
    for (x, y) in visible_tiles {
        if let Some(tile) = grid.get_mut(x, y) {
            tile.visible = true;
            tile.explored = true;
        }
    }
}

/// Collect entities that should be rendered, with fog of war applied
pub fn collect_renderables(world: &World, grid: &Grid, player_entity: Entity) -> Vec<RenderEntity> {
    let mut entities_to_render: Vec<RenderEntity> = Vec::new();
    let mut player_render: Option<RenderEntity> = None;

    for (id, (pos, vis_pos, sprite)) in
        world.query::<(&Position, &VisualPosition, &Sprite)>().iter()
    {
        let (is_explored, is_visible) = grid
            .get(pos.x, pos.y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Actors (enemies) are only visible in FOV, not in fog
        let is_actor = world.get::<&Actor>(id).is_ok();


        // Check if this is an open door (render darker)
        let is_open_door = world.get::<&Door>(id).map(|door| door.is_open).unwrap_or(false);

        let entity_effects = effects::NONE;

        if id == player_entity {
            player_render = Some(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness: 1.0,
                effects: effects::NONE,
            });
        } else if is_visible {
            // Open doors render at 50% brightness
            let brightness = if is_open_door { 0.5 } else { 1.0 };
            entities_to_render.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                effects: entity_effects,
            });
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            // Open doors in fog render even darker
            let brightness = if is_open_door { 0.25 } else { 0.5 };
            entities_to_render.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                effects: effects::NONE,
            });
        }
    }

    // Player is always rendered last (on top)
    if let Some(player) = player_render {
        entities_to_render.push(player);
    }

    entities_to_render
}
