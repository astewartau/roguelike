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

/// Overlay sprite component - rendered on top of the main sprite
/// Used for displaying equipped weapons on enemies
#[derive(Debug, Clone, Copy)]
pub struct OverlaySprite {
    pub tile_id: u32,
}

impl OverlaySprite {
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
    // Potions
    HealthPotion,
    RegenerationPotion,
    StrengthPotion,
    ConfusionPotion, // Throwable
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
    /// Reduced incoming damage
    Protected,
    /// Random movement, ignores player (enemies only)
    Confused,
    /// Flees from player (enemies only)
    Feared,
    /// Reduced speed (enemies only)
    Slowed,
}

/// An active status effect with remaining duration
#[derive(Debug, Clone, Copy)]
pub struct ActiveEffect {
    pub effect_type: EffectType,
    /// Remaining duration in game-time seconds
    pub remaining_duration: f32,
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

    /// Check if entity has a specific effect active
    pub fn has_effect(&self, effect_type: EffectType) -> bool {
        self.effects.iter().any(|e| e.effect_type == effect_type)
    }

    /// Add or refresh an effect with the given duration
    pub fn add_effect(&mut self, effect_type: EffectType, duration: f32) {
        // Refresh duration if already have effect, otherwise add new
        if let Some(existing) = self.effects.iter_mut().find(|e| e.effect_type == effect_type) {
            existing.remaining_duration = duration;
        } else {
            self.effects.push(ActiveEffect {
                effect_type,
                remaining_duration: duration,
            });
        }
    }

    /// Get remaining duration of an effect (None if not active)
    pub fn get_duration(&self, effect_type: EffectType) -> Option<f32> {
        self.effects
            .iter()
            .find(|e| e.effect_type == effect_type)
            .map(|e| e.remaining_duration)
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
    ThrowPotion { target_x: i32, target_y: i32 },
    /// Teleporting to a target position (Blink)
    Blink { target_x: i32, target_y: i32 },
    /// Casting fireball at a target position
    CastFireball { target_x: i32, target_y: i32 },
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
            ActionType::Wait => 1,
            ActionType::ShootBow { .. } => 1,
            ActionType::UseStairs { .. } => 1,
            ActionType::TalkTo { .. } => 1,
            ActionType::ThrowPotion { .. } => 1,
            ActionType::Blink { .. } => 1,
            ActionType::CastFireball { .. } => 1,
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

    /// Returns true if this AI has ranged attack capability
    pub fn has_ranged(&self) -> bool {
        self.ranged_max > 0
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
    pub ranged: Option<RangedSlot>,
}

impl Equipment {
    pub fn new() -> Self {
        Self { weapon: None, ranged: None }
    }

    pub fn with_weapon(weapon: Weapon) -> Self {
        Self { weapon: Some(weapon), ranged: None }
    }

    pub fn with_weapons(weapon: Weapon, ranged: RangedWeapon) -> Self {
        Self { weapon: Some(weapon), ranged: Some(RangedSlot::Bow(ranged)) }
    }

    /// Check if a bow is equipped
    pub fn has_bow(&self) -> bool {
        matches!(self.ranged, Some(RangedSlot::Bow(_)))
    }

    /// Check if a throwable is equipped
    pub fn has_throwable(&self) -> bool {
        matches!(self.ranged, Some(RangedSlot::Throwable { .. }))
    }

    /// Get the equipped bow, if any
    pub fn get_bow(&self) -> Option<&RangedWeapon> {
        match &self.ranged {
            Some(RangedSlot::Bow(bow)) => Some(bow),
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

    /// Create a bow for enemies with custom damage
    pub fn enemy_bow(damage: i32) -> Self {
        Self {
            name: "Bow".to_string(),
            tile_id: crate::tile::tile_ids::BOW,
            base_damage: damage,
            arrow_speed: ARROW_SPEED,
        }
    }
}

/// What's equipped in the ranged slot - either a bow or a throwable potion
#[derive(Debug, Clone)]
pub enum RangedSlot {
    /// A bow that shoots arrows
    Bow(RangedWeapon),
    /// A throwable potion that can be thrown at enemies
    Throwable {
        item_type: ItemType,
        tile_id: u32,
    },
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

// =============================================================================
// NPC / DIALOGUE COMPONENTS
// =============================================================================

/// Marker for friendly NPCs (not attackable, triggers dialogue on bump)
#[derive(Debug, Clone, Copy)]
pub struct FriendlyNPC;

/// A dialogue option the player can choose
#[derive(Debug, Clone)]
pub struct DialogueOption {
    /// Button text shown to player
    pub label: String,
    /// Index of next dialogue node (None = end dialogue)
    pub next_node: Option<usize>,
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

    /// Get the current dialogue node
    pub fn current(&self) -> Option<&DialogueNode> {
        self.nodes.get(self.current_node)
    }

    /// Advance to the next node based on option selection
    /// Returns true if dialogue continues, false if it ended
    pub fn select_option(&mut self, option_index: usize) -> bool {
        if let Some(node) = self.nodes.get(self.current_node) {
            if let Some(option) = node.options.get(option_index) {
                if let Some(next) = option.next_node {
                    self.current_node = next;
                    return true;
                }
            }
        }
        false
    }

    /// Reset dialogue to start
    pub fn reset(&mut self) {
        self.current_node = 0;
    }
}
