//! Developer tools: the state behind the docked panel (`ui::devtools`), the
//! cursor tools it arms, and the commands it sends.
//!
//! The panel only ever reads the world. Anything it wants changed it queues
//! as a `DevCommand`, or arms a `CursorTool` that the engine applies where
//! the map is clicked; both are carried out here with the whole `GameState`
//! to hand, so a spawn, a paint or a teleport keeps the spatial cache, the AI
//! and the field of view in step.
//!
//! Dev randomness comes from its own entropy-seeded rng: placing things by
//! hand must never shift a seeded run's stream.

pub mod paint;
pub mod spawn;

use hecs::{Entity, World};
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::components::{Inventory, ItemInstance, ItemType};
use crate::engine::GameState;
use crate::events::EventQueue;
use crate::tile::TileType;
use crate::ui::RainStyle;
use crate::vfx::VfxManager;

pub use spawn::{SpawnOptions, Spawnable, Wakefulness};

/// The panel's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DevTab {
    #[default]
    Spawn,
    Paint,
    Effects,
}

impl DevTab {
    pub const ALL: [DevTab; 3] = [DevTab::Spawn, DevTab::Paint, DevTab::Effects];

    pub fn name(self) -> &'static str {
        match self {
            DevTab::Spawn => "Spawn",
            DevTab::Paint => "Paint",
            DevTab::Effects => "Effects",
        }
    }
}

/// What a click on the map does while a tool is armed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorTool {
    /// Place something on the clicked tile.
    Place(Spawnable),
    /// Paint terrain under the cursor (click or drag).
    Paint(TileType),
    /// A looping particle fire on the tile: purely visual, nothing burns.
    ParticleFire,
}

impl CursorTool {
    pub fn describe(&self) -> String {
        match self {
            CursorTool::Place(what) => format!("Place {}", what.name()),
            CursorTool::Paint(tile) => format!("Paint {tile:?}"),
            CursorTool::ParticleFire => "Particle fire (visual only)".into(),
        }
    }

    /// Whether holding the button and dragging keeps applying it.
    pub fn drags(&self) -> bool {
        matches!(self, CursorTool::Paint(_))
    }
}

/// Something the panel asked for that isn't a click on the map.
#[derive(Debug, Clone, PartialEq)]
pub enum DevCommand {
    /// Put `count` of an item in the player's pack (gear rolled at the
    /// panel's rarity), ignoring the weight limit.
    GiveItem { kind: ItemType, count: u32 },
    /// Remove every particle fire placed with `CursorTool::ParticleFire`.
    ClearParticleFires,
}

/// Everything the developer tools remember between frames.
pub struct DevTools {
    pub visible: bool,
    pub tab: DevTab,
    /// The armed cursor tool, if any.
    pub tool: Option<CursorTool>,
    pub spawn_options: SpawnOptions,
    /// Paint brush radius: 0 paints one tile, 1 a 3x3 square, 2 a 5x5.
    pub brush: i32,
    /// Filters the lists on the spawn tab.
    pub search: String,
    /// How Call Rain is drawn, while we choose between the looks.
    pub rain_style: RainStyle,
    /// Queued by the panel, carried out by `apply_commands`.
    pub commands: Vec<DevCommand>,
    /// Width of the docked panel last frame, in physical pixels (0 while
    /// hidden), for the camera to keep the map centred beside it.
    pub panel_px: f32,
    /// One line of feedback for the panel ("Placed 3 Goblins").
    pub status: Option<String>,
    /// Tile the mouse button went down on (and has dragged to), while a
    /// tool is held down.
    held_at: Option<(i32, i32)>,
    rng: StdRng,
}

impl Default for DevTools {
    fn default() -> Self {
        Self {
            visible: false,
            tab: DevTab::default(),
            tool: None,
            spawn_options: SpawnOptions::default(),
            brush: 0,
            search: String::new(),
            rain_style: RainStyle::default(),
            commands: Vec::new(),
            panel_px: 0.0,
            status: None,
            held_at: None,
            rng: StdRng::from_entropy(),
        }
    }
}

impl DevTools {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.disarm();
        }
    }

    /// Whether a map click goes to a tool rather than to play.
    pub fn has_active_tool(&self) -> bool {
        self.visible && self.tool.is_some()
    }

    pub fn disarm(&mut self) {
        self.tool = None;
        self.held_at = None;
    }

    /// The mouse went down on `at` with a tool armed: apply it once.
    pub fn press(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager, at: (i32, i32)) {
        self.held_at = Some(at);
        self.use_tool(state, events, vfx, at);
    }

    /// The mouse is held at `at`: a dragging tool applies again on each new
    /// tile it passes over.
    pub fn drag(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager, at: (i32, i32)) {
        let Some(tool) = self.tool else {
            return;
        };
        if self.held_at.is_some() && self.held_at != Some(at) && tool.drags() {
            self.held_at = Some(at);
            self.use_tool(state, events, vfx, at);
        }
    }

    pub fn release(&mut self) {
        self.held_at = None;
    }

    pub fn is_held(&self) -> bool {
        self.held_at.is_some()
    }

    /// Apply the armed tool at `at`.
    pub fn use_tool(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager, at: (i32, i32)) {
        let Some(tool) = self.tool else {
            return;
        };
        match tool {
            CursorTool::Place(what) => {
                let floor = state.current_floor;
                let options = self.spawn_options;
                let result = spawn::spawn(&mut state.actor_ctx(events), what, at, &options, floor, &mut self.rng);
                self.status = Some(match result {
                    Ok(made) if made.len() > 1 => format!("Placed {} x {}", made.len(), what.name()),
                    Ok(_) => format!("Placed {}", what.name()),
                    Err(why) => format!("Can't place there: {why}"),
                });
                state.fov_dirty = true;
            }
            CursorTool::Paint(tile) => {
                if paint::paint(&mut state.world, &mut state.grid, at, self.brush, tile) {
                    state.fov_dirty = true;
                }
            }
            CursorTool::ParticleFire => vfx.spawn_fire(at.0 as f32 + 0.5, at.1 as f32 + 0.5),
        }
    }

    /// Carry out everything the panel queued this frame.
    pub fn apply_commands(&mut self, state: &mut GameState, vfx: &mut VfxManager) {
        for command in std::mem::take(&mut self.commands) {
            match command {
                DevCommand::GiveItem { kind, count } => {
                    for _ in 0..count {
                        give_item(&mut state.world, state.player_entity, kind, &self.spawn_options, &mut self.rng);
                    }
                    let name = crate::systems::items::item_name(kind);
                    self.status = Some(format!("Gave {count} x {name}"));
                }
                DevCommand::ClearParticleFires => vfx.fires.clear(),
            }
        }
    }
}

