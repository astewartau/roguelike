use crate::constants::*;
use crate::tile::{tile_ids, SpriteSheet};
use hecs::Entity;

// =============================================================================
// PLAYER CLASS
// =============================================================================

/// Player class selection - determines starting stats, equipment, and appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerClass {
    Fighter,
    Ranger,
    Druid,
}

impl PlayerClass {
    /// All available player classes
    pub const ALL: [PlayerClass; 3] = [PlayerClass::Fighter, PlayerClass::Ranger, PlayerClass::Druid];

    /// Display name for the class
    pub fn name(&self) -> &'static str {
        match self {
            PlayerClass::Fighter => "Fighter",
            PlayerClass::Ranger => "Ranger",
            PlayerClass::Druid => "Druid",
        }
    }

    /// Sprite reference for this class
    pub fn sprite(&self) -> (SpriteSheet, u32) {
        match self {
            PlayerClass::Fighter => tile_ids::FIGHTER,
            PlayerClass::Ranger => tile_ids::RANGER,
            PlayerClass::Druid => tile_ids::ELF,
        }
    }

    /// Starting stats: (strength, intelligence, agility)
    pub fn stats(&self) -> (i32, i32, i32) {
        match self {
            PlayerClass::Fighter => (16, 10, 12),
            PlayerClass::Ranger => (12, 10, 16),
            PlayerClass::Druid => (10, 16, 14),
        }
    }

    /// Starting equipped weapon
    pub fn starting_weapon(&self) -> EquippedWeapon {
        match self {
            PlayerClass::Fighter => EquippedWeapon::Melee(Weapon::sword()),
            PlayerClass::Ranger => EquippedWeapon::Ranged(RangedWeapon::bow()),
            PlayerClass::Druid => EquippedWeapon::Melee(Weapon::staff()),
        }
    }

    /// Starting inventory items
    pub fn starting_inventory(&self) -> Vec<ItemType> {
        match self {
            PlayerClass::Fighter => vec![],
            PlayerClass::Ranger => vec![ItemType::Dagger],
            PlayerClass::Druid => vec![ItemType::RegenerationPotion],
        }
    }

    /// Class innate ability
    pub fn ability(&self) -> AbilityType {
        match self {
            PlayerClass::Fighter => AbilityType::Cleave,
            PlayerClass::Ranger => AbilityType::Sprint,
            PlayerClass::Druid => AbilityType::Tame,
        }
    }

    /// Cooldown duration for class ability
    pub fn ability_cooldown(&self) -> f32 {
        match self {
            PlayerClass::Fighter => CLEAVE_COOLDOWN,
            PlayerClass::Ranger => SPRINT_COOLDOWN,
            PlayerClass::Druid => TAME_COOLDOWN,
        }
    }
}

// =============================================================================
// CLASS ABILITIES
// =============================================================================

/// Types of class abilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityType {
    /// Fighter: Attack all adjacent enemies
    Cleave,
    /// Ranger: Temporary speed boost
    Sprint,
    /// Druid: Tame a nearby animal
    Tame,
    /// Druid: Protective bark armor (50% damage reduction)
    Barkskin,
}

impl AbilityType {
    /// Display name for the ability
    pub fn name(&self) -> &'static str {
        match self {
            AbilityType::Cleave => "Cleave",
            AbilityType::Sprint => "Sprint",
            AbilityType::Tame => "Tame Animal",
            AbilityType::Barkskin => "Barkskin",
        }
    }

    /// Description of what the ability does
    pub fn description(&self) -> &'static str {
        match self {
            AbilityType::Cleave => "Attack all adjacent enemies",
            AbilityType::Sprint => "Double movement speed for 10 seconds",
            AbilityType::Tame => "Channel to tame a nearby animal",
            AbilityType::Barkskin => "Reduce damage by 50% for 15 seconds",
        }
    }

    /// Energy cost to use this ability
    pub fn energy_cost(&self) -> i32 {
        match self {
            AbilityType::Cleave => CLEAVE_ENERGY_COST,
            AbilityType::Sprint => SPRINT_ENERGY_COST,
            AbilityType::Tame => TAME_ENERGY_COST,
            AbilityType::Barkskin => BARKSKIN_ENERGY_COST,
        }
    }
}

