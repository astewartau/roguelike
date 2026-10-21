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
pub mod field_docs;
pub mod prefab;

pub mod spawn;

use hecs::Entity;
use crate::world::World;
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
/// What the add palette adds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteTarget {
    /// The prefab at this index in the library.
    Prefab(usize),
    /// An entity in the world (from the Inspect tab).
    Entity(Entity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DevTab {
    #[default]
    Spawn,
    Paint,
    Player,
    World,
    Inspect,
    Overlays,
    Effects,
}

impl DevTab {
    pub const ALL: [DevTab; 7] = [
        DevTab::Spawn,
        DevTab::Paint,
        DevTab::Player,
        DevTab::World,
        DevTab::Inspect,
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
    /// Place a copy of what's on the clipboard (`DevTools::copy_entity`).
    Paste,
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
            CursorTool::Paste => "Paste copies".into(),
            CursorTool::LookOfExplosion { fiery: true } => "Explosion (look only)".into(),
            CursorTool::LookOfExplosion { fiery: false } => "Shockwave (look only)".into(),
            CursorTool::Shoot(shot) => format!("Shoot {}", shot.name()),
        }
    }

    /// The panel it's chosen from: it stays armed only while that panel is
    /// open. None for Inspect, which is no panel's: it's what a click on
    /// the map does whenever nothing else is armed.
    pub fn panel(self) -> Option<DevTab> {
        Some(match self {
            CursorTool::Place(_) | CursorTool::PlacePrefab(_) => DevTab::Spawn,
            CursorTool::Paint(_) => DevTab::Paint,
            CursorTool::Teleport
            | CursorTool::Ignite
            | CursorTool::Douse
            | CursorTool::Blast
            | CursorTool::Kill
            | CursorTool::Delete => DevTab::World,
            // Inspect is the default, and Paste comes from the inspector
            // but places anywhere until dropped.
            CursorTool::Inspect | CursorTool::Paste => return None,
            CursorTool::ParticleFire | CursorTool::LookOfExplosion { .. } | CursorTool::Shoot(_) => DevTab::Effects,
        })
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
    /// A new player was placed: their hotbars and HUD start afresh.
    PlayerArrived,
}

/// Everything the developer tools remember between frames.
pub struct DevTools {
    pub visible: bool,
    /// The tool whose panel the rail shows (or last showed), and whether
    /// that panel is open.
    pub tab: DevTab,
    pub panel_open: bool,
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
    /// How far the game's HUD keeps clear of the tools last frame, in egui
    /// points: below the toolbar, and left of a panel over the top-right
    /// corner.
    /// Both 0 while hidden.
    pub hud_top: f32,
    pub hud_right: f32,
    /// One line of feedback for the toolbar ("Placed 3 Goblins"), and when
    /// it was first shown: it goes after `DEVTOOLS_STATUS_SECONDS`.
    pub status: Option<String>,
    status_shown: Option<(String, std::time::Instant)>,
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
    /// The world clock: while running, game time passes on its own at
    /// `clock_speed` game seconds per real second, player or no player
    /// (one who is there just stands, and can act whenever they like).
    pub clock_running: bool,
    pub clock_speed: f32,
    /// Run the world on to its next action completion, once.
    pub clock_step: bool,
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
    /// What a press on the map picked up (with Inspect), carried by the
    /// drag until the button comes up.
    grabbed: Option<Entity>,
    /// A copied entity, as a prefab (not saved), for Paste to place.
    pub clipboard: Option<prefab::Prefab>,
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
    /// The add palette (components and building blocks to add), if open,
    /// and what it adds to; the highlighted result; whether its search box
    /// still needs focusing.
    pub palette: Option<PaletteTarget>,
    pub palette_selected: usize,
    pub palette_focus: bool,
    /// The one part card open in the prefab builder, and in the inspector.
    pub expanded_part: Option<String>,
    pub expanded_component: Option<&'static str>,
    /// What's typed in the palette's search box, and whether internal
    /// components are listed.
    pub picker_search: String,
    pub picker_internal: bool,
    /// Which prefab each placed entity came from (by name), so the
    /// inspector can offer to update it from a tweaked copy.
    pub placed_from: std::collections::HashMap<Entity, String>,
    prefab_path: std::path::PathBuf,
    last_prefab_save: Option<std::time::Instant>,
    /// Asked for by commands, handled by the engine (`take_requests`).
    requests: Vec<EngineRequest>,
    /// God mode as last applied, to notice it being switched off.
    god_mode_applied: bool,
    /// Whether the tools were shown as of the last relight, to relight the
    /// map (editor lighting, or the player's) when that changes.
    visible_applied: bool,
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
            panel_open: false,
            tool: None,
            spawn_options: SpawnOptions::default(),
            brush: 0,
            search: String::new(),
            rain_style: RainStyle::default(),
            commands: Vec::new(),
            hud_top: 0.0,
            hud_right: 0.0,
            status: None,
            status_shown: None,
            god_mode: false,
            keep_fed: false,
            no_fog: false,
            anim_speed: 1.0,
            ai_frozen: false,
            clock_running: false,
            clock_speed: 1.0,
            clock_step: false,
            fps: 0.0,
            last_frame: None,
            floor: 0,
            seed: 0,
            effect_seconds: 30.0,
            target_floor: 1,
            advance_seconds: 10.0,
            inspect_tile: None,
            inspected: None,
            grabbed: None,
            clipboard: None,
            drafts: std::collections::HashMap::new(),
            overlays: Overlays::default(),
            explosion_radius: 2,
            prefabs: prefab::built_ins()
                .into_iter()
                .chain(prefab::load(std::path::Path::new(prefab::PREFAB_FILE)))
                .collect(),
            selected_prefab: None,
            prefabs_dirty: false,
            palette: None,
            palette_selected: 0,
            palette_focus: false,
            expanded_part: None,
            expanded_component: None,
            picker_search: String::new(),
            picker_internal: false,
            placed_from: std::collections::HashMap::new(),
            prefab_path: std::path::PathBuf::from(prefab::PREFAB_FILE),
            last_prefab_save: None,
            requests: Vec::new(),
            god_mode_applied: false,
            visible_applied: false,
            held_at: None,
            rng: StdRng::from_entropy(),
        }
    }
}

