//! Developer tool spawning functionality.
//!
//! Handles spawning entities at cursor position via the dev menu.

use crate::camera::Camera;
use crate::events::EventQueue;
use crate::systems::dev_tools::{self, DevSpawnResult};
use crate::time_system::{ActionScheduler, GameClock};
use crate::ui::DevTool;
use crate::vfx::VfxManager;
use hecs::{Entity, World};

/// Execute a dev spawn at the mouse position.
/// Returns true if a VFX was requested (caller should handle).
pub fn spawn_at_cursor(
    tool: DevTool,
    mouse_pos: (f32, f32),
    camera: &Camera,
    world: &mut World,
    grid: &mut crate::grid::Grid,
    player_entity: Entity,
    game_clock: &GameClock,
    action_scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
) -> bool {
    let world_pos = camera.screen_to_world(mouse_pos.0, mouse_pos.1);
    let tile_x = world_pos.x.round() as i32;
    let tile_y = world_pos.y.round() as i32;

    let result = dev_tools::execute_dev_spawn(
        world,
        grid,
        tool,
        tile_x,
        tile_y,
        player_entity,
        game_clock,
        action_scheduler,
        events,
    );

    matches!(result, DevSpawnResult::VfxRequested)
}

/// Handle VFX spawning after a dev tool action.
pub fn spawn_vfx_for_tool(tool: DevTool, tile_x: i32, tile_y: i32, vfx: &mut VfxManager) {
    if matches!(tool, DevTool::SpawnFire) {
        vfx.spawn_fire(tile_x as f32 + 0.5, tile_y as f32 + 0.5);
    }
}
