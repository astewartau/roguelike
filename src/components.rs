use glam::Vec3;

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

/// Sprite component - visual representation
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub color: Vec3,
}

impl Sprite {
    pub fn new(color: Vec3) -> Self {
        Self { color }
    }
}

/// Player marker component
#[derive(Debug, Clone, Copy)]
pub struct Player;

/// Health component
#[derive(Debug, Clone, Copy)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self {
            current: max,
            max,
        }
    }

    pub fn percentage(&self) -> f32 {
        (self.current as f32 / self.max as f32).clamp(0.0, 1.0)
    }

    pub fn heal(&mut self, amount: i32) {
        self.current = (self.current + amount).min(self.max);
    }
}

/// Stats component - character attributes
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub strength: i32,
    pub intelligence: i32,
    pub agility: i32,
}

impl Stats {
    pub fn new(strength: i32, intelligence: i32, agility: i32) -> Self {
        Self {
            strength,
            intelligence,
            agility,
        }
    }

    /// Calculate carry capacity in kg based on strength
    pub fn carry_capacity_kg(&self) -> f32 {
        (self.strength as f32) * 2.0 // 2kg per strength point
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

/// Inventory component
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

    pub fn add_item(&mut self, item_type: ItemType) {
        self.current_weight_kg += item_type.weight_kg();
        self.items.push(item_type);
    }

    pub fn remove_item(&mut self, index: usize) -> Option<ItemType> {
        if index < self.items.len() {
            let item = self.items.remove(index);
            self.current_weight_kg -= item.weight_kg();
            Some(item)
        } else {
            None
        }
    }

    pub fn weight_percentage(&self, max_weight: f32) -> f32 {
        (self.current_weight_kg / max_weight).clamp(0.0, 1.0)
    }
}

/// Container component (for chests)
#[derive(Debug, Clone)]
pub struct Container {
    pub items: Vec<ItemType>,
    pub is_open: bool,
}

impl Container {
    pub fn new(items: Vec<ItemType>) -> Self {
        Self {
            items,
            is_open: false,
        }
    }

    pub fn take_all(&mut self) -> Vec<ItemType> {
        let items = self.items.clone();
        self.items.clear();
        items
    }
}