/// Tracks the player's class ability and its cooldown state
#[derive(Debug, Clone)]
pub struct ClassAbility {
    pub ability_type: AbilityType,
    /// Seconds remaining on cooldown (0 = ready)
    pub cooldown_remaining: f32,
    /// Total cooldown duration
    pub cooldown_total: f32,
}

impl ClassAbility {
    pub fn new(ability_type: AbilityType, cooldown_total: f32) -> Self {
        Self {
            ability_type,
            cooldown_remaining: 0.0,
            cooldown_total,
        }
    }

    /// Start the cooldown timer
    pub fn start_cooldown(&mut self) {
        self.cooldown_remaining = self.cooldown_total;
    }

    /// Check if the ability is ready to use
    pub fn is_ready(&self) -> bool {
        self.cooldown_remaining <= 0.0
    }
}

/// Optional secondary class ability (currently only Druid has this)
#[derive(Debug, Clone)]
pub struct SecondaryAbility {
    pub ability_type: AbilityType,
    /// Seconds remaining on cooldown (0 = ready)
    pub cooldown_remaining: f32,
    /// Total cooldown duration
    pub cooldown_total: f32,
}

impl SecondaryAbility {
    pub fn new(ability_type: AbilityType, cooldown_total: f32) -> Self {
        Self {
            ability_type,
            cooldown_remaining: 0.0,
            cooldown_total,
        }
    }

    /// Start the cooldown timer
    pub fn start_cooldown(&mut self) {
        self.cooldown_remaining = self.cooldown_total;
    }

    /// Check if the ability is ready to use
    pub fn is_ready(&self) -> bool {
        self.cooldown_remaining <= 0.0
    }
}

// =============================================================================
// COMPONENTS
// =============================================================================

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
    pub sheet: SpriteSheet,
    pub tile_id: u32,
}

impl Sprite {
    pub fn new(sheet: SpriteSheet, tile_id: u32) -> Self {
        Self { sheet, tile_id }
    }

    /// Create from a (SpriteSheet, u32) tuple (common format in tile_ids)
    pub fn from_ref(sprite_ref: (SpriteSheet, u32)) -> Self {
        Self {
            sheet: sprite_ref.0,
            tile_id: sprite_ref.1,
        }
    }
}

/// Overlay sprite component - rendered on top of the main sprite
/// Used for displaying equipped weapons on enemies
#[derive(Debug, Clone, Copy)]
pub struct OverlaySprite {
    pub sheet: SpriteSheet,
    pub tile_id: u32,
}

impl OverlaySprite {
    pub fn new(sheet: SpriteSheet, tile_id: u32) -> Self {
        Self { sheet, tile_id }
    }

    /// Create from a (SpriteSheet, u32) tuple
    pub fn from_ref(sprite_ref: (SpriteSheet, u32)) -> Self {
        Self {
            sheet: sprite_ref.0,
            tile_id: sprite_ref.1,
        }
    }
}

/// Animated sprite component - cycles through frames in real-time
#[derive(Debug, Clone, Copy)]
pub struct AnimatedSprite {
    pub sheet: SpriteSheet,
    /// First frame's tile ID
    pub base_tile_id: u32,
    /// Number of frames in the animation
    pub frame_count: u32,
    /// Duration of each frame in seconds (real-time)
    pub frame_duration: f32,
    /// Random phase offset (0.0 to 1.0) to desync animations
    pub phase_offset: f32,
    /// Render order (lower = rendered first/below, higher = rendered last/above)
    pub z_order: u8,
}

impl AnimatedSprite {
    pub fn new(sheet: SpriteSheet, base_tile_id: u32, frame_count: u32, frame_duration: f32) -> Self {
        Self {
            sheet,
            base_tile_id,
            frame_count,
            frame_duration,
            phase_offset: 0.0,
            z_order: 1,
        }
    }

    /// Create with a random phase offset
    pub fn with_random_phase(mut self) -> Self {
        self.phase_offset = rand::random();
        self
    }

