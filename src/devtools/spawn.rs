//! Everything the developer tools can place, and the one path that places
//! it: whatever is spawned is registered with the spatial cache and, if it
//! acts, with the AI, so the world stays coherent however it was made.

use hecs::Entity;
use rand::rngs::StdRng;

use crate::components::{
    Actor, BlocksMovement, BlocksVision, BossAbility, ChaseAI, CompanionAI, Container, Door,
    FurnitureKind, TrapEffect, ItemInstance, ItemType, LanternStyle, Position, Rarity, RainCloud,
    Sprite, Follower, VisualPosition,
};
use crate::engine::ActorCtx;
use crate::spawning::{self, enemies, EnemyDef};
use crate::tile::{tile_ids, SpriteSheet};

/// Every ordinary enemy, in the order the panel lists them.
pub static ENEMIES: [&EnemyDef; 11] = [
    &enemies::RAT,
    &enemies::BAT,
    &enemies::SLIME,
    &enemies::GOBLIN,
    &enemies::GOBLIN_SHAMAN,
    &enemies::SKELETON,
    &enemies::SKELETON_ARCHER,
    &enemies::ZOMBIE,
    &enemies::ORC,
    &enemies::LESSER_GIANT_SPIDER,
    &enemies::GIANT_SPIDER,
];

/// The floor each boss guards, which is how `spawning::spawn_boss` picks it.
pub const BOSSES: [(BossAbility, u32); 3] =
    [(BossAbility::GroundSlam, 3), (BossAbility::SummonSpiders, 6), (BossAbility::RaiseDead, 9)];

/// Something the developer tools can put on a tile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spawnable {
    Enemy(usize),
    Boss(BossAbility),
    Wizard,
    Merchant,
    Item(ItemType),
    Campfire,
    Brazier,
    Lantern(LanternStyle),
    FirePatch,
    DroppedTorch,
    OilBarrel,
    OilPuddle,
    Web,
    RainCloud,
    Furniture(FurnitureKind),
    Trap(TrapEffect),
    Chest,
    Door(DoorKind),
    SecretDoor,
    Stalagmites,
    GlowMushrooms,
    CrystalCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorKind {
    Wooden,
    Green,
    Grated,
}

/// Whether a creature is placed awake, asleep, or however its spawn rolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wakefulness {
    #[default]
    Awake,
    Asleep,
    AsRolled,
}

/// How the panel's spawn options are set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnOptions {
    pub wake: Wakefulness,
    /// Creatures come tamed, following the player.
    pub companion: bool,
    /// How many to place: the first on the tile, the rest on the nearest free
    /// tiles around it.
    pub count: u32,
    /// Rarity for gear (weapons, armour, accessories).
    pub rarity: Rarity,
    /// Traps are placed already revealed.
    pub revealed: bool,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self { wake: Wakefulness::Awake, companion: false, count: 1, rarity: Rarity::Common, revealed: true }
    }
}

impl Spawnable {
    pub fn name(&self) -> String {
        match self {
            Spawnable::Enemy(i) => ENEMIES[*i].name.to_string(),
            Spawnable::Boss(ability) => BOSSES
                .iter()
                .find(|(a, _)| a == ability)
                .and_then(|(_, floor)| spawning::boss_for_floor(*floor))
                .map(|(name, _)| name.to_string())
                .unwrap_or_else(|| "Boss".into()),
            Spawnable::Wizard => "Old Wizard".into(),
            Spawnable::Merchant => "Wandering Merchant".into(),
            Spawnable::Item(kind) => crate::systems::items::item_name(*kind).to_string(),
            Spawnable::Campfire => "Campfire".into(),
            Spawnable::Brazier => "Brazier".into(),
            Spawnable::Lantern(LanternStyle::Warm) => "Lantern".into(),
            Spawnable::Lantern(LanternStyle::Ghost) => "Ghost lantern".into(),
            Spawnable::FirePatch => "Fire".into(),
            Spawnable::DroppedTorch => "Burning torch".into(),
            Spawnable::OilBarrel => "Oil barrel".into(),
            Spawnable::OilPuddle => "Oil puddle".into(),
            Spawnable::Web => "Spider web".into(),
            Spawnable::RainCloud => "Rain cloud".into(),
            Spawnable::Furniture(kind) => format!("{kind:?}"),
            Spawnable::Trap(effect) => effect.name().into(),
            Spawnable::Chest => "Chest".into(),
            Spawnable::Door(DoorKind::Wooden) => "Door".into(),
            Spawnable::Door(DoorKind::Green) => "Green door".into(),
            Spawnable::Door(DoorKind::Grated) => "Grated door".into(),
            Spawnable::SecretDoor => "Secret door".into(),
            Spawnable::Stalagmites => "Stalagmites".into(),
            Spawnable::GlowMushrooms => "Glow mushrooms".into(),
            Spawnable::CrystalCluster => "Crystal cluster".into(),
        }
    }