/// Put one of `kind` in `player`'s pack, ignoring the weight limit. Gear is
/// rolled at the panel's rarity (accessories at least Magic, as they carry
/// nothing else).
fn give_item(world: &mut World, player: Entity, kind: ItemType, options: &SpawnOptions, rng: &mut StdRng) {
    use crate::systems::item_defs::{get_def, roll_gear_with_rarity, ItemCategory};
    let gear = matches!(get_def(kind).category, ItemCategory::Weapon | ItemCategory::Armor | ItemCategory::Accessory);
    let item = if gear { roll_gear_with_rarity(kind, options.rarity, rng) } else { ItemInstance::plain(kind) };
    if let Ok(mut inventory) = world.get::<&mut Inventory>(player) {
        inventory.current_weight_kg += crate::systems::items::item_weight(kind);
        inventory.items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::spawn::{DoorKind, BOSSES, ENEMIES};
    use super::*;
    use crate::components::{
        Actor, DungeonTrapKind, FurnitureKind, LanternStyle, PlayerClass, Position,
    };
    use crate::engine::GameEngine;

    fn live_run() -> GameEngine {
        let mut engine = GameEngine::new();
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        engine.start_game(PlayerClass::Fighter, 77, &mut camera);
        engine
    }

    #[test]
    fn placing_every_spawnable_keeps_the_world_coherent() {
        let mut engine = live_run();
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let player = state.player_entity;
        let near = state.world.get::<&Position>(player).map(|p| (p.x, p.y)).unwrap();

        let everything: Vec<Spawnable> = (0..ENEMIES.len())
            .map(Spawnable::Enemy)
            .chain(BOSSES.iter().map(|(a, _)| Spawnable::Boss(*a)))
            .chain([
                Spawnable::Wizard,
                Spawnable::Merchant,
                Spawnable::Item(ItemType::Sword),
                Spawnable::Item(ItemType::HealthPotion),
                Spawnable::Campfire,
                Spawnable::Brazier,
                Spawnable::Lantern(LanternStyle::Ghost),
                Spawnable::FirePatch,
                Spawnable::DroppedTorch,
                Spawnable::OilBarrel,
                Spawnable::OilPuddle,
                Spawnable::Web,
                Spawnable::RainCloud,
                Spawnable::Furniture(FurnitureKind::Altar),
                Spawnable::Trap(DungeonTrapKind::Spike),
                Spawnable::Chest,
                Spawnable::Door(DoorKind::Grated),
                Spawnable::SecretDoor,
                Spawnable::Stalagmites,
                Spawnable::GlowMushrooms,
                Spawnable::CrystalCluster,
            ])
            .collect();

        let mut dev = DevTools::new();
        dev.visible = true;
        dev.spawn_options.count = 2;
        for what in everything {
            dev.tool = Some(CursorTool::Place(what));
            dev.use_tool(state, &mut events, &mut vfx, near);
            assert!(
                dev.status.as_deref().is_some_and(|s| s.starts_with("Placed")),
                "{}: {:?}",
                what.name(),
                dev.status
            );
        }
        state.spatial_cache.assert_coherent_with_world(&state.world, "after dev placement");
        for (id, _) in state.world.query::<&Actor>().iter().filter(|(id, _)| *id != player) {
            assert!(state.active_ai_tracker.is_tracked(id), "every creature placed is known to the AI");
        }

        // Painting stairs moves the marker and asks for a fresh field of view.
        state.fov_dirty = false;
        dev.tool = Some(CursorTool::Paint(TileType::StairsDown));
        dev.use_tool(state, &mut events, &mut vfx, (near.0 + 1, near.1));
        assert_eq!(state.grid.stairs_down_pos, Some((near.0 + 1, near.1)));
        assert!(state.fov_dirty);
    }

    #[test]
    fn giving_an_item_puts_it_in_the_pack() {
        let mut engine = live_run();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut dev = DevTools::new();
        dev.commands.push(DevCommand::GiveItem { kind: ItemType::ScrollOfFireball, count: 3 });
        dev.apply_commands(state, &mut vfx);
        let carried = state
            .world
            .get::<&Inventory>(state.player_entity)
            .unwrap()
            .items
            .iter()
            .filter(|i| i.kind == ItemType::ScrollOfFireball)
            .count();
        assert!(carried >= 3);
    }
}
