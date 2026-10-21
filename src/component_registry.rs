//! The component registry: every component a floor entity can carry, each
//! read and written as JSON. It is how a floor is saved when the player
//! leaves it (`snapshot` / `restore`), and what the developer tools' inspector
//! and prefabs are built on.
//!
//! hecs has no type-erased access to components, so each one is listed here.
//! A component missing from the list is lost when its floor is saved; the
//! test `every_component_on_a_floor_is_saved` keeps the list complete. Most go through their serde form as is; the
//! two that hold links to other entities (`Actor`'s action in progress,
//! `ChaseAI`'s threat table) expose only their settings, and leave the links
//! alone on write.

use std::any::TypeId;

use hecs::Entity;
use crate::world::World;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::components::*;
use crate::tile::tile_ids;

/// Serde for a link to another entity (`#[serde(with = "entity_ref")]`).
///
/// Written as the entity's bits. Read back through the remapping that
/// `restore` sets up while it rebuilds a floor, since every entity it
/// restores comes back with a new id; a link to anything it isn't restoring
/// (the player, who stays) is kept as it was. hecs ids carry a generation,
/// so a link to something long gone can't come back pointing at a stranger.
pub mod entity_ref {
    use hecs::Entity;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static REMAP: RefCell<HashMap<u64, Entity>> = RefCell::new(HashMap::new());
    }

    /// Run `f` with links to the keys of `map` pointing at its values.
    pub(crate) fn with_remap<R>(map: HashMap<u64, Entity>, f: impl FnOnce() -> R) -> R {
        REMAP.with(|r| *r.borrow_mut() = map);
        let out = f();
        REMAP.with(|r| r.borrow_mut().clear());
        out
    }

    fn resolve(bits: u64) -> Option<Entity> {
        REMAP.with(|r| r.borrow().get(&bits).copied()).or_else(|| Entity::from_bits(bits))
    }

    pub fn serialize<S: Serializer>(e: &Entity, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(e.to_bits().get())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Entity, D::Error> {
        let bits = u64::deserialize(d)?;
        resolve(bits).ok_or_else(|| serde::de::Error::custom("not an entity"))
    }

    /// The same, for an `Option<Entity>`.
    pub mod option {
        use super::*;

        pub fn serialize<S: Serializer>(e: &Option<Entity>, s: S) -> Result<S::Ok, S::Error> {
            match e {
                Some(e) => s.serialize_some(&e.to_bits().get()),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Entity>, D::Error> {
            Ok(Option::<u64>::deserialize(d)?.and_then(resolve))
        }
    }
}

/// Serde for a time that may be forever (`f32::INFINITY`: a hearth that
/// never burns out, a channel that runs until broken). JSON has no infinity,
/// so forever is written as the string "forever"
/// (`#[serde(with = "crate::component_registry::seconds")]`).
pub mod seconds {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(t: &f32, s: S) -> Result<S::Ok, S::Error> {
        if t.is_infinite() {
            s.serialize_str("forever")
        } else {
            s.serialize_f32(*t)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Time {
            Seconds(f32),
            Word(String),
        }
        match Time::deserialize(d)? {
            Time::Seconds(t) => Ok(t),
            Time::Word(w) if w == "forever" => Ok(f32::INFINITY),
            Time::Word(w) => Err(serde::de::Error::custom(format!("not a time: {w}"))),
        }
    }
}

/// An entity as a list of its components' JSON: what a floor save holds for
/// each thing on the floor, and what a prefab is.
pub type Snapshot = Vec<(&'static str, Value)>;

/// Every registered component `e` carries.
pub fn snapshot(world: &World, e: Entity) -> Snapshot {
    components_of(world, e)
        .0
        .into_iter()
        .filter_map(|entry| (entry.read)(world, e).map(|v| (entry.name, v)))
        .collect()
}

/// Rebuild entities from snapshots taken together (`snapshot`), with the
/// links between them intact. Returns the new entities in the same order.
/// A component that no longer parses is skipped, not a panic: one odd value
/// shouldn't cost the whole floor.
pub fn restore(world: &mut World, saved: &[(Entity, Snapshot)]) -> Vec<Entity> {
    let fresh: Vec<Entity> = saved.iter().map(|_| world.spawn(())).collect();
    let remap = saved.iter().zip(&fresh).map(|((old, _), new)| (old.to_bits().get(), *new)).collect();
    entity_ref::with_remap(remap, || {
        for ((_, parts), &e) in saved.iter().zip(&fresh) {
            for (name, value) in parts {
                if let Some(entry) = find(name) {
                    if let Err(err) = (entry.write)(world, e, value.clone()) {
                        eprintln!("[save] restoring {name}: {err}");
                    }
                }
            }
        }
    });
    fresh
}

/// One component the tools understand.
pub struct ComponentEntry {
    pub name: &'static str,
    pub type_id: fn() -> TypeId,
    pub read: fn(&World, Entity) -> Option<Value>,
    /// Insert (or replace) it from JSON; the error says what didn't parse.
    pub write: fn(&mut World, Entity, Value) -> Result<(), String>,
    pub remove: fn(&mut World, Entity),
    /// A sensible value to start from when adding it.
    pub default: fn() -> Value,
}

macro_rules! plain {
    ($ty:ty, $default:expr) => {
        ComponentEntry {
            name: stringify!($ty),
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
            plain!(Position, Position::new(0, 0)),
            plain!(Name, Name::new("Thing")),
            plain!(Sprite, Sprite::from_ref(tile_ids::SKELETON)),
            plain!(SpriteTint, SpriteTint { r: 1.0, g: 1.0, b: 1.0 }),
            plain!(AnimatedSprite, AnimatedSprite::flames()),
            plain!(Health, Health::new(10)),
            plain!(Stats, Stats::new(10, 10, 10)),
            actor_entry(),
            chase_ai_entry(),
            plain!(StatusEffects, StatusEffects::new()),
            plain!(Asleep, Asleep),
            plain!(CanOpenDoors, CanOpenDoors),
            plain!(SeesInDark, SeesInDark),
            plain!(Flying, Flying),
            plain!(FearImmune, FearImmune),
            plain!(Tameable, Tameable),
            plain!(Venomous, Venomous { slow_duration: 3.0, poison_duration: 6.0 }),
            plain!(Lacerating, Lacerating { bleed_chance: 0.3 }),
            plain!(Splits, Splits { generation: 0, spent: false }),
            plain!(DevFrozen, DevFrozen),
            plain!(BlocksMovement, BlocksMovement),
            plain!(BlocksVision, BlocksVision),
            plain!(Pushable, Pushable),
            plain!(Combustible, Combustible { flammability: 0.5 }),
            plain!(Fuel, Fuel::oil()),
            plain!(OnFire, OnFire { remaining: 10.0 }),
            plain!(CausesBurning, CausesBurning),
            plain!(OilPuddle, OilPuddle),
            plain!(Explosive, Explosive),
            plain!(WetTile, WetTile { remaining: 10.0 }),
            plain!(RainCloud,
                RainCloud {
                    radius: crate::constants::CALL_RAIN_RADIUS,
                    remaining: crate::constants::CALL_RAIN_DURATION,
                    since_soak: 0.0
                }
            ),
            plain!(LightSource, LightSource::torch()),
            plain!(Topples, Topples { toppled: false }),
            plain!(Shatters, Shatters),
            plain!(CarriedTorch, CarriedTorch { lit: true }),
            plain!(Container, Container::chest(Vec::new(), 0)),
            plain!(Door, Door::new()),
            plain!(Trap, Trap { effect: TrapEffect::spike(), owner: None }),
            plain!(Concealed, {
                let (sprite, tint) = TrapEffect::spike().revealed_look();
                Concealed { sprite, tint }
            }),
            plain!(FlameOverlay, FlameOverlay),
            plain!(VisualPosition, VisualPosition { x: 0.0, y: 0.0 }),
            plain!(OverlaySprite, OverlaySprite::from_ref(tile_ids::BOW)),
            plain!(SecretDoor, SecretDoor),
            plain!(GroundItemPile, GroundItemPile),
            plain!(Raisable, Raisable),
            plain!(XpMultiplier, XpMultiplier(2)),
            plain!(BarrelFuse, BarrelFuse::new(3.0, FuseCause::Fire)),
            plain!(Equipment, Equipment::empty()),
            plain!(Channel, Channel {
                effect: ChannelEffect::Shout { radius: 5 },
                target: None,
                elapsed: 0.0,
                required: 1.5,
                range: 0,
                breaks_on_move: true,
                breaks_on_damage: true,
            }),
            plain!(LightExposure, LightExposure(0.0)),
            plain!(RootStruggleNoticed, RootStruggleNoticed { remaining: 0.0 }),
            plain!(RangedCooldown, RangedCooldown { remaining: 0.0 }),
            plain!(Charger, Charger::default()),
            plain!(HitAndRun, HitAndRun::default()),
            plain!(Grabber, Grabber),
            plain!(GrabbedBy, GrabbedBy { grabber: Entity::DANGLING }),
            plain!(Flanker, Flanker),
            plain!(PackHunter, PackHunter),
            plain!(SupportAI, SupportAI { cooldown: 0.0 }),
            plain!(WebWalker, WebWalker),
            plain!(WebSpinner, WebSpinner { cooldown: 0.0, interval: 8.0 }),
            plain!(Web, Web { spinner: None }),
            plain!(Boss, Boss { ability: BossAbility::GroundSlam, cooldown: 0.0, announced: false }),
            plain!(Summoned, Summoned { by: Entity::DANGLING }),
            plain!(Follower, Follower { leader: Entity::DANGLING }),
            plain!(CompanionAI, CompanionAI::new()),
            plain!(FriendlyNPC, FriendlyNPC),
            plain!(Vendor, Vendor::new(Vec::new(), 0)),
            plain!(Dialogue, Dialogue::new(
                "Someone",
                vec![DialogueNode {
                    text: "Hello.".into(),
                    options: vec![DialogueOption { label: "Goodbye".into(), next_node: None, action: DialogueAction::None }],
                }],
            )),
            plain!(Furniture, Furniture { kind: FurnitureKind::Fountain, used: false }),
        ]
    })
}

/// What each component can't work without: adding it brings these along
/// (at their defaults), and removing one of these takes it away too, so an
/// entity can never hold a component without what it needs. Applied by the
/// world itself (`world::World`'s hooks), whoever makes the change.
///
/// Only hard requirements belong here: data the component reads, with one
/// right answer. "Needs one of these" hints (a tint needs a Sprite or an
/// AnimatedSprite) are the prefab builder's (`devtools::catalog`). Each
/// entry is (component, what it requires).
pub const REQUIRES: &[(&str, &[&str])] = &[
    // A mind needs turns to act in; a companion's needs someone to follow.
    ("ChaseAI", &["Actor"]),
    ("CompanionAI", &["Actor", "Follower"]),
    // A channel advances on its channeller's turns.
    ("Channel", &["Actor"]),
    // Settings of a hostile mind: they only mean something to one.
    ("Asleep", &["ChaseAI"]),
    ("SeesInDark", &["ChaseAI"]),
    ("SupportAI", &["ChaseAI"]),
    ("Flanker", &["ChaseAI"]),
    ("PackHunter", &["ChaseAI"]),
    ("Charger", &["ChaseAI"]),
    ("HitAndRun", &["ChaseAI"]),
    ("WebSpinner", &["ChaseAI"]),
    // Burning is a status effect.
    ("Combustible", &["StatusEffects"]),
    ("Explosive", &["StatusEffects"]),
    ("BarrelFuse", &["Explosive"]),
    ("OnFire", &["Fuel"]),
    ("Splits", &["Health"]),
    ("Pushable", &["BlocksMovement"]),
    // Remains and loose items are containers of what they hold.
    ("GroundItemPile", &["Container"]),
    ("Raisable", &["Container"]),
    // Someone you can talk to has something to say (without it, talking
    // opened no window and swallowed all input); a shopkeeper is one.
    ("FriendlyNPC", &["Dialogue"]),
    ("Vendor", &["FriendlyNPC"]),
    ("OverlaySprite", &["Sprite"]),
];

/// The requirement table by component type: what each requires, and what
/// requires it.
struct Requirements {
    requires: std::collections::HashMap<TypeId, Vec<&'static ComponentEntry>>,
    required_by: std::collections::HashMap<TypeId, Vec<&'static ComponentEntry>>,
}

fn requirements() -> &'static Requirements {
    static TABLE: std::sync::OnceLock<Requirements> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Requirements { requires: Default::default(), required_by: Default::default() };
        for (name, needs) in REQUIRES {
            let Some(entry) = find(name) else { continue };
            for need in *needs {
                let Some(needed) = find(need) else { continue };
                table.requires.entry((entry.type_id)()).or_default().push(needed);
                table.required_by.entry((needed.type_id)()).or_default().push(entry);
            }
        }
        table
    })
}

