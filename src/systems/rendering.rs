//! Rendering-related systems and data structures.

use crate::components::{Actor, BlocksVision, Door, EffectType, OverlaySprite, Position, Sprite, VisualPosition};
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
    pub alpha: f32, // Transparency (1.0 = opaque, 0.0 = invisible)
    #[allow(dead_code)] // Reserved for shader effect flags
    pub effects: u32, // Bitfield of active effects
    pub overlay: Option<Sprite>, // Optional overlay sprite (e.g., weapon)
}

/// Update field of view from player position.
/// Also applies magical reveal effects (Scroll of Reveal) based on current time.
pub fn update_fov(world: &World, grid: &mut Grid, player_entity: Entity, radius: i32, current_time: f32) {
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };

    // Clear visibility, but apply magical reveal for tiles that haven't expired
    for tile in &mut grid.tiles {
        tile.visible = false;
        // Check if tile is magically revealed
        if let Some(reveal_time) = tile.revealed_until {
            if reveal_time > current_time {
                tile.visible = true;
            } else {
                // Reveal expired, clear it
                tile.revealed_until = None;
            }
        }
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

/// Collect entities that should be rendered, with fog of war applied.
/// Entities are sorted by layer: ground items first, then actors, then player on top.
pub fn collect_renderables(world: &World, grid: &Grid, player_entity: Entity) -> Vec<RenderEntity> {
    // Separate entities by render layer
    let mut ground_layer: Vec<RenderEntity> = Vec::new();  // Bones, items on ground
    let mut actor_layer: Vec<RenderEntity> = Vec::new();   // Enemies and NPCs
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

        // Check for overlay sprite (e.g., weapon)
        let overlay = world.get::<&OverlaySprite>(id).ok().map(|o| Sprite::new(o.sheet, o.tile_id));

        let entity_effects = effects::NONE;

        if id == player_entity {
            // Check if player is invisible for transparency effect
            let alpha = if super::effects::entity_has_effect(world, id, EffectType::Invisible) {
                0.4 // Semi-transparent when invisible
            } else {
                1.0
            };

            player_render = Some(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness: 1.0,
                alpha,
                effects: effects::NONE,
                overlay,
            });
        } else if is_visible {
            // Open doors render at 50% brightness
            let brightness = if is_open_door { 0.5 } else { 1.0 };
            let render_entity = RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                alpha: 1.0,
                effects: entity_effects,
                overlay,
            };
            // Actors go on top layer, everything else on ground layer
            if is_actor {
                actor_layer.push(render_entity);
            } else {
                ground_layer.push(render_entity);
            }
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            // Open doors in fog render even darker
            let brightness = if is_open_door { 0.25 } else { 0.5 };
            ground_layer.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                alpha: 1.0,
                effects: effects::NONE,
                overlay: None, // Don't show overlays in fog
            });
        }
    }

    // Combine layers: ground first, then actors, then player on top
    let mut entities_to_render = ground_layer;
    entities_to_render.append(&mut actor_layer);
    if let Some(player) = player_render {
        entities_to_render.push(player);
    }

    entities_to_render
}

/// Reveal all tiles on the map (Scroll of Mapping effect)
pub fn reveal_entire_map(grid: &mut Grid) {
    for tile in &mut grid.tiles {
        tile.explored = true;
    }
}

/// Magically reveal tiles around all enemies (Scroll of Reveal effect).
/// Sets tiles as visible for a duration, making enemies and their surroundings
/// fully visible as if in line of sight.
pub fn reveal_enemies(world: &World, grid: &mut Grid, current_time: f32) {
    use crate::components::ChaseAI;
    use crate::constants::{REVEAL_DURATION, REVEAL_RADIUS};

    let reveal_until = current_time + REVEAL_DURATION;

    for (_, (pos, _)) in world.query::<(&Position, &ChaseAI)>().iter() {
        // Reveal tiles in a radius around each enemy
        for dy in -REVEAL_RADIUS..=REVEAL_RADIUS {
            for dx in -REVEAL_RADIUS..=REVEAL_RADIUS {
                if let Some(tile) = grid.get_mut(pos.x + dx, pos.y + dy) {
                    tile.explored = true;
                    // Set or extend the magical reveal time
                    tile.revealed_until = Some(
                        tile.revealed_until
                            .map(|t| t.max(reveal_until))
                            .unwrap_or(reveal_until)
                    );
                }
            }
        }
    }
}