    /// Get the current tile ID based on real time
    pub fn current_tile_id(&self, real_time: f32) -> u32 {
        let total_duration = self.frame_duration * self.frame_count as f32;
        // Add phase offset to desync animations
        let offset_time = real_time + self.phase_offset * total_duration;
        let time_in_cycle = offset_time % total_duration;
        let frame = (time_in_cycle / self.frame_duration) as u32;
        self.base_tile_id + frame.min(self.frame_count - 1)
    }

    /// Create a fire pit animation with random phase
    pub fn fire_pit() -> Self {
        use crate::tile::tile_ids;
        Self {
            sheet: SpriteSheet::AnimatedTiles,
            base_tile_id: tile_ids::FIRE_PIT.1,
            frame_count: 6,
            frame_duration: 0.15, // ~6.7 FPS, full cycle in 0.9 seconds
            phase_offset: rand::random(),
            z_order: 1,
        }
    }

    /// Create a brazier animation with random phase
    pub fn brazier() -> Self {
        use crate::tile::tile_ids;
        Self {
            sheet: SpriteSheet::AnimatedTiles,
            base_tile_id: tile_ids::BRAZIER.1,
            frame_count: 6,
            frame_duration: 0.12, // Slightly faster than fire pit
            phase_offset: rand::random(),
            z_order: 1,
        }
    }

