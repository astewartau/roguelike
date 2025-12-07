//! Animation systems for visual effects.

use crate::components::{LungeAnimation, Position, VisualPosition};
use crate::constants::*;
use hecs::World;

/// Smoothly interpolate visual positions toward logical positions
pub fn visual_lerp(world: &mut World, dt: f32) {
    let lerp_speed = dt * VISUAL_LERP_SPEED;
    for (_id, (pos, vis_pos, lunge)) in
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
            // Normal interpolation
            let tx = pos.x as f32;
            let ty = pos.y as f32;
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