impl DevTools {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the add palette for `target`, search cleared, ready to type.
    pub fn open_palette(&mut self, target: PaletteTarget) {
        self.palette = Some(target);
        self.picker_search.clear();
        self.palette_selected = 0;
        self.palette_focus = true;
    }

    /// Show `tab`'s panel.
    pub fn open_panel(&mut self, tab: DevTab) {
        self.tab = tab;
        self.panel_open = true;
        self.sync_panel_tool();
    }

    /// Close the open panel (and drop its own tool). The inspector's panel
    /// only shows what's picked: closing it lets go of that and carries on
    /// picking, until it's closed again with nothing picked.
    pub fn close_panel(&mut self) {
        if self.panel_open && self.tab == DevTab::Inspect && self.inspect_tile.is_some() {
            self.deselect();
        } else {
            self.leave_panel();
        }
    }

    /// Put the open panel away entirely (inspecting included).
    fn leave_panel(&mut self) {
        if self.tab == DevTab::Inspect {
            self.deselect();
        }
        self.panel_open = false;
        self.sync_panel_tool();
    }

    /// A rail icon was clicked: open its panel, or put it away if it's the
    /// one already open. Inspect starts with nothing picked, so its panel
    /// waits for a click on the map.
    pub fn rail_clicked(&mut self, tab: DevTab) {
        if self.panel_open && self.tab == tab {
            self.leave_panel();
        } else {
            if tab == DevTab::Inspect {
                self.deselect();
            }
            self.open_panel(tab);
        }
    }

    /// Stop inspecting what's picked.
    fn deselect(&mut self) {
        self.inspect_tile = None;
        self.inspected = None;
        self.drafts.clear();
        self.grabbed = None;
    }

    /// Whether the armed tool is the default, Inspect: what a click does
    /// when nothing else is armed, so there's nothing to drop.
    pub fn tool_is_default(&self) -> bool {
        self.tool == Some(CursorTool::Inspect)
    }

