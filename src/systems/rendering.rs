//! Rendering-related systems and data structures.

use crate::components::{Actor, AnimatedSprite, BlocksVision, CarriedTorch, Container, Door, EffectType, GroundItemPile, OverlaySprite, FlameOverlay, Position, Projectile, ProjectileKind, Sprite, SpriteTint, StatusEffects, VisualPosition};
use crate::constants::{FIRE_FRAME_DURATION, HELD_TORCH_FRAME_DURATION, INVISIBLE_PLAYER_ALPHA, PROJECTILE_TRAIL};
use crate::tile::{SpriteSheet, tile_ids};
use crate::constants::{OILED_SPRITE_TINT, WET_SPRITE_TINT};
use crate::fov::Fov;
use crate::grid::Grid;
use hecs::Entity;
use crate::world::World;
use std::collections::HashSet;

/// Ambient brightness at the edge of FOV (base illumination in visible area)
const AMBIENT_BRIGHTNESS: f32 = 0.3;
/// Brightness for explored but not visible tiles (fog of war); see the
/// constant's own docs for the limits on it.
const FOG_BRIGHTNESS: f32 = crate::constants::LIGHT_FOG_BRIGHTNESS;

/// Visual effect flags (bitfield)
pub mod effects {
    pub const NONE: u32 = 0;
    // pub const POISONED: u32 = 1 << 0;
    /// Fire overlay - rendered in final pass on top of all entities
    pub const BURNING: u32 = 1 << 1;
    // pub const FROZEN: u32 = 1 << 2;
    // pub const SHIELDED: u32 = 1 << 3;
}

/// Default (no-op) sprite tint
const NO_TINT: (f32, f32, f32) = (1.0, 1.0, 1.0);

/// Entity ready for rendering with all visual state
pub struct RenderEntity {
    pub x: f32,
    pub y: f32,
    pub sprite: Sprite,
    pub brightness: f32,
    pub alpha: f32, // Transparency (1.0 = opaque, 0.0 = invisible)
    #[allow(dead_code)] // Reserved for shader effect flags
    pub effects: u32, // Bitfield of active effects
    /// Drawn over the sprite, in order: an archer's bow or a ground item's
    /// badge, then a carried torch.
    pub overlays: Vec<Overlay>,
    /// RGB color tint multiplied with the sprite texture (1,1,1 = untinted)
    pub tint: (f32, f32, f32),
}

/// A sprite drawn over an entity's own, untinted.
#[derive(Debug, Clone, Copy)]
pub struct Overlay {
    pub sprite: Sprite,
    /// Shift from the entity's position, in tiles (world y points up). A held
    /// torch moves to its carrier's hand.
    pub offset: (f32, f32),
    /// Flipped left to right.
    pub mirrored: bool,
}

impl Overlay {
    /// Drawn square over the entity, as it is on the sheet.
    pub fn in_place(sprite: Sprite) -> Self {
        Self { sprite, offset: (0.0, 0.0), mirrored: false }
    }
}

/// The torch a creature carries, in the off hand of the sprite it's drawn
/// with: the lit one flickers (frames desynced per carrier so a band of
/// goblins doesn't pulse in step), the unlit one doesn't.
fn held_torch_overlay(carrier: Entity, sprite: &Sprite, lit: bool, real_time: f32) -> Overlay {
    let hand = tile_ids::off_hand_anchor((sprite.sheet, sprite.tile_id));
    let (grip_x, grip_y) = tile_ids::HELD_TORCH_GRIP;
    // Mirroring the cell mirrors where the grip sits in it.
    let grip_x = if hand.mirrored { tile_ids::CELL_PX - 1 - grip_x } else { grip_x };
    let cell = tile_ids::CELL_PX as f32;
    // Sheet pixels count down from the top; world y counts up.
    let offset = ((hand.x - grip_x) as f32 / cell, (grip_y - hand.y) as f32 / cell);
    let tile_id = if lit {
        let frame = (real_time / HELD_TORCH_FRAME_DURATION) as u32 + carrier.id();
        tile_ids::HELD_TORCH.1 + frame % tile_ids::HELD_TORCH_FRAMES
    } else {
        tile_ids::HELD_TORCH_UNLIT.1
    };
    Overlay {
        sprite: Sprite::new(tile_ids::HELD_TORCH.0, tile_id),
        offset,
        mirrored: hand.mirrored,
    }
}

