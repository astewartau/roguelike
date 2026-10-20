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

pub mod cheats;
pub mod paint;
pub mod catalog;
pub mod prefab;

pub mod spawn;

use hecs::{Entity, World};
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::component_registry as registry;
use crate::components::{AbilityType, EffectType, Inventory, ItemInstance, ItemType, Stats};
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
    Player,
    World,
    Inspect,
    Prefabs,
    Overlays,
    Effects,
}

impl DevTab {
    pub const ALL: [DevTab; 8] = [
        DevTab::Spawn,
        DevTab::Paint,
        DevTab::Player,
        DevTab::World,
        DevTab::Inspect,
        DevTab::Prefabs,
        DevTab::Overlays,
        DevTab::Effects,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DevTab::Spawn => "Spawn",
            DevTab::Paint => "Paint",
            DevTab::Player => "Player",
            DevTab::World => "World",
            DevTab::Inspect => "Inspect",
            DevTab::Prefabs => "Prefabs",
            DevTab::Overlays => "Overlays",
            DevTab::Effects => "Effects",
        }
    }
    /// A glyph for the tab's pill (from the fonts egui ships, so it always
    /// draws).
    pub fn icon(self) -> &'static str {
        match self {
            DevTab::Spawn => "🐀",
            DevTab::Paint => "🖌",
            DevTab::Player => "👤",
            DevTab::World => "🌍",
            DevTab::Inspect => "🔍",
            DevTab::Prefabs => "📦",
            DevTab::Overlays => "👁",
            DevTab::Effects => "✨",
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
    /// Move the player there.
    Teleport,
    /// Set the tile alight, and whatever stands on it.
    Ignite,
    /// Splash water over the tile and those around it.
    Douse,
    /// A fireball's blast, centred on the tile (it can hurt you).
    Blast,
    /// Whatever on the tile has health drops to none.
    Kill,
    /// Remove everything on the tile but you.
    Delete,
    /// Pick what's on the tile to look at in the Inspect tab.
    Inspect,
    /// Place one of the prefabs (by its index in `DevTools::prefabs`).
    PlacePrefab(usize),
    /// An explosion's look on the tile, and nothing else: fiery, or just
    /// the shockwave of a ground slam.
    LookOfExplosion { fiery: bool },
    /// Fire a projectile from the player at the tile, for real.
    Shoot(Shot),
}

/// What the effects bench can fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Arrow,
    FireArrow,
    GraveBolt,
    Fireball,
    WaterFlask,
    HealthPotion,
}

impl Shot {
    pub const ALL: [Shot; 6] =
        [Shot::Arrow, Shot::FireArrow, Shot::GraveBolt, Shot::Fireball, Shot::WaterFlask, Shot::HealthPotion];

    pub fn name(self) -> &'static str {
        match self {
            Shot::Arrow => "Arrow",
            Shot::FireArrow => "Fire arrow",
            Shot::GraveBolt => "Grave bolt",
            Shot::Fireball => "Fireball",
            Shot::WaterFlask => "Water flask",
            Shot::HealthPotion => "Health potion",
        }
    }
}

/// Which debug overlays are drawn over the map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlays {
    /// How lit each tile is.
    pub light: bool,
    /// How far a noise at the cursor carries.
    pub noise: bool,
    pub noise_radius: i32,
    /// Each creature's AI state and sight, and who can see you.
    pub ai: bool,
    /// The route the inspected creature would take to you.
    pub path: bool,
    /// What blocks movement and sight.
    pub blockers: bool,
    /// Wet ground, burning fuel and rain.
    pub fire_and_water: bool,
    /// Tile coordinates.
    pub coords: bool,
}

impl Default for Overlays {
    fn default() -> Self {
        Self {
            light: false,
            noise: false,
            noise_radius: crate::constants::THROWN_IMPACT_NOISE_RADIUS,
            ai: false,
            path: false,
            blockers: false,
            fire_and_water: false,
            coords: false,
        }
    }
}

