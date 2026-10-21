//! Prefabs: an entity described as a list of registered components and
//! their settings, to place as many times as you like.
//!
//! Made from scratch, captured from an entity in the world, or started from
//! anything the Spawn panel can place (built in a scratch world and
//! captured). Kept in `PREFAB_FILE` in the working directory; reading and
//! writing it is best effort, like the run history: a prefab that doesn't
//! parse is skipped, a write that fails is logged.

use std::path::Path;

use hecs::Entity;
use crate::world::World;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::component_registry as registry;
use super::spawn::{self, SpawnOptions, Spawnable};
use crate::components::{Position, VisualPosition};
use crate::engine::ActorCtx;

/// Where prefabs are kept, relative to the working directory.
pub const PREFAB_FILE: &str = "dev_prefabs.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prefab {
    pub name: String,
    pub components: Vec<PrefabComponent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrefabComponent {
    /// A registry name (`registry::find`).
    pub component: String,
    pub value: Value,
}

impl Prefab {
    pub fn empty(name: &str) -> Self {
        Self { name: name.to_string(), components: Vec::new() }
    }

    /// The names of the components it has.
    pub fn names(&self) -> Vec<&str> {
        self.components.iter().map(|c| c.component.as_str()).collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.components.iter().any(|c| c.component == name)
    }

    /// Add `name` at its registry default, if it isn't there already.
    /// Returns whether it was added.
    pub fn add(&mut self, name: &str) -> bool {
        if self.has(name) || name == "Position" {
            return false;
        }
        let Some(entry) = registry::find(name) else {
            return false;
        };
        self.components.push(PrefabComponent { component: entry.name.to_string(), value: (entry.default)() });
        // What it requires comes with it, as the world would add it anyway.
        for needed in registry::all_requirements(name) {
            if !self.has(needed) {
                if let Some(entry) = registry::find(needed) {
                    self.components.push(PrefabComponent { component: entry.name.to_string(), value: (entry.default)() });
                }
            }
        }
        true
    }

    /// Remove the component at `index`, and anything that requires it (as
    /// the world would).
    pub fn remove_at(&mut self, index: usize) {
        let Some(part) = self.components.get(index) else { return };
        let mut gone = registry::all_dependents(&part.component);
        gone.push(registry::find(&part.component).map(|e| e.name).unwrap_or(""));
        let name = part.component.clone();
        self.components.retain(|c| c.component != name && !gone.contains(&c.component.as_str()));
    }

    /// What in this prefab requires `name` (so it can't go without them).
    pub fn needed_by(&self, name: &str) -> Vec<&str> {
        registry::all_dependents(name).into_iter().filter(|d| self.has(d)).collect()
    }

    /// The value of `name`, if it has it.
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.components.iter().find(|c| c.component == name).map(|c| &c.value)
    }

    /// What it's missing for its components to do anything: (component,
    /// what it needs), where the need is "A" or "A|B".
    pub fn unmet_needs(&self) -> Vec<(&str, &'static str)> {
        let have = self.names();
        let mut out = Vec::new();
        for name in &have {
            let Some(info) = super::catalog::info_of(name) else { continue };
            for need in info.needs {
                if !super::catalog::need_met(need, &have) {
                    out.push((*name, *need));
                }
            }
        }
        out
    }

    /// Whether it blocks its tile, so needs a clear one to go on.
    pub fn blocks(&self) -> bool {
        self.components.iter().any(|c| c.component == "BlocksMovement")
    }

    /// The sprite it shows, for lists and the cursor's ghost.
    pub fn sprite(&self) -> Option<(crate::tile::SpriteSheet, u32)> {
        let look = self
            .components
            .iter()
            .find(|c| c.component == "Sprite" || c.component == "AnimatedSprite")?;
        let sheet = serde_json::from_value(look.value.get("sheet")?.clone()).ok()?;
        let id = look.value.get("tile_id").or_else(|| look.value.get("base_tile_id"))?.as_u64()?;
        Some((sheet, id as u32))
    }
}

