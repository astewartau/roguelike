//! Rendering-related systems and data structures.

use crate::components::{Actor, AnimatedSprite, BlocksVision, Door, EffectType, LightSource, OverlaySprite, Position, Sprite, VisualPosition};
use crate::fov::FOV;
use crate::grid::Grid;
use hecs::{Entity, World};
use std::collections::HashSet;

/// Ambient brightness at the edge of FOV (base illumination in visible area)
const AMBIENT_BRIGHTNESS: f32 = 0.3;
/// Brightness for explored but not visible tiles (fog of war)
/// Must be less than AMBIENT_BRIGHTNESS to avoid jarring edge at FOV boundary
const FOG_BRIGHTNESS: f32 = 0.2;

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

/// Update visibility based on illumination and line-of-sight.
///
/// A tile is visible if:
/// - Player has line-of-sight to it (no walls blocking)
/// - AND it's illuminated (by player's light OR by a light source player can see)
///
/// This means you can see distant lit areas (campfires) as long as nothing blocks your view.
/// Also applies magical reveal effects (Scroll of Reveal) based on current time.
pub fn update_fov(world: &World, grid: &mut Grid, player_entity: Entity, radius: i32, current_time: f32) {
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };
    let player_x = player_pos.x;
    let player_y = player_pos.y;

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

    // 1. Player's personal light - use shadowcasting for efficiency
    let player_lit_tiles = FOV::calculate(
        grid,
        player_x,
        player_y,
        radius,
        Some(|x: i32, y: i32| blocking_positions.contains(&(x, y))),
    );
    for (x, y) in player_lit_tiles {
        if let Some(tile) = grid.get_mut(x, y) {
            tile.visible = true;
            tile.explored = true;
        }
    }

    // 2. External light sources - tiles are visible if:
    //    a) Light can reach the tile (light has LOS to tile)
    //    b) Player can see the tile (player has LOS to tile)
    //    This allows seeing lit areas even when the light source itself is hidden.
    let light_sources: Vec<(i32, i32, f32)> = world
        .query::<(&Position, &LightSource)>()
        .iter()
        .map(|(_, (pos, light))| (pos.x, pos.y, light.radius))
        .collect();

    for (light_x, light_y, light_radius) in light_sources {
        let r = light_radius.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = light_x + dx;
                let y = light_y + dy;

                // Skip out of bounds
                if x < 0 || y < 0 || x >= grid.width as i32 || y >= grid.height as i32 {
                    continue;
                }

                // Skip if already visible
                if let Some(tile) = grid.get(x, y) {
                    if tile.visible {
                        continue;
                    }
                }

                // Check if within light radius
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                if dist > light_radius {
                    continue;
                }

                // Check if light can reach this tile (light propagation)
                if !has_line_of_sight(grid, &blocking_positions, light_x, light_y, x, y) {
                    continue;
                }

                // Check if player can see this tile
                if has_line_of_sight(grid, &blocking_positions, player_x, player_y, x, y) {
                    if let Some(tile) = grid.get_mut(x, y) {
                        tile.visible = true;
                        tile.explored = true;
                    }
                }
            }
        }
    }
}

/// Check if there's a clear line of sight between two points.
/// Uses Bresenham's line algorithm to check for blocking tiles.
fn has_line_of_sight(
    grid: &Grid,
    blocking_entities: &HashSet<(i32, i32)>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> bool {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        // Don't check the destination tile itself
        if x == x1 && y == y1 {
            break;
        }

        // Check if this tile blocks vision
        if let Some(tile) = grid.get(x, y) {
            if tile.tile_type.blocks_vision() {
                return false;
            }
        }

        // Check if an entity blocks vision here
        if blocking_entities.contains(&(x, y)) {
            return false;
        }
    }

    true
}