    /// The sprite the panel lists it with, and the cursor's ghost shows.
    pub fn sprite(&self) -> (SpriteSheet, u32) {
        match self {
            Spawnable::Enemy(i) => ENEMIES[*i].sprite,
            Spawnable::Boss(BossAbility::GroundSlam) => tile_ids::ORC,
            Spawnable::Boss(BossAbility::SummonSpiders) => tile_ids::GIANT_SPIDER,
            Spawnable::Boss(BossAbility::RaiseDead) => tile_ids::SKELETON,
            Spawnable::Wizard => tile_ids::WIZARD,
            Spawnable::Merchant => tile_ids::DWARF,
            Spawnable::Item(kind) => crate::systems::items::item_sprite(*kind),
            Spawnable::Campfire => tile_ids::FIRE_PIT,
            Spawnable::Brazier => tile_ids::BRAZIER,
            Spawnable::Lantern(_) => tile_ids::LANTERN,
            Spawnable::FirePatch => tile_ids::FIRE_EFFECT,
            Spawnable::DroppedTorch => tile_ids::TORCH,
            Spawnable::OilBarrel => tile_ids::BARREL,
            Spawnable::OilPuddle => tile_ids::OIL_PUDDLE,
            Spawnable::Web => tile_ids::WEB,
            Spawnable::RainCloud => tile_ids::CALL_RAIN,
            Spawnable::Furniture(FurnitureKind::Fountain) => tile_ids::FOUNTAIN,
            Spawnable::Furniture(FurnitureKind::Altar) => tile_ids::ALTAR,
            Spawnable::Furniture(FurnitureKind::Shrine) => tile_ids::SHRINE,
            Spawnable::Trap(TrapEffect::Spike { .. }) => tile_ids::SPIKES,
            Spawnable::Trap(_) => tile_ids::TRAP_DOOR,
            Spawnable::Chest => tile_ids::CHEST_CLOSED,
            Spawnable::Door(DoorKind::Wooden) => Door::new().closed_sprite,
            Spawnable::Door(DoorKind::Green) => Door::green().closed_sprite,
            Spawnable::Door(DoorKind::Grated) => Door::grated().closed_sprite,
            Spawnable::SecretDoor => tile_ids::WALL,
            Spawnable::Stalagmites => tile_ids::CAVE_STALAGMITES,
            Spawnable::GlowMushrooms => tile_ids::CAVE_GLOW_MUSHROOMS,
            Spawnable::CrystalCluster => tile_ids::CAVE_CRYSTAL_CLUSTER,
        }
    }

    /// Whether it needs a tile nothing else blocks (creatures, furniture);
    /// floor things like oil or items can go under them.
    pub fn needs_clear_tile(&self) -> bool {
        !matches!(
            self,
            Spawnable::Item(_)
                | Spawnable::FirePatch
                | Spawnable::DroppedTorch
                | Spawnable::OilPuddle
                | Spawnable::Web
                | Spawnable::RainCloud
                | Spawnable::Trap(_)
                | Spawnable::Campfire
        )
    }
}

