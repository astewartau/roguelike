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
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self { current: max, max }
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

/// Actor component - for entities that take turns
/// Pure data: energy accumulates, speed is cost to act
#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub energy: i32,
    pub speed: i32, // Lower = faster. Cost to take an action.
}

impl Actor {
    pub fn new(speed: i32) -> Self {
        Self { energy: 0, speed }
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
            tile_id: 65,  // sword tile
            base_damage: 8,
            damage_bonus: 2,
        }
    }
}

/// Equipped items for an entity
#[derive(Debug, Clone)]
pub struct Equipment {
    pub weapon: Option<Weapon>,
}

impl Equipment {
    pub fn new() -> Self {
        Self { weapon: None }
    }

    pub fn with_weapon(weapon: Weapon) -> Self {
        Self { weapon: Some(weapon) }
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
        Self { timer: 0.15 }  // 150ms flash
    }
}