impl CursorTool {
    pub fn describe(&self) -> String {
        match self {
            CursorTool::Place(what) => format!("Place {}", what.name()),
            CursorTool::Paint(tile) => format!("Paint {tile:?}"),
            CursorTool::ParticleFire => "Particle fire (visual only)".into(),
            CursorTool::Teleport => "Teleport".into(),
            CursorTool::Ignite => "Ignite".into(),
            CursorTool::Douse => "Douse".into(),
            CursorTool::Blast => "Blast".into(),
            CursorTool::Kill => "Kill".into(),
            CursorTool::Delete => "Delete".into(),
            CursorTool::Inspect => "Inspect".into(),
            CursorTool::PlacePrefab(_) => "Place prefab".into(),
            CursorTool::LookOfExplosion { fiery: true } => "Explosion (look only)".into(),
            CursorTool::LookOfExplosion { fiery: false } => "Shockwave (look only)".into(),
            CursorTool::Shoot(shot) => format!("Shoot {}", shot.name()),
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
    FullHeal,
    SetHealth { current: i32, max: i32 },
    SetStats { strength: i32, intelligence: i32, agility: i32 },
    AddXp(u32),
    AddGold(u32),
    SetMeters { hunger: f32, fatigue: f32 },
    Learn(AbilityType),
    ResetCooldowns,
    AddEffect(EffectType, f32),
    RemoveEffect(EffectType),
    ClearEffects,
    RevealMap,
    KillHostiles { visible_only: bool },
    FreezeAi(bool),
    WakeAll,
    GoToFloor(u32),
    RerollFloor,
    /// Let `n` game seconds pass (the player waits).
    AdvanceTime(f32),
    /// Replace one of an entity's components with this JSON.
    SetComponent { entity: Entity, name: &'static str, value: serde_json::Value },
    /// Give an entity a component, at its default.
    AddComponent { entity: Entity, name: &'static str },
    RemoveComponent { entity: Entity, name: &'static str },
    DeleteEntity(Entity),
    KillEntity(Entity),
    TeleportPlayerTo((i32, i32)),
    /// Rattle the camera, as a hit of this strength would.
    Shake(f32),
    /// Float a damage number over the player.
    DamageNumber(i32),
}

/// What a command needs the engine itself for, beyond the `GameState`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineRequest {
    GoToFloor(u32),
    RerollFloor,
    AdvanceTime(f32),
    /// A spell was learned: put it on the hotbar.
    Learned(AbilityType),
    /// The player moved: drop any path they were following.
    ClearPath,
    /// The player fired something: wait until it lands, so it flies.
    WaitForShots,
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
    /// The player can't be hurt (Invulnerable, kept topped up).
    pub god_mode: bool,
    /// Hunger stays full and fatigue at nothing.
    pub keep_fed: bool,
    /// Every tile is seen, fog of war or not.
    pub no_fog: bool,
    /// Multiplies how fast animations and effects play (1 is normal), for
    /// watching them slowly. Game time is unaffected.
    pub anim_speed: f32,
    /// Creatures are held still (`DevFrozen`).
    pub ai_frozen: bool,
    /// Frames per second, smoothed, for the run info.
    pub fps: f32,
    last_frame: Option<std::time::Instant>,
    /// The run's floor and seed, refreshed every frame for the panel.
    pub floor: u32,
    pub seed: u64,
    /// The panel's working values: seconds for an added status effect, the
    /// floor to jump to, and how long to let pass.
    pub effect_seconds: f32,
    pub target_floor: u32,
    pub advance_seconds: f32,
    /// The tile and entity the Inspect tab is looking at.
    pub inspect_tile: Option<(i32, i32)>,
    pub inspected: Option<Entity>,
    /// Inspector edits that didn't parse, kept so the text being typed isn't
    /// lost, with why: component name -> (draft, error).
    pub drafts: std::collections::HashMap<&'static str, (serde_json::Value, String)>,
    pub overlays: Overlays,
    /// Radius of the look-only explosion.
    pub explosion_radius: i32,
    /// The prefab library, and the one being edited.
    pub prefabs: Vec<prefab::Prefab>,
    pub selected_prefab: Option<usize>,
    /// Set by the panel when a prefab changes; saved soon after.
    pub prefabs_dirty: bool,
    /// The prefab builder's add-component picker: whether it's open, what's
    /// typed in its search box, and whether internal components are listed.
    pub picker_open: bool,
    /// The same picker, open on the Inspect tab.
    pub inspect_picker_open: bool,
    pub picker_search: String,
    pub picker_internal: bool,
    /// The prefab component whose sprite is being picked, and the sheet the
    /// picker shows.
    pub sprite_pick: Option<usize>,
    pub sprite_sheet: crate::tile::SpriteSheet,
    /// Which prefab each placed entity came from (by name), so the
    /// inspector can offer to update it from a tweaked copy.
    pub placed_from: std::collections::HashMap<Entity, String>,
    prefab_path: std::path::PathBuf,
    last_prefab_save: Option<std::time::Instant>,
    /// Asked for by commands, handled by the engine (`take_requests`).
    requests: Vec<EngineRequest>,
    /// God mode as last applied, to notice it being switched off.
    god_mode_applied: bool,
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
            god_mode: false,
            keep_fed: false,
            no_fog: false,
            anim_speed: 1.0,
            ai_frozen: false,
            fps: 0.0,
            last_frame: None,
            floor: 0,
            seed: 0,
            effect_seconds: 30.0,
            target_floor: 1,
            advance_seconds: 10.0,
            inspect_tile: None,
            inspected: None,
            drafts: std::collections::HashMap::new(),
            overlays: Overlays::default(),
            explosion_radius: 2,
            prefabs: prefab::load(std::path::Path::new(prefab::PREFAB_FILE)),
            selected_prefab: None,
            prefabs_dirty: false,
            picker_open: false,
            inspect_picker_open: false,
            picker_search: String::new(),
            picker_internal: false,
            sprite_pick: None,
            sprite_sheet: crate::tile::SpriteSheet::Monsters,
            placed_from: std::collections::HashMap::new(),
            prefab_path: std::path::PathBuf::from(prefab::PREFAB_FILE),
            last_prefab_save: None,
            requests: Vec::new(),
            god_mode_applied: false,
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
            CursorTool::Teleport => match cheats::teleport(state, at) {
                Ok(()) => {
                    self.requests.push(EngineRequest::ClearPath);
                    self.status = None;
                }
                Err(why) => self.status = Some(format!("Can't teleport there: {why}")),
            },
            CursorTool::Ignite => {
                crate::systems::fire::spill_fire_at(&mut state.world, &state.grid, at.0, at.1, events)
            }
            CursorTool::Douse => {
                crate::systems::fire::splash_water(&mut state.world, &state.grid, at.0, at.1, 1, events);
            }
            CursorTool::Blast => {
                use crate::constants::{FIREBALL_DAMAGE, FIREBALL_RADIUS};
                let player = state.player_entity;
                crate::systems::actions::detonate_fireball(
                    &mut state.world,
                    &state.grid,
                    player,
                    at,
                    FIREBALL_DAMAGE,
                    events,
                    &mut self.rng,
                );
                events.push(crate::events::GameEvent::FireballExplosion { x: at.0, y: at.1, radius: FIREBALL_RADIUS });
            }
            CursorTool::Kill => {
                let n = cheats::kill_at(state, at);
                self.status = Some(format!("Killed {n}"));
            }
            CursorTool::Delete => {
                let n = cheats::delete_at(state, at);
                self.status = Some(format!("Deleted {n}"));
            }
            CursorTool::Inspect => {
                let player = state.player_entity;
                let mut here: Vec<Entity> = state
                    .world
                    .query::<&crate::components::Position>()
                    .iter()
                    .filter(|(_, p)| (p.x, p.y) == at)
                    .map(|(id, _)| id)
                    .collect();
                // Creatures first, the player last.
                here.sort_by_key(|&id| {
                    (id == player, state.world.get::<&crate::components::Actor>(id).is_err())
                });
                self.select(at, here.first().copied());
                self.tab = DevTab::Inspect;
            }
            CursorTool::LookOfExplosion { fiery } => {
                let (x, y) = (at.0 as f32 + 0.5, at.1 as f32 + 0.5);
                let radius = self.explosion_radius;
                vfx.spawn(x, y, crate::vfx::VfxType::Explosion { radius, fiery });
            }
            CursorTool::Shoot(shot) => match shoot(state, events, &mut self.rng, shot, at) {
                Ok(()) => self.requests.push(EngineRequest::WaitForShots),
                Err(why) => self.status = Some(format!("Can't shoot there: {why}")),
            },
            CursorTool::PlacePrefab(index) => {
                let Some(prefab) = self.prefabs.get(index).cloned() else {
                    self.disarm();
                    return;
                };
                let mut ctx = state.actor_ctx(events);
                let tiles = spawn::placement_tiles(&ctx, prefab.blocks(), at, self.spawn_options.count.max(1) as usize);
                let mut problems = Vec::new();
                for tile in &tiles {
                    let (made, trouble) = prefab::instantiate(&mut ctx, &prefab, *tile);
                    self.placed_from.insert(made, prefab.name.clone());
                    problems.extend(trouble);
                }
                problems.dedup();
                self.status = Some(match (tiles.len(), problems.first()) {
                    (0, _) => "Can't place there: something is in the way".into(),
                    (n, None) => format!("Placed {n} x {}", prefab.name),
                    (n, Some(problem)) => format!("Placed {n} x {}, but {problem}", prefab.name),
                });
                state.fov_dirty = true;
            }
        }
    }

    /// Carry out everything the panel queued this frame.
    pub fn apply_commands(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager) {
        self.save_prefabs_if_changed();
        let player = state.player_entity;
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
                DevCommand::FullHeal => {
                    if let Ok(mut health) = state.world.get::<&mut crate::components::Health>(player) {
                        health.current = health.max;
                    }
                }
                DevCommand::SetHealth { current, max } => cheats::set_health(&mut state.world, player, current, max),
                DevCommand::SetStats { strength, intelligence, agility } => {
                    cheats::set_stats(&mut state.world, player, Stats { strength, intelligence, agility })
                }
                DevCommand::AddXp(n) => cheats::add_xp(&mut state.world, player, n, events),
                DevCommand::AddGold(n) => cheats::add_gold(&mut state.world, player, n),
                DevCommand::SetMeters { hunger, fatigue } => {
                    cheats::set_meters(&mut state.world, player, hunger, fatigue)
                }
                DevCommand::Learn(ability) => {
                    if cheats::learn(&mut state.world, player, ability) {
                        self.requests.push(EngineRequest::Learned(ability));
                        self.status = Some(format!("Learned {}", ability.name()));
                    }
                }
                DevCommand::ResetCooldowns => cheats::reset_cooldowns(&mut state.world, player),
                DevCommand::AddEffect(effect, seconds) => {
                    crate::systems::effects::add_effect_to_entity(&mut state.world, player, effect, seconds);
                }
                DevCommand::RemoveEffect(effect) => {
                    crate::systems::effects::remove_effect_from_entity(&mut state.world, player, effect)
                }
                DevCommand::ClearEffects => cheats::clear_effects(&mut state.world, player),
                DevCommand::RevealMap => {
                    crate::systems::rendering::reveal_entire_map(&mut state.grid);
                    state.fov_dirty = true;
                }
                DevCommand::KillHostiles { visible_only } => {
                    let n = cheats::kill_hostiles(state, visible_only);
                    self.status = Some(format!("Killed {n}"));
                }
                DevCommand::FreezeAi(frozen) => {
                    cheats::freeze_ai(&mut state.world, frozen);
                    self.ai_frozen = frozen;
                }
                DevCommand::WakeAll => {
                    let n = cheats::wake_all(&mut state.world);
                    self.status = Some(format!("Woke {n}"));
                }
                DevCommand::GoToFloor(floor) => self.requests.push(EngineRequest::GoToFloor(floor)),
                DevCommand::RerollFloor => self.requests.push(EngineRequest::RerollFloor),
                DevCommand::AdvanceTime(seconds) => self.requests.push(EngineRequest::AdvanceTime(seconds)),
                DevCommand::SetComponent { entity, name, value } => {
                    let Some(entry) = registry::find(name) else { continue };
                    match (entry.write)(&mut state.world, entity, value.clone()) {
                        Ok(()) => {
                            self.drafts.remove(name);
                            resync(state, entity);
                        }
                        Err(why) => {
                            self.drafts.insert(name, (value, why));
                        }
                    }
                }
                DevCommand::AddComponent { entity, name } => {
                    if let Some(entry) = registry::find(name) {
                        let _ = (entry.write)(&mut state.world, entity, (entry.default)());
                        resync(state, entity);
                    }
                }
                DevCommand::RemoveComponent { entity, name } => {
                    if let Some(entry) = registry::find(name) {
                        (entry.remove)(&mut state.world, entity);
                        self.drafts.remove(name);
                        resync(state, entity);
                    }
                }
                DevCommand::DeleteEntity(entity) => {
                    if entity != player && state.world.contains(entity) {
                        state.spatial_cache.remove_entity(entity);
                        state.active_ai_tracker.remove_entity(entity);
                        state.action_scheduler.cancel_for_entity(entity);
                        let _ = state.world.despawn(entity);
                        state.fov_dirty = true;
                        self.inspected = None;
                    }
                }
                DevCommand::KillEntity(entity) => {
                    if let Ok(mut health) = state.world.get::<&mut crate::components::Health>(entity) {
                        health.current = 0;
                    }
                }
                DevCommand::Shake(amplitude) => vfx.request_shake(crate::camera::ShakeRequest {
                    amplitude,
                    origin: None,
                    direction: None,
                }),
                DevCommand::DamageNumber(amount) => {
                    if let Some(pos) = state.world.get::<&crate::components::Position>(player).ok().map(|p| *p) {
                        vfx.spawn_damage_number(
                            pos.x as f32 + 0.5,
                            pos.y as f32 + 0.5,
                            amount,
                            crate::vfx::DamageTier::Dealt,
                        );
                    }
                }
                DevCommand::TeleportPlayerTo(at) => match cheats::teleport(state, at) {
                    Ok(()) => self.requests.push(EngineRequest::ClearPath),
                    Err(why) => self.status = Some(format!("Can't teleport there: {why}")),
                },
            }
        }
    }
}