/// Everything but items that the Spawn tab places, for templates.
pub fn all_spawnables() -> Vec<Spawnable> {
    (0..ENEMIES.len())
        .map(Spawnable::Enemy)
        .chain(BOSSES.iter().map(|(a, _)| Spawnable::Boss(*a)))
        .chain([
            Spawnable::Wizard,
            Spawnable::Merchant,
            Spawnable::Campfire,
            Spawnable::Brazier,
            Spawnable::Lantern(LanternStyle::Warm),
            Spawnable::Lantern(LanternStyle::Ghost),
            Spawnable::FirePatch,
            Spawnable::DroppedTorch,
            Spawnable::OilBarrel,
            Spawnable::OilPuddle,
            Spawnable::Web,
            Spawnable::RainCloud,
            Spawnable::Furniture(FurnitureKind::Fountain),
            Spawnable::Furniture(FurnitureKind::Altar),
            Spawnable::Furniture(FurnitureKind::Shrine),
            Spawnable::Trap(TrapEffect::spike()),
            Spawnable::Trap(TrapEffect::fire()),
            Spawnable::Trap(TrapEffect::snare()),
            Spawnable::Trap(TrapEffect::alarm()),
            Spawnable::Chest,
            Spawnable::Door(DoorKind::Wooden),
            Spawnable::Door(DoorKind::Green),
            Spawnable::Door(DoorKind::Grated),
            Spawnable::Stalagmites,
            Spawnable::GlowMushrooms,
            Spawnable::CrystalCluster,
        ])
        .collect()
}

/// Place `what` at `at` (and `options.count - 1` more nearby), keeping the
/// spatial cache and AI in step. Returns what was made, or why not.
pub fn spawn(
    ctx: &mut ActorCtx,
    what: Spawnable,
    at: (i32, i32),
    options: &SpawnOptions,
    floor: u32,
    rng: &mut StdRng,
) -> Result<Vec<Entity>, String> {
    if !ctx.grid.is_walkable(at.0, at.1) {
        return Err("not a floor tile".into());
    }
    let tiles = placement_tiles(ctx, what.needs_clear_tile(), at, options.count.max(1) as usize);
    if tiles.is_empty() {
        return Err("something is in the way".into());
    }
    let mut made = Vec::new();
    for tile in tiles {
        if let Some(e) = spawn_one(ctx, what, tile, options, floor, rng) {
            adopt(ctx, e);
            made.push(e);
        }
    }
    if made.is_empty() {
        Err(format!("couldn't place {}", what.name()))
    } else {
        Ok(made)
    }
}

