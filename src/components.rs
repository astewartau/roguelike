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

/// Item type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemType {
    HealthPotion,
}

impl ItemType {
    pub fn name(&self) -> &str {
        match self {
            ItemType::HealthPotion => "Health Potion",
        }
    }

    pub fn weight_kg(&self) -> f32 {
        match self {
            ItemType::HealthPotion => 0.5,
        }
    }

    pub fn heal_amount(&self) -> i32 {
        match self {
            ItemType::HealthPotion => 50,
        }
    }
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
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            current_weight_kg: 0.0,
        }
    }
}

/// Container component (for chests)
#[derive(Debug, Clone)]
pub struct Container {
    pub items: Vec<ItemType>,
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

/// AI behavior: wander randomly
#[derive(Debug, Clone, Copy)]
pub struct RandomWanderAI;

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
        Self { items, is_open: false }
    }
}
