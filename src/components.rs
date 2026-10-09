use crate::constants::*;
use crate::tile::{tile_ids, SpriteSheet};
use hecs::Entity;

// =============================================================================
// IDENTITY
// =============================================================================

/// Human-readable display name for an entity.
///
/// Used by the message log (and, in future, tooltips and a bestiary) to refer
/// to entities by name instead of by their opaque ECS id.
#[derive(Debug, Clone)]
pub struct Name(pub String);

impl Name {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

// =============================================================================
// PLAYER CLASS
// =============================================================================

/// Player class selection - determines starting stats, equipment, and appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerClass {
    Fighter,
    Ranger,
    Druid,
    Necromancer,
}

impl PlayerClass {
    /// All available player classes
    pub const ALL: [PlayerClass; 4] = [PlayerClass::Fighter, PlayerClass::Ranger, PlayerClass::Druid, PlayerClass::Necromancer];

    /// Display name for the class
    pub fn name(&self) -> &'static str {
        match self {
            PlayerClass::Fighter => "Fighter",
            PlayerClass::Ranger => "Ranger",
            PlayerClass::Druid => "Druid",
            PlayerClass::Necromancer => "Necromancer",
        }
    }

    /// Sprite reference for this class
    pub fn sprite(&self) -> (SpriteSheet, u32) {
        match self {
            PlayerClass::Fighter => tile_ids::FIGHTER,
            PlayerClass::Ranger => tile_ids::RANGER,
            PlayerClass::Druid => tile_ids::ELF,
            PlayerClass::Necromancer => tile_ids::MONK,
        }
    }

    /// Starting stats: (strength, intelligence, agility)
    pub fn stats(&self) -> (i32, i32, i32) {
        match self {
            PlayerClass::Fighter => (16, 10, 12),
            PlayerClass::Ranger => (12, 10, 16),
            PlayerClass::Druid => (10, 16, 14),
            PlayerClass::Necromancer => (10, 18, 10),
        }
    }

    /// Starting equipped weapon
    pub fn starting_weapon(&self) -> EquippedWeapon {
        match self {
            PlayerClass::Fighter => EquippedWeapon::Melee(Weapon::sword()),
            PlayerClass::Ranger => EquippedWeapon::Ranged(RangedWeapon::bow()),
            PlayerClass::Druid => EquippedWeapon::Melee(Weapon::staff()),
            PlayerClass::Necromancer => EquippedWeapon::Melee(Weapon::staff()),
        }
    }

    /// Starting inventory items
    pub fn starting_inventory(&self) -> Vec<ItemInstance> {
        let kinds: Vec<ItemType> = match self {
            PlayerClass::Fighter => vec![],
            // Ranger gets dagger and starting arrows
            PlayerClass::Ranger => {
                let mut items = vec![ItemType::Dagger];
                // Add STARTING_ARROW_COUNT arrows
                for _ in 0..crate::constants::STARTING_ARROW_COUNT {
                    items.push(ItemType::Arrow);
                }
                items
            },
            PlayerClass::Druid => vec![ItemType::RegenerationPotion],
            PlayerClass::Necromancer => vec![ItemType::HealthPotion],
        };
        kinds.into_iter().map(ItemInstance::plain).collect()
    }

    /// Class innate ability
    pub fn ability(&self) -> AbilityType {
        match self {
            PlayerClass::Fighter => AbilityType::Cleave,
            PlayerClass::Ranger => AbilityType::Sprint,
            PlayerClass::Druid => AbilityType::Tame,
            PlayerClass::Necromancer => AbilityType::LifeDrain,
        }
    }

    /// Cooldown duration for class ability
    pub fn ability_cooldown(&self) -> f32 {
        match self {
            PlayerClass::Fighter => CLEAVE_COOLDOWN,
            PlayerClass::Ranger => SPRINT_COOLDOWN,
            PlayerClass::Druid => TAME_COOLDOWN,
            PlayerClass::Necromancer => LIFE_DRAIN_COOLDOWN,
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
    /// Ranger: Temporary speed boost (legacy, being phased out)
    Sprint,
    /// Druid: Tame a nearby animal
    Tame,
    /// Druid: Protective bark armor (50% damage reduction)
    Barkskin,
    /// Necromancer: Drain life from a nearby enemy
    LifeDrain,
    /// Necromancer: Cause nearby enemies to flee
    Fear,
    /// Ranger: Leap away from nearest enemy
    Disengage,
    /// Ranger: Roll to target position with brief invulnerability
    Tumble,
    /// Ranger: Place a snare trap that roots enemies
    SnareTrap,
    /// Ranger: Shoot an arrow that slows the target
    CripplingShot,
    /// Fighter: Stun all nearby enemies for a few seconds
    Stun,
    /// Universal: Rest to fast-forward time until healed or interrupted
    Rest,
    /// Universal: Sleep to fast-forward time until fatigue is gone or
    /// interrupted (you are unaware while asleep — enemies sneak-attack you)
    Sleep,
    /// Learned spell (studied from a Scroll of Blink)
    LearnedBlink,
    /// Learned spell (studied from a Scroll of Fireball)
    LearnedFireball,
    /// Learned spell (studied from a Scroll of Fear)
    LearnedFear,
    /// Learned spell (studied from a Scroll of Slow)
    LearnedSlow,
    /// Learned spell (studied from a Scroll of Protection)
    LearnedProtection,
    /// Learned spell (studied from a Scroll of Speed)
    LearnedSpeed,
    /// Learned spell (studied from a Scroll of Invisibility)
    LearnedInvisibility,
    /// Necromancer: channel over a bones pile to raise a skeleton companion
    RaiseDead,
    /// Fighter: brace for a blow. Melee hits during the guard are mostly
    /// blocked and the attacker is staggered. Reactive: active from the
    /// moment the action starts.
    Guard,
    /// Necromancer: a ward of bones that absorbs the next few hits (more
    /// charges with corpses nearby). Reactive: up at action start.
    BoneWard,
    /// Necromancer: swap places with one of your raised skeletons. Reactive:
    /// the swap happens at action start, so swings at you find the skeleton.
    Sacrifice,
    /// Necromancer: detonate a corpse, damaging hostiles around it.
    CorpseExplosion,
    /// Druid: melee attackers take damage back while the buff lasts.
    Thorns,
    /// Druid: vines root hostiles around a tile (longer in grass).
    Entangle,
}

impl AbilityType {
    /// Display name for the ability
    pub fn name(&self) -> &'static str {
        match self {
            AbilityType::Cleave => "Cleave",
            AbilityType::Sprint => "Sprint",
            AbilityType::Tame => "Tame Animal",
            AbilityType::Barkskin => "Barkskin",
            AbilityType::LifeDrain => "Life Drain",
            AbilityType::Fear => "Fear",
            AbilityType::Disengage => "Disengage",
            AbilityType::Tumble => "Tumble",
            AbilityType::SnareTrap => "Snare Trap",
            AbilityType::CripplingShot => "Crippling Shot",
            AbilityType::Stun => "Stun",
            AbilityType::Rest => "Rest",
            AbilityType::Sleep => "Sleep",
            AbilityType::LearnedBlink => "Blink",
            AbilityType::LearnedFireball => "Fireball",
            AbilityType::LearnedFear => "Fear",
            AbilityType::LearnedSlow => "Slow",
            AbilityType::LearnedProtection => "Protection",
            AbilityType::LearnedSpeed => "Speed",
            AbilityType::LearnedInvisibility => "Invisibility",
            AbilityType::RaiseDead => "Raise Dead",
            AbilityType::Guard => "Guard",
            AbilityType::BoneWard => "Bone Ward",
            AbilityType::Sacrifice => "Sacrifice",
            AbilityType::CorpseExplosion => "Corpse Explosion",
            AbilityType::Thorns => "Thorns",
            AbilityType::Entangle => "Entangle",
        }
    }

    /// Description of what the ability does
    pub fn description(&self) -> &'static str {
        match self {
            AbilityType::Cleave => "Attack all adjacent enemies",
            AbilityType::Sprint => "Double movement speed for 10 seconds",
            AbilityType::Tame => "Channel to tame a nearby animal",
            AbilityType::Barkskin => "Reduce damage by 50% for 15 seconds",
            AbilityType::LifeDrain => "Channel to drain life from a nearby enemy",
            AbilityType::Fear => "Cause nearby enemies to flee",
            AbilityType::Disengage => "Leap 3 tiles away from nearest enemy",
            AbilityType::Tumble => "Roll to target with brief invulnerability",
            AbilityType::SnareTrap => "Place a trap that roots enemies",
            AbilityType::CripplingShot => "Arrow that slows the target",
            AbilityType::Stun => "Stun all nearby enemies for 5 seconds",
            AbilityType::Rest => "Rest until healed or an enemy spots you",
            AbilityType::Sleep => "Sleep off your fatigue — but you're defenseless while asleep",
            AbilityType::LearnedBlink => "Teleport to a nearby tile (scales with INT)",
            AbilityType::LearnedFireball => "Hurl an explosive fireball (scales with INT)",
            AbilityType::LearnedFear => "Terrify all visible enemies (scales with INT)",
            AbilityType::LearnedSlow => "Slow all visible enemies (scales with INT)",
            AbilityType::LearnedProtection => "Halve incoming damage for a while (scales with INT)",
            AbilityType::LearnedSpeed => "Move and act faster for a while (scales with INT)",
            AbilityType::LearnedInvisibility => "Fade from sight for a while (scales with INT)",
            AbilityType::RaiseDead => "Channel over bones to raise a skeleton ally",
            AbilityType::Guard => "Brace: block 75% of melee hits and stagger the attacker",
            AbilityType::BoneWard => "Absorb the next hit (+1 per nearby corpse, max 3)",
            AbilityType::Sacrifice => "Swap places with one of your raised skeletons",
            AbilityType::CorpseExplosion => "Detonate a corpse, hurting nearby enemies",
            AbilityType::Thorns => "Melee attackers take damage back (scales with INT)",
            AbilityType::Entangle => "Root enemies around a tile (longer in grass)",
        }
    }

    /// How tiring this ability is (feeds fatigue; nothing is gated on it)
    /// How tiring this ability is, as a flat amount. See `Effort`.
    pub fn energy_cost(&self) -> f32 {
        match self {
            AbilityType::Cleave => CLEAVE_ENERGY_COST,
            AbilityType::Sprint => SPRINT_ENERGY_COST,
            AbilityType::Tame => TAME_ENERGY_COST,
            AbilityType::Barkskin => BARKSKIN_ENERGY_COST,
            AbilityType::LifeDrain => LIFE_DRAIN_ENERGY_COST,
            AbilityType::Fear => FEAR_ABILITY_ENERGY_COST,
            AbilityType::Disengage => DISENGAGE_ENERGY_COST,
            AbilityType::Tumble => TUMBLE_ENERGY_COST,
            AbilityType::SnareTrap => SNARE_TRAP_ENERGY_COST,
            AbilityType::CripplingShot => CRIPPLING_SHOT_ENERGY_COST,
            AbilityType::Stun => STUN_ENERGY_COST,
            AbilityType::Rest => 0.0,
            AbilityType::Sleep => 0.0,
            AbilityType::LearnedBlink => LEARNED_BLINK_ENERGY_COST,
            AbilityType::LearnedFireball => LEARNED_FIREBALL_ENERGY_COST,
            AbilityType::LearnedFear => LEARNED_FEAR_ENERGY_COST,
            AbilityType::LearnedSlow => LEARNED_SLOW_ENERGY_COST,
            AbilityType::LearnedProtection => LEARNED_PROTECTION_ENERGY_COST,
            AbilityType::LearnedSpeed => LEARNED_SPEED_ENERGY_COST,
            AbilityType::LearnedInvisibility => LEARNED_INVISIBILITY_ENERGY_COST,
            AbilityType::RaiseDead => RAISE_DEAD_ENERGY_COST,
            AbilityType::Guard => GUARD_ENERGY_COST,
            AbilityType::BoneWard => BONE_WARD_ENERGY_COST,
            AbilityType::Sacrifice => SACRIFICE_ENERGY_COST,
            AbilityType::CorpseExplosion => CORPSE_EXPLOSION_ENERGY_COST,
            AbilityType::Thorns => THORNS_ENERGY_COST,
            AbilityType::Entangle => ENTANGLE_ENERGY_COST,
        }
    }

    /// Cooldown for a learned/studied spell (or Raise Dead). `None` for
    /// abilities that live on other components (class/secondary/kit).
    pub fn learned_cooldown(&self) -> Option<f32> {
        match self {
            AbilityType::LearnedBlink => Some(LEARNED_BLINK_COOLDOWN),
            AbilityType::LearnedFireball => Some(LEARNED_FIREBALL_COOLDOWN),
            AbilityType::LearnedFear => Some(LEARNED_FEAR_COOLDOWN),
            AbilityType::LearnedSlow => Some(LEARNED_SLOW_COOLDOWN),
            AbilityType::LearnedProtection => Some(LEARNED_PROTECTION_COOLDOWN),
            AbilityType::LearnedSpeed => Some(LEARNED_SPEED_COOLDOWN),
            AbilityType::LearnedInvisibility => Some(LEARNED_INVISIBILITY_COOLDOWN),
            AbilityType::RaiseDead => Some(RAISE_DEAD_COOLDOWN),
            _ => None,
        }
    }
}

