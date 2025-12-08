use crate::constants::*;
use hecs::Entity;

/// Position component - world coordinates (grid-based)
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Sprite component - visual representation using tileset
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub tile_id: u32,
}

impl Sprite {
    pub fn new(tile_id: u32) -> Self {
        Self { tile_id }
    }
}

/// Player marker component
#[derive(Debug, Clone, Copy)]
pub struct Player;

/// Health component - pure data
#[derive(Debug, Clone, Copy)]
pub struct Health {
    pub current: i32,
    pub max: i32,
    /// HP regenerated per regen event
    pub regen_amount: i32,
    /// Seconds between regen events (0.0 = no regen)
    pub regen_interval: f32,
    /// Game time of last regen event
    pub last_regen_time: f32,
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self {
            current: max,
            max,
            regen_amount: 0,
            regen_interval: 0.0,
            last_regen_time: 0.0,
        }
    }

    /// Create health with regeneration (time-based)
    pub fn with_regen(max: i32, regen_amount: i32, regen_interval: f32) -> Self {
        Self {
            current: max,
            max,
            regen_amount,
            regen_interval,
            last_regen_time: 0.0,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0
    }
}

/// Stats component - pure data
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub strength: i32,
    pub intelligence: i32,
    pub agility: i32,
}

impl Stats {
    pub fn new(strength: i32, intelligence: i32, agility: i32) -> Self {
        Self { strength, intelligence, agility }
    }
}

/// Experience component - pure data for XP and level
#[derive(Debug, Clone, Copy)]
pub struct Experience {
    pub current: u32,
    pub level: u32,
}

impl Experience {
    pub fn new() -> Self {
        Self { current: 0, level: 1 }
    }
}

/// Item type - pure data enum, properties defined in systems
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemType {
    HealthPotion,
}

/// Item component
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub item_type: ItemType,
}

impl Item {
    pub fn new(item_type: ItemType) -> Self {
        Self { item_type }
    }
}

/// Inventory component - pure data
#[derive(Debug, Clone)]
pub struct Inventory {
    pub items: Vec<ItemType>,
    pub current_weight_kg: f32,
    pub gold: u32,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_weight_kg: 0.0,
            gold: 0,
        }
    }
}

/// Container component (for chests and bones)
#[derive(Debug, Clone)]
pub struct Container {
    pub items: Vec<ItemType>,
    pub gold: u32,
    pub is_open: bool,
}

// =============================================================================
// TIME SYSTEM COMPONENTS
// =============================================================================

/// Types of actions an actor can perform
#[derive(Debug, Clone, Copy)]
pub enum ActionType {
    /// Moving in a direction
    Move { dx: i32, dy: i32, is_diagonal: bool },
    /// Attacking a target entity
    Attack { target: Entity },
    /// Opening a door
    OpenDoor { door: Entity },
    /// Opening/interacting with a chest
    OpenChest { chest: Entity },
    /// Waiting in place (pass turn)
    Wait,
    /// Shooting a bow at a target position
    ShootBow { target_x: i32, target_y: i32 },
    /// Using stairs to change floors
    UseStairs { x: i32, y: i32, direction: crate::events::StairDirection },
}

impl ActionType {
    /// Energy cost to start this action
    pub fn energy_cost(&self) -> i32 {
        match self {
            // All basic actions cost 1 energy by default
            // This can be customized per-action as needed
            ActionType::Move { .. } => 1,
            ActionType::Attack { .. } => 1,
            ActionType::OpenDoor { .. } => 1,
            ActionType::OpenChest { .. } => 1,
            ActionType::Wait => 1,
            ActionType::ShootBow { .. } => 1,
            ActionType::UseStairs { .. } => 1,
        }
    }
}

/// An action currently being executed by an entity
#[derive(Debug, Clone, Copy)]
pub struct ActionInProgress {
    pub action_type: ActionType,
    pub start_time: f32,
    pub completion_time: f32,
}

/// Actor component - for entities that take actions in game time
/// Energy is a budget: spend to start actions, regen over time
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    /// Current energy pool (0 to max_energy)
    pub energy: i32,
    /// Maximum energy (budget cap)
    pub max_energy: i32,
    /// Speed multiplier (1.0 = normal, higher = faster)
    pub speed: f32,
    /// Currently executing action (None if idle and ready)
    pub current_action: Option<ActionInProgress>,
    /// Seconds between energy regeneration events
    pub energy_regen_interval: f32,
    /// Game time of last energy regen event
    pub last_energy_regen_time: f32,
}

/// Default energy regen interval (1 second per energy point)
pub const DEFAULT_ENERGY_REGEN_INTERVAL: f32 = 1.0;

impl Actor {
    pub fn new(max_energy: i32, speed: f32) -> Self {
        Self {
            energy: max_energy, // Start with full energy
            max_energy,
            speed,
            current_action: None,
            energy_regen_interval: DEFAULT_ENERGY_REGEN_INTERVAL,
            last_energy_regen_time: 0.0,
        }
    }

    pub fn with_energy_regen(max_energy: i32, speed: f32, energy_regen_interval: f32) -> Self {
        Self {
            energy: max_energy,
            max_energy,
            speed,
            current_action: None,
            energy_regen_interval,
            last_energy_regen_time: 0.0,
        }
    }

    /// Can start a new action (has energy and not mid-action)
    pub fn can_act(&self) -> bool {
        self.energy > 0 && self.current_action.is_none()
    }

    /// Is currently busy with an action
    pub fn is_busy(&self) -> bool {
        self.current_action.is_some()
    }
}