impl DevTools {
    /// Every frame: keep god mode and full meters going, and let go of god
    /// mode's Invulnerable once it's switched off.
    pub fn maintain(&mut self, state: &mut GameState) {
        cheats::maintain(state, self.god_mode, self.keep_fed);
        if self.god_mode_applied && !self.god_mode {
            cheats::end_god_mode(state);
        }
        self.god_mode_applied = self.god_mode;
        self.floor = state.current_floor;
        self.seed = state.seed;
        // Measured here from the wall clock: the frame time the engine gets
        // is clamped for animation.
        let now = std::time::Instant::now();
        if let Some(last) = self.last_frame.replace(now) {
            let dt = (now - last).as_secs_f32();
            if dt > 0.0 {
                self.fps += (1.0 / dt - self.fps) * 0.05;
            }
        }
    }

    /// What commands asked the engine to do (floor jumps, waiting, hotbar).
    pub fn take_requests(&mut self) -> Vec<EngineRequest> {
        std::mem::take(&mut self.requests)
    }
}

impl DevTools {
    /// Look at `entity` (on `tile`) in the Inspect tab.
    pub fn select(&mut self, tile: (i32, i32), entity: Option<Entity>) {
        self.inspect_tile = Some(tile);
        if self.inspected != entity {
            self.drafts.clear();
        }
        self.inspected = entity;
    }
}