    /// Create an animated water tile with random phase
    pub fn water() -> Self {
        use crate::tile::tile_ids;
        Self {
            sheet: SpriteSheet::AnimatedTiles,
            base_tile_id: tile_ids::WATER_ANIMATED.1,
            frame_count: 11,
            frame_duration: 0.2, // Gentle wave animation
            phase_offset: rand::random(),
            z_order: 0, // Water renders below other animated sprites
        }
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
    // Weapons
    Sword,
    Bow,
    Dagger,
    Staff,
    // Potions
    HealthPotion,
    RegenerationPotion,
    StrengthPotion,
    ConfusionPotion,
    // Scrolls
    ScrollOfInvisibility,
    ScrollOfSpeed,
    ScrollOfProtection,
    ScrollOfBlink,
    ScrollOfFear,
    ScrollOfFireball,
    ScrollOfReveal,
    ScrollOfMapping,
    ScrollOfSlow,
    // Food
    Cheese,
    Bread,
    Apple,
    // Traps
    FireTrap,
}

// =============================================================================
// STATUS EFFECTS
// =============================================================================

/// Types of status effects that can be applied to entities
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectType {
    /// Entity cannot be seen by enemies
    Invisible,
    /// Entity moves and acts faster (multiplier applied to speed)
    SpeedBoost,
    /// Boosted HP regeneration
    Regenerating,
    /// Increased damage output
    Strengthened,
    /// Reduced incoming damage (from Protection scroll)
    Protected,
    /// Reduced incoming damage (from Barkskin ability - nature themed VFX)
    Barkskin,
    /// Random movement, ignores player (enemies only)
    Confused,
    /// Flees from player (enemies only)
    Feared,
    /// Reduced speed (enemies only)
    Slowed,
    /// On fire - takes damage over time
    Burning,
}

/// An active status effect with remaining duration
#[derive(Debug, Clone, Copy)]
pub struct ActiveEffect {
    pub effect_type: EffectType,
    /// Remaining duration in game-time seconds
    pub remaining_duration: f32,
    /// Last time damage was dealt (for DoT effects like Burning)
    pub last_damage_tick: f32,
}

/// Component for entities with active status effects
#[derive(Debug, Clone, Default)]
pub struct StatusEffects {
    pub effects: Vec<ActiveEffect>,
}

impl StatusEffects {
    pub fn new() -> Self {
        Self { effects: Vec::new() }
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

/// Type of container (affects sprite and behavior)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerType {
    Chest,
    Coffin,
    Barrel,
}

/// Container component (for chests, coffins, barrels)
#[derive(Debug, Clone)]
pub struct Container {
    pub container_type: ContainerType,
    pub items: Vec<ItemType>,
    pub gold: u32,
    pub is_open: bool,
    /// Chance to spawn enemy when opened (for coffins, 0.0-1.0)
    pub spawn_chance: f32,
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
    /// Attacking in a direction (hits whatever is there at completion, or whiffs)
    AttackDirection { dx: i32, dy: i32 },
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
    /// Talking to a friendly NPC
    TalkTo { npc: Entity },
    /// Throwing a potion at a target position
    ThrowPotion { potion_type: ItemType, target_x: i32, target_y: i32 },
    /// Teleporting to a target position (Blink)
    Blink { target_x: i32, target_y: i32 },
    /// Casting fireball at a target position
    CastFireball { target_x: i32, target_y: i32 },
    /// Equip a weapon from inventory
    #[allow(dead_code)] // Constructed via action system
    EquipWeapon { item_index: usize },
    /// Unequip current weapon to inventory
    #[allow(dead_code)] // Constructed via action system
    UnequipWeapon,
    /// Drop an item from inventory onto the ground
    #[allow(dead_code)] // Constructed via action system
    DropItem { item_index: usize },
    /// Drop currently equipped weapon onto the ground
    #[allow(dead_code)] // Constructed via action system
    DropEquippedWeapon,
    /// Fighter ability: attack all adjacent enemies
    Cleave,
    /// Ranger ability: activate sprint (speed boost)
    ActivateSprint,
    /// Druid ability: start taming an animal
    StartTaming { target: Entity },
    /// Druid ability: activate barkskin (damage reduction)
    ActivateBarkskin,
    /// Place a fire trap at target location
    PlaceFireTrap { target_x: i32, target_y: i32 },
}

impl ActionType {
    /// Energy cost to start this action
    pub fn energy_cost(&self) -> i32 {
        match self {
            // All basic actions cost 1 energy by default
            // This can be customized per-action as needed
            ActionType::Move { .. } => 1,
            ActionType::Attack { .. } => 1,
            ActionType::AttackDirection { .. } => 1,
            ActionType::OpenDoor { .. } => 1,
            ActionType::OpenChest { .. } => 1,
            ActionType::Wait => 0, // Standing still is free
            ActionType::ShootBow { .. } => 1,
            ActionType::UseStairs { .. } => 1,
            ActionType::TalkTo { .. } => 1,
            ActionType::ThrowPotion { .. } => 1,
            ActionType::Blink { .. } => 1,
            ActionType::CastFireball { .. } => 1,
            ActionType::EquipWeapon { .. } => 0, // Free action
            ActionType::UnequipWeapon => 0,      // Free action
            ActionType::DropItem { .. } => 1,
            ActionType::DropEquippedWeapon => 1,
            ActionType::Cleave => CLEAVE_ENERGY_COST,
            ActionType::ActivateSprint => SPRINT_ENERGY_COST,
            ActionType::StartTaming { .. } => TAME_ENERGY_COST,
            ActionType::ActivateBarkskin => BARKSKIN_ENERGY_COST,
            ActionType::PlaceFireTrap { .. } => 1,
        }
    }
}

/// An action currently being executed by an entity
#[derive(Debug, Clone, Copy)]
pub struct ActionInProgress {
    pub action_type: ActionType,
    #[allow(dead_code)] // Reserved for animation timing
    pub start_time: f32,
    #[allow(dead_code)] // Reserved for animation timing
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

    /// Can start a new action (has energy and not mid-action)
    pub fn can_act(&self) -> bool {
        self.energy > 0 && self.current_action.is_none()
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
    /// Minimum range to use ranged attack (0 = melee only)
    pub ranged_min: i32,
    /// Maximum range for ranged attack (0 = melee only)
    pub ranged_max: i32,
}

impl ChaseAI {
    pub fn new(sight_radius: i32) -> Self {
        Self {
            sight_radius,
            state: AIState::Idle,
            last_known_pos: None,
            ranged_min: 0,
            ranged_max: 0,
        }
    }