/// A spell the player has permanently learned (by studying a scroll), or an
/// innate spell-list ability (the Necromancer's Raise Dead). Effort comes from
/// [`AbilityType::energy_cost`]; the cooldown is stored per entry and is the
/// only thing gating use.
#[derive(Debug, Clone, Copy)]
pub struct LearnedSpell {
    pub ability: AbilityType,
    /// Seconds remaining on cooldown (0 = ready)
    pub cooldown_remaining: f32,
    /// Total cooldown duration
    pub cooldown_total: f32,
}

/// The player's spell list: permanently learned abilities with individual
/// long cooldowns. Ticked alongside the other ability cooldowns.
#[derive(Debug, Clone, Default)]
pub struct LearnedAbilities {
    pub spells: Vec<LearnedSpell>,
}

impl LearnedAbilities {
    /// Does this list already contain the given ability?
    pub fn knows(&self, ability: AbilityType) -> bool {
        self.spells.iter().any(|s| s.ability == ability)
    }

    /// Add a newly learned ability (no-op if already known). Returns whether
    /// it was added.
    pub fn learn(&mut self, ability: AbilityType) -> bool {
        if self.knows(ability) {
            return false;
        }
        let Some(cooldown) = ability.learned_cooldown() else {
            return false;
        };
        self.spells.push(LearnedSpell {
            ability,
            cooldown_remaining: 0.0,
            cooldown_total: cooldown,
        });
        true
    }

    /// Look up a learned spell entry.
    pub fn get(&self, ability: AbilityType) -> Option<&LearnedSpell> {
        self.spells.iter().find(|s| s.ability == ability)
    }

    /// Start the cooldown for a learned spell (no-op if not known).
    pub fn start_cooldown(&mut self, ability: AbilityType) {
        if let Some(spell) = self.spells.iter_mut().find(|s| s.ability == ability) {
            spell.cooldown_remaining = spell.cooldown_total;
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

/// Marker for an entity that is sneaking (player crouch toggle): slower movement,
/// much harder for unaware/unalerted enemies to detect.
#[derive(Debug, Clone, Copy)]
pub struct Sneaking;

// =============================================================================
// SURVIVAL METERS (player-only)
// =============================================================================

/// Coarse hunger state, derived from the `Hunger` meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HungerState {
    /// Above the hungry threshold: no penalties.
    Fed,
    /// Below the hungry threshold: natural HP regen stops.
    Hungry,
    /// Meter at zero: taking periodic starvation damage.
    Starving,
}

/// Hunger meter (player-only — enemies never hunger). Starts full and drains
/// over game time (see `systems::survival`); eating food restores it.
#[derive(Debug, Clone, Copy)]
pub struct Hunger {
    /// Current fullness, 0.0 (starving) to `HUNGER_MAX` (fully fed).
    pub value: f32,
    /// Accumulated game-time toward the next starvation damage tick (only
    /// meaningful while starving; reset when fed).
    pub starvation_timer: f32,
}

impl Hunger {
    pub fn new() -> Self {
        Self {
            value: crate::constants::HUNGER_MAX,
            starvation_timer: 0.0,
        }
    }

    /// Coarse state for UI labels and threshold-crossing messages.
    pub fn state(&self) -> HungerState {
        if self.value <= 0.0 {
            HungerState::Starving
        } else if self.value < crate::constants::HUNGER_HUNGRY_THRESHOLD {
            HungerState::Hungry
        } else {
            HungerState::Fed
        }
    }

    /// Hungry or worse: natural HP regen is stopped.
    pub fn is_hungry(&self) -> bool {
        self.value < crate::constants::HUNGER_HUNGRY_THRESHOLD
    }

    /// Meter empty: periodic starvation damage.
    pub fn is_starving(&self) -> bool {
        self.value <= 0.0
    }

    /// Restore hunger from eating, clamped to the cap.
    pub fn eat(&mut self, amount: f32) {
        self.value = (self.value + amount).min(crate::constants::HUNGER_MAX);
    }
}

impl Default for Hunger {
    fn default() -> Self {
        Self::new()
    }
}

/// Coarse fatigue state, derived from the `Fatigue` meter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatigueState {
    /// Below the tired threshold: no penalties.
    Rested,
    /// Above the tired threshold: enemies notice the player faster (+25%
    /// alertness gain) and the player's damage drops (-10%).
    Tired,
    /// Meter at cap: actions are 25% slower until slept off.
    Exhausted,
}

/// Fatigue meter (player-only).
///
/// Starts empty and grows with **effort** — every action adds its own effort,
/// so fighting tires you several times faster than walking does — plus a small
/// amount simply for being awake. Sleeping is the only thing that drains it.
///
/// It is deliberately a long meter: nothing is gated on it until it maxes out,
/// at which point actions slow down. Fighting should cost you over the course
/// of a run, not throttle you in the middle of a fight.
#[derive(Debug, Clone, Copy)]
pub struct Fatigue {
    /// Current fatigue, 0.0 (fully rested) to `FATIGUE_MAX` (exhausted).
    pub value: f32,
}

impl Fatigue {
    pub fn new() -> Self {
        Self { value: 0.0 }
    }