    /// Drop the armed tool once the panel it came from isn't open (closed,
    /// or another opened), so nothing is placed or painted with its panel
    /// gone; and with nothing armed while the tools are shown, clicks
    /// inspect.
    fn sync_panel_tool(&mut self) {
        let open = (self.visible && self.panel_open).then_some(self.tab);
        if self.tool.is_some_and(|tool| tool.panel().is_some_and(|panel| Some(panel) != open)) {
            self.disarm();
        }
        if !self.visible {
            self.disarm();
        } else if self.tool.is_none() {
            self.tool = Some(CursorTool::Inspect);
        }
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
    /// Inspect picks the thing there (switching to its panel) and picks it
    /// up, for a drag to carry it; on bare floor it takes nothing, and
    /// returns false so the press can pan the view instead (a click there
    /// is `click_floor`'s).
    pub fn press(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager, at: (i32, i32)) -> bool {
        if self.tool == Some(CursorTool::Inspect) {
            let Some(e) = thing_at(state, at) else {
                return false;
            };
            self.select(at, Some(e));
            self.open_panel(DevTab::Inspect);
            self.grabbed = Some(e);
            self.held_at = Some(at);
            return true;
        }
        self.held_at = Some(at);
        self.use_tool(state, events, vfx, at);
        true
    }

    /// A click (no drag) on bare floor that no tool took. With nothing else
    /// open, or the inspector, it looks at the tile; with another panel
    /// open it does nothing, so that panel stays.
    pub fn click_floor(&mut self, at: (i32, i32)) {
        let inspecting = !self.panel_open || self.tab == DevTab::Inspect;
        if self.tool_is_default() && inspecting {
            self.select(at, None);
            self.open_panel(DevTab::Inspect);
        }
    }

    /// The mouse is held at `at`: a dragging tool applies again on each new
    /// tile it passes over.
    pub fn drag(&mut self, state: &mut GameState, events: &mut EventQueue, vfx: &mut VfxManager, at: (i32, i32)) {
        if let Some(e) = self.grabbed {
            if self.held_at != Some(at) {
                self.carry(state, e, at);
            }
            return;
        }
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
        self.grabbed = None;
    }

    /// Carry the grabbed entity to `at`, if it can stand there; the
    /// inspector follows it.
    fn carry(&mut self, state: &mut GameState, e: Entity, at: (i32, i32)) {
        match cheats::move_entity(state, e, at) {
            Ok(()) => {
                self.held_at = Some(at);
                self.inspect_tile = Some(at);
                if e == state.player_entity {
                    self.requests.push(EngineRequest::ClearPath);
                }
                self.status = None;
            }
            Err(why) => self.status = Some(format!("Can't move it there: {why}")),
        }
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
                self.select(at, thing_at(state, at));
                self.open_panel(DevTab::Inspect);
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
                let count = self.spawn_options.count.max(1) as usize;
                self.place_prefab(state, events, &prefab, at, count, true);
            }
            CursorTool::Paste => self.paste_at(state, events, at),
        }
    }

    /// Place `count` of `prefab` around `at` (`remember`: note where each
    /// came from, for the inspector's "Update from this"). A player prefab
    /// places one, and it's you. Returns what was made.
    fn place_prefab(
        &mut self,
        state: &mut GameState,
        events: &mut EventQueue,
        prefab: &prefab::Prefab,
        at: (i32, i32),
        count: usize,
        remember: bool,
    ) -> Vec<Entity> {
        if prefab.has("Player") {
            self.status = Some(match place_as_player(state, events, prefab, at) {
                Ok((made, problems)) => {
                    self.placed_from.insert(made, prefab.name.clone());
                    self.requests.push(EngineRequest::PlayerArrived);
                    match problems.first() {
                        None => format!("You're here, as {}", prefab.name),
                        Some(problem) => format!("You're here, as {}, but {problem}", prefab.name),
                    }
                }
                Err(why) => format!("Can't place you there: {why}"),
            });
            return Vec::new();
        }
        let mut ctx = state.actor_ctx(events);
        let tiles = spawn::placement_tiles(&ctx, prefab.blocks(), at, count);
        let mut problems = Vec::new();
        let mut made_all = Vec::new();
        for tile in &tiles {
            let (made, trouble) = prefab::instantiate(&mut ctx, prefab, *tile);
            if remember {
                self.placed_from.insert(made, prefab.name.clone());
            }
            made_all.push(made);
            problems.extend(trouble);
        }
        problems.dedup();
        self.status = Some(match (tiles.len(), problems.first()) {
            (0, _) => "Can't place there: something is in the way".into(),
            (n, None) => format!("Placed {n} x {}", prefab.name),
            (n, Some(problem)) => format!("Placed {n} x {}, but {problem}", prefab.name),
        });
        state.fov_dirty = true;
        made_all
    }

    /// Copy `e` to the clipboard and arm Paste, so each click on the map
    /// places another. A copy of the player is a copy of their body, not
    /// another you: it leaves Player behind.
    pub fn copy_entity(&mut self, world: &World, e: Entity) {
        let name = world.get::<&crate::components::Name>(e).map(|n| n.0.clone()).unwrap_or_else(|_| "Thing".into());
        let mut copy = prefab::capture(world, e, &name);
        if let Some(i) = copy.components.iter().position(|c| c.component == "Player") {
            copy.remove_at(i);
        }
        self.clipboard = Some(copy);
        self.tool = Some(CursorTool::Paste);
        self.status = Some(format!("Copied {name}: click to place copies (Ctrl+V pastes at the cursor)"));
    }

    /// Place one copy of what's on the clipboard at `at`, and look at it.
    pub fn paste_at(&mut self, state: &mut GameState, events: &mut EventQueue, at: (i32, i32)) {
        let Some(copy) = self.clipboard.clone() else {
            self.status = Some("Nothing copied yet (Ctrl+C copies what you're inspecting)".into());
            return;
        };
        if let Some(&made) = self.place_prefab(state, events, &copy, at, 1, false).first() {
            self.select(at, Some(made));
        }
    }

    /// Delete what's being inspected (never the player).
    pub fn delete_inspected(&mut self, player: Entity) {
        if let Some(e) = self.inspected.filter(|&e| e != player) {
            self.commands.push(DevCommand::DeleteEntity(e));
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
                        // Giving something Player makes it you.
                        if entry.name == "Player" && entity != state.player_entity {
                            become_player(state, entity);
                            self.requests.push(EngineRequest::PlayerArrived);
                            self.status = Some("You're it now".into());
                        }
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
            }
        }
    }
}