/// An entity as a prefab: every registered component it has but its
/// position (which comes from where it's placed).
pub fn capture(world: &World, e: Entity, name: &str) -> Prefab {
    let (known, _) = registry::components_of(world, e);
    let components = known
        .iter()
        .filter(|entry| entry.name != "Position")
        .filter_map(|entry| {
            Some(PrefabComponent { component: entry.name.to_string(), value: (entry.read)(world, e)? })
        })
        .collect();
    Prefab { name: name.to_string(), components }
}

/// A prefab of something the Spawn panel places: built in a scratch world (so
/// nothing in the run is touched) and captured.
pub fn template(what: Spawnable) -> Option<Prefab> {
    let mut world = World::new();
    let mut grid = crate::grid::Grid::filled(5, 5, crate::tile::TileType::Floor);
    let mut clock = crate::time_system::GameClock::new();
    let mut scheduler = crate::time_system::ActionScheduler::new();
    let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
    let mut spatial = crate::spatial_cache::SpatialCache::new();
    let mut events = crate::events::EventQueue::new();
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    let player = world.spawn((Position::new(0, 0),));
    let mut ctx = ActorCtx {
        world: &mut world,
        grid: &mut grid,
        player,
        clock: &mut clock,
        scheduler: &mut scheduler,
        tracker: &mut tracker,
        spatial: &mut spatial,
        events: &mut events,
        rng: &mut rng,
    };
    let mut dev_rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    let made = spawn::spawn(&mut ctx, what, (2, 2), &SpawnOptions::default(), 1, &mut dev_rng).ok()?;
    Some(capture(&world, *made.first()?, &what.name()))
}

/// Make one of `prefab` at `at`, keeping the spatial cache and AI in step.
/// Components it can't build are skipped and named in the error, with the
/// entity made anyway.
pub fn instantiate(ctx: &mut ActorCtx, prefab: &Prefab, at: (i32, i32)) -> (Entity, Vec<String>) {
    let pos = Position::new(at.0, at.1);
    let e = ctx.world.spawn((pos, VisualPosition::from_position(&pos)));
    let mut problems = Vec::new();
    for part in &prefab.components {
        let Some(entry) = registry::find(&part.component) else {
            problems.push(format!("{}: not a known component", part.component));
            continue;
        };
        if let Err(why) = (entry.write)(ctx.world, e, part.value.clone()) {
            problems.push(format!("{}: {why}", part.component));
        }
    }
    // A follower with nobody to follow (a prefab made from scratch) follows
    // the player.
    let leaderless = ctx.world.get::<&crate::components::Follower>(e).is_ok_and(|f| !ctx.world.contains(f.leader));
    if leaderless {
        let _ = ctx.world.insert_one(e, crate::components::Follower { leader: ctx.player });
    }
    spawn::adopt(ctx, e);
    (e, problems)
}

/// Read the prefab file, skipping anything that doesn't parse. A missing
/// file is no prefabs.
pub fn load(path: &Path) -> Vec<Prefab> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&text) else {
        eprintln!("{}: not a list of prefabs; ignoring it", path.display());
        return Vec::new();
    };
    items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).map(migrate).collect()
}

/// Bring a prefab saved by an older version up to date: `Attackable` is now
/// part of `Health` (anything with health can be hit), and
/// `HitOnlyOnPurpose` is its "on purpose" targeting setting.
fn migrate(mut prefab: Prefab) -> Prefab {
    let on_purpose = prefab.has("HitOnlyOnPurpose");
    prefab.components.retain(|c| c.component != "Attackable" && c.component != "HitOnlyOnPurpose");
    if on_purpose {
        if let Some(Value::Object(health)) =
            prefab.components.iter_mut().find(|c| c.component == "Health").map(|c| &mut c.value)
        {
            health.insert("targeting".into(), Value::from("OnPurpose"));
        }
    }
    prefab
}