    /// Coarse state for UI labels and threshold-crossing messages.
    pub fn state(&self) -> FatigueState {
        if self.value >= crate::constants::FATIGUE_MAX {
            FatigueState::Exhausted
        } else if self.value > crate::constants::FATIGUE_TIRED_THRESHOLD {
            FatigueState::Tired
        } else {
            FatigueState::Rested
        }
    }

    /// Tired or worse: alertness/damage penalties apply.
    pub fn is_tired(&self) -> bool {
        self.value > crate::constants::FATIGUE_TIRED_THRESHOLD
    }

    /// Meter maxed: actions are slower until the player sleeps it off.
    pub fn is_exhausted(&self) -> bool {
        self.value >= crate::constants::FATIGUE_MAX
    }
}

impl Default for Fatigue {
    fn default() -> Self {
        Self::new()
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemType {
    // Weapons
    Sword,
    Bow,
    Dagger,
    Staff,
    // Armor
    LeatherArmor,
    ChainMail,
    Helmet,
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
    // Ammunition
    Arrow,
    FireArrow,
    // Accessories (pure affix carriers, no base stats)
    Ring,
    Amulet,
    // Utility
    /// Empty water flask: use next to (or in) water to fill it
    WaterFlaskEmpty,
    /// Filled water flask: drink to douse yourself, or throw to splash-douse
    /// fires and wet grass (see systems::fire)
    WaterFlaskFull,
}

impl ItemType {
    /// Returns true if this item type stacks in inventory
    pub fn is_stackable(&self) -> bool {
        matches!(self, ItemType::Arrow | ItemType::FireArrow)
    }

    /// Returns true if this item type is bow ammunition
    pub fn is_ammo(&self) -> bool {
        matches!(self, ItemType::Arrow | ItemType::FireArrow)
    }
}

// =============================================================================
// ITEM INSTANCES (rarity + affixes)
// =============================================================================

/// Rarity tier of a gear instance. Drives affix count, tooltip color, and price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Magic,
    Rare,
    /// 3-4 affix components and a generated name ("Emberfang, Sword of the Wolf")
    Legendary,
}

impl Rarity {
    /// Display label used in tooltips.
    pub fn label(&self) -> &'static str {
        match self {
            Rarity::Common => "Common",
            Rarity::Magic => "Magic",
            Rarity::Rare => "Rare",
            Rarity::Legendary => "Legendary",
        }
    }
}

/// A rolled modifier component on a gear instance. Empty for consumables.
///
/// Affixes are composable components: higher rarity means more of them, and a
/// Legendary is an unexpected *combination*, not a bigger number. Stat and
/// flat-bonus affixes apply through `queries::effective_stats` and the
/// damage/defense chokepoints; on-hit affixes resolve centrally in
/// `systems::combat::resolve_weapon_on_hit`.
///
/// Curse affixes are negative components that can roll alongside good ones
/// (gamble items). They are hidden until the item is identified (see
/// `systems::identify`), but apply while equipped regardless: `CursedFragile`
/// through defense math, `CursedHeavy` through action speed in the time
/// system, and `CursedLoud` through attack noise radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Affix {
    /// Bonus melee/ranged damage (weapons)
    Damage(i32),
    /// Bonus flat defense (armor)
    Defense(i32),
    /// Bonus Strength while equipped
    Strength(i32),
    /// Bonus Agility while equipped
    Agility(i32),
    /// Bonus Intelligence while equipped
    Intelligence(i32),
    /// Bonus maximum health while equipped
    MaxHealth(i32),
    /// Chance (0..1) to set the target Burning on a weapon hit
    OnHitIgnite(f32),
    /// Chance (0..1) to Slow the target on a weapon hit
    OnHitSlow(f32),
    /// Chance (0..1) to Fear the target on a weapon hit
    OnHitFear(f32),
    /// Fraction (0..1) of damage dealt healed back to the attacker
    OnHitLifesteal(f32),
    /// Weapon hits push the target one tile away (if walkable)
    OnHitKnockback,
    /// On killing blow: heal the attacker N health
    KillHeal(i32),
    /// Below the low-health threshold: +X fraction bonus damage
    LowHealthDamage(f32),
    /// Curse: -N defense
    CursedFragile(i32),
    /// Curse: slower attack/movement (fractional penalty; applied in a later pass)
    CursedHeavy(f32),
    /// Curse: attacks make double noise radius (applied in a later pass)
    CursedLoud,
}

impl Affix {
    /// Human-readable description for tooltips, e.g. "+2 Damage".
    pub fn describe(&self) -> String {
        match self {
            Affix::Damage(n) => format!("+{} Damage", n),
            Affix::Defense(n) => format!("+{} Defense", n),
            Affix::Strength(n) => format!("+{} Strength", n),
            Affix::Agility(n) => format!("+{} Agility", n),
            Affix::Intelligence(n) => format!("+{} Intelligence", n),
            Affix::MaxHealth(n) => format!("+{} Max Health", n),
            Affix::OnHitIgnite(c) => format!("{:.0}% chance to ignite on hit", c * 100.0),
            Affix::OnHitSlow(c) => format!("{:.0}% chance to slow on hit", c * 100.0),
            Affix::OnHitFear(c) => format!("{:.0}% chance to terrify on hit", c * 100.0),
            Affix::OnHitLifesteal(f) => format!("Heals {:.0}% of damage dealt", f * 100.0),
            Affix::OnHitKnockback => "Knocks targets back on hit".to_string(),
            Affix::KillHeal(n) => format!("Heals {} health on kill", n),
            Affix::LowHealthDamage(f) => {
                format!("+{:.0}% damage while below 30% health", f * 100.0)
            }
            Affix::CursedFragile(n) => format!("Cursed: -{} Defense", n),
            Affix::CursedHeavy(f) => format!("Cursed: {:.0}% slower", f * 100.0),
            Affix::CursedLoud => "Cursed: attacks ring out twice as loud".to_string(),
        }
    }

    /// Whether this affix is a curse (negative component).
    pub fn is_curse(&self) -> bool {
        matches!(
            self,
            Affix::CursedFragile(_) | Affix::CursedHeavy(_) | Affix::CursedLoud
        )
    }
}

/// One concrete item in the world. Consumables are trivial instances (no
/// affixes, Common); gear can carry rolled rarity, affixes, and (for
/// Legendary items) a generated name.
#[derive(Debug, Clone)]
pub struct ItemInstance {
    pub kind: ItemType,
    pub rarity: Rarity,
    pub affixes: Vec<Affix>,
    /// Generated name for Legendary items (e.g. "Emberfang, Sword of the Wolf").
    pub name: Option<String>,
    /// Whether the item's affixes (and Legendary name) are known to the
    /// carrier. Magic+ gear drops unidentified; carrying it long enough
    /// identifies it (see `systems::identify`).
    pub identified: bool,
    /// Game-time seconds this item has been carried toward identification.
    pub identify_progress: f32,
}

impl ItemInstance {
    /// A plain, unmodified item — consumables, vendor stock, arrows, starting gear.
    pub fn plain(kind: ItemType) -> Self {
        Self {
            kind,
            rarity: Rarity::Common,
            affixes: Vec::new(),
            name: None,
            identified: true,
            identify_progress: 0.0,
        }
    }

    /// Display name: the generated Legendary name if present, else the base
    /// item name from its definition. Unidentified gear hides its Legendary
    /// name and reads as e.g. "Unidentified Sword".
    pub fn display_name(&self) -> String {
        let base = crate::systems::items::item_name(self.kind);
        if !self.identified {
            return format!("Unidentified {}", base);
        }
        match &self.name {
            Some(name) => name.clone(),
            None => base.to_string(),
        }
    }

    /// Whether any of this instance's affixes is a curse.
    pub fn has_curse(&self) -> bool {
        self.affixes.iter().any(|a| a.is_curse())
    }

    /// Sum of all `Affix::Damage` modifiers on this instance.
    pub fn damage_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::Damage(n) => *n,
                _ => 0,
            })
            .sum()
    }

    /// Sum of all `Affix::Defense` modifiers on this instance, minus any
    /// `CursedFragile` penalty.
    pub fn defense_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::Defense(n) => *n,
                Affix::CursedFragile(n) => -*n,
                _ => 0,
            })
            .sum()
    }

    /// Sum of all `Affix::Strength` modifiers on this instance.
    pub fn strength_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::Strength(n) => *n,
                _ => 0,
            })
            .sum()
    }

    /// Sum of all `Affix::Agility` modifiers on this instance.
    pub fn agility_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::Agility(n) => *n,
                _ => 0,
            })
            .sum()
    }

    /// Sum of all `Affix::Intelligence` modifiers on this instance.
    pub fn intelligence_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::Intelligence(n) => *n,
                _ => 0,
            })
            .sum()
    }

    /// Sum of all `Affix::MaxHealth` modifiers on this instance.
    pub fn max_health_bonus(&self) -> i32 {
        self.affixes
            .iter()
            .map(|a| match a {
                Affix::MaxHealth(n) => *n,
                _ => 0,
            })
            .sum()
    }
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
    /// Cannot move but can still attack (from Snare Trap)
    Rooted,
    /// Immune to all damage (brief, from Tumble)
    Invulnerable,
    /// Cannot act at all (from Fighter's Stun ability)
    Stunned,
    /// Bracing (Fighter's Guard): melee hits are mostly blocked and the
    /// attacker is staggered. Lasts as long as the Guard action.
    Guarding,
    /// Melee attackers take damage back (Druid's Thorns)
    Thorns,
    /// A ward of bones absorbs whole hits (Necromancer's Bone Ward). The
    /// charge count lives on the [`BoneWard`] component; this effect is its
    /// timer and HUD pip.
    BoneWard,
}