impl DevTools {
    /// Write the prefab library if the panel changed it, at most twice a
    /// second (dragging a value changes it every frame).
    fn save_prefabs_if_changed(&mut self) {
        let due = self.last_prefab_save.is_none_or(|t| t.elapsed().as_secs_f32() > 0.5);
        if self.prefabs_dirty && due {
            prefab::save(&self.prefab_path, &self.prefabs);
            self.prefabs_dirty = false;
            self.last_prefab_save = Some(std::time::Instant::now());
        }
    }
}

/// Fire `shot` from the player at `at` through the game's own projectile
/// code (arrows are made by hand, since shooting a bow needs one equipped).
fn shoot(state: &mut GameState, events: &mut EventQueue, rng: &mut StdRng, shot: Shot, at: (i32, i32)) -> Result<(), String> {
    use crate::components::{EffectType, Position, Projectile, ProjectileKind, ProjectileMarker, Sprite, SpriteTint, VisualPosition};
    use crate::systems::actions::{self, ActionResult};
    let player = state.player_entity;
    let now = state.game_clock.time;
    let from = state.world.get::<&Position>(player).map(|p| (p.x, p.y)).map_err(|_| "no player".to_string())?;
    let result = match shot {
        Shot::Fireball => actions::apply_fireball(&mut state.world, &state.grid, player, at, events, rng, now),
        Shot::GraveBolt => actions::apply_grave_bolt(
            &mut crate::engine::EffectCtx {
                world: &mut state.world,
                grid: &mut state.grid,
                spatial: &mut state.spatial_cache,
                events,
                rng,
            },
            player,
            at.0,
            at.1,
            now,
        ),
        Shot::WaterFlask | Shot::HealthPotion => {
            let kind = if shot == Shot::WaterFlask { ItemType::WaterFlaskFull } else { ItemType::HealthPotion };
            actions::apply_throw_potion(&mut state.world, player, kind, at.0, at.1, events, now)
        }
        Shot::Arrow | Shot::FireArrow => {
            let fire = shot == Shot::FireArrow;
            let speed = crate::constants::ARROW_SPEED;
            let path = actions::calculate_arrow_path(from.0, from.1, at.0, at.1, speed, &state.grid);
            if path.is_empty() {
                return Err("that's where you are".into());
            }
            let (dx, dy) = ((at.0 - from.0) as f32, (at.1 - from.1) as f32);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let pos = Position::new(from.0, from.1);
            let arrow = state.world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::from_ref(crate::tile::tile_ids::ARROW),
                Projectile {
                    source: player,
                    damage: crate::constants::BOW_BASE_DAMAGE,
                    path,
                    path_index: 0,
                    direction: (dx / len, dy / len),
                    spawn_time: now,
                    finished: None,
                    potion_type: None,
                    on_hit_effect: fire.then_some((EffectType::Burning, crate::constants::BURNING_DURATION)),
                    hit_enemy: false,
                    incendiary: fire,
                    kind: ProjectileKind::Missile,
                    speed,
                },
                ProjectileMarker,
            ));
            if fire {
                let (r, g, b) = crate::constants::FIRE_ARROW_TINT;
                let _ = state.world.insert_one(arrow, SpriteTint { r, g, b });
            }
            ActionResult::Completed
        }
    };
    match result {
        ActionResult::Completed => Ok(()),
        other => Err(format!("{other:?}")),
    }
}

