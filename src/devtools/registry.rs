//! The component registry: the components the inspector can show and edit
//! and prefabs can be made of, each read and written as JSON.
//!
//! hecs has no type-erased access to components, so each one the tools
//! understand is listed here. Most go through their serde form as is; the
//! two that hold links to other entities (`Actor`'s action in progress,
//! `ChaseAI`'s threat table) expose only their settings, and leave the links
//! alone on write.

use std::any::TypeId;

use hecs::{Entity, World};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::components::*;
use crate::tile::tile_ids;

/// One component the tools understand.
pub struct ComponentEntry {
    pub name: &'static str,
    /// Grouping for the add-component menu.
    pub category: &'static str,
    pub type_id: fn() -> TypeId,
    pub read: fn(&World, Entity) -> Option<Value>,
    /// Insert (or replace) it from JSON; the error says what didn't parse.
    pub write: fn(&mut World, Entity, Value) -> Result<(), String>,
    pub remove: fn(&mut World, Entity),
    /// A sensible value to start from when adding it.
    pub default: fn() -> Value,
}

macro_rules! plain {
    ($ty:ty, $cat:expr, $default:expr) => {
        ComponentEntry {
            name: stringify!($ty),
            category: $cat,
            type_id: || TypeId::of::<$ty>(),
            read: |world, e| world.get::<&$ty>(e).ok().and_then(|c| serde_json::to_value(&*c).ok()),
            write: |world, e, value| {
                let component: $ty = serde_json::from_value(value).map_err(|err| err.to_string())?;
                world.insert_one(e, component).map_err(|err| err.to_string())
            },
            remove: |world, e| {
                let _ = world.remove_one::<$ty>(e);
            },
            default: || serde_json::to_value(&$default).unwrap_or(Value::Null),
        }
    };
}

/// `Actor` as the tools see it: how fast it acts. Its action in progress is
/// the scheduler's business and is left as it is.
#[derive(Serialize, Deserialize)]
struct ActorSettings {
    speed: f32,
}

/// `ChaseAI` as the tools see it: its senses and state. Its threat table
/// (who it's after) is left as it is.
#[derive(Serialize, Deserialize)]
struct ChaseAiSettings {
    sight_radius: i32,
    state: AIState,
    ranged_min: i32,
    ranged_max: i32,
    alertness: f32,
}

fn actor_entry() -> ComponentEntry {
    ComponentEntry {
        name: "Actor",
        category: "Creature",
        type_id: || TypeId::of::<Actor>(),
        read: |world, e| {
            let a = world.get::<&Actor>(e).ok()?;
            serde_json::to_value(ActorSettings { speed: a.speed }).ok()
        },
        write: |world, e, value| {
            let s: ActorSettings = serde_json::from_value(value).map_err(|err| err.to_string())?;
            if let Ok(mut a) = world.get::<&mut Actor>(e) {
                a.speed = s.speed;
                return Ok(());
            }
            world.insert_one(e, Actor::new(s.speed)).map_err(|err| err.to_string())
        },
        remove: |world, e| {
            let _ = world.remove_one::<Actor>(e);
        },
        default: || serde_json::to_value(ActorSettings { speed: 1.0 }).unwrap_or(Value::Null),
    }
}

fn chase_ai_entry() -> ComponentEntry {
    ComponentEntry {
        name: "ChaseAI",
        category: "Creature",
        type_id: || TypeId::of::<ChaseAI>(),
        read: |world, e| {
            let ai = world.get::<&ChaseAI>(e).ok()?;
            serde_json::to_value(ChaseAiSettings {
                sight_radius: ai.sight_radius,
                state: ai.state,
                ranged_min: ai.ranged_min,
                ranged_max: ai.ranged_max,
                alertness: ai.alertness,
            })
            .ok()
        },
        write: |world, e, value| {
            let s: ChaseAiSettings = serde_json::from_value(value).map_err(|err| err.to_string())?;
            let apply = |ai: &mut ChaseAI| {
                ai.sight_radius = s.sight_radius;
                ai.state = s.state;
                ai.ranged_min = s.ranged_min;
                ai.ranged_max = s.ranged_max;
                ai.alertness = s.alertness;
            };
            if let Ok(mut ai) = world.get::<&mut ChaseAI>(e) {
                apply(&mut ai);
                return Ok(());
            }
            let mut ai = ChaseAI::new(s.sight_radius);
            apply(&mut ai);
            world.insert_one(e, ai).map_err(|err| err.to_string())
        },
        remove: |world, e| {
            let _ = world.remove_one::<ChaseAI>(e);
        },
        default: || {
            serde_json::to_value(ChaseAiSettings {
                sight_radius: 8,
                state: AIState::Idle,
                ranged_min: 0,
                ranged_max: 0,
                alertness: 0.0,
            })
            .unwrap_or(Value::Null)
        },
    }
}