/// The cell a projectile is drawn with in flight, turned to face its heading:
/// arrows, grave bolts and fireballs have art for every heading. A thrown
/// potion keeps its own sprite.
fn flight_sprite(projectile: &Projectile, own: Sprite) -> Sprite {
    let first = match projectile.kind {
        ProjectileKind::Missile if projectile.potion_type.is_some() => return own,
        ProjectileKind::Missile => tile_ids::ARROW_FLIGHT,
        ProjectileKind::GraveBolt => tile_ids::GRAVE_BOLT_FLIGHT,
        ProjectileKind::Fireball => tile_ids::FIREBALL_FLIGHT,
    };
    Sprite::new(first.0, first.1 + tile_ids::flight_heading(projectile.direction))
}

/// A projectile as drawn: faded copies trailing behind it while it is still
/// moving (`PROJECTILE_TRAIL`, a motion blur), then the projectile itself.
fn projectile_renders(
    projectile: &Projectile,
    pos: &Position,
    vis: &VisualPosition,
    sprite: Sprite,
    brightness: f32,
    tint: (f32, f32, f32),
) -> Vec<RenderEntity> {
    let at = |x: f32, y: f32, alpha: f32| RenderEntity {
        x,
        y,
        sprite,
        brightness,
        alpha,
        effects: effects::NONE,
        overlays: Vec::new(),
        tint,
    };
    let moving = (pos.x as f32 - vis.x).hypot(pos.y as f32 - vis.y) > 0.05;
    let (dx, dy) = projectile.direction;
    let mut out: Vec<RenderEntity> = Vec::new();
    if moving {
        // Furthest back first, so nearer copies draw over it.
        for &(behind, alpha) in PROJECTILE_TRAIL.iter().rev() {
            out.push(at(vis.x - dx * behind, vis.y - dy * behind, alpha));
        }
    }
    out.push(at(vis.x, vis.y, 1.0));
    out
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
    let player_lit_tiles = Fov::calculate(
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
    //    a) Some of the light reaches the tile (through open ground; walls and
    //       closed doors stop it, see systems::light::LightOcclusion)
    //    b) Player can see the tile (player has LOS to tile)
    //    This allows seeing lit areas even when the light source itself is hidden.
    let occlusion = crate::systems::light::LightOcclusion::build(world, grid);
    let light_sources: Vec<(i32, i32, f32)> = crate::systems::light::light_emitters(world)
        .into_iter()
        .map(|((x, y), light)| (x, y, light.radius))
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
                if occlusion.between((light_x, light_y), (x, y)) <= 0.0 {
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

/// Calculate per-tile illumination from the player's light and light sources.
/// `player_light` is whose light the scene is seen by: None for the
/// editor's even lighting (no pool around anyone), as with no player.
/// Light sources contribute what reaches the tile through walls, doors and
/// objects (`systems::light::light_reaching`).
/// Must be called after update_fov (requires visible flags to be set).
pub fn calculate_illumination(
    world: &World,
    grid: &mut Grid,
    player_light: Option<Entity>,
    fov_radius: i32,
) {
    let width = grid.width;
    let height = grid.height;

    // Reset illumination
    for illum in &mut grid.illumination {
        *illum = 0.0;
    }

    // Where the player stands. Without their light (the editor, or nobody
    // there), whatever is in view is lit evenly, with no pool around anyone.
    let player_pos = player_light.and_then(|e| world.get::<&Position>(e).ok()).map(|p| (p.x, p.y));
    let player_light_radius = fov_radius as f32;

    // Collect all light sources, and what the floor lets through between them
    // and each tile.
    let light_sources = crate::systems::light::light_emitters(world);
    let occlusion = crate::systems::light::LightOcclusion::build(world, grid);

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
            let player_contrib = match player_pos {
                Some((player_x, player_y)) => {
                    let dx = (x - player_x) as f32;
                    let dy = (y - player_y) as f32;
                    let distance = (dx * dx + dy * dy).sqrt();

                    // Player light: smooth quadratic falloff
                    let t = (1.0 - distance / player_light_radius).max(0.0);
                    AMBIENT_BRIGHTNESS + (1.0 - AMBIENT_BRIGHTNESS) * t * t
                }
                None => AMBIENT_BRIGHTNESS,
            };
            let mut total_illumination = player_contrib;

            // Add what each light source gets through to this tile
            for (src, light) in &light_sources {
                total_illumination +=
                    crate::systems::light::light_reaching(&occlusion, *src, light, (x, y));
            }

            // Clamp to reasonable range (allow slight overbrightness for bloom-like effect)
            grid.illumination[idx] = total_illumination.min(1.5);
        }
    }
}

/// Multiply a Wet (bluish) or Oiled (greasy) look into an entity's tint, so
/// the surface status reads on the sprite as well as in the HUD.
fn surface_status_tint(world: &World, id: Entity, (r, g, b): (f32, f32, f32)) -> (f32, f32, f32) {
    let status = if super::effects::entity_has_effect(world, id, EffectType::Wet) {
        WET_SPRITE_TINT
    } else if super::effects::entity_has_effect(world, id, EffectType::Oiled) {
        OILED_SPRITE_TINT
    } else {
        return (r, g, b);
    };
    (r * status.0, g * status.1, b * status.2)
}

/// Collect entities that should be rendered, with fog of war applied.
/// Entities are sorted by layer: ground items first, then actors, then player on top.
pub fn collect_renderables(world: &World, grid: &Grid, player_entity: Entity, real_time: f32) -> Vec<RenderEntity> {
    // Separate entities by render layer
    let mut ground_layer: Vec<RenderEntity> = Vec::new();  // Bones, items on ground, campfires
    let mut actor_layer: Vec<RenderEntity> = Vec::new();   // Enemies and NPCs
    let mut air_layer: Vec<RenderEntity> = Vec::new();     // Projectiles in flight
    let mut player_render: Option<RenderEntity> = None;

    // Process entities with static Sprite component
    for (id, (pos, vis_pos, sprite)) in
        world.query::<(&Position, &VisualPosition, &Sprite)>().iter()
    {
        // Anything other than the player that is invisible is simply not
        // drawn. The player still sees themselves, faded (below).
        if id != player_entity && super::effects::entity_has_effect(world, id, EffectType::Invisible) {
            continue;
        }

        let (is_explored, is_visible) = grid
            .get(pos.x, pos.y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Actors (enemies) are only visible in FOV, not in fog
        let is_actor = world.get::<&Actor>(id).is_ok();

        // Check if this is an open door (render darker)
        let is_open_door = world.get::<&Door>(id).map(|door| door.is_open).unwrap_or(false);

        // A pile of items shows whatever is on top of it, badge and all, so
        // taking that item uncovers the next rather than leaving its picture
        // behind. (A pile of nothing but gold keeps its coins.)
        let pile_top = if world.get::<&GroundItemPile>(id).is_ok() {
            world.get::<&Container>(id).ok().and_then(|c| c.items.first().map(|i| i.kind))
        } else {
            None
        };
        let sprite = &pile_top
            .map(|kind| Sprite::from_ref(crate::systems::item_defs::get_def(kind).sprite))
            .unwrap_or(*sprite);

        // Overlays: a bow on an archer, a ground item's badge, a carried torch.
        let mut overlays: Vec<Overlay> = Vec::new();
        if let Ok(o) = world.get::<&OverlaySprite>(id) {
            overlays.push(Overlay::in_place(Sprite::new(o.sheet, o.tile_id)));
        }
        if let Some(badge) = pile_top.and_then(crate::systems::item_defs::badge) {
            overlays.push(Overlay::in_place(Sprite::from_ref(badge)));
        }
        if let Ok(torch) = world.get::<&CarriedTorch>(id) {
            overlays.push(held_torch_overlay(id, sprite, torch.lit, real_time));
        }

        // Per-entity color tint (e.g., fire arrows), lifted toward the
        // hit-flash colour while this entity is still flashing from a hit.
        let base_tint = world
            .get::<&SpriteTint>(id)
            .map(|t| (t.r, t.g, t.b))
            .unwrap_or(NO_TINT);
        let base_tint = surface_status_tint(world, id, base_tint);
        let tint = super::animation::hit_flash_tint(world, id, id == player_entity, base_tint);

        let entity_effects = effects::NONE;

        // Projectiles fly over everyone's heads, facing where they're going.
        // Spells carry their own light; an arrow is as lit as where it is.
        if let Ok(projectile) = world.get::<&Projectile>(id) {
            if is_visible {
                let brightness = match projectile.kind {
                    ProjectileKind::GraveBolt | ProjectileKind::Fireball => 1.0,
                    ProjectileKind::Missile => grid
                        .illumination
                        .get(pos.y as usize * grid.width + pos.x as usize)
                        .map_or(0.5, |b| (b + 0.1).min(1.0)),
                };
                let sprite = flight_sprite(&projectile, *sprite);
                air_layer.extend(projectile_renders(&projectile, pos, vis_pos, sprite, brightness, tint));
            }
            continue;
        }

        if id == player_entity {
            // Check if player is invisible for transparency effect
            let alpha = if super::effects::entity_has_effect(world, id, EffectType::Invisible) {
                INVISIBLE_PLAYER_ALPHA
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
                overlays,
                tint,
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
                overlays,
                tint,
            };
            // Actors go on top layer, everything else on ground layer
            if is_actor {
                actor_layer.push(render_entity);
            } else {
                ground_layer.push(render_entity);
            }
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            // Use consistent fog brightness for all fog entities
            ground_layer.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness: FOG_BRIGHTNESS,
                alpha: 1.0,
                effects: effects::NONE,
                // Only non-actors are remembered in fog: a ground item keeps
                // its badge there.
                overlays,
                tint,
            });
        }
    }

    // Process entities with AnimatedSprite component (e.g., campfires, water)
    // Collect and sort by z_order so water renders below braziers/fire
    let mut animated_entities: Vec<_> = world
        .query::<(&Position, &VisualPosition, &AnimatedSprite)>()
        .iter()
        .map(|(_, (pos, vis_pos, anim))| (pos.x, pos.y, vis_pos.x, vis_pos.y, *anim))
        .collect();
    // A burning fuel's flames, drawn over whatever it is.
    animated_entities.extend(
        world
            .query::<(&Position, &VisualPosition, &crate::components::Fuel)>()
            .with::<&crate::components::OnFire>()
            .iter()
            .map(|(_, (pos, vis_pos, fuel))| (pos.x, pos.y, vis_pos.x, vis_pos.y, fuel.lit_look)),
    );
    animated_entities.sort_by_key(|(_, _, _, _, anim)| anim.z_order);

    for (pos_x, pos_y, vis_x, vis_y, anim) in animated_entities {
        let (is_explored, is_visible) = grid
            .get(pos_x, pos_y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Calculate current frame based on real time
        let current_tile_id = anim.current_tile_id(real_time);
        let sprite = Sprite::new(anim.sheet, current_tile_id);

        if is_visible {
            let tile_brightness = grid.illumination
                .get(pos_y as usize * grid.width + pos_x as usize)
                .copied()
                .unwrap_or(0.5);
            // Animated sprites (like fire) render at full tile brightness
            ground_layer.push(RenderEntity {
                x: vis_x,
                y: vis_y,
                sprite,
                brightness: tile_brightness,
                alpha: 1.0,
                effects: effects::NONE,
                overlays: Vec::new(),
                tint: NO_TINT,
            });
        } else if is_explored {
            // Show in fog at same brightness as other fog tiles
            ground_layer.push(RenderEntity {
                x: vis_x,
                y: vis_y,
                sprite,
                brightness: FOG_BRIGHTNESS,
                alpha: 1.0,
                effects: effects::NONE,
                overlays: Vec::new(),
                tint: NO_TINT,
            });
        }
    }

    // Combine layers: ground first, then actors, then player, then anything
    // in the air
    let mut entities_to_render = ground_layer;
    entities_to_render.append(&mut actor_layer);
    if let Some(player) = player_render {
        entities_to_render.push(player);
    }
    entities_to_render.append(&mut air_layer);

    // Add fire overlays for burning entities (rendered on top)
    let mut fire_overlays: Vec<RenderEntity> = Vec::new();
    // Deliberately includes invisible enemies: their flames still show, which
    // is the one way to spot them.
    for (_, (pos, vis_pos, status)) in
        world.query::<(&Position, &VisualPosition, &StatusEffects)>().iter()
    {
        // Check if entity is burning
        if status.effects.iter().any(|e| e.effect_type == EffectType::Burning) {
            let is_visible = grid
                .get(pos.x, pos.y)
                .map(|tile| tile.visible)
                .unwrap_or(false);

            if is_visible {
                // Calculate fire animation frame (row 9 has 6 frames)
                let frame_duration = FIRE_FRAME_DURATION;
                let frame_count = 6;
                let adjusted_time = real_time + (pos.x as f32 * 0.1 + pos.y as f32 * 0.07); // Phase offset
                let frame = ((adjusted_time / frame_duration) as u32) % frame_count;
                let fire_tile_id = tile_ids::FIRE_EFFECT.1 + frame;

                let tile_brightness = grid.illumination
                    .get(pos.y as usize * grid.width + pos.x as usize)
                    .copied()
                    .unwrap_or(0.5);

                fire_overlays.push(RenderEntity {
                    x: vis_pos.x,
                    y: vis_pos.y,
                    sprite: Sprite::new(SpriteSheet::AnimatedTiles, fire_tile_id),
                    brightness: (tile_brightness + 0.2).min(1.0), // Fire is bright
                    alpha: 0.8, // Slightly transparent so entity shows through
                    effects: effects::BURNING, // Mark for third-pass rendering
                    overlays: Vec::new(),
                    tint: NO_TINT,
                });
            }
        }
    }

    // Flames drawn over anything that asks for them (an armed fire trap)
    for (_, (pos, vis_pos, _)) in
        world.query::<(&Position, &VisualPosition, &FlameOverlay)>().iter()
    {
        let is_visible = grid
            .get(pos.x, pos.y)
            .map(|tile| tile.visible)
            .unwrap_or(false);

        if is_visible {
            // Calculate fire animation frame (same as burning entities)
            let frame_duration = FIRE_FRAME_DURATION;
            let frame_count = 6;
            let adjusted_time = real_time + (pos.x as f32 * 0.1 + pos.y as f32 * 0.07);
            let frame = ((adjusted_time / frame_duration) as u32) % frame_count;
            let fire_tile_id = tile_ids::FIRE_EFFECT.1 + frame;

            let tile_brightness = grid.illumination
                .get(pos.y as usize * grid.width + pos.x as usize)
                .copied()
                .unwrap_or(0.5);

            fire_overlays.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: Sprite::new(SpriteSheet::AnimatedTiles, fire_tile_id),
                brightness: (tile_brightness + 0.3).min(1.2), // Trap fire is bright
                alpha: 0.9,
                effects: effects::BURNING,
                overlays: Vec::new(),
                tint: NO_TINT,
            });
        }
    }

    entities_to_render.append(&mut fire_overlays);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spawning::enemies::{GOBLIN, GOBLIN_SHAMAN};
    use crate::tile::TileType;
    use rand::{rngs::StdRng, SeedableRng};

    /// The held torch drawn on whichever entity wears `sprite`, at `real_time`.
    fn torch_on(world: &World, grid: &Grid, player: Entity, sprite: (SpriteSheet, u32), real_time: f32) -> Overlay {
        let drawn = collect_renderables(world, grid, player, real_time);
        let carrier = drawn
            .iter()
            .find(|e| (e.sprite.sheet, e.sprite.tile_id) == sprite)
            .expect("the carrier is drawn");
        *carrier
            .overlays
            .iter()
            .find(|o| o.sprite.sheet == tile_ids::HELD_TORCH.0)
            .expect("with a torch")
    }

    fn lit_grid() -> Grid {
        let mut grid = Grid::filled(9, 9, TileType::Floor);
        for tile in &mut grid.tiles {
            tile.visible = true;
            tile.explored = true;
        }
        grid
    }

    #[test]
    fn an_arrow_faces_its_heading_and_trails_only_while_it_moves() {
        use crate::components::{Projectile, ProjectileMarker};
        let mut world = World::new();
        let grid = lit_grid();
        let at = Position::new(0, 0);
        let player = world.spawn((at, VisualPosition::from_position(&at), Sprite::from_ref(tile_ids::FIGHTER)));
        // Flying straight up the screen (+y), past a rat it should draw over.
        world.spawn((Position::new(2, 4), VisualPosition { x: 2.0, y: 4.0 }, Sprite::from_ref(tile_ids::RAT), Actor::new(1.0)));
        let arrow = world.spawn((
            Position::new(2, 6),
            VisualPosition { x: 2.0, y: 4.0 },
            Sprite::from_ref(tile_ids::ARROW),
            Projectile {
                source: player,
                damage: 1,
                path: vec![],
                path_index: 0,
                direction: (0.0, 1.0),
                spawn_time: 0.0,
                finished: None,
                potion_type: None,
                on_hit_effect: None,
                hit_enemy: false,
                incendiary: false,
                kind: ProjectileKind::Missile,
                speed: crate::constants::ARROW_SPEED,
            },
            ProjectileMarker,
        ));
        let north = tile_ids::ARROW_FLIGHT.1 + tile_ids::FLIGHT_DIRECTIONS / 4;
        let arrows = |world: &World| -> Vec<(usize, f32)> {
            collect_renderables(world, &grid, player, 0.0)
                .iter()
                .enumerate()
                .filter(|(_, e)| e.sprite.tile_id == north && e.sprite.sheet == tile_ids::ARROW_FLIGHT.0)
                .map(|(i, e)| (i, e.alpha))
                .collect()
        };

        let drawn = arrows(&world);
        assert_eq!(drawn.len(), 1 + PROJECTILE_TRAIL.len(), "the arrow and its blur, all facing up");
        assert_eq!(drawn.last().map(|d| d.1), Some(1.0), "the arrow itself is drawn last, solid");
        let rat_at = collect_renderables(&world, &grid, player, 0.0)
            .iter()
            .position(|e| (e.sprite.sheet, e.sprite.tile_id) == tile_ids::RAT)
            .unwrap();
        assert!(drawn.iter().all(|&(i, _)| i > rat_at), "it flies over the rat, not under it");

        // Arrived: no blur.
        *world.get::<&mut VisualPosition>(arrow).unwrap() = VisualPosition { x: 2.0, y: 6.0 };
        assert_eq!(arrows(&world).len(), 1);
    }

    #[test]
    fn a_pile_shows_its_top_item_and_that_items_badge() {
        use crate::components::{Container, ItemInstance, ItemType};
        let mut world = World::new();
        let grid = lit_grid();
        let at = Position::new(0, 0);
        let player = world.spawn((at, VisualPosition::from_position(&at), Sprite::from_ref(tile_ids::FIGHTER)));
        let pile = crate::systems::inventory::spawn_ground_item(
            &mut world,
            3,
            3,
            ItemInstance::plain(ItemType::ScrollOfFireball),
        );
        crate::systems::inventory::spawn_ground_item(&mut world, 3, 3, ItemInstance::plain(ItemType::Sword));
        let drawn = |world: &World| {
            let all = collect_renderables(world, &grid, player, 0.0);
            let e = all.iter().find(|e| (e.x, e.y) == (3.0, 3.0)).expect("the pile is drawn");
            let badges: Vec<_> = e.overlays.iter().map(|o| (o.sprite.sheet, o.sprite.tile_id)).collect();
            ((e.sprite.sheet, e.sprite.tile_id), badges)
        };

        assert_eq!(drawn(&world), (tile_ids::SCROLL, vec![tile_ids::BADGE_FIREBALL]));
        world.get::<&mut Container>(pile).unwrap().items.remove(0);
        assert_eq!(drawn(&world), (tile_ids::SWORD, vec![]), "taking the scroll uncovers the sword");
    }

    #[test]
    fn a_carried_torch_is_drawn_in_the_hand_and_flickers_while_lit() {
        let mut world = World::new();
        let grid = lit_grid();
        let at = Position::new(0, 0);
        let player = world.spawn((at, VisualPosition::from_position(&at), Sprite::from_ref(tile_ids::FIGHTER)));
        let mut rng = StdRng::seed_from_u64(1);
        let goblin = GOBLIN.spawn(&mut world, 4, 4, &mut rng);
        GOBLIN_SHAMAN.spawn(&mut world, 6, 4, &mut rng);

        // The grip (23, 21) goes to the goblin's hand at (20, 22): three
        // pixels left and one down, and down is -y in the world.
        let lit = torch_on(&world, &grid, player, tile_ids::GOBLIN, 0.0);
        assert_eq!(lit.offset, (-3.0 / 32.0, -1.0 / 32.0));
        assert!(!lit.mirrored);
        let later = torch_on(&world, &grid, player, tile_ids::GOBLIN, HELD_TORCH_FRAME_DURATION);
        assert_ne!(lit.sprite.tile_id, later.sprite.tile_id, "the flame flickers");

        // The shaman's staff fills its right hand, so it holds the torch
        // mirrored in its left, where the mirrored grip (8, 21) already is.
        let shaman = torch_on(&world, &grid, player, tile_ids::GOBLIN_SHAMAN, 0.0);
        assert!(shaman.mirrored);
        assert_eq!(shaman.offset, (0.0, -1.0 / 32.0));

        let mut events = crate::events::EventQueue::new();
        crate::systems::torches::douse_torch(&mut world, goblin, &mut events);
        for t in [0.0, HELD_TORCH_FRAME_DURATION] {
            let out = torch_on(&world, &grid, player, tile_ids::GOBLIN, t);
            assert_eq!(out.sprite.tile_id, tile_ids::HELD_TORCH_UNLIT.1, "a doused torch is dark and still");
        }
    }
}