/// What a component of type `t` requires.
pub fn requires_of(t: TypeId) -> &'static [&'static ComponentEntry] {
    requirements().requires.get(&t).map(Vec::as_slice).unwrap_or(&[])
}

/// What requires a component of type `t`.
pub fn required_by(t: TypeId) -> &'static [&'static ComponentEntry] {
    requirements().required_by.get(&t).map(Vec::as_slice).unwrap_or(&[])
}

/// Requirements `e` doesn't meet, as (component, what it's missing). Always
/// empty while the world's hooks hold; the combination fuzz test checks.
#[cfg(test)]
pub fn unmet_requirements(world: &World, e: Entity) -> Vec<(&'static str, &'static str)> {
    let (known, _) = components_of(world, e);
    let names: Vec<&str> = known.iter().map(|k| k.name).collect();
    REQUIRES
        .iter()
        .filter(|(component, _)| names.contains(component))
        .flat_map(|(component, needs)| needs.iter().filter(|n| !names.contains(n)).map(move |n| (*component, *n)))
        .collect()
}

/// Everything `name` requires, following requirements of requirements, in
/// the order they'd be added.
pub fn all_requirements(name: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let mut stack = vec![name];
    while let Some(next) = stack.pop() {
        for (component, needs) in REQUIRES {
            if *component == next {
                for need in *needs {
                    if !out.contains(need) && *need != name {
                        out.push(need);
                        stack.push(need);
                    }
                }
            }
        }
    }
    out
}