/// Write every prefab to the file. Failures are logged and otherwise
/// ignored: losing a dev convenience must never stop the game.
pub fn save(path: &Path, prefabs: &[Prefab]) {
    match serde_json::to_string_pretty(prefabs) {
        Ok(text) => {
            if let Err(err) = std::fs::write(path, text) {
                eprintln!("Couldn't save prefabs to {}: {err}", path.display());
            }
        }
        Err(err) => eprintln!("Couldn't write prefabs: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Spawn panel offers "Make a prefab from this" on every built-in,
    /// items included: each has to come out as a prefab.
    #[test]
    fn every_built_in_can_be_made_into_a_prefab() {
        let items = crate::systems::item_defs::ITEM_DEFS.iter().map(|d| Spawnable::Item(d.item_type));
        for what in spawn::all_spawnables().into_iter().chain(items) {
            let made = template(what);
            assert!(made.is_some_and(|p| !p.components.is_empty()), "{} made no prefab", what.name());
        }
    }

    #[test]
    fn a_template_saves_loads_and_builds_the_same_thing() {
        let goblin = template(Spawnable::Enemy(3)).expect("a goblin template");
        assert!(goblin.components.iter().any(|c| c.component == "ChaseAI"));
        assert!(goblin.blocks());
        assert!(goblin.sprite().is_some());

        let dir = std::env::temp_dir().join(format!("grid_prefabs_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefabs.json");
        save(&path, std::slice::from_ref(&goblin));
        let loaded = load(&path);
        assert_eq!(loaded, vec![goblin.clone()]);

        // Built in another scratch world, it has what the template has.
        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0),));
        let mut grid = crate::grid::Grid::filled(5, 5, crate::tile::TileType::Floor);
        let (mut clock, mut scheduler) = (crate::time_system::GameClock::new(), crate::time_system::ActionScheduler::new());
        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        let mut spatial = crate::spatial_cache::SpatialCache::new();
        let mut events = crate::events::EventQueue::new();
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(3);
        let mut ctx = ActorCtx {
            world: &mut world,
            grid: &mut grid,
            player,
            clock: &mut clock,
            scheduler: &mut scheduler,
            tracker: &mut tracker,
            spatial: &mut spatial,
            events: &mut events,
            rng: &mut rng,
        };
        let (e, problems) = instantiate(&mut ctx, &loaded[0], (1, 1));
        assert!(problems.is_empty(), "{problems:?}");
        let rebuilt = capture(&world, e, &goblin.name);
        let names = |p: &Prefab| p.components.iter().map(|c| c.component.clone()).collect::<Vec<_>>();
        assert_eq!(names(&rebuilt), names(&goblin));
        spatial.assert_coherent_with_world(&world, "after building a prefab");

        // A corrupt file, or a corrupt entry in it, is skipped, not a panic.
        std::fs::write(&path, r#"[{"name": "ok", "components": []}, {"nonsense": 1}]"#).unwrap();
        assert_eq!(load(&path).len(), 1);
        std::fs::write(&path, "not json").unwrap();
        assert!(load(&path).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A prefab saved before Attackable became part of Health loads as it
    /// meant: hittable, and "on purpose" where it said HitOnlyOnPurpose.
    #[test]
    fn an_old_prefab_with_attackable_loads_as_health() {
        let old = serde_json::json!([{
            "name": "Old lamp",
            "components": [
                {"component": "Health", "value": {"current": 5, "max": 5, "regen_amount": 0, "regen_interval": 0.0, "last_regen_time": 0.0}},
                {"component": "Attackable", "value": null},
                {"component": "HitOnlyOnPurpose", "value": null}
            ]
        }]);
        let dir = std::env::temp_dir().join(format!("grid_old_prefab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefabs.json");
        std::fs::write(&path, old.to_string()).unwrap();
        let loaded = load(&path);
        let names: Vec<&str> = loaded[0].names();
        assert_eq!(names, vec!["Health"]);
        assert_eq!(loaded[0].value("Health").unwrap()["targeting"], "OnPurpose");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