/// An active status effect with remaining duration
#[derive(Debug, Clone, Copy)]
pub struct ActiveEffect {
    pub effect_type: EffectType,
    /// Remaining duration in game-time seconds
    pub remaining_duration: f32,
    /// The duration this effect was last applied or refreshed with, so the HUD
    /// can draw how much of it is left. Presentation only — nothing in the
    /// simulation reads it, and a refresh resets it along with the remainder.
    pub total_duration: f32,
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
    pub items: Vec<ItemInstance>,
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
    /// Dead enemy corpse/bones
    Corpse,
    /// Items dropped on the ground
    GroundPile,
}

/// Container component (for chests, coffins, barrels)
#[derive(Debug, Clone)]
pub struct Container {
    pub container_type: ContainerType,
    pub items: Vec<ItemInstance>,
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
    /// Interact with something in a direction (Ctrl+movement)
    InteractDirection { dx: i32, dy: i32 },
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
    /// Necromancer ability: start draining life from a target (channeled)
    StartLifeDrain { target: Entity },
    /// Necromancer ability: cause nearby enemies to flee
    ActivateFear,
    /// Fighter ability: stun nearby enemies
    ActivateStun,
    /// Place a fire trap at target location
    PlaceFireTrap { target_x: i32, target_y: i32 },
    /// Ranger ability: leap away from nearest enemy
    Disengage,
    /// Ranger ability: roll to target position with invulnerability
    Tumble { target_x: i32, target_y: i32 },
    /// Ranger ability: place a snare trap that roots enemies
    PlaceSnareTrap { target_x: i32, target_y: i32 },
    /// Ranger ability: shoot arrow that slows target
    ShootCripplingShot { target_x: i32, target_y: i32 },
    /// Cast a learned (studied) spell. Target coords are ignored for
    /// untargeted spells (Fear/Slow/Protection/Speed/Invisibility).
    CastLearnedSpell { ability: AbilityType, target_x: i32, target_y: i32 },
    /// Necromancer ability: start channeling Raise Dead on a bones pile
    StartRaiseDead { target: Entity },
    /// Recovery after shooting (auto-queued, allows arrow to fly)
    Recover,
    /// Boss wind-up for Gnash's ground slam. The shockwave (damage + stun
    /// within `BOSS_SLAM_RADIUS` of the boss) is applied when this completes,
    /// so the wind-up is the window to get clear.
    BossGroundSlam,
    /// Fighter kit: Guard. Reactive — the Guarding effect goes up when the
    /// action STARTS and drops when it completes.
    Guard,
    /// Necromancer kit: Bone Ward. Reactive — the ward goes up at start.
    BoneWard,
    /// Necromancer kit: swap places with a raised skeleton. Reactive — the
    /// swap happens at start.
    Sacrifice { skeleton: Entity },
    /// Necromancer kit: detonate a corpse (applied at completion).
    CorpseExplosion { corpse: Entity },
    /// Druid kit: Thorns self-buff (applied at completion).
    ActivateThorns,
    /// Druid kit: root hostiles around a tile (applied at completion).
    Entangle { target_x: i32, target_y: i32 },
}