    /// Create a ChaseAI with ranged attack capability
    pub fn with_ranged(sight_radius: i32, ranged_min: i32, ranged_max: i32) -> Self {
        Self {
            sight_radius,
            state: AIState::Idle,
            last_known_pos: None,
            ranged_min,
            ranged_max,
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
    /// Create a chest container (default)
    pub fn new(items: Vec<ItemType>) -> Self {
        Self {
            container_type: ContainerType::Chest,
            items,
            gold: 0,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    /// Create a chest with gold
    pub fn with_gold(items: Vec<ItemType>, gold: u32) -> Self {
        Self {
            container_type: ContainerType::Chest,
            items,
            gold,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    /// Create a coffin (may spawn enemy when opened)
    pub fn coffin(items: Vec<ItemType>, gold: u32, spawn_chance: f32) -> Self {
        Self {
            container_type: ContainerType::Coffin,
            items,
            gold,
            is_open: false,
            spawn_chance,
        }
    }

    /// Create a barrel (contains food)
    pub fn barrel(items: Vec<ItemType>) -> Self {
        Self {
            container_type: ContainerType::Barrel,
            items,
            gold: 0,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.gold == 0
    }
}

/// Door component - can be open or closed
#[derive(Debug, Clone, Copy)]
pub struct Door {
    pub is_open: bool,
    /// Sprite to use when door is open
    pub open_sprite: (crate::tile::SpriteSheet, u32),
}

impl Door {
    /// Standard dungeon door
    pub fn new() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_OPEN,
        }
    }

    /// Green door for overgrown rooms
    pub fn green() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_GREEN_OPEN,
        }
    }

    /// Grated door for crypt rooms
    pub fn grated() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_GRATED, // Same sprite open/closed
        }
    }

    /// Shop door
    pub fn shop() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_SHOP_OPEN,
        }
    }
}

/// Marker component for entities that block vision when present
#[derive(Debug, Clone, Copy)]
pub struct BlocksVision;

/// Marker component for entities that block movement when present
#[derive(Debug, Clone, Copy)]
pub struct BlocksMovement;

/// Marker component for ground item piles dropped by entities
#[derive(Debug, Clone, Copy)]
pub struct GroundItemPile;

/// Weapon data - pure data, damage calculation in systems
#[derive(Debug, Clone)]
pub struct Weapon {
    #[allow(dead_code)] // Reserved for UI display
    pub name: String,
    #[allow(dead_code)] // Reserved for inventory icons
    pub sprite: (SpriteSheet, u32),
    pub base_damage: i32,
    pub damage_bonus: i32,
}

impl Weapon {
    pub fn sword() -> Self {
        Self {
            name: "Sword".to_string(),
            sprite: crate::tile::tile_ids::SWORD,
            base_damage: SWORD_BASE_DAMAGE,
            damage_bonus: SWORD_DAMAGE_BONUS,
        }
    }

    pub fn dagger() -> Self {
        Self {
            name: "Dagger".to_string(),
            sprite: crate::tile::tile_ids::DAGGER,
            base_damage: DAGGER_BASE_DAMAGE,
            damage_bonus: DAGGER_DAMAGE_BONUS,
        }
    }

    pub fn claws(base_damage: i32) -> Self {
        Self {
            name: "Claws".to_string(),
            sprite: crate::tile::tile_ids::BONES, // No specific icon, use bones
            base_damage,
            damage_bonus: 0,
        }
    }

    pub fn staff() -> Self {
        Self {
            name: "Staff".to_string(),
            sprite: crate::tile::tile_ids::STAFF,
            base_damage: STAFF_BASE_DAMAGE,
            damage_bonus: STAFF_DAMAGE_BONUS,
        }
    }
}

/// What type of weapon is equipped
#[derive(Debug, Clone)]
pub enum EquippedWeapon {
    /// A melee weapon (sword, claws, etc.)
    Melee(Weapon),
    /// A ranged weapon (bow)
    Ranged(RangedWeapon),
}

/// Equipped items for an entity
#[derive(Debug, Clone)]
pub struct Equipment {
    /// Single weapon slot - can be melee or ranged (used by player)
    pub weapon: Option<EquippedWeapon>,
    /// Additional ranged weapon (used by enemies who have both melee and ranged)
    pub enemy_ranged: Option<RangedWeapon>,
}

impl Equipment {
    pub fn with_melee(weapon: Weapon) -> Self {
        Self { weapon: Some(EquippedWeapon::Melee(weapon)), enemy_ranged: None }
    }