/// Everything that requires `name`, directly or through a chain: what
/// removing it would take away too.
pub fn all_dependents(name: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let mut stack = vec![name];
    while let Some(next) = stack.pop() {
        for (component, needs) in REQUIRES {
            if needs.contains(&next) && !out.contains(component) && *component != name {
                out.push(component);
                stack.push(component);
            }
        }
    }
    out
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
            let known: Vec<&str> = components_of(&world, e).0.iter().map(|k| k.name).collect();
            assert!(known.contains(&entry.name), "{} is recognised", entry.name);
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

    /// Requirements come along when a component is added, from any path,
    /// and go when they're removed: a hostile mind always has turns to act
    /// in, a companion someone to follow, and neither outlives them.
    #[test]
    fn requirements_follow_and_cascade() {
        let mut world = World::new();
        let creature = world.spawn((Position::new(1, 1), CompanionAI::new()));
        assert!(world.get::<&Actor>(creature).is_ok(), "a companion mind brings turns");
        assert!(world.get::<&Follower>(creature).is_ok(), "and someone to follow");

        let sleeper = world.spawn((Position::new(2, 2),));
        world.insert_one(sleeper, Asleep).unwrap();
        assert!(world.get::<&ChaseAI>(sleeper).is_ok(), "chains: Asleep brings ChaseAI");
        assert!(world.get::<&Actor>(sleeper).is_ok(), "which brings Actor");

        // Take away its turns: the minds that needed them go, and with them
        // the settings that only meant something to the hostile one.
        world.remove_one::<Actor>(sleeper).unwrap();
        assert!(world.get::<&ChaseAI>(sleeper).is_err());
        assert!(world.get::<&Asleep>(sleeper).is_err());

        // Something you can talk to always has something to say.
        let npc = world.spawn((Position::new(3, 3), FriendlyNPC));
        let dialogue = world.get::<&Dialogue>(npc).expect("talking needs words");
        assert!(!dialogue.nodes.is_empty(), "a usable dialogue, not an empty one that locks input");
    }
}