/// Every registered component, in the order the inspector lists them.
pub fn entries() -> &'static [ComponentEntry] {
    static ENTRIES: std::sync::OnceLock<Vec<ComponentEntry>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        vec![
            plain!(Position, "Core", Position::new(0, 0)),
            plain!(Name, "Core", Name::new("Thing")),
            plain!(Sprite, "Look", Sprite::from_ref(tile_ids::SKELETON)),
            plain!(SpriteTint, "Look", SpriteTint { r: 1.0, g: 1.0, b: 1.0 }),
            plain!(AnimatedSprite, "Look", AnimatedSprite::flames()),
            plain!(Health, "Creature", Health::new(10)),
            plain!(Stats, "Creature", Stats::new(10, 10, 10)),
            actor_entry(),
            chase_ai_entry(),
            plain!(StatusEffects, "Creature", StatusEffects::new()),
            plain!(Asleep, "Creature", Asleep),
            plain!(CanOpenDoors, "Creature", CanOpenDoors),
            plain!(SeesInDark, "Creature", SeesInDark),
            plain!(Flying, "Creature", Flying),
            plain!(FearImmune, "Creature", FearImmune),
            plain!(Tameable, "Creature", Tameable),
            plain!(Venomous, "Creature", Venomous { slow_duration: 3.0, poison_duration: 6.0 }),
            plain!(Lacerating, "Creature", Lacerating { bleed_chance: 0.3 }),
            plain!(Splits, "Creature", Splits { generation: 0, spent: false }),
            plain!(DevFrozen, "Creature", DevFrozen),
            plain!(Attackable, "Physics", Attackable),
            plain!(BlocksMovement, "Physics", BlocksMovement),
            plain!(BlocksVision, "Physics", BlocksVision),
            plain!(Pushable, "Physics", Pushable),
            plain!(Combustible, "Fire and water", Combustible { flammability: 0.5 }),
            plain!(Fuel, "Fire and water", Fuel::oil()),
            plain!(OnFire, "Fire and water", OnFire { remaining: 10.0 }),
            plain!(CausesBurning, "Fire and water", CausesBurning),
            plain!(OilPuddle, "Fire and water", OilPuddle),
            plain!(OilBarrel, "Fire and water", OilBarrel),
            plain!(WetTile, "Fire and water", WetTile { remaining: 10.0 }),
            plain!(
                RainCloud,
                "Fire and water",
                RainCloud {
                    radius: crate::constants::CALL_RAIN_RADIUS,
                    remaining: crate::constants::CALL_RAIN_DURATION,
                    since_soak: 0.0
                }
            ),
            plain!(LightSource, "Light", LightSource::torch()),
            plain!(Lantern, "Light", Lantern { style: LanternStyle::Warm }),
            plain!(Brazier, "Light", Brazier { toppled: false }),
            plain!(Campfire, "Light", Campfire),
            plain!(CarriedTorch, "Light", CarriedTorch { lit: true }),
            plain!(GlowMushroom, "Light", GlowMushroom),
            plain!(CrystalCluster, "Light", CrystalCluster),
            plain!(Container, "Things", Container::chest(Vec::new(), 0)),
            plain!(Door, "Things", Door::new()),
            plain!(DungeonTrap, "Things", DungeonTrap { kind: DungeonTrapKind::Spike, revealed: true }),
            plain!(Furniture, "Things", Furniture { kind: FurnitureKind::Fountain, used: false }),
            plain!(Decoration, "Things", Decoration),
            plain!(Stalagmite, "Things", Stalagmite),
        ]
    })
}

pub fn find(name: &str) -> Option<&'static ComponentEntry> {
    entries().iter().find(|e| e.name == name)
}

/// The registered components an entity has, and how many others it carries
/// that the tools don't understand.
pub fn components_of(world: &World, e: Entity) -> (Vec<&'static ComponentEntry>, usize) {
    let Ok(entity) = world.entity(e) else {
        return (Vec::new(), 0);
    };
    let types: Vec<TypeId> = entity.component_types().collect();
    let known: Vec<&'static ComponentEntry> =
        entries().iter().filter(|entry| types.contains(&(entry.type_id)())).collect();
    let unknown = types.len() - known.len();
    (known, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_reads_back_what_it_writes() {
        let mut world = World::new();
        for entry in entries() {
            let e = world.spawn(());
            let value = (entry.default)();
            (entry.write)(&mut world, e, value.clone())
                .unwrap_or_else(|err| panic!("{}: default doesn't write: {err}", entry.name));
            let read = (entry.read)(&world, e).unwrap_or_else(|| panic!("{}: nothing to read", entry.name));
            assert_eq!(read, value, "{} round trip", entry.name);
            assert_eq!(components_of(&world, e).0.len(), 1, "{} is recognised", entry.name);
            (entry.remove)(&mut world, e);
            assert!((entry.read)(&world, e).is_none(), "{} removed", entry.name);
        }
    }

    #[test]
    fn a_bad_value_is_refused_not_panicked_on() {
        let mut world = World::new();
        let e = world.spawn(());
        let health = find("Health").unwrap();
        assert!((health.write)(&mut world, e, serde_json::json!({"current": "lots"})).is_err());
        assert!(world.get::<&Health>(e).is_err());
    }
}