impl DevTools {
    /// Every frame: keep god mode and full meters going, and let go of god
    /// mode's Invulnerable once it's switched off.
    pub fn maintain(&mut self, state: &mut GameState) {
        self.sync_panel_tool();
        cheats::maintain(state, self.god_mode, self.keep_fed);
        if self.god_mode_applied && !self.god_mode {
            cheats::end_god_mode(state);
        }
        self.god_mode_applied = self.god_mode;
        // A status line goes after a few seconds (a new one starts afresh).
        let now = std::time::Instant::now();
        match (&self.status, &self.status_shown) {
            (None, _) => self.status_shown = None,
            (Some(line), Some((shown, since))) if line == shown => {
                if (now - *since).as_secs_f32() > crate::constants::DEVTOOLS_STATUS_SECONDS {
                    self.status = None;
                    self.status_shown = None;
                }
            }
            (Some(line), _) => self.status_shown = Some((line.clone(), now)),
        }
        // Shown, the map is lit the editor's way; hidden, by the player.
        if self.visible != self.visible_applied {
            state.fov_dirty = true;
            self.visible_applied = self.visible;
        }
        self.floor = state.current_floor;
        self.seed = state.seed;
        // Measured here from the wall clock: the frame time the engine gets
        // is clamped for animation.
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

/// What a click on `at` picks: a creature first, the player last.
fn thing_at(state: &GameState, at: (i32, i32)) -> Option<Entity> {
    let player = state.player_entity;
    let mut here: Vec<Entity> = state
        .world
        .query::<&crate::components::Position>()
        .iter()
        .filter(|(_, p)| (p.x, p.y) == at)
        .map(|(id, _)| id)
        .collect();
    here.sort_by_key(|&id| (id == player, state.world.get::<&crate::components::Actor>(id).is_err()));
    here.first().copied()
}

/// Make `e` the player, in place of whoever was (they go). It drops any
/// mind of its own, which would fight you for it, and anything it was in
/// the middle of; from now on it acts when you do.
fn become_player(state: &mut GameState, e: Entity) {
    use crate::components::{Actor, ChaseAI, CompanionAI, Follower, Player};
    let old = state.player_entity;
    if old != e && state.world.contains(old) {
        let _ = state.world.despawn(old);
    }
    let _ = state.world.remove_one::<ChaseAI>(e);
    let _ = state.world.remove_one::<CompanionAI>(e);
    let _ = state.world.remove_one::<Follower>(e);
    if state.world.get::<&Player>(e).is_err() {
        let _ = state.world.insert_one(e, Player);
    }
    let busy = state.world.get::<&mut Actor>(e).map(|mut a| a.current_action.take().is_some()).unwrap_or(false);
    if busy {
        state.action_scheduler.cancel_for_entity(e);
    }
    state.sync_world_changes();
    state.active_ai_tracker.remove_entity(e);
    state.player_entity = e;
    let at = state.world.get::<&crate::components::Position>(e).ok().map(|p| (p.x, p.y));
    state.active_ai_tracker.initialize_from_world(&state.world, at);
    state.fov_dirty = true;
}

/// Place one of `prefab` (which has `Player`) at `at` and make it the
/// player. The tile has to be floor, and free but for the player there now
/// (who is replaced).
fn place_as_player(
    state: &mut GameState,
    events: &mut EventQueue,
    prefab: &prefab::Prefab,
    at: (i32, i32),
) -> Result<(Entity, Vec<String>), &'static str> {
    if !state.grid.is_walkable(at.0, at.1) {
        return Err("not a floor tile");
    }
    let old = state.player_entity;
    let old_here = state.world.get::<&crate::components::Position>(old).is_ok_and(|p| (p.x, p.y) == at);
    if prefab.blocks() && state.spatial_cache.is_blocked(at) && !old_here {
        return Err("something is in the way");
    }
    let (e, problems) = prefab::instantiate(&mut state.actor_ctx(events), prefab, at);
    become_player(state, e);
    Ok((e, problems))
}

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
    /// With nothing else armed, a press on the map picks what's there and
    /// switches to the inspector, whatever panel was open, and a drag
    /// carries it (never into a wall), keeping the spatial cache in step. A
    /// press on bare floor is left to pan the view; a click there looks at
    /// the tile only if nothing else is open. Closing the inspector lets go
    /// of the pick, then puts it away; clicks still inspect.
    #[test]
    fn the_inspector_picks_and_carries() {
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        let mut engine = GameEngine::new();
        engine.start_sandbox(PlayerClass::Fighter, &mut camera);
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut dev = DevTools::new();
        dev.visible = true;
        dev.tool = Some(CursorTool::Place(Spawnable::Enemy(3)));
        dev.use_tool(state, &mut events, &mut vfx, (5, 5));
        dev.tool = None;
        dev.open_panel(DevTab::Spawn);
        assert_eq!(dev.tool, Some(CursorTool::Inspect), "nothing armed: clicks inspect");

        assert!(dev.press(state, &mut events, &mut vfx, (5, 5)));
        let goblin = dev.inspected.expect("picked the goblin");
        assert!(dev.panel_open && dev.tab == DevTab::Inspect, "switched to the inspector");
        let at = |state: &GameState| state.world.get::<&Position>(goblin).map(|p| (p.x, p.y)).unwrap();
        dev.drag(state, &mut events, &mut vfx, (6, 5));
        dev.drag(state, &mut events, &mut vfx, (7, 6));
        dev.release();
        assert_eq!(at(state), (7, 6));
        assert_eq!(dev.inspected, Some(goblin));
        assert_eq!(dev.inspect_tile, Some((7, 6)), "the inspector follows it");
        assert!(state.spatial_cache.is_blocked((7, 6)) && !state.spatial_cache.is_blocked((5, 5)));
        state.spatial_cache.assert_coherent_with_world(&state.world, "after a move");

        assert!(dev.press(state, &mut events, &mut vfx, (7, 6)));
        dev.drag(state, &mut events, &mut vfx, (0, 6));
        dev.release();
        assert_eq!(at(state), (7, 6), "not into the wall");
        assert!(dev.status.as_deref().is_some_and(|s| s.starts_with("Can't move it")));

        assert!(!dev.press(state, &mut events, &mut vfx, (12, 12)), "bare floor: the press pans");
        assert_eq!(dev.inspected, Some(goblin));
        dev.open_panel(DevTab::Spawn);
        dev.click_floor((12, 12));
        assert_eq!(dev.tab, DevTab::Spawn, "a floor click leaves another panel be");
        dev.close_panel();
        dev.click_floor((12, 12));
        assert!(dev.panel_open && dev.tab == DevTab::Inspect && dev.inspect_tile == Some((12, 12)));

        dev.close_panel();
        assert!(dev.panel_open && dev.inspect_tile.is_none());
        dev.close_panel();
        assert!(!dev.panel_open && dev.tool == Some(CursorTool::Inspect), "clicks still inspect");
    }