    pub fn with_ranged(ranged: RangedWeapon) -> Self {
        Self { weapon: Some(EquippedWeapon::Ranged(ranged)), enemy_ranged: None }
    }

    /// Create equipment with an already-constructed EquippedWeapon
    pub fn with_equipped(weapon: EquippedWeapon) -> Self {
        Self { weapon: Some(weapon), enemy_ranged: None }
    }

    /// Create equipment for enemies that can use both melee (claws) and ranged (bow)
    pub fn with_weapons(melee: Weapon, ranged: RangedWeapon) -> Self {
        Self {
            weapon: Some(EquippedWeapon::Melee(melee)),
            enemy_ranged: Some(ranged),
        }
    }

    /// Create equipment with just a melee weapon (for melee-only enemies)
    pub fn with_weapon(weapon: Weapon) -> Self {
        Self { weapon: Some(EquippedWeapon::Melee(weapon)), enemy_ranged: None }
    }

    /// Check if a bow is equipped (either in main slot or enemy_ranged)
    pub fn has_bow(&self) -> bool {
        matches!(self.weapon, Some(EquippedWeapon::Ranged(_))) || self.enemy_ranged.is_some()
    }

    /// Get the equipped bow, if any (checks both main slot and enemy_ranged)
    pub fn get_bow(&self) -> Option<&RangedWeapon> {
        match &self.weapon {
            Some(EquippedWeapon::Ranged(bow)) => Some(bow),
            _ => self.enemy_ranged.as_ref(),
        }
    }

