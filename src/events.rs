//! Game event system for decoupled communication between systems.
//!
//! Actions and systems emit events, other systems consume them.
//! This allows VFX, audio, UI, etc. to react without tight coupling.

use hecs::Entity;

/// Direction of floor transition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairDirection {
    Up,
    Down,
}

/// How a hit was delivered, so the message log can describe it specifically
/// ("with your dagger", "your cleave", "the Skeleton Archer's arrow", ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageKind {
    /// Basic melee swing with the attacker's equipped weapon.
    Melee,
    /// Fighter's Cleave ability.
    Cleave,
    /// Fireball scroll explosion.
    Fireball,
    /// A fired arrow (bow shot).
    Arrow,
    /// Ranger's Crippling Shot (a slowing arrow).
    CripplingShot,
    /// A thrown potion impact (splash, no direct damage).
    Potion,
    /// A boss ground slam (Gnash's shockwave).
    Slam,
    /// An orc's charge connecting at the end of its dash.
    Charge,
    /// Druid's Thorns biting back at a melee attacker. The "attacker" of the
    /// hit is the thorny defender.
    Thorns,
    /// Necromancer's Corpse Explosion blast.
    CorpseExplosion,
    /// A Poisoned damage-over-time tick (never in `AttackHit`; see `DotDamage`).
    Poison,
    /// A Bleeding damage-over-time tick (never in `AttackHit`; see `DotDamage`).
    Bleed,
}

/// Why a locked-target melee attack (`ActionType::Attack`) failed to connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissReason {
    /// The target is still there but stepped out of reach (`MELEE_REACH`)
    /// before the swing landed.
    OutOfReach,
    /// The target died, despawned or stopped being attackable mid-swing.
    TargetGone,
}