    /// Copy and paste: copying arms Paste, each paste places a copy and
    /// looks at it, Paste stays armed whatever panel is opened, and a copy
    /// of the player is only their body (not another you).
    #[test]
    fn copies_paste_where_clicked() {
        use crate::components::{Name, Player};
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        let mut engine = GameEngine::new();
        engine.start_sandbox(PlayerClass::Fighter, &mut camera);
        let mut events = EventQueue::new();
        let mut vfx = VfxManager::new();
        let state = engine.state.as_mut().unwrap();
        let mut dev = DevTools::new();
        dev.visible = true;
        dev.open_panel(DevTab::Spawn);
        dev.tool = Some(CursorTool::Place(Spawnable::Enemy(3)));
        dev.use_tool(state, &mut events, &mut vfx, (5, 5));
        dev.close_panel();
        assert!(dev.press(state, &mut events, &mut vfx, (5, 5)));
        dev.release();
        let goblin = dev.inspected.expect("picked the goblin");

        dev.copy_entity(&state.world, goblin);
        assert_eq!(dev.tool, Some(CursorTool::Paste));
        dev.open_panel(DevTab::Paint);
        assert_eq!(dev.tool, Some(CursorTool::Paste), "Paste is no panel's: it stays");
        for at in [(8, 8), (9, 8)] {
            assert!(dev.press(state, &mut events, &mut vfx, at));
            dev.release();
            let copy = dev.inspected.expect("the copy is looked at");
            assert_ne!(copy, goblin);
            assert_eq!(state.world.get::<&Name>(copy).map(|n| n.0.clone()).unwrap(), "Goblin");
            assert_eq!(state.world.get::<&Position>(copy).map(|p| (p.x, p.y)).unwrap(), at);
        }
        state.spatial_cache.assert_coherent_with_world(&state.world, "after pasting");

        let player = crate::engine::spawn_player(&mut state.world, PlayerClass::Druid, Position::new(12, 12));
        dev.copy_entity(&state.world, player);
        assert!(dev.clipboard.as_ref().is_some_and(|c| !c.has("Player") && c.has("ClassKit")), "a body, not you");

        dev.select((9, 8), dev.inspected);
        let pasted = dev.inspected.unwrap();
        dev.delete_inspected(state.player_entity);
        dev.apply_commands(state, &mut events, &mut vfx);
        assert!(!state.world.contains(pasted), "Delete deletes what's inspected");
        assert!(state.world.get::<&Player>(player).is_ok());
    }

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