/// AI behavior state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AIState {
    /// Wandering randomly, hasn't seen the player
    Idle,
    /// Actively chasing the player (can see them)
    Chasing,
    /// Moving to last known player position after losing sight
    Investigating,
}

/// AI behavior: chase the player when spotted, wander otherwise
#[derive(Debug, Clone, Copy)]
pub struct ChaseAI {
    pub sight_radius: i32,
    pub state: AIState,
    pub last_known_pos: Option<(i32, i32)>,
}

impl ChaseAI {
    pub fn new(sight_radius: i32) -> Self {
        Self {
            sight_radius,
            state: AIState::Idle,
            last_known_pos: None,
        }
    }
}

/// Visual position for smooth interpolation (separate from logical grid Position)
#[derive(Debug, Clone, Copy)]
pub struct VisualPosition {
    pub x: f32,
    pub y: f32,
}

impl VisualPosition {
    pub fn from_position(pos: &Position) -> Self {
        Self { x: pos.x as f32, y: pos.y as f32 }
    }
}

impl Container {
    pub fn new(items: Vec<ItemType>) -> Self {
        Self { items, gold: 0, is_open: false }
    }

    pub fn with_gold(items: Vec<ItemType>, gold: u32) -> Self {
        Self { items, gold, is_open: false }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.gold == 0
    }
}

/// Door component - can be open or closed
#[derive(Debug, Clone, Copy)]
pub struct Door {
    pub is_open: bool,
}

impl Door {
    pub fn new() -> Self {
        Self { is_open: false }
    }
}

/// Marker component for entities that block vision when present
#[derive(Debug, Clone, Copy)]
pub struct BlocksVision;

/// Marker component for entities that block movement when present
#[derive(Debug, Clone, Copy)]
pub struct BlocksMovement;

/// Weapon data - pure data, damage calculation in systems
#[derive(Debug, Clone)]
pub struct Weapon {
    pub name: String,
    pub tile_id: u32,
    pub base_damage: i32,
    pub damage_bonus: i32,
}

impl Weapon {
    pub fn sword() -> Self {
        Self {
            name: "Sword".to_string(),
            tile_id: crate::tile::tile_ids::SWORD,
            base_damage: SWORD_BASE_DAMAGE,
            damage_bonus: SWORD_DAMAGE_BONUS,
        }
    }

    pub fn claws(base_damage: i32) -> Self {
        Self {
            name: "Claws".to_string(),
            tile_id: crate::tile::tile_ids::BONES, // No specific icon, use bones
            base_damage,
            damage_bonus: 0,
        }
    }
}

/// Equipped items for an entity
#[derive(Debug, Clone)]
pub struct Equipment {
    pub weapon: Option<Weapon>,
    pub ranged_weapon: Option<RangedWeapon>,
}

impl Equipment {
    pub fn new() -> Self {
        Self { weapon: None, ranged_weapon: None }
    }

    pub fn with_weapon(weapon: Weapon) -> Self {
        Self { weapon: Some(weapon), ranged_weapon: None }
    }

    pub fn with_weapons(weapon: Weapon, ranged: RangedWeapon) -> Self {
        Self { weapon: Some(weapon), ranged_weapon: Some(ranged) }
    }
}

/// Marker for entities that can be attacked
#[derive(Debug, Clone, Copy)]
pub struct Attackable;

/// Visual effect: lunge animation toward a target
#[derive(Debug, Clone, Copy)]
pub struct LungeAnimation {
    pub target_x: f32,
    pub target_y: f32,
    pub progress: f32,      // 0.0 to 1.0, then back to 0.0
    pub returning: bool,
}

impl LungeAnimation {
    pub fn new(target_x: f32, target_y: f32) -> Self {
        Self {
            target_x,
            target_y,
            progress: 0.0,
            returning: false,
        }
    }
}

/// Visual effect: flash when hit
#[derive(Debug, Clone, Copy)]
pub struct HitFlash {
    pub timer: f32,  // Seconds remaining
}

impl HitFlash {
    pub fn new() -> Self {
        Self { timer: HIT_FLASH_DURATION }
    }
}

/// Ranged weapon data
#[derive(Debug, Clone)]
pub struct RangedWeapon {
    pub name: String,
    pub tile_id: u32,
    pub base_damage: i32,
    pub arrow_speed: f32,  // Tiles per second
}

impl RangedWeapon {
    pub fn bow() -> Self {
        Self {
            name: "Bow".to_string(),
            tile_id: crate::tile::tile_ids::BOW,
            base_damage: BOW_BASE_DAMAGE,
            arrow_speed: ARROW_SPEED,
        }
    }
}

/// Projectile component - for arrows and other flying objects
#[derive(Debug, Clone)]
pub struct Projectile {
    /// The entity that fired this projectile
    pub source: Entity,
    /// Damage dealt on hit
    pub damage: i32,
    /// Remaining path: list of (x, y, time_to_reach) for each tile
    /// time_to_reach is relative to spawn_time
    pub path: Vec<(i32, i32, f32)>,
    /// Index into path - which tile we're heading toward
    pub path_index: usize,
    /// Direction for sprite rotation (normalized)
    pub direction: (f32, f32),
    /// Game time when the projectile was spawned
    pub spawn_time: f32,
    /// If Some, the projectile has finished its game-time journey and is
    /// waiting for visual catch-up. Contains the final position and game time when it finished.
    pub finished: Option<(i32, i32, f32)>,
}

/// Marker component for projectiles (for queries)
#[derive(Debug, Clone, Copy)]
pub struct ProjectileMarker;