    /// Get the equipped melee weapon, if any
    pub fn get_melee(&self) -> Option<&Weapon> {
        match &self.weapon {
            Some(EquippedWeapon::Melee(weapon)) => Some(weapon),
            _ => None,
        }
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


/// Ranged weapon data
#[derive(Debug, Clone)]
pub struct RangedWeapon {
    #[allow(dead_code)] // Reserved for UI display
    pub name: String,
    #[allow(dead_code)] // Reserved for inventory icons
    pub sprite: (SpriteSheet, u32),
    pub base_damage: i32,
    pub arrow_speed: f32,  // Tiles per second
}

impl RangedWeapon {
    pub fn bow() -> Self {
        Self {
            name: "Bow".to_string(),
            sprite: crate::tile::tile_ids::BOW,
            base_damage: BOW_BASE_DAMAGE,
            arrow_speed: ARROW_SPEED,
        }
    }

    /// Create a bow for enemies with custom damage
    pub fn enemy_bow(damage: i32) -> Self {
        Self {
            name: "Bow".to_string(),
            sprite: crate::tile::tile_ids::BOW,
            base_damage: damage,
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
    #[allow(dead_code)] // Reserved for arrow rotation rendering
    pub direction: (f32, f32),
    /// Game time when the projectile was spawned
    pub spawn_time: f32,
    /// If Some, the projectile has finished its game-time journey and is
    /// waiting for visual catch-up. Contains the final position and game time when it finished.
    pub finished: Option<(i32, i32, f32)>,
    /// If Some, this is a thrown potion that should splash on impact
    pub potion_type: Option<ItemType>,
}

/// Marker component for projectiles (for queries)
#[derive(Debug, Clone, Copy)]
pub struct ProjectileMarker;

// =============================================================================
// NPC / DIALOGUE COMPONENTS
// =============================================================================

/// Marker for friendly NPCs (not attackable, triggers dialogue on bump)
#[derive(Debug, Clone, Copy)]
pub struct FriendlyNPC;

/// Actions that can be triggered by dialogue options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogueAction {
    /// No special action
    #[default]
    None,
    /// Open the shop UI (for vendor NPCs)
    OpenShop,
}

/// A dialogue option the player can choose
#[derive(Debug, Clone)]
pub struct DialogueOption {
    /// Button text shown to player
    pub label: String,
    /// Index of next dialogue node (None = end dialogue)
    pub next_node: Option<usize>,
    /// Special action to trigger when this option is selected
    pub action: DialogueAction,
}

/// A single node in a dialogue tree
#[derive(Debug, Clone)]
pub struct DialogueNode {
    /// What the NPC says
    pub text: String,
    /// Player response choices
    pub options: Vec<DialogueOption>,
}

/// Dialogue tree stored on NPCs
#[derive(Debug, Clone)]
pub struct Dialogue {
    /// NPC name for dialogue window title
    pub name: String,
    /// All dialogue nodes
    pub nodes: Vec<DialogueNode>,
    /// Current position in dialogue (for active conversations)
    pub current_node: usize,
}

impl Dialogue {
    pub fn new(name: impl Into<String>, nodes: Vec<DialogueNode>) -> Self {
        Self {
            name: name.into(),
            nodes,
            current_node: 0,
        }
    }
}

// =============================================================================
// VENDOR SYSTEM
// =============================================================================

/// Vendor component - NPCs that can buy/sell items
#[derive(Debug, Clone)]
pub struct Vendor {
    /// Items for sale: (item type, stock count)
    pub inventory: Vec<(ItemType, u32)>,
    /// Vendor's gold (used for buying from player)
    pub gold: u32,
}

impl Vendor {
    pub fn new(inventory: Vec<(ItemType, u32)>, gold: u32) -> Self {
        Self { inventory, gold }
    }
}

// =============================================================================
// LIGHT SOURCE
// =============================================================================

/// Light source component - emits light in a radius
#[derive(Debug, Clone, Copy)]
pub struct LightSource {
    /// Radius of light emission (in tiles)
    pub radius: f32,
    /// Light intensity (0.0 to 1.0, multiplied with falloff)
    pub intensity: f32,
}

impl LightSource {
    /// Create a standard campfire light
    pub fn campfire() -> Self {
        Self {
            radius: 8.0,
            intensity: 1.0,
        }
    }

    /// Create a brazier light (smaller than campfire)
    pub fn brazier() -> Self {
        Self {
            radius: 6.0,
            intensity: 0.95,
        }
    }
}

/// Marker component for entities that cause burning when stepped on
#[derive(Debug, Clone, Copy)]
pub struct CausesBurning;

/// Fire trap component - causes burning when stepped on (but not by owner or their pets)
#[derive(Debug, Clone, Copy)]
pub struct PlacedFireTrap {
    /// The entity that placed this trap
    pub owner: Entity,
    /// Initial burst damage when triggered
    pub burst_damage: i32,
}

// =============================================================================
// TAMING SYSTEM
// =============================================================================

/// Marker for animals that can be tamed by the Druid
#[derive(Debug, Clone, Copy)]
pub struct Tameable;

/// Tracks active taming progress for a player
#[derive(Debug, Clone, Copy)]
pub struct TamingInProgress {
    /// The entity being tamed
    pub target: Entity,
    /// Current taming progress in seconds
    pub progress: f32,
    /// Required time to complete taming
    pub required: f32,
}

/// Marks an animal as tamed
#[derive(Debug, Clone, Copy)]
pub struct TamedBy {
    /// The player who tamed this animal
    pub owner: Entity,
}

/// AI for tamed companions - follows owner and attacks enemies
#[derive(Debug, Clone, Copy)]
pub struct CompanionAI {
    /// The player this companion follows
    pub owner: Entity,
    /// Maximum distance before following (Manhattan distance)
    pub follow_distance: i32,
    /// Entity that last attacked this companion (for retaliation)
    pub last_attacker: Option<Entity>,
}

/// Tracks the player's last attack target (for companion assistance)
#[derive(Debug, Clone, Copy)]
pub struct PlayerAttackTarget {
    pub target: Option<Entity>,
}

// =============================================================================
// RANGED COOLDOWN
// =============================================================================

/// Cooldown tracker for ranged attacks (used by skeleton archers)
#[derive(Debug, Clone, Copy)]
pub struct RangedCooldown {
    /// Remaining cooldown time in seconds
    pub remaining: f32,
}