impl ActionType {
    /// Energy cost to start this action
    /// How tiring this action is, and how that is shaped.
    ///
    /// Two shapes, because two different things are being modelled:
    ///
    /// - [`Effort::PerSecond`] for continuous exertion — walking, fighting,
    ///   hauling a door open. Accrued per game-second of acting, so the rate is
    ///   the same whatever your speed. A flat amount here would make tiredness
    ///   track the speed stat rather than the work done.
    /// - [`Effort::Flat`] for abilities, which are discrete commitments rather
    ///   than sustained effort. Casting a spell quickly should not make it less
    ///   tiring, so these are speed-independent by construction.
    pub fn effort(&self) -> Effort {
        use Effort::{Flat, PerSecond};
        match self {
            // --- Free: no effort at all ---
            ActionType::Wait => PerSecond(EXERTION_IDLE),
            ActionType::Recover => PerSecond(EXERTION_IDLE), // post-shot settle
            ActionType::EquipWeapon { .. } => PerSecond(EXERTION_IDLE),
            ActionType::UnequipWeapon => PerSecond(EXERTION_IDLE),

            // --- Light: walking, and things no harder than walking ---
            ActionType::Move { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::InteractDirection { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::OpenDoor { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::OpenChest { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::UseStairs { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::TalkTo { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::DropItem { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::DropEquippedWeapon => PerSecond(EXERTION_LIGHT),
            ActionType::ThrowPotion { .. } => PerSecond(EXERTION_LIGHT),

            // --- Heavy: fighting ---
            ActionType::Attack { .. } => PerSecond(EXERTION_HEAVY),
            ActionType::AttackDirection { .. } => PerSecond(EXERTION_HEAVY),
            ActionType::ShootBow { .. } => PerSecond(EXERTION_HEAVY),
            ActionType::BossGroundSlam => PerSecond(EXERTION_HEAVY),

            // --- Flat: deliberate abilities, each with its own price ---
            ActionType::Cleave => Flat(CLEAVE_ENERGY_COST),
            ActionType::ActivateSprint => Flat(SPRINT_ENERGY_COST),
            ActionType::StartTaming { .. } => Flat(TAME_ENERGY_COST),
            ActionType::ActivateBarkskin => Flat(BARKSKIN_ENERGY_COST),
            ActionType::StartLifeDrain { .. } => Flat(LIFE_DRAIN_ENERGY_COST),
            ActionType::ActivateFear => Flat(FEAR_ABILITY_ENERGY_COST),
            ActionType::ActivateStun => Flat(STUN_ENERGY_COST),
            ActionType::Disengage => Flat(DISENGAGE_ENERGY_COST),
            ActionType::Tumble { .. } => Flat(TUMBLE_ENERGY_COST),
            ActionType::PlaceSnareTrap { .. } => Flat(SNARE_TRAP_ENERGY_COST),
            ActionType::ShootCripplingShot { .. } => Flat(CRIPPLING_SHOT_ENERGY_COST),
            ActionType::PlaceFireTrap { .. } => PerSecond(EXERTION_LIGHT),
            ActionType::Blink { .. } => Flat(LEARNED_BLINK_ENERGY_COST),
            ActionType::CastFireball { .. } => Flat(LEARNED_FIREBALL_ENERGY_COST),
            ActionType::StartRaiseDead { .. } => Flat(RAISE_DEAD_ENERGY_COST),
            ActionType::CastLearnedSpell { ability, .. } => Flat(ability.energy_cost()),
            ActionType::Guard => Flat(GUARD_ENERGY_COST),
            ActionType::BoneWard => Flat(BONE_WARD_ENERGY_COST),
            ActionType::Sacrifice { .. } => Flat(SACRIFICE_ENERGY_COST),
            ActionType::CorpseExplosion { .. } => Flat(CORPSE_EXPLOSION_ENERGY_COST),
            ActionType::ActivateThorns => Flat(THORNS_ENERGY_COST),
            ActionType::Entangle { .. } => Flat(ENTANGLE_ENERGY_COST),
        }
    }

    /// How much effort this action adds, given how long it runs.
    pub fn effort_for_duration(&self, duration: f32) -> f32 {
        match self.effort() {
            Effort::PerSecond(rate) => rate * duration.max(0.0),
            Effort::Flat(amount) => amount,
        }
    }
}

/// How tiring an action is, and how that is calculated.
///
/// Effort is not a resource the player spends — nothing is gated on it. It is
/// only an input to the long-term fatigue meter, so that a run full of fighting
/// sends you to bed sooner than a run spent creeping down corridors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effort {
    /// Effort per game-second of acting; the total is this times the action's
    /// duration. For continuous exertion, where tiredness should track time
    /// spent — and where a flat amount would make a fast creature tire in
    /// proportion to its own speed, which is backwards.
    PerSecond(f32),
    /// A fixed amount regardless of how long the action takes. For abilities,
    /// which are discrete commitments rather than sustained effort.
    Flat(f32),
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
    /// Speed multiplier (1.0 = normal, higher = faster)
    pub speed: f32,
    /// Currently executing action (None if idle and ready)
    pub current_action: Option<ActionInProgress>,
}

impl Actor {
    pub fn new(speed: f32) -> Self {
        Self {
            speed,
            current_action: None,
        }
    }

    /// Can start a new action (has energy and not mid-action)
    /// Can start a new action: simply not already busy.
    ///
    /// There is deliberately no resource gate here. Actions are paced by their
    /// own durations and abilities by their cooldowns; an energy pool on top of
    /// those was a third lock on a door that already had two, and the one it
    /// produced was "you press the button and nothing happens".
    pub fn can_act(&self) -> bool {
        self.current_action.is_none()
    }
}

/// AI behavior state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AIState {
    /// Not yet aware of any target (sleeping or idly patrolling). Won't chase
    /// until it perceives a target or is woken by noise/a shout.
    Unaware,
    /// Wandering randomly, has been aware but currently has no target
    Idle,
    /// Actively chasing a target (can see them)
    Chasing,
    /// Moving to last known target position after losing sight
    Investigating,
}

/// Marker for an enemy that is asleep (a deeper subset of `AIState::Unaware`).
/// Asleep enemies stay put, perceive at a reduced range, and take extra damage
/// from the first hit (sneak attack). Removed when the enemy wakes.
#[derive(Debug, Clone, Copy)]
pub struct Asleep;

/// The most recent thing that hurt the player ("Goblin", "burning",
/// "starvation", "a spike trap", ...). Kept on the player entity and updated
/// during event processing; read at death as the best-effort cause of death
/// for the run-history record.
#[derive(Debug, Clone)]
pub struct LastDamageSource(pub String);

/// Marker for enemies intelligent enough to open doors (and to raise an alarm
/// shout). Dumb beasts/undead lack it, so a closed door stops them.
#[derive(Debug, Clone, Copy)]
pub struct CanOpenDoors;

/// Active alarm-shout channel. While present the enemy is busy shouting; when
/// `remaining` reaches zero it wakes nearby unaware allies. Interruptible —
/// removed if the shouter takes damage.
#[derive(Debug, Clone, Copy)]
pub struct AlarmInProgress {
    pub remaining: f32,
}

/// A single entry in an entity's threat table.
#[derive(Debug, Clone, Copy)]
pub struct ThreatEntry {
    pub entity: Entity,
    pub threat: f32,
    /// Last known position of this target (for investigating after losing sight)
    pub last_known_pos: Option<(i32, i32)>,
    /// How long threat has been at the minimum floor (seconds).
    /// Entry is only pruned after this exceeds THREAT_MEMORY_DURATION.
    pub time_at_minimum: f32,
}

/// AI behavior: chase the highest-threat target, wander otherwise
#[derive(Debug, Clone)]
pub struct ChaseAI {
    pub sight_radius: i32,
    pub state: AIState,
    /// Per-target threat tracking
    pub threat_table: Vec<ThreatEntry>,
    /// The entity currently being pursued
    pub current_target: Option<Entity>,
    /// Minimum range to use ranged attack (0 = melee only)
    pub ranged_min: i32,
    /// Maximum range for ranged attack (0 = melee only)
    pub ranged_max: i32,
    /// Seconds until this enemy can raise an alarm shout again (0 = ready)
    pub shout_cooldown: f32,
    /// Detection meter while Unaware: builds when the player is seen, decays
    /// otherwise; crossing the wake threshold rouses the enemy (gradual waking).
    pub alertness: f32,
}

impl ChaseAI {
    pub fn new(sight_radius: i32) -> Self {
        Self {
            sight_radius,
            state: AIState::Idle,
            threat_table: Vec::new(),
            current_target: None,
            ranged_min: 0,
            ranged_max: 0,
            shout_cooldown: 0.0,
            alertness: 0.0,
        }
    }

    /// Create a ChaseAI with ranged attack capability
    pub fn with_ranged(sight_radius: i32, ranged_min: i32, ranged_max: i32) -> Self {
        Self {
            sight_radius,
            state: AIState::Idle,
            threat_table: Vec::new(),
            current_target: None,
            ranged_min,
            ranged_max,
            shout_cooldown: 0.0,
            alertness: 0.0,
        }
    }

    /// Add threat for a specific entity. Creates entry if not present.
    pub fn add_threat(&mut self, target: Entity, amount: f32) {
        if let Some(entry) = self.threat_table.iter_mut().find(|e| e.entity == target) {
            entry.threat += amount;
            entry.time_at_minimum = 0.0; // Reset memory timer on new threat
        } else {
            self.threat_table.push(ThreatEntry {
                entity: target,
                threat: amount,
                last_known_pos: None,
                time_at_minimum: 0.0,
            });
        }
    }

    /// Remove a target from the threat table (e.g., on death).
    pub fn remove_target(&mut self, target: Entity) {
        self.threat_table.retain(|e| e.entity != target);
        if self.current_target == Some(target) {
            self.current_target = None;
        }
    }

    /// Update last_known_pos for a target.
    pub fn update_target_pos(&mut self, target: Entity, pos: (i32, i32)) {
        if let Some(entry) = self.threat_table.iter_mut().find(|e| e.entity == target) {
            entry.last_known_pos = Some(pos);
        }
    }

    /// Get last_known_pos for a specific target.
    pub fn last_known_pos_for(&self, target: Entity) -> Option<(i32, i32)> {
        self.threat_table.iter().find(|e| e.entity == target).and_then(|e| e.last_known_pos)
    }

    /// Get the highest-threat entry.
    pub fn highest_threat(&self) -> Option<&ThreatEntry> {
        self.threat_table.iter().max_by(|a, b| a.threat.partial_cmp(&b.threat).unwrap_or(std::cmp::Ordering::Equal))
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
    /// Create a chest container
    pub fn chest(items: Vec<ItemInstance>, gold: u32) -> Self {
        Self {
            container_type: ContainerType::Chest,
            items,
            gold,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    /// Create a coffin (may spawn enemy when opened)
    pub fn coffin(items: Vec<ItemInstance>, gold: u32, spawn_chance: f32) -> Self {
        Self {
            container_type: ContainerType::Coffin,
            items,
            gold,
            is_open: false,
            spawn_chance,
        }
    }

    /// Create a barrel (contains food)
    pub fn barrel(items: Vec<ItemInstance>) -> Self {
        Self {
            container_type: ContainerType::Barrel,
            items,
            gold: 0,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    /// Create a corpse/bones container (from dead enemies)
    pub fn corpse(items: Vec<ItemInstance>, gold: u32) -> Self {
        Self {
            container_type: ContainerType::Corpse,
            items,
            gold,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    /// Create a ground item pile
    pub fn ground_pile(items: Vec<ItemInstance>) -> Self {
        Self {
            container_type: ContainerType::GroundPile,
            items,
            gold: 0,
            is_open: false,
            spawn_chance: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.gold == 0
    }

    /// Opened and stripped of everything it held — scenery rather than a
    /// container with anything left to give.
    ///
    /// A looted container stops blocking its tile. Both the live sweep
    /// (`systems::unblock_emptied_containers`) and the floor-restore path
    /// (`load_floor`) decide that from here, so the two cannot drift: they used
    /// to disagree, and a floor you stayed on kept its looted chests as
    /// obstacles while the same floor revisited let you walk over them.
    pub fn is_looted(&self) -> bool {
        self.is_open && self.is_empty()
    }
}

/// Door component - can be open or closed
#[derive(Debug, Clone, Copy)]
pub struct Door {
    pub is_open: bool,
    /// Sprite to use when door is open
    pub open_sprite: (crate::tile::SpriteSheet, u32),
    /// Sprite to use when door is closed
    pub closed_sprite: (crate::tile::SpriteSheet, u32),
}

impl Door {
    /// Standard dungeon door
    pub fn new() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_OPEN,
            closed_sprite: tile_ids::DOOR,
        }
    }

    /// Green door for overgrown rooms
    pub fn green() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_GREEN_OPEN,
            closed_sprite: tile_ids::DOOR_GREEN,
        }
    }

    /// Grated door for crypt rooms
    pub fn grated() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_GRATED, // Same sprite open/closed
            closed_sprite: tile_ids::DOOR_GRATED,
        }
    }

    /// Shop door
    pub fn shop() -> Self {
        use crate::tile::tile_ids;
        Self {
            is_open: false,
            open_sprite: tile_ids::DOOR_SHOP_OPEN,
            closed_sprite: tile_ids::DOOR_SHOP,
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
    /// The inventory instance backing the player's equipped weapon. Carries
    /// rarity/affixes so they survive equip→unequip and feed the damage formula.
    /// `None` for enemies (their claws/bows are not item instances).
    pub weapon_source: Option<ItemInstance>,
    /// Body armor slot (generic - any entity with Equipment can wear armor)
    pub body: Option<ItemInstance>,
    /// Head armor slot
    pub head: Option<ItemInstance>,
    /// Ring accessory slot (pure affix carrier)
    pub ring: Option<ItemInstance>,
    /// Amulet accessory slot (pure affix carrier)
    pub amulet: Option<ItemInstance>,
}

impl Equipment {
    /// Create equipment with an already-constructed EquippedWeapon
    pub fn with_equipped(weapon: EquippedWeapon) -> Self {
        Self { weapon: Some(weapon), ..Self::empty() }
    }

    /// Create equipment for enemies that can use both melee (claws) and ranged (bow)
    pub fn with_weapons(melee: Weapon, ranged: RangedWeapon) -> Self {
        Self {
            weapon: Some(EquippedWeapon::Melee(melee)),
            enemy_ranged: Some(ranged),
            ..Self::empty()
        }
    }

    /// Create equipment with just a melee weapon (for melee-only enemies)
    pub fn with_weapon(weapon: Weapon) -> Self {
        Self { weapon: Some(EquippedWeapon::Melee(weapon)), ..Self::empty() }
    }

    /// Empty equipment with all slots unfilled.
    pub fn empty() -> Self {
        Self {
            weapon: None,
            enemy_ranged: None,
            weapon_source: None,
            body: None,
            head: None,
            ring: None,
            amulet: None,
        }
    }

    /// Iterate over every equipped item instance (weapon source, armor,
    /// accessories). The single place that defines "what counts as worn gear"
    /// for affix aggregation.
    pub fn equipped_instances(&self) -> impl Iterator<Item = &ItemInstance> {
        [
            self.weapon_source.as_ref(),
            self.body.as_ref(),
            self.head.as_ref(),
            self.ring.as_ref(),
            self.amulet.as_ref(),
        ]
        .into_iter()
        .flatten()
    }

    /// Mutable variant of [`equipped_instances`] (used by identification).
    pub fn equipped_instances_mut(&mut self) -> impl Iterator<Item = &mut ItemInstance> {
        [
            self.weapon_source.as_mut(),
            self.body.as_mut(),
            self.head.as_mut(),
            self.ring.as_mut(),
            self.amulet.as_mut(),
        ]
        .into_iter()
        .flatten()
    }

    /// Total flat defense from all armor + accessory slots (base by kind +
    /// Defense affixes, minus CursedFragile). Works for any entity with an
    /// Equipment component, so granting an enemy armor later is just a matter
    /// of filling a slot.
    pub fn total_defense(&self) -> i32 {
        [&self.body, &self.head, &self.ring, &self.amulet]
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|inst| crate::systems::item_defs::armor_base_defense(inst.kind) + inst.defense_bonus())
            .sum()
    }

    /// Total `Affix::Damage` bonus from the weapon's backing instance plus
    /// accessories. Rings/amulets are pure affix carriers, so their damage
    /// affixes apply to every weapon (and unarmed) attack.
    pub fn affix_damage_bonus(&self) -> i32 {
        self.weapon_source.as_ref().map_or(0, |w| w.damage_bonus())
            + self.ring.as_ref().map_or(0, |r| r.damage_bonus())
            + self.amulet.as_ref().map_or(0, |a| a.damage_bonus())
    }

    /// Sum of all `CursedHeavy` fractions across worn gear. Applied as an
    /// action-speed penalty in the time system (curses bite whether or not
    /// the item has been identified).
    pub fn cursed_heavy_total(&self) -> f32 {
        self.equipped_instances()
            .flat_map(|inst| inst.affixes.iter())
            .map(|a| match a {
                Affix::CursedHeavy(f) => *f,
                _ => 0.0,
            })
            .sum()
    }

    /// Whether any worn gear carries `CursedLoud` (attacks make double noise).
    pub fn has_cursed_loud(&self) -> bool {
        self.equipped_instances()
            .any(|inst| inst.affixes.iter().any(|a| matches!(a, Affix::CursedLoud)))
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
    /// Optional status effect to apply on hit (effect_type, duration)
    pub on_hit_effect: Option<(EffectType, f32)>,
    /// Whether this projectile hit an enemy (used for arrow recovery)
    pub hit_enemy: bool,
    /// Fire arrows: ignites the landing tile (grass catches) and burns up
    /// on impact (never recoverable).
    pub incendiary: bool,
}

/// Marker component for projectiles (for queries)
#[derive(Debug, Clone, Copy)]
pub struct ProjectileMarker;

/// The ammo type a shooter prefers to load next (toggled from the inventory).
/// Absent = normal arrows. Shots fall back to whatever ammo is actually carried.
#[derive(Debug, Clone, Copy)]
pub struct ActiveAmmo {
    pub kind: ItemType,
}

/// Per-entity sprite color tint (multiplied with the texture color).
/// Used e.g. to render fire arrows as red/orange-tinted normal arrows.
#[derive(Debug, Clone, Copy)]
pub struct SpriteTint {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// Present only while a sprite is flashing from a hit it just took, counting
/// down to zero. Added by `systems::animation::flash_on_damage` from damage
/// events and removed by `systems::animation::update_hit_flashes`; rendering
/// lifts the sprite's tint toward the flash colour while it is alive.
///
/// Counts down in *real* time, not game time, like the other per-entity
/// animation components here — see `update_hit_flashes` for why.
#[derive(Debug, Clone, Copy)]
pub struct HitFlash {
    /// Seconds of flash left, starting at `HIT_FLASH_DURATION`.
    pub remaining: f32,
}

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

/// Light source component - emits light in a radius, in a colour, with a
/// flicker of its own.
///
/// `color` and `flicker`/`phase` exist so the dungeon reads as *lit by
/// something* rather than evenly lit: see `src/constants/lighting.rs` for the
/// palette and the flicker shaping, and `Engine::light_sources` for where the
/// flicker is actually evaluated.
#[derive(Debug, Clone, Copy)]
pub struct LightSource {
    /// Radius of light emission (in tiles)
    pub radius: f32,
    /// Light intensity (0.0 to 1.0, multiplied with falloff)
    pub intensity: f32,
    /// Linear RGB tint of the emitted light. `(1.0, 1.0, 1.0)` is the old
    /// untinted white. Read through [`LightSource::color_or_default`], never
    /// directly, so an unset or nonsense colour degrades to white instead of
    /// rendering a black pool.
    pub color: (f32, f32, f32),
    /// How strongly this light flickers, as a multiplier on the global
    /// amplitudes. `1.0` is full firelight, `0.0` is dead steady.
    pub flicker: f32,
    /// Per-light phase offset, in radians, so no two lights flicker in step.
    /// Follows the `FireEffect { seed }` pattern in src/vfx.rs: a cosmetic
    /// seed drawn from the thread rng, deliberately *not* the seeded game rng,
    /// so it cannot desync a seeded run.
    pub phase: f32,
}

/// A fresh cosmetic phase offset for a light, in radians.
///
/// Thread rng rather than the game rng: flicker phase is pure decoration, and
/// drawing from the seeded run rng would make a cosmetic detail part of
/// simulation determinism. src/vfx.rs seeds `FireEffect` the same way.
fn random_light_phase() -> f32 {
    rand::random::<f32>() * std::f32::consts::TAU
}

impl LightSource {
    /// Create a standard campfire light
    pub fn campfire() -> Self {
        Self {
            radius: 8.0,
            intensity: 1.0,
            color: crate::constants::LIGHT_COLOR_FIRE,
            flicker: crate::constants::LIGHT_FLICKER_SCALE_FIRE,
            phase: random_light_phase(),
        }
    }

    /// Create a brazier light (smaller than campfire)
    pub fn brazier() -> Self {
        Self {
            radius: 6.0,
            intensity: 0.95,
            color: crate::constants::LIGHT_COLOR_FIRE,
            flicker: crate::constants::LIGHT_FLICKER_SCALE_FIRE,
            phase: random_light_phase(),
        }
    }

    /// Glowing cave fungus: a soft, close pool of light.
    pub fn mushroom() -> Self {
        Self {
            radius: 3.5,
            intensity: 0.55,
            color: crate::constants::LIGHT_COLOR_FUNGUS,
            flicker: crate::constants::LIGHT_FLICKER_SCALE_FUNGUS,
            phase: random_light_phase(),
        }
    }

    /// Cave crystal cluster: colder and a little further-reaching than fungus.
    pub fn crystal() -> Self {
        Self {
            radius: 4.5,
            intensity: 0.7,
            color: crate::constants::LIGHT_COLOR_CRYSTAL,
            flicker: crate::constants::LIGHT_FLICKER_SCALE_CRYSTAL,
            phase: random_light_phase(),
        }
    }

    /// This light's colour, falling back to
    /// [`crate::constants::LIGHT_COLOR_DEFAULT`] when the stored one is
    /// unusable.
    ///
    /// "Unusable" means a non-finite channel, a negative channel, or all three
    /// at zero - the last of which would render as a pool of pure black rather
    /// than as no light at all. A `LightSource` built with struct literal
    /// syntax and no colour therefore lights white instead of going wrong, in
    /// keeping with the house rule that content-shaped data degrades rather
    /// than panics.
    pub fn color_or_default(&self) -> (f32, f32, f32) {
        let (r, g, b) = self.color;
        let usable = [r, g, b].iter().all(|c| c.is_finite() && *c >= 0.0) && (r + g + b) > 0.0;
        if usable {
            self.color
        } else {
            crate::constants::LIGHT_COLOR_DEFAULT
        }
    }
}

impl Default for LightSource {
    /// A steady white light of brazier reach - the behaviour every light had
    /// before colour and flicker existed.
    fn default() -> Self {
        Self {
            radius: 6.0,
            intensity: 1.0,
            color: crate::constants::LIGHT_COLOR_DEFAULT,
            flicker: 0.0,
            phase: 0.0,
        }
    }
}

/// Marker for a patch of glowing cave fungus. Walkable, sheds light, and
/// burns: `systems::fire` consumes a patch the fire reaches and leaves a
/// grass fire in its place.
#[derive(Debug, Clone, Copy)]
pub struct GlowMushroom;

/// Marker component for entities that cause burning when stepped on
#[derive(Debug, Clone, Copy)]
pub struct CausesBurning;

/// How readily an entity catches fire. Entities without this component never
/// ignite. `flammability` is a 0..1 chance multiplier applied to ignition rolls.
#[derive(Debug, Clone, Copy)]
pub struct Combustible {
    pub flammability: f32,
}

/// A patch of tall grass that is currently on fire. Lives as a short fire entity
/// (with `CausesBurning` + a fire sprite); when `remaining` hits zero it burns
/// out and its tile reverts to floor.
#[derive(Debug, Clone, Copy)]
pub struct BurningGrass {
    pub remaining: f32,
}

/// A puddle of spilled oil on the floor. Walkable, doesn't block vision, and
/// harmless until fire reaches it — then it ignites readily (see
/// `systems::fire`). Water splashes douse the flames but the puddle remains,
/// re-ignitable.
#[derive(Debug, Clone, Copy)]
pub struct OilPuddle;

/// An oil puddle that is currently burning. Paired with `CausesBurning`, a
/// fire sprite, and a light source while alight; when `remaining` hits zero
/// the fuel is spent and the puddle burns away entirely.
#[derive(Debug, Clone, Copy)]
pub struct BurningOil {
    pub remaining: f32,
}

/// Marker for explosive oil barrels. Blocks movement and is highly
/// combustible: once Burning, a short fuse (`BarrelFuse`) starts, then the
/// barrel explodes — damage in a radius plus a spray of burning oil puddles.
/// Destroying one by damage also sets it off.
#[derive(Debug, Clone, Copy)]
pub struct OilBarrel;

/// Lit fuse on an ignited oil barrel. Ticked by `systems::fire`; the barrel
/// explodes when `remaining` reaches zero.
#[derive(Debug, Clone, Copy)]
pub struct BarrelFuse {
    pub remaining: f32,
}

/// Grass tile soaked by a water splash: unignitable until it dries out.
/// Lives as an invisible timer entity on the tile, checked by fire spread.
#[derive(Debug, Clone, Copy)]
pub struct WetGrass {
    pub remaining: f32,
}

/// A standing brazier (dungeon light source). Toppling it (interaction or a
/// knockback into it) snuffs the stand and spills fire onto nearby tiles;
/// `lit` is false once toppled.
#[derive(Debug, Clone, Copy)]
pub struct Brazier {
    pub lit: bool,
}

/// Fire trap component - causes burning when stepped on (but not by owner or their pets)
#[derive(Debug, Clone, Copy)]
pub struct PlacedFireTrap {
    /// The entity that placed this trap
    pub owner: Entity,
    /// Initial burst damage when triggered
    pub burst_damage: i32,
}

// =============================================================================
// GENERALIZED TRAP SYSTEM
// =============================================================================

/// Types of placed traps.
///
/// `Fire` is not constructed yet: it is the unbuilt half of the migration this
/// enum exists for. Fire traps still use the older `PlacedFireTrap` component
/// (live in rendering, AI fire-avoidance and trap triggering); only `Snare` has
/// moved across so far.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrapType {
    /// Fire trap: deals burst damage and applies Burning
    Fire { burst_damage: i32 },
    /// Snare trap: applies Rooted effect
    Snare { root_duration: f32 },
}

/// Generalized trap component (will eventually replace PlacedFireTrap)
#[derive(Debug, Clone, Copy)]
pub struct PlacedTrap {
    /// The entity that placed this trap
    pub owner: Entity,
    /// Type of trap and its parameters
    pub trap_type: TrapType,
}

// =============================================================================
// DUNGEON TRAPS, FURNITURE, AND SECRET DOORS (dungeon-generated discoveries)
// =============================================================================

/// Kinds of hidden floor traps placed by dungeon generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DungeonTrapKind {
    /// Direct damage through `apply_damage`
    Spike,
    /// Small burst + Burning; spills fire onto the tile (ignites grass/oil)
    Fire,
    /// Roots the victim in place for a few seconds
    Snare,
    /// Harmless but loud: wakes every enemy in a wide radius
    Alarm,
}

/// A hidden floor trap placed by dungeon generation. Spawned without a
/// `Sprite` (invisible); when the player detects it (Agility-scaled per-step
/// roll while adjacent — see `systems::discovery`) it gains a tinted trap-door
/// sprite and can be stepped around. Stepping ON it — player or enemy,
/// revealed or not — triggers and consumes it.
#[derive(Debug, Clone, Copy)]
pub struct DungeonTrap {
    pub kind: DungeonTrapKind,
    /// Whether the player has spotted this trap (it renders once revealed)
    pub revealed: bool,
}

/// Kinds of room furniture interactables placed by dungeon generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FurnitureKind {
    /// Drink for a random outcome (heal / food / buff / mild debuff); one use
    Fountain,
    /// Sacrifice an inventory item for a chance at a blessing; reusable
    Altar,
    /// Identifies everything carried + a protection ward; one use
    Shrine,
}

/// Room furniture the player can interact with (bump or Ctrl+direction).
/// See `systems::furniture` for the interaction logic.
#[derive(Debug, Clone, Copy)]
pub struct Furniture {
    pub kind: FurnitureKind,
    /// One-use pieces (fountain, shrine) flip this and go inert/grey
    pub used: bool,
}

/// A sealed doorway hiding a secret room. Renders as a wall and blocks
/// movement + vision like one. Passive discovery when the player is adjacent
/// (Agility-scaled roll, same helper as traps) converts it into a normal
/// openable `Door` with a distinct tint.
#[derive(Debug, Clone, Copy)]
pub struct SecretDoor;

// =============================================================================
// CLASS KIT
// =============================================================================

/// One ability in a [`ClassKit`], with its own cooldown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KitAbility {
    pub ability: AbilityType,
    /// Seconds remaining on cooldown (0 = ready)
    pub cooldown_remaining: f32,
    /// Total cooldown duration
    pub cooldown_total: f32,
}

/// The per-class list of extra abilities, each with an independent cooldown.
///
/// Every class gets one at init (see [`ClassKit::for_class`]); it sits beside
/// the single [`ClassAbility`] / [`SecondaryAbility`] slots rather than
/// replacing them. Replaces the Ranger-only `RangerAbilities` array.
#[derive(Debug, Clone, Default)]
pub struct ClassKit {
    pub abilities: Vec<KitAbility>,
}

impl ClassKit {
    /// Build a kit from (ability, cooldown) pairs, all starting ready.
    pub fn new(entries: &[(AbilityType, f32)]) -> Self {
        Self {
            abilities: entries
                .iter()
                .map(|&(ability, cooldown_total)| KitAbility {
                    ability,
                    cooldown_remaining: 0.0,
                    cooldown_total,
                })
                .collect(),
        }
    }

    /// The starting kit for a class.
    pub fn for_class(class: PlayerClass) -> Self {
        match class {
            PlayerClass::Fighter => Self::new(&[(AbilityType::Guard, GUARD_COOLDOWN)]),
            PlayerClass::Ranger => Self::new(&[
                (AbilityType::Disengage, DISENGAGE_COOLDOWN),
                (AbilityType::Tumble, TUMBLE_COOLDOWN),
                (AbilityType::SnareTrap, SNARE_TRAP_COOLDOWN),
                (AbilityType::CripplingShot, CRIPPLING_SHOT_COOLDOWN),
            ]),
            PlayerClass::Druid => Self::new(&[
                (AbilityType::Thorns, THORNS_COOLDOWN),
                (AbilityType::Entangle, ENTANGLE_COOLDOWN),
            ]),
            PlayerClass::Necromancer => Self::new(&[
                (AbilityType::BoneWard, BONE_WARD_COOLDOWN),
                (AbilityType::Sacrifice, SACRIFICE_COOLDOWN),
                (AbilityType::CorpseExplosion, CORPSE_EXPLOSION_COOLDOWN),
            ]),
        }
    }

    /// The ability at `index`, if any.
    pub fn get(&self, index: usize) -> Option<&KitAbility> {
        self.abilities.get(index)
    }

    /// Index of `ability` in the kit, if the kit holds it.
    pub fn position(&self, ability: AbilityType) -> Option<usize> {
        self.abilities.iter().position(|k| k.ability == ability)
    }

    /// Start the cooldown for the ability at `index` (no-op if out of range).
    pub fn start_cooldown(&mut self, index: usize) {
        if let Some(k) = self.abilities.get_mut(index) {
            k.cooldown_remaining = k.cooldown_total;
        }
    }

    /// Start the cooldown for `ability` (no-op if the kit doesn't hold it).
    pub fn start_cooldown_for(&mut self, ability: AbilityType) {
        if let Some(index) = self.position(ability) {
            self.start_cooldown(index);
        }
    }

    /// Advance every cooldown by `elapsed` game seconds.
    pub fn tick(&mut self, elapsed: f32) {
        for k in self.abilities.iter_mut() {
            if k.cooldown_remaining > 0.0 {
                k.cooldown_remaining = (k.cooldown_remaining - elapsed).max(0.0);
            }
        }
    }
}

/// Charges left on a Necromancer's Bone Ward. Each damaging hit through
/// `combat::apply_damage` spends one and is fully absorbed, while the
/// `EffectType::BoneWard` timer is running. Removed when the last charge goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneWard {
    pub charges: u32,
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

/// Tracks an active Raise Dead channel (Necromancer). Mirrors the Tame
/// channel: progress accrues while the caster Waits in range of the bones.
#[derive(Debug, Clone, Copy)]
pub struct RaiseDeadInProgress {
    /// The bones/corpse container being raised
    pub target: Entity,
    /// Current channel progress in seconds
    pub progress: f32,
    /// Required channel time to complete
    pub required: f32,
}

/// Marks a skeleton companion created by Raise Dead (counts against the
/// caster's INT-scaled control cap while alive).
#[derive(Debug, Clone, Copy)]
pub struct RaisedUndead;

/// Tracks active life drain channeling for the Necromancer
#[derive(Debug, Clone, Copy)]
pub struct LifeDrainInProgress {
    /// The entity being drained
    pub target: Entity,
    /// Time until next tick of damage/healing
    pub tick_timer: f32,
}

/// Marks an animal as tamed
#[derive(Debug, Clone, Copy)]
pub struct TamedBy {
    /// The player who tamed this animal
    pub owner: Entity,
}

/// AI for tamed companions - defensive mode, attacks enemies threatening owner
#[derive(Debug, Clone)]
pub struct CompanionAI {
    /// The player this companion follows
    pub owner: Entity,
    /// Maximum distance before following (Manhattan distance)
    pub follow_distance: i32,
    /// Per-target threat tracking (populated by being attacked or owner combat)
    pub threat_table: Vec<ThreatEntry>,
}

impl CompanionAI {
    /// Add threat for a specific entity. Creates entry if not present.
    pub fn add_threat(&mut self, target: Entity, amount: f32) {
        if let Some(entry) = self.threat_table.iter_mut().find(|e| e.entity == target) {
            entry.threat += amount;
            entry.time_at_minimum = 0.0; // Reset memory timer on new threat
        } else {
            self.threat_table.push(ThreatEntry {
                entity: target,
                threat: amount,
                last_known_pos: None,
                time_at_minimum: 0.0,
            });
        }
    }

    /// Remove a target from the threat table (e.g., on death).
    pub fn remove_target(&mut self, target: Entity) {
        self.threat_table.retain(|e| e.entity != target);
    }
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

// =============================================================================
// ENEMY ROLES: SUPPORT CASTERS, SPIDERS & WEBS, BOSSES
// =============================================================================

/// Support-caster AI (Goblin Shaman): kites its threat target and, on a
/// cooldown, heals the most wounded visible ally — or hastes one attacking
/// the player. Behavior lives in `systems::ai`; the cooldown is ticked by
/// `systems::ai::tick_role_cooldowns`.
#[derive(Debug, Clone, Copy)]
pub struct SupportAI {
    /// Seconds until the next support cast (heal/haste) is available.
    pub cooldown: f32,
}

/// Marker: spider-kin. Spiders never trigger (or consume) webs.
#[derive(Debug, Clone, Copy)]
pub struct Spider;

/// Periodically lays `Web` entities on its own tile while awake
/// (see `systems::webs::try_lay_web`).
#[derive(Debug, Clone, Copy)]
pub struct WebSpinner {
    /// Seconds until the next web can be laid.
    pub cooldown: f32,
    /// Seconds between webs for this spinner (bosses spin faster).
    pub interval: f32,
}

/// A sticky web on the floor. Non-spider entities stepping in are Rooted
/// briefly and the web is consumed. Highly flammable (see `systems::fire`).
#[derive(Debug, Clone, Copy)]
pub struct Web {
    /// The spider that laid it (None for dev-spawned webs); used for the
    /// per-spider live-web cap.
    pub spinner: Option<Entity>,
}

/// Timer on an ignited web: when it expires the web has burnt away.
#[derive(Debug, Clone, Copy)]
pub struct BurningWeb {
    pub remaining: f32,
}

/// The unique ability a boss cycles on its cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossAbility {
    /// Gnash: stun + damage everything near him.
    GroundSlam,
    /// Silkrot: spawn lesser spiders (capped) while webbing constantly.
    SummonSpiders,
    /// Vhal: raise a hostile skeleton from nearby bones.
    RaiseDead,
}

/// A named floor boss: scaled-up enemy with a unique cooldown ability,
/// immune to fear/morale, announced on first sighting, bonus XP on death.
#[derive(Debug, Clone, Copy)]
pub struct Boss {
    pub ability: BossAbility,
    /// Seconds until the ability is ready (ticked by `tick_role_cooldowns`).
    pub cooldown: f32,
    /// Whether the first-sighting message has been shown.
    pub announced: bool,
}

/// A minion summoned by a boss (counts toward its alive-minion cap).
#[derive(Debug, Clone, Copy)]
pub struct BossMinion {
    pub boss: Entity,
}

/// Marker: never panics from morale checks and cannot be Feared (bosses).
#[derive(Debug, Clone, Copy)]
pub struct FearImmune;

/// A venomous melee attacker: successful hits apply Slowed for this long
/// (Giant Spider). Applied directly in the enemy melee path since enemy
/// natural weapons are not item instances with on-hit affixes.
#[derive(Debug, Clone, Copy)]
pub struct Venomous {
    pub slow_duration: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(kind: ItemType, affixes: Vec<Affix>) -> ItemInstance {
        ItemInstance {
            kind,
            rarity: Rarity::Magic,
            affixes,
            name: None,
            identified: true,
            identify_progress: 0.0,
        }
    }

    #[test]
    fn test_total_defense_includes_accessories_and_cursed_fragile() {
        let mut eq = Equipment::empty();
        eq.body = Some(instance(ItemType::LeatherArmor, vec![Affix::Defense(2)]));
        eq.ring = Some(instance(ItemType::Ring, vec![Affix::Defense(1)]));
        eq.amulet = Some(instance(ItemType::Amulet, vec![Affix::CursedFragile(2)]));

        // Leather base 2 + 2 affix + 1 ring - 2 cursed amulet = 3
        assert_eq!(eq.total_defense(), crate::constants::LEATHER_ARMOR_DEFENSE + 2 + 1 - 2);
    }

    #[test]
    fn test_cursed_heavy_and_loud_aggregate_across_worn_gear() {
        let mut eq = Equipment::empty();
        assert_eq!(eq.cursed_heavy_total(), 0.0);
        assert!(!eq.has_cursed_loud());

        eq.weapon_source = Some(instance(ItemType::Sword, vec![Affix::CursedHeavy(0.2)]));
        eq.ring = Some(instance(ItemType::Ring, vec![Affix::CursedHeavy(0.1)]));
        eq.amulet = Some(instance(ItemType::Amulet, vec![Affix::CursedLoud]));

        assert!((eq.cursed_heavy_total() - 0.3).abs() < 0.0001);
        assert!(eq.has_cursed_loud());
    }

    #[test]
    fn test_affix_damage_bonus_includes_accessories() {
        let mut eq = Equipment::empty();
        eq.weapon_source = Some(instance(ItemType::Sword, vec![Affix::Damage(2)]));
        eq.ring = Some(instance(ItemType::Ring, vec![Affix::Damage(1)]));
        eq.amulet = Some(instance(ItemType::Amulet, vec![Affix::Damage(3)]));
        assert_eq!(eq.affix_damage_bonus(), 6);
    }
}

#[cfg(test)]
mod light_source_tests {
    use super::*;

    #[test]
    fn constructors_carry_their_palette_colour() {
        assert_eq!(LightSource::campfire().color, LIGHT_COLOR_FIRE);
        assert_eq!(LightSource::brazier().color, LIGHT_COLOR_FIRE);
        assert_eq!(LightSource::mushroom().color, LIGHT_COLOR_FUNGUS);
        assert_eq!(LightSource::crystal().color, LIGHT_COLOR_CRYSTAL);
    }

    /// Fire gutters, crystal barely moves. If these ever equalise the whole
    /// point of per-type flicker is gone.
    #[test]
    fn fire_flickers_harder_than_crystal() {
        assert!(LightSource::brazier().flicker > LightSource::mushroom().flicker);
        assert!(LightSource::mushroom().flicker > LightSource::crystal().flicker);
    }

    /// Phase seeds come from the thread rng, so two lights spawned back to
    /// back should not share a phase. (Vanishingly unlikely to collide; this
    /// is really guarding against the field being left at a constant.)
    #[test]
    fn each_light_gets_its_own_phase() {
        let phases: Vec<f32> = (0..16).map(|_| LightSource::brazier().phase).collect();
        let distinct = phases.iter().filter(|p| **p != phases[0]).count();
        assert!(distinct > 10, "phases barely varied: {phases:?}");
        for p in phases {
            assert!(p.is_finite() && (0.0..=std::f32::consts::TAU).contains(&p));
        }
    }

    /// A light whose colour was never set, or was set to something unusable,
    /// has to degrade to white rather than panic or render a black pool.
    #[test]
    fn unusable_colours_fall_back_to_white() {
        let unusable = [
            (0.0, 0.0, 0.0),
            (f32::NAN, 1.0, 1.0),
            (1.0, f32::INFINITY, 1.0),
            (-1.0, 0.5, 0.5),
        ];
        for color in unusable {
            let light = LightSource { color, ..LightSource::default() };
            assert_eq!(
                light.color_or_default(),
                LIGHT_COLOR_DEFAULT,
                "{color:?} should have degraded to white"
            );
        }
    }

    #[test]
    fn usable_colours_pass_straight_through() {
        for color in [LIGHT_COLOR_FIRE, LIGHT_COLOR_FUNGUS, LIGHT_COLOR_CRYSTAL] {
            let light = LightSource { color, ..LightSource::default() };
            assert_eq!(light.color_or_default(), color);
        }
    }

    /// The default is the pre-colour behaviour: steady and white.
    #[test]
    fn default_light_is_steady_and_white() {
        let light = LightSource::default();
        assert_eq!(light.color_or_default(), LIGHT_COLOR_DEFAULT);
        assert_eq!(light.flicker, 0.0);
    }
}
