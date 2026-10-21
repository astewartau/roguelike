//! The developer tools' debug overlays: what the game knows about each
//! tile and creature, drawn over the map (under the UI), as toggled on the
//! Overlays tab.

use hecs::Entity;
use crate::world::World;

use crate::camera::Camera;
use crate::components::{
    AIState, Asleep, BlocksMovement, BlocksVision, CanOpenDoors, ChaseAI, DevFrozen, Fuel, OnFire,
    Position, RainCloud, Sneaking, VisualPosition, WetTile,
};
use crate::devtools::DevTools;
use crate::grid::Grid;

use super::vfx::tile_rect;

/// Draw every overlay that's switched on.
pub fn draw_dev_overlays(
    ctx: &egui::Context,
    dev: &DevTools,
    camera: &Camera,
    world: &World,
    grid: &Grid,
    player: Entity,
    hover: (i32, i32),
) {
    let o = &dev.overlays;
    if !dev.visible || !(o.light || o.noise || o.ai || o.path || o.blockers || o.fire_and_water || o.coords) {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("dev_overlays")));
    let ppp = ctx.pixels_per_point();
    let cell = |x: i32, y: i32| tile_rect(camera, ppp, x as f32 + 0.5, y as f32 + 0.5);
    let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();
    let in_view = |x: i32, y: i32| x >= min_x && x <= max_x && y >= min_y && y <= max_y;
    let font = egui::FontId::monospace(10.0);

    if o.light {
        for y in min_y.max(0)..=max_y.min(grid.height as i32 - 1) {
            for x in min_x.max(0)..=max_x.min(grid.width as i32 - 1) {
                if !grid.get(x, y).is_some_and(|t| t.explored) {
                    continue;
                }
                let b = grid.illumination.get(y as usize * grid.width + x as usize).copied().unwrap_or(0.0);
                let t = (b / 1.5).clamp(0.0, 1.0);
                let lerp = |a: f32, z: f32| (a + (z - a) * t) as u8;
                let colour =
                    egui::Color32::from_rgba_unmultiplied(lerp(40.0, 255.0), lerp(40.0, 225.0), lerp(140.0, 110.0), 110);
                painter.rect_filled(cell(x, y), 0.0, colour);
            }
        }
    }

    if o.noise {
        let reach = crate::systems::noise::noise_reach(world, grid, hover, o.noise_radius);
        for (&(x, y), &cost) in &reach {
            if !in_view(x, y) {
                continue;
            }
            let strength = 1.0 - cost as f32 / (o.noise_radius as f32 + 1.0);
            let colour = egui::Color32::from_rgba_unmultiplied(255, 160, 60, (40.0 + 120.0 * strength) as u8);
            painter.rect_filled(cell(x, y), 0.0, colour);
            painter.text(cell(x, y).center(), egui::Align2::CENTER_CENTER, cost.to_string(), font.clone(), egui::Color32::BLACK);
        }
    }

    if o.blockers {
        for (_, (p, _)) in world.query::<(&Position, &BlocksVision)>().iter() {
            if in_view(p.x, p.y) {
                painter.rect_filled(cell(p.x, p.y), 0.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 120));
            }
        }
        for (_, (p, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
            if in_view(p.x, p.y) {
                let r = cell(p.x, p.y).shrink(2.0);
                let stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(230, 80, 70));
                painter.line_segment([r.left_top(), r.right_bottom()], stroke);
                painter.line_segment([r.right_top(), r.left_bottom()], stroke);
            }
        }
    }

    if o.fire_and_water {
        for (_, (p, _)) in world.query::<(&Position, &WetTile)>().iter() {
            if in_view(p.x, p.y) {
                painter.rect_filled(cell(p.x, p.y), 0.0, egui::Color32::from_rgba_unmultiplied(60, 120, 230, 70));
            }
        }
        for (_, (p, _)) in world.query::<(&Position, &Fuel)>().iter() {
            if in_view(p.x, p.y) {
                painter.rect_stroke(cell(p.x, p.y).shrink(1.0), 0.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(200, 140, 60)));
            }
        }
        for (_, (p, _, _)) in world.query::<(&Position, &Fuel, &OnFire)>().iter() {
            if in_view(p.x, p.y) {
                painter.rect_stroke(cell(p.x, p.y).shrink(1.0), 0.0, egui::Stroke::new(2.5_f32, egui::Color32::from_rgb(255, 90, 30)));
            }
        }
        for (_, (p, cloud)) in world.query::<(&Position, &RainCloud)>().iter() {
            let r = cloud.radius;
            let rect = cell(p.x - r, p.y - r).union(cell(p.x + r, p.y + r));
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(120, 180, 255)));
        }
    }

    // The AI and path overlays ask the AI's own questions, which want the
    // spatial cache: rebuilt from the world here, for reading only.
    if o.ai || o.path {
        let spatial = crate::spatial_cache::SpatialCache::rebuild_from_world(world);
        let player_pos = world.get::<&Position>(player).map(|p| (p.x, p.y)).ok();
        let sneaking = world.get::<&Sneaking>(player).is_ok();
        let tile_size = camera.zoom / ppp;

        if o.ai {
            for (id, (p, vis, ai)) in world.query::<(&Position, &VisualPosition, &ChaseAI)>().iter() {
                if !grid.get(p.x, p.y).is_some_and(|t| t.visible) {
                    continue;
                }
                let centre = tile_rect(camera, ppp, vis.x + 0.5, vis.y + 0.5).center();
                let (label, colour) = match ai.state {
                    AIState::Unaware => ("unaware", egui::Color32::from_rgb(150, 150, 150)),
                    AIState::Idle => ("idle", egui::Color32::from_rgb(200, 200, 120)),
                    AIState::Investigating => ("investigating", egui::Color32::from_rgb(240, 170, 60)),
                    AIState::Chasing => ("chasing", egui::Color32::from_rgb(240, 80, 70)),
                };
                let mut text = label.to_string();
                if world.get::<&Asleep>(id).is_ok() {
                    text.push_str(", asleep");
                }
                if world.get::<&DevFrozen>(id).is_ok() {
                    text.push_str(", frozen");
                }
                painter.text(
                    centre - egui::vec2(0.0, tile_size * 0.75),
                    egui::Align2::CENTER_BOTTOM,
                    text,
                    font.clone(),
                    colour,
                );
                // The same sight rule the AI uses: an idle one sees less of
                // you while you sneak.
                let sight = if sneaking && ai.state == AIState::Idle {
                    (((ai.sight_radius as f32) * crate::constants::SNEAK_SIGHT_MULT).round() as i32).max(1)
                } else {
                    ai.sight_radius
                };
                painter.circle_stroke(
                    centre,
                    sight as f32 * tile_size,
                    egui::Stroke::new(1.0_f32, colour.gamma_multiply(0.6)),
                );
                if let Some(target) = player_pos {
                    let sees = crate::systems::ai::can_see_target(
                        world,
                        grid,
                        (p.x, p.y),
                        target,
                        sight,
                        Some(player),
                        &spatial,
                    );
                    if sees {
                        let to = cell(target.0, target.1).center();
                        painter.line_segment([centre, to], egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 70, 60)));
                    }
                }
            }
        }

        if o.path {
            let from = dev.inspected.and_then(|e| world.get::<&Position>(e).ok().map(|p| (e, (p.x, p.y))));
            if let (Some((creature, start)), Some(goal)) = (from, player_pos) {
                let doors = world.get::<&CanOpenDoors>(creature).is_ok();
                let blocked = crate::systems::ai::ai_pathfinding_blocked(world, &spatial, doors);
                if let Some(path) = crate::pathfinding::find_path(grid, start, goal, &blocked) {
                    for (x, y) in path {
                        painter.circle_filled(cell(x, y).center(), tile_size * 0.12, egui::Color32::from_rgb(120, 230, 160));
                    }
                }
            }
        }
    }

    if o.coords && camera.zoom / ppp >= 28.0 {
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                if grid.get(x, y).is_some_and(|t| t.explored) {
                    let r = cell(x, y);
                    painter.text(
                        r.left_top() + egui::vec2(1.0, 1.0),
                        egui::Align2::LEFT_TOP,
                        format!("{x},{y}"),
                        egui::FontId::monospace(8.0),
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 140),
                    );
                }
            }
        }
    }
}