/// Calculate per-tile illumination from player and light sources.
/// Light sources contribute if they have LOS to the tile (light propagation).
/// Must be called after update_fov (requires visible flags to be set).
pub fn calculate_illumination(
    world: &World,
    grid: &mut Grid,
    player_entity: Entity,
    fov_radius: i32,
) {
    let width = grid.width;
    let height = grid.height;

    // Reset illumination
    for illum in &mut grid.illumination {
        *illum = 0.0;
    }

    // Get player position
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };
    let player_x = player_pos.x;
    let player_y = player_pos.y;
    let player_light_radius = fov_radius as f32;

    // Collect all light sources (we'll check LOS per-tile)
    let light_sources: Vec<(i32, i32, f32, f32)> = world
        .query::<(&Position, &LightSource)>()
        .iter()
        .map(|(_, (pos, light))| (pos.x, pos.y, light.radius, light.intensity))
        .collect();

    // Collect positions of entities that block vision (for light LOS checks)
    let blocking_positions: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate illumination for each tile
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let idx = y as usize * width + x as usize;
            let tile = &grid.tiles[idx];

            if !tile.visible && !tile.explored {
                // Completely unexplored - no illumination
                continue;
            }

            if !tile.visible {
                // Explored but not visible - fog of war brightness
                grid.illumination[idx] = FOG_BRIGHTNESS;
                continue;
            }

            // Tile is visible - calculate illumination from player
            let dx = (x - player_x) as f32;
            let dy = (y - player_y) as f32;
            let distance = (dx * dx + dy * dy).sqrt();

            // Player light: smooth quadratic falloff
            let t = (1.0 - distance / player_light_radius).max(0.0);
            let player_contrib = AMBIENT_BRIGHTNESS + (1.0 - AMBIENT_BRIGHTNESS) * t * t;
            let mut total_illumination = player_contrib;

            // Add contributions from light sources that can reach this tile
            for &(light_x, light_y, radius, intensity) in &light_sources {
                let ldx = (x - light_x) as f32;
                let ldy = (y - light_y) as f32;
                let light_dist = (ldx * ldx + ldy * ldy).sqrt();

                // Check if within light radius
                if light_dist > radius {
                    continue;
                }

                // Check if light can reach this tile (LOS from light to tile)
                if !has_line_of_sight(grid, &blocking_positions, light_x, light_y, x, y) {
                    continue;
                }

                let lt = 1.0 - (light_dist / radius);
                let contribution = intensity * lt * lt;
                total_illumination += contribution;
            }

            // Clamp to reasonable range (allow slight overbrightness for bloom-like effect)
            grid.illumination[idx] = total_illumination.min(1.5);
        }
    }
}

/// Collect entities that should be rendered, with fog of war applied.
/// Entities are sorted by layer: ground items first, then actors, then player on top.
pub fn collect_renderables(world: &World, grid: &Grid, player_entity: Entity, real_time: f32) -> Vec<RenderEntity> {
    // Separate entities by render layer
    let mut ground_layer: Vec<RenderEntity> = Vec::new();  // Bones, items on ground, campfires
    let mut actor_layer: Vec<RenderEntity> = Vec::new();   // Enemies and NPCs
    let mut player_render: Option<RenderEntity> = None;

    // Process entities with static Sprite component
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
            // Use tile illumination for entity brightness (+0.1 to stand out, capped at 1.0)
            let tile_brightness = grid.illumination
                .get(pos.y as usize * grid.width + pos.x as usize)
                .copied()
                .unwrap_or(0.5);
            let brightness = if is_open_door {
                tile_brightness * 0.5
            } else {
                (tile_brightness + 0.1).min(1.0)
            };
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

    // Process entities with AnimatedSprite component (e.g., campfires)
    for (_, (pos, vis_pos, anim)) in
        world.query::<(&Position, &VisualPosition, &AnimatedSprite)>().iter()
    {
        let (is_explored, is_visible) = grid
            .get(pos.x, pos.y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Calculate current frame based on real time
        let current_tile_id = anim.current_tile_id(real_time);
        let sprite = Sprite::new(anim.sheet, current_tile_id);

        if is_visible {
            let tile_brightness = grid.illumination
                .get(pos.y as usize * grid.width + pos.x as usize)
                .copied()
                .unwrap_or(0.5);
            // Animated sprites (like fire) render at full tile brightness
            ground_layer.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite,
                brightness: tile_brightness,
                alpha: 1.0,
                effects: effects::NONE,
                overlay: None,
            });
        } else if is_explored {
            // Show in fog at reduced brightness
            ground_layer.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite,
                brightness: 0.5,
                alpha: 1.0,
                effects: effects::NONE,
                overlay: None,
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