/// After an entity's components were changed by hand: re-register it with
/// the spatial cache as it now is, put its sprite where its position says,
/// and hand it to the AI if it has just started acting.
fn resync(state: &mut GameState, e: Entity) {
    use crate::components::{Actor, Position, VisualPosition};
    if !state.world.contains(e) {
        return;
    }
    state.spatial_cache.remove_entity(e);
    if let Some(pos) = state.world.get::<&Position>(e).ok().map(|p| *p) {
        if let Ok(mut vis) = state.world.get::<&mut VisualPosition>(e) {
            *vis = VisualPosition::from_position(&pos);
        }
    }
    let needs_ai = e != state.player_entity
        && state.world.get::<&Actor>(e).is_ok()
        && !state.active_ai_tracker.is_tracked(e);
    if needs_ai {
        spawn::adopt(&mut state.actor_ctx(&mut EventQueue::new()), e);
    } else {
        let (blocks, sight) = state
            .world
            .entity(e)
            .map(|r| (r.has::<crate::components::BlocksMovement>(), r.has::<crate::components::BlocksVision>()))
            .unwrap_or((false, false));
        let pos = state.world.get::<&Position>(e).ok().map(|p| (p.x, p.y));
        if let (Some(pos), true) = (pos, blocks || sight) {
            state.spatial_cache.register_entity(e, pos, blocks, sight);
        }
    }
    state.fov_dirty = true;
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
        Actor, FurnitureKind, TrapEffect, LanternStyle, PlayerClass, Position,
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
                Spawnable::Trap(TrapEffect::spike()),
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
        dev.apply_commands(state, &mut EventQueue::new(), &mut vfx);
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

    /// A free floor tile near the player.
    fn free_tile_near(state: &GameState) -> (i32, i32) {
        let p = state.world.get::<&Position>(state.player_entity).map(|p| (p.x, p.y)).unwrap();
        (1..8)
            .flat_map(|r| [(p.0 + r, p.1), (p.0 - r, p.1), (p.0, p.1 + r), (p.0, p.1 - r)])
            .find(|&t| state.grid.is_walkable(t.0, t.1) && !state.spatial_cache.is_blocked(t))
            .expect("open floor near the start")
    }

    #[test]
    fn cheats_do_what_they_say() {
        use crate::components::{Experience, Health};
        let mut engine = live_run();
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let player = state.player_entity;
        let mut dev = DevTools::new();
        dev.visible = true;

        // God mode: no damage gets through, and it lets go when switched off.
        dev.god_mode = true;
        dev.maintain(state);
        let before = state.world.get::<&Health>(player).unwrap().current;
        let mut rng = <StdRng as SeedableRng>::seed_from_u64(1);
        crate::systems::combat::apply_damage(&mut state.world, &state.grid, player, 50, &mut rng, &mut events);
        assert_eq!(state.world.get::<&Health>(player).unwrap().current, before);
        dev.god_mode = false;
        dev.maintain(state);
        assert!(!crate::systems::effects::entity_has_effect(&state.world, player, EffectType::Invulnerable));

        // XP levels you up, announced.
        dev.commands.push(DevCommand::AddXp(5000));
        dev.apply_commands(state, &mut events, &mut vfx);
        assert!(state.world.get::<&Experience>(player).unwrap().level > 1);
        assert!(events.drain().any(|e| matches!(e, crate::events::GameEvent::LevelUp { .. })));

        // Teleport, then a spawned rat killed and another deleted; the cache
        // keeps up throughout.
        let to = free_tile_near(state);
        dev.tool = Some(CursorTool::Teleport);
        dev.use_tool(state, &mut events, &mut vfx, to);
        assert_eq!(state.world.get::<&Position>(player).map(|p| (p.x, p.y)).unwrap(), to);
        assert_eq!(dev.take_requests(), vec![EngineRequest::ClearPath]);
        let spot = free_tile_near(state);
        dev.tool = Some(CursorTool::Place(Spawnable::Enemy(0)));
        dev.use_tool(state, &mut events, &mut vfx, spot);
        let rats = |w: &World| w.query::<&crate::components::ChaseAI>().iter().count();
        let with_rat = rats(&state.world);
        dev.tool = Some(CursorTool::Delete);
        dev.use_tool(state, &mut events, &mut vfx, spot);
        assert_eq!(rats(&state.world), with_rat - 1, "the rat is gone");
        state.spatial_cache.assert_coherent_with_world(&state.world, "after teleport and delete");

        // Freezing holds every creature; unfreezing lets them go.
        dev.commands.push(DevCommand::FreezeAi(true));
        dev.apply_commands(state, &mut events, &mut vfx);
        let frozen = state.world.query::<&crate::components::DevFrozen>().iter().count();
        assert!(frozen > 0 && dev.ai_frozen);
        dev.commands.push(DevCommand::FreezeAi(false));
        dev.apply_commands(state, &mut events, &mut vfx);
        assert_eq!(state.world.query::<&crate::components::DevFrozen>().iter().count(), 0);
    }

    #[test]
    fn floor_jumps_rerolls_and_waiting_go_through_the_engine() {
        let mut engine = live_run();
        let start_time = engine.state.as_ref().unwrap().game_clock.time;

        engine.dev.commands.push(DevCommand::GoToFloor(4));
        engine.process_ui_actions(&crate::ui::UiActions::default());
        let state = engine.state.as_ref().unwrap();
        assert_eq!(state.current_floor, 4);
        let at = state.world.get::<&Position>(state.player_entity).map(|p| (p.x, p.y)).unwrap();
        assert!(state.grid.is_walkable(at.0, at.1), "landed on floor");
        state.spatial_cache.assert_coherent_with_world(&state.world, "after floor jump");
        let layout: Vec<_> = state.grid.tiles.iter().map(|t| t.tile_type).collect();

        engine.dev.commands.push(DevCommand::RerollFloor);
        engine.process_ui_actions(&crate::ui::UiActions::default());
        let state = engine.state.as_ref().unwrap();
        assert_eq!(state.current_floor, 4);
        let rerolled: Vec<_> = state.grid.tiles.iter().map(|t| t.tile_type).collect();
        assert_ne!(layout, rerolled, "a fresh layout");
        assert!(!state.floors.contains_key(&4), "the old floor 4 isn't kept");

        engine.dev.commands.push(DevCommand::AdvanceTime(10.0));
        engine.process_ui_actions(&crate::ui::UiActions::default());
        assert!(engine.state.as_ref().unwrap().game_clock.time - start_time >= 10.0);
    }

    #[test]
    fn every_bench_shot_lands() {
        let mut engine = live_run();
        let (player, from) = {
            let state = engine.state.as_ref().unwrap();
            let p = state.player_entity;
            (p, state.world.get::<&Position>(p).map(|p| (p.x, p.y)).unwrap())
        };
        // Two tiles off in a straight line of open floor, so even the
        // spells that want a clear line of sight have one.
        let target = {
            let state = engine.state.as_ref().unwrap();
            let open = |x: i32, y: i32| state.grid.is_walkable(x, y) && !state.spatial_cache.is_blocked((x, y));
            [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)]
                .into_iter()
                .map(|(dx, dy)| ((from.0 + dx, from.1 + dy), (from.0 + 2 * dx, from.1 + 2 * dy)))
                .find(|&(a, b)| open(a.0, a.1) && open(b.0, b.1))
                .expect("open floor beside the start")
                .1
        };
        for shot in Shot::ALL {
            engine.dev.tool = Some(CursorTool::Shoot(shot));
            let state = engine.state.as_mut().unwrap();
            engine.dev.use_tool(state, &mut engine.events, &mut engine.vfx, target);
            let refused = engine.dev.status.as_deref().is_some_and(|s| s.starts_with("Can't"));
            assert!(!refused, "{}: {:?}", shot.name(), engine.dev.status);
            engine.process_ui_actions(&crate::ui::UiActions::default());
            let state = engine.state.as_ref().unwrap();
            assert!(
                !crate::systems::projectile::has_projectile_in_flight(&state.world, player),
                "{} is still in the air",
                shot.name()
            );
            state.spatial_cache.assert_coherent_with_world(&state.world, "after a bench shot");
            // Heal back up in case a blast caught the player.
            engine.dev.commands.push(DevCommand::FullHeal);
            engine.process_ui_actions(&crate::ui::UiActions::default());
        }
    }

    #[test]
    fn every_overlay_draws_on_a_live_run() {
        let engine = live_run();
        let state = engine.state.as_ref().unwrap();
        let player = state.player_entity;
        let at = state.world.get::<&Position>(player).map(|p| (p.x, p.y)).unwrap();
        let mut dev = DevTools::new();
        dev.visible = true;
        dev.overlays = Overlays {
            light: true,
            noise: true,
            ai: true,
            path: true,
            blockers: true,
            fire_and_water: true,
            coords: true,
            ..Overlays::default()
        };
        // Something to path from.
        dev.inspected = state.world.query::<&crate::components::ChaseAI>().iter().map(|(id, _)| id).next();
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        camera.zoom = 48.0;
        camera.position = glam::Vec2::new(at.0 as f32, at.1 as f32);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            crate::ui::draw_dev_overlays(ctx, &dev, &camera, &state.world, &state.grid, player, at);
        });
    }

    #[test]
    fn inspector_edits_keep_the_world_coherent() {
        use crate::components::{BlocksMovement, Health};
        let mut engine = live_run();
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut dev = DevTools::new();
        let spot = free_tile_near(state);
        dev.tool = Some(CursorTool::Place(Spawnable::Enemy(3)));
        dev.use_tool(state, &mut events, &mut vfx, spot);
        dev.tool = Some(CursorTool::Inspect);
        dev.use_tool(state, &mut events, &mut vfx, spot);
        let goblin = dev.inspected.expect("picked the goblin");
        assert_eq!(dev.tab, DevTab::Inspect);

        // An edit lands; a bad one is kept as a draft with the reason.
        dev.commands.push(DevCommand::SetComponent {
            entity: goblin,
            name: "Health",
            value: serde_json::json!({"current": 3, "max": 99, "regen_amount": 0, "regen_interval": 5.0, "last_regen_time": 0.0}),
        });
        dev.commands.push(DevCommand::SetComponent { entity: goblin, name: "Stats", value: serde_json::json!({"strength": "huge"}) });
        dev.apply_commands(state, &mut events, &mut vfx);
        assert_eq!(state.world.get::<&Health>(goblin).map(|h| (h.current, h.max)).unwrap(), (3, 99));
        assert!(dev.drafts.contains_key("Stats"), "the bad edit is kept to fix");

        // Moving it and taking its blocking away keep the cache in step.
        let elsewhere = free_tile_near(state);
        let mut pos = (registry::find("Position").unwrap().read)(&state.world, goblin).unwrap();
        pos["x"] = serde_json::json!(elsewhere.0);
        pos["y"] = serde_json::json!(elsewhere.1);
        dev.commands.push(DevCommand::SetComponent { entity: goblin, name: "Position", value: pos });
        dev.commands.push(DevCommand::RemoveComponent { entity: goblin, name: "BlocksMovement" });
        dev.apply_commands(state, &mut events, &mut vfx);
        state.spatial_cache.assert_coherent_with_world(&state.world, "after inspector edits");
        assert!(state.world.get::<&BlocksMovement>(goblin).is_err());
        dev.commands.push(DevCommand::AddComponent { entity: goblin, name: "BlocksMovement" });
        dev.apply_commands(state, &mut events, &mut vfx);
        state.spatial_cache.assert_coherent_with_world(&state.world, "after adding it back");
        assert!(state.spatial_cache.is_blocked(elsewhere));
    }

    #[test]
    fn placing_a_prefab_builds_it_where_clicked() {
        let mut engine = live_run();
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut dev = DevTools::new();
        dev.prefabs = vec![prefab::template(Spawnable::Enemy(3)).unwrap()];
        dev.spawn_options.count = 3;
        let spot = free_tile_near(state);
        let before = state.world.query::<&crate::components::ChaseAI>().iter().count();
        dev.tool = Some(CursorTool::PlacePrefab(0));
        dev.use_tool(state, &mut events, &mut vfx, spot);
        assert!(dev.status.as_deref().is_some_and(|s| s.starts_with("Placed 3")), "{:?}", dev.status);
        assert_eq!(state.world.query::<&crate::components::ChaseAI>().iter().count(), before + 3);
        state.spatial_cache.assert_coherent_with_world(&state.world, "after placing prefabs");
        assert!(!dev.prefabs_dirty, "placing doesn't touch the library");
    }

    /// A prefab put together from building blocks alone works when placed:
    /// a creature turned companion follows the player, and remembers which
    /// prefab it came from so the inspector can update it.
    #[test]
    fn a_prefab_from_blocks_comes_alive() {
        let mut engine = live_run();
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut friend = prefab::Prefab::empty("Friend");
        for block in ["Creature", "Companion"] {
            for part in catalog::BLOCKS.iter().find(|b| b.name == block).unwrap().parts {
                friend.add(part);
            }
        }
        // Follow, don't hunt.
        friend.components.retain(|c| c.component != "ChaseAI");
        assert!(friend.unmet_needs().is_empty(), "{:?}", friend.unmet_needs());

        let mut dev = DevTools::new();
        dev.prefabs = vec![friend];
        let spot = free_tile_near(state);
        dev.tool = Some(CursorTool::PlacePrefab(0));
        dev.use_tool(state, &mut events, &mut vfx, spot);
        let (made, origin) = dev.placed_from.iter().next().map(|(e, n)| (*e, n.clone())).expect("placed");
        assert_eq!(origin, "Friend");
        let leader = state.world.get::<&crate::components::Follower>(made).map(|f| f.leader).unwrap();
        assert_eq!(leader, state.player_entity, "a follower of nobody follows you");
        assert!(state.active_ai_tracker.is_tracked(made), "it takes turns");
        state.spatial_cache.assert_coherent_with_world(&state.world, "after placing a built prefab");
    }
}