/// Up to `count` tiles nearest `at` first: walkable, and clear if
/// `needs_clear`.
pub(crate) fn placement_tiles(ctx: &ActorCtx, needs_clear: bool, at: (i32, i32), count: usize) -> Vec<(i32, i32)> {
    let ok = |(x, y): (i32, i32)| ctx.grid.is_walkable(x, y) && !(needs_clear && ctx.spatial.is_blocked((x, y)));
    let mut out = Vec::new();
    for r in 0..=6i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let tile = (at.0 + dx, at.1 + dy);
                if ok(tile) && !out.contains(&tile) {
                    out.push(tile);
                    if out.len() == count {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn spawn_one(
    ctx: &mut ActorCtx,
    what: Spawnable,
    (x, y): (i32, i32),
    options: &SpawnOptions,
    floor: u32,
    rng: &mut StdRng,
) -> Option<Entity> {
    let world = &mut *ctx.world;
    let e = match what {
        Spawnable::Enemy(i) => {
            let e = ENEMIES[i].spawn(world, x, y, rng);
            set_wakefulness(world, e, options.wake);
            if options.companion {
                make_companion(world, e, ctx.player);
            }
            e
        }
        Spawnable::Boss(ability) => {
            let floor = BOSSES.iter().find(|(a, _)| *a == ability)?.1;
            let e = spawning::spawn_boss(world, floor, x, y, rng)?;
            set_wakefulness(world, e, options.wake);
            e
        }
        Spawnable::Wizard => spawning::npcs::WIZARD.spawn(world, x, y),
        Spawnable::Merchant => spawning::vendors::MERCHANT.spawn(world, x, y, floor),
        Spawnable::Item(kind) => {
            let gear = matches!(
                crate::systems::item_defs::get_def(kind).category,
                crate::systems::item_defs::ItemCategory::Weapon
                    | crate::systems::item_defs::ItemCategory::Armor
                    | crate::systems::item_defs::ItemCategory::Accessory
            );
            let item = if gear {
                crate::systems::item_defs::roll_gear_with_rarity(kind, options.rarity, rng)
            } else {
                ItemInstance::plain(kind)
            };
            crate::systems::inventory::spawn_ground_item(world, x, y, item)
        }
        Spawnable::Campfire => spawning::spawn_campfire(world, x, y),
        Spawnable::Brazier => spawning::spawn_brazier(world, x, y),
        Spawnable::Lantern(style) => spawning::spawn_lantern(world, x, y, style),
        Spawnable::FirePatch => spawning::spawn_fire_patch(world, x, y),
        Spawnable::DroppedTorch => spawning::spawn_dropped_torch(world, x, y),
        Spawnable::OilBarrel => spawning::spawn_oil_barrel(world, x, y),
        Spawnable::OilPuddle => spawning::spawn_oil_puddle(world, x, y),
        Spawnable::Web => spawning::spawn_web(world, x, y, None),
        Spawnable::RainCloud => world.spawn((
            Position::new(x, y),
            RainCloud {
                radius: crate::constants::CALL_RAIN_RADIUS,
                remaining: crate::constants::CALL_RAIN_DURATION,
                since_soak: 0.0,
            },
        )),
        Spawnable::Furniture(kind) => spawning::spawn_furniture(world, x, y, kind),
        Spawnable::Trap(effect) => {
            let e = spawning::spawn_dungeon_trap(world, x, y, effect);
            if options.revealed {
                crate::systems::discovery::reveal(world, e);
            }
            e
        }
        Spawnable::Chest => {
            let container: Container = crate::engine::initialization::generate_chest_contents(floor, rng);
            let pos = Position::new(x, y);
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::from_ref(tile_ids::CHEST_CLOSED),
                container,
                BlocksMovement,
            ))
        }
        Spawnable::Door(kind) => {
            let door = match kind {
                DoorKind::Wooden => Door::new(),
                DoorKind::Green => Door::green(),
                DoorKind::Grated => Door::grated(),
            };
            let pos = Position::new(x, y);
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::from_ref(door.closed_sprite),
                door,
                BlocksVision,
                BlocksMovement,
            ))
        }
        Spawnable::SecretDoor => {
            let wall = crate::engine::initialization::secret_door_wall_sprite(ctx.grid, x, y);
            spawning::spawn_secret_door(world, x, y, wall)
        }
        Spawnable::Stalagmites => spawning::spawn_stalagmites(world, x, y),
        Spawnable::GlowMushrooms => spawning::spawn_glow_mushrooms(world, x, y),
        Spawnable::CrystalCluster => spawning::spawn_crystal_cluster(world, x, y),
    };
    Some(e)
}

fn set_wakefulness(world: &mut crate::world::World, e: Entity, wake: Wakefulness) {
    match wake {
        Wakefulness::Awake => {
            let _ = world.remove_one::<crate::components::Asleep>(e);
        }
        Wakefulness::Asleep => {
            let _ = world.insert_one(e, crate::components::Asleep);
        }
        Wakefulness::AsRolled => {}
    }
}

/// Tame a freshly made creature to `owner`, the way taming does it.
fn make_companion(world: &mut crate::world::World, e: Entity, owner: Entity) {
    let _ = world.remove_one::<ChaseAI>(e);
    let _ = world.remove_one::<crate::components::Asleep>(e);
    let _ = world.insert(
        e,
        (Follower { leader: owner }, CompanionAI::new()),
    );
}

/// Register a new entity with the spatial cache and, if it acts, the AI.
pub fn adopt(ctx: &mut ActorCtx, e: Entity) {
    let Some(pos) = ctx.world.get::<&Position>(e).ok().map(|p| (p.x, p.y)) else {
        return;
    };
    let (blocks, sight) = ctx
        .world
        .entity(e)
        .map(|r| (r.has::<BlocksMovement>(), r.has::<BlocksVision>()))
        .unwrap_or((false, false));
    if blocks || sight {
        ctx.spatial.register_entity(e, pos, blocks, sight);
    }
    if ctx.world.get::<&Actor>(e).is_ok() {
        // A dev rng for the AI's first decision, so a seeded run's stream
        // isn't shifted by what you placed.
        let mut rng = <StdRng as rand::SeedableRng>::from_entropy();
        let mut dev_ctx = ActorCtx { rng: &mut rng, ..ctx.reborrow() };
        crate::engine::initialize_single_ai_actor(&mut dev_ctx, e);
    }
}