/// Game events that systems can emit and subscribe to.
/// Many event fields exist for future handlers (VFX, audio, logging).
#[derive(Debug, Clone)]
#[allow(dead_code)] // Event fields reserved for future handlers
pub enum GameEvent {
    /// An entity moved to a new position
    EntityMoved {
        entity: Entity,
        from: (i32, i32),
        to: (i32, i32),
    },
    /// An entity attacked another entity
    AttackHit {
        attacker: Entity,
        target: Entity,
        target_pos: (f32, f32),
        damage: i32,
        /// How the hit was delivered (melee weapon, cleave, fireball, ...).
        kind: DamageKind,
        /// Whether this was a critical hit.
        crit: bool,
        /// The target was flanked: a creature hostile to it stood on the far
        /// side from the attacker, and the hit took `FLANK_DAMAGE_MULT`.
        flanked: bool,
        /// The hit left the target dead (hit-stop, kill feedback).
        killed: bool,
    },
    /// A melee attack locked onto `target` landed on nothing: by the time
    /// the swing completed the target had moved out of reach or was gone. No
    /// damage was dealt.
    AttackMissed {
        attacker: Entity,
        target: Entity,
        /// Tile centre the target occupies now, if it still has a position
        /// (for the floating "miss" text).
        target_pos: Option<(f32, f32)>,
        reason: MissReason,
    },
    /// A melee blow landed on a Guarding defender: most of it was blocked
    /// and the attacker was staggered. An `AttackHit` for the reduced damage
    /// follows.
    AttackBlocked {
        attacker: Entity,
        defender: Entity,
        /// Tile centre of the defender (for the floating "BLOCK" text).
        defender_pos: (f32, f32),
    },
    /// A Bone Ward went up with this many charges.
    BoneWardRaised {
        entity: Entity,
        charges: u32,
    },
    /// A Bone Ward absorbed a whole hit; `charges_left` is 0 when it shatters.
    BoneWardAbsorbed {
        entity: Entity,
        charges_left: u32,
        /// Tile centre of the warded entity.
        position: (f32, f32),
    },
    /// A necromancer swapped places with one of their raised skeletons.
    SacrificeSwapped {
        caster: Entity,
        skeleton: Entity,
    },
    /// A corpse was detonated. `hits` is how many hostiles the blast struck.
    /// VFX/audio/shake ride on the `FireballExplosion` emitted alongside.
    CorpseExploded {
        caster: Entity,
        position: (i32, i32),
        hits: u32,
    },
    /// Entangle took hold around `position`; `rooted` hostiles were caught.
    /// `tiles` is the patch, for the vine-burst VFX.
    EntangleCast {
        caster: Entity,
        position: (i32, i32),
        rooted: u32,
        tiles: Vec<(i32, i32)>,
    },
    /// An entity died
    EntityDied {
        entity: Entity,
        position: (f32, f32),
    },
    /// An entity opened a door
    DoorOpened {
        door: Entity,
        opener: Entity,
        position: (i32, i32),
    },
    /// An entity closed a door
    DoorClosed {
        door: Entity,
        closer: Entity,
        position: (i32, i32),
    },
    /// An entity opened a container (chest, bones, etc.)
    ContainerOpened {
        container: Entity,
        opener: Entity,
        container_type: Option<crate::components::ContainerType>,
        position: (i32, i32),
    },
    /// An entity picked up an item, or a stack of one kind of item
    ItemPickedUp {
        entity: Entity,
        item: crate::components::ItemType,
        /// How many were picked up at once (a bundle of arrows is one event)
        count: u32,
    },
    /// An entity picked up gold
    GoldPickedUp {
        entity: Entity,
        amount: u32,
    },
    /// An entity regenerated health
    HealthRegenerated {
        entity: Entity,
        amount: i32,
    },
    /// Player leveled up
    LevelUp {
        new_level: u32,
    },
    /// AI state changed (for debugging/UI feedback)
    AIStateChanged {
        entity: Entity,
        new_state: crate::components::AIState,
    },
    /// An enemy raised an alarm shout (for VFX bubble / audio)
    EnemyShout {
        entity: Entity,
        position: (i32, i32),
    },
    /// A projectile was spawned
    ProjectileSpawned {
        projectile: Entity,
        source: Entity,
    },
    /// A projectile hit something
    ProjectileHit {
        projectile: Entity,
        /// The entity that fired the projectile.
        source: Entity,
        target: Option<Entity>,
        position: (i32, i32),
        damage: i32,
        /// What kind of projectile this was (arrow, crippling shot, potion).
        kind: DamageKind,
    },
    /// Player used stairs to change floors
    FloorTransition {
        direction: StairDirection,
        from_floor: u32,
    },
    /// Player initiated dialogue with an NPC
    DialogueStarted {
        npc: Entity,
        player: Entity,
    },
    /// A fireball exploded at a location
    FireballExplosion {
        x: i32,
        y: i32,
        radius: i32,
    },
    /// A potion splashed at a location
    PotionSplash {
        x: i32,
        y: i32,
        potion_type: crate::components::ItemType,
    },
    /// An entity dropped an item on the ground
    ItemDropped {
        entity: Entity,
        item: crate::components::ItemType,
        position: (i32, i32),
    },
    /// A cleave attack was performed (fighter ability)
    CleavePerformed {
        center: (i32, i32),
    },
    /// A skeleton should spawn from a coffin at this position
    CoffinSkeletonSpawn {
        position: (i32, i32),
    },
    /// Taming has started
    TamingStarted {
        tamer: Entity,
        target: Entity,
    },
    /// Taming progress updated
    TamingProgress {
        tamer: Entity,
        target: Entity,
        progress: f32,
        required: f32,
    },
    /// Taming completed successfully
    TamingCompleted {
        tamer: Entity,
        target: Entity,
    },
    /// Taming failed (too far away)
    TamingFailed {
        tamer: Entity,
        target: Entity,
    },
    /// Barkskin ability activated (druid)
    BarkskinActivated {
        entity: Entity,
    },
    /// Fear ability activated (necromancer)
    FearActivated {
        entity: Entity,
        position: (i32, i32),
    },
    /// Stun ability activated (fighter)
    StunActivated {
        entity: Entity,
        position: (i32, i32),
    },
    /// Player opened a shop with a vendor
    ShopOpened {
        vendor: Entity,
        player: Entity,
    },
    /// Player purchased an item from a vendor
    ItemPurchased {
        vendor: Entity,
        item: crate::components::ItemType,
        price: u32,
    },
    /// Player sold an item to a vendor
    ItemSold {
        vendor: Entity,
        item: crate::components::ItemType,
        value: u32,
    },
    /// A carried item finished identifying (see systems::identify)
    ItemIdentified {
        /// Full display name now that it's known (e.g. a Legendary name)
        name: String,
        /// Whether the item turned out to carry a curse affix
        cursed: bool,
    },
    /// A damage-over-time status (Poisoned / Bleeding) ticked. `kind` is
    /// `DamageKind::Poison` or `DamageKind::Bleed`.
    DotDamage {
        entity: Entity,
        position: (f32, f32),
        damage: i32,
        kind: DamageKind,
    },
    /// An entity newly gained a status effect that the player should hear
    /// about (Wet, Oiled, Poisoned, Bleeding). Refreshes are not announced.
    StatusEffectGained {
        entity: Entity,
        effect: crate::components::EffectType,
    },
    /// A Druid called down rain over `position`. `tiles` is the soaked patch,
    /// for the splash VFX; `doused` counts fires put out (creatures, grass,
    /// oil, webs).
    RainCalled {
        caster: Entity,
        position: (i32, i32),
        tiles: Vec<(i32, i32)>,
        doused: u32,
    },
    /// An entity took burn damage from being on fire
    BurnDamage {
        entity: Entity,
        position: (f32, f32),
        damage: i32,
    },
    /// An entity caught on fire
    CaughtFire {
        entity: Entity,
        position: (i32, i32),
    },
    /// An oil barrel exploded (damage + burning oil spray). VFX/audio reuse
    /// the FireballExplosion event emitted alongside this one.
    BarrelExploded {
        position: (i32, i32),
    },
    /// A brazier was toppled (by interaction or knockback), spilling fire.
    BrazierToppled {
        position: (i32, i32),
    },
    /// The player filled an empty water flask from a water tile.
    FlaskFilled {
        entity: Entity,
    },
    /// The player tried to fill a flask with no water in reach.
    FlaskFillFailed {
        entity: Entity,
    },
    /// A fire trap was placed
    FireTrapPlaced {
        trap: Entity,
        placer: Entity,
        position: (i32, i32),
    },
    /// A fire trap was triggered
    FireTrapTriggered {
        trap: Entity,
        victim: Entity,
        position: (i32, i32),
    },
    /// A snare trap was triggered
    SnareTrapTriggered {
        trap: Entity,
        victim: Entity,
        position: (i32, i32),
    },
    /// Life drain channeling started (necromancer)
    LifeDrainStarted {
        caster: Entity,
        target: Entity,
    },
    /// Life drain tick (damage dealt, health restored)
    LifeDrainTick {
        caster: Entity,
        target: Entity,
        caster_pos: (f32, f32),
        target_pos: (f32, f32),
        damage: i32,
        healed: i32,
    },
    /// Life drain ended (target died or out of range)
    LifeDrainEnded {
        caster: Entity,
        target: Entity,
    },
    /// Life drain was interrupted (caster took damage)
    LifeDrainInterrupted {
        caster: Entity,
        target: Entity,
    },
    /// A potion was drunk (consumed directly, not thrown)
    PotionDrunk {
        entity: Entity,
        potion_type: crate::components::ItemType,
    },
    /// A weapon was equipped
    WeaponEquipped {
        entity: Entity,
        weapon_type: crate::components::ItemType,
    },
    /// A utility ability was activated that has no other dedicated event
    /// (Sprint, Disengage, Tumble, Snare Trap). Used by the message log.
    AbilityActivated {
        entity: Entity,
        ability: crate::components::AbilityType,
    },
    /// The player's hunger crossed into a new coarse state (one-shot per
    /// crossing; the message log turns these into warnings).
    HungerStateChanged {
        state: crate::components::HungerState,
    },
    /// The player's fatigue crossed into a new coarse state.
    FatigueStateChanged {
        state: crate::components::FatigueState,
    },
    /// The player took starvation damage (hunger at zero). Applied directly
    /// to Health — armor and protection do not reduce it.
    StarvationDamage {
        entity: Entity,
        position: (f32, f32),
        damage: i32,
    },
    /// The player studied a scroll and permanently learned its spell.
    SpellLearned {
        ability: crate::components::AbilityType,
    },
    /// The player tried to study a scroll without enough Intelligence.
    SpellStudyFailed {
        scroll: crate::components::ItemType,
        required_int: i32,
        current_int: i32,
    },
    /// The player tried to study a scroll whose spell is already known.
    SpellAlreadyKnown {
        ability: crate::components::AbilityType,
    },
    /// Raise Dead channel started on a bones pile.
    RaiseDeadStarted {
        caster: Entity,
        target: Entity,
    },
    /// Raise Dead channel failed (moved / out of range / bones gone).
    RaiseDeadFailed {
        caster: Entity,
    },
    /// Raise Dead completed: spawn a skeleton companion at this position.
    /// Handled by the engine (needs the scheduler), like CoffinSkeletonSpawn.
    SkeletonRaised {
        owner: Entity,
        position: (i32, i32),
    },
    /// The player spotted a hidden dungeon trap (it now renders).
    TrapSpotted {
        position: (i32, i32),
    },
    /// A dungeon-generated floor trap was triggered (and consumed).
    DungeonTrapTriggered {
        kind: crate::components::DungeonTrapKind,
        victim: Entity,
        position: (i32, i32),
        /// Damage dealt (spike/fire traps; 0 for snare/alarm)
        damage: i32,
    },
    /// The player noticed a hidden passage (secret door became a real door).
    SecretDoorFound {
        position: (i32, i32),
    },
    /// The player drank from a fountain (or found it dry).
    FountainUsed {
        entity: Entity,
        outcome: FountainOutcome,
    },
    /// The player interacted with an altar: open the sacrifice window.
    AltarOpened {
        altar: Entity,
        player: Entity,
    },
    /// The player sacrificed an item at an altar.
    AltarSacrificed {
        item_name: String,
        blessed: bool,
        /// Human-readable outcome ("Your Strength increases!", ...)
        detail: String,
    },
    /// The player touched a shrine. `fresh` is false if it was already spent.
    ShrineUsed {
        entity: Entity,
        fresh: bool,
    },
    /// A support caster (Goblin Shaman) healed an ally. Only emitted when the
    /// target tile is visible to the player.
    EnemyHealed {
        healer: Entity,
        target: Entity,
        amount: i32,
        position: (i32, i32),
    },
    /// A support caster hasted an ally attacking the player. Only emitted
    /// when the target tile is visible to the player.
    EnemyHasted {
        healer: Entity,
        target: Entity,
        position: (i32, i32),
    },
    /// A zombie's hit took hold: `target` is Grabbed (cannot walk) for a
    /// moment.
    Grabbed {
        grabber: Entity,
        target: Entity,
    },
    /// A grab failed to take hold because the target was slippery (Oiled).
    GrabSlipped {
        grabber: Entity,
        target: Entity,
    },
    /// `entity` is no longer held: the grab timed out, or its holder died,
    /// was stunned or is no longer adjacent.
    GrabReleased {
        entity: Entity,
    },
    /// `entity` tried to walk while Grabbed and spent the step struggling.
    GrabStruggle {
        entity: Entity,
    },
    /// `entity` tried to walk while Rooted and spent the step struggling.
    /// Said once per root application, not on every attempt
    /// (`grab::pinned_in_place`).
    RootStruggle {
        entity: Entity,
    },
    /// An orc lowered its head to charge down a lane (a unit step `dir`);
    /// the dash comes when the wind-up completes. Only emitted when the orc
    /// stands on a tile the player can see.
    ChargeWindup {
        attacker: Entity,
        position: (i32, i32),
        dir: (i32, i32),
    },
    /// A charge that hit nobody ended (a hit is reported as an `AttackHit`
    /// of kind `Charge`). Only emitted when the end tile is visible.
    ChargeMissed {
        attacker: Entity,
        position: (i32, i32),
        outcome: ChargeOutcome,
    },
    /// A badly hurt slime split in two: `child` appeared at `position`.
    SlimeSplit {
        parent: Entity,
        child: Entity,
        position: (i32, i32),
    },
    /// A non-spider entity blundered into a web (Rooted; web consumed).
    WebTouched {
        victim: Entity,
        position: (i32, i32),
    },
    /// The player laid eyes on a floor boss for the first time.
    BossSighted {
        boss: Entity,
        name: String,
    },
    /// A floor boss died (milestone message + bonus XP already granted).
    BossDefeated {
        name: String,
    },
    /// A boss used its unique ability (message log / VFX flourish).
    BossAbilityUsed {
        boss: Entity,
        ability: crate::components::BossAbility,
        position: (i32, i32),
    },
    /// A boss started winding up its unique ability; it lands when the
    /// wind-up action completes (message log warning).
    BossAbilityWindup {
        boss: Entity,
        ability: crate::components::BossAbility,
        position: (i32, i32),
    },
    /// A boss summoned a minion: spawn it at this position (handled by the
    /// engine like CoffinSkeletonSpawn, since spawning needs the scheduler).
    BossMinionSpawn {
        boss: Entity,
        position: (i32, i32),
    },
}

/// How a charge that hit nobody ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeOutcome {
    /// Ran into a wall or furniture: stunned for `ORC_WALL_STUN`.
    Wall,
    /// Ran its full length, or was stopped by one of its own: stunned for
    /// `ORC_STUMBLE_DURATION`.
    Stumble,
}

/// What drinking from a fountain did (for the message log / VFX).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FountainOutcome {
    /// Fully healed
    Heal,
    /// Hunger fully restored
    Food,
    /// Random beneficial effect
    Buff(crate::components::EffectType),
    /// Mild debuff (Confused or Slowed)
    Bad(crate::components::EffectType),
    /// The fountain was already used up
    Dry,
}

/// Simple event queue - events are pushed during update, processed at end of frame
#[derive(Default)]
pub struct EventQueue {
    events: Vec<GameEvent>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Push an event to be processed later
    pub fn push(&mut self, event: GameEvent) {
        self.events.push(event);
    }

    /// Drain all events for processing
    pub fn drain(&mut self) -> impl Iterator<Item = GameEvent> + '_ {
        self.events.drain(..)
    }
}