    /// The inspector picks whatever is clicked while its panel is open,
    /// with no button to arm it; and a tool lasts only as long as the panel
    /// it was chosen from is open.
    #[test]
    fn tools_last_only_while_their_panel_is_open() {
        let mut dev = DevTools::new();
        dev.visible = true;
        dev.sync_panel_tool();
        assert_eq!(dev.tool, Some(CursorTool::Inspect), "no panel open: clicks inspect");
        assert!(dev.tool_is_default());

        dev.open_panel(DevTab::Spawn);
        dev.tool = Some(CursorTool::Place(Spawnable::Enemy(3)));
        dev.sync_panel_tool();
        assert!(!dev.tool_is_default(), "kept while Spawn is open");
        dev.rail_clicked(DevTab::Spawn);
        assert_eq!(dev.tool, Some(CursorTool::Inspect), "closing Spawn stops the spawning");

        dev.rail_clicked(DevTab::World);
        dev.tool = Some(CursorTool::Teleport);
        dev.rail_clicked(DevTab::Paint);
        assert_eq!(dev.tool, Some(CursorTool::Inspect), "opening another panel drops it too");

        dev.toggle();
        assert_eq!(dev.tool, None, "hidden, nothing is armed");
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
        assert!(dev.panel_open, "picking something opens the inspector");

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

