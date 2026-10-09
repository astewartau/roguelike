//! Continuous event-driven time system.
//!
//! Manages game time progression through an event-driven loop where time
//! jumps forward to the next action completion rather than ticking.


use crate::components::{
    ActionInProgress, ActionType, Actor, EffectType, Health, RangedCooldown, StatusEffects,
};
use crate::constants::*;
use crate::engine::{ActorCtx, EffectCtx};
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::systems::action_dispatch;
use crate::systems::actions::{self, ActionResult};
use crate::systems::effects;
use hecs::{Entity, World};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

// =============================================================================
// GAME CLOCK
// =============================================================================

/// Global game time clock (in seconds)
#[derive(Debug, Clone)]
pub struct GameClock {
    /// Current game time in seconds (simulation time, not real time)
    pub time: f32,
}

impl GameClock {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    /// Advance time to the given timestamp
    pub fn advance_to(&mut self, time: f32) {
        debug_assert!(
            time >= self.time,
            "Cannot go backwards in time: {} -> {}",
            self.time,
            time
        );
        self.time = time;
    }
}

impl Default for GameClock {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ACTION SCHEDULER
// =============================================================================

/// A scheduled action completion event
#[derive(Debug, Clone, Copy)]
struct ScheduledCompletion {
    entity: Entity,
    completion_time: f32,
}

impl PartialEq for ScheduledCompletion {
    fn eq(&self, other: &Self) -> bool {
        self.completion_time == other.completion_time && self.entity == other.entity
    }
}

impl Eq for ScheduledCompletion {}

impl PartialOrd for ScheduledCompletion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledCompletion {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior (earliest time first)
        other
            .completion_time
            .partial_cmp(&self.completion_time)
            .unwrap_or(Ordering::Equal)
    }
}

/// Manages the event-driven time loop
#[derive(Debug, Clone)]
pub struct ActionScheduler {
    /// Entities with pending action completions, ordered by completion time (min-heap)
    pending_completions: BinaryHeap<ScheduledCompletion>,
}

impl ActionScheduler {
    pub fn new() -> Self {
        Self {
            pending_completions: BinaryHeap::new(),
        }
    }

    /// Schedule an action completion for an entity
    pub fn schedule(&mut self, entity: Entity, completion_time: f32) {
        self.pending_completions.push(ScheduledCompletion {
            entity,
            completion_time,
        });
    }

    /// Get the next completion (earliest), if any
    #[allow(dead_code)] // Public API for debugging/inspection
    pub fn peek_next(&self) -> Option<(Entity, f32)> {
        self.pending_completions
            .peek()
            .map(|sc| (sc.entity, sc.completion_time))
    }

    /// Pop the next completion (earliest)
    pub fn pop_next(&mut self) -> Option<(Entity, f32)> {
        self.pending_completions
            .pop()
            .map(|sc| (sc.entity, sc.completion_time))
    }

    /// Remove all completions for a specific entity (e.g., on death)
    pub fn cancel_for_entity(&mut self, entity: Entity) {
        // Rebuild the heap without the cancelled entity
        let remaining: Vec<_> = self
            .pending_completions
            .drain()
            .filter(|sc| sc.entity != entity)
            .collect();
        self.pending_completions = remaining.into_iter().collect();
    }

    /// Check if there are any pending completions
    #[allow(dead_code)] // Public API for debugging/inspection
    pub fn is_empty(&self) -> bool {
        self.pending_completions.is_empty()
    }
}

impl Default for ActionScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ACTION STARTING
// =============================================================================

/// Start an action for an entity. Returns Ok(()) if successful.
pub fn start_action(
    world: &mut World,
    entity: Entity,
    action_type: ActionType,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
) -> Result<(), &'static str> {
    start_action_with_events(world, entity, action_type, clock, scheduler, None)
}

/// Start an action for an entity with optional event emission. Returns Ok(()) if successful.
pub fn start_action_with_events(
    world: &mut World,
    entity: Entity,
    action_type: ActionType,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: Option<&mut EventQueue>,
) -> Result<(), &'static str> {
    // Check for speed-modifying effects before borrowing Actor
    let has_speed_boost = effects::entity_has_effect(world, entity, EffectType::SpeedBoost);
    let has_slow = effects::entity_has_effect(world, entity, EffectType::Slowed);
    let is_sneaking = world.get::<&crate::components::Sneaking>(entity).is_ok();
    // CursedHeavy gear slows all actions (like Slowed, but from worn items).
    // Applies whether or not the curse has been identified.
    let cursed_heavy = world
        .get::<&crate::components::Equipment>(entity)
        .map(|e| e.cursed_heavy_total())
        .unwrap_or(0.0);
    // Exhausted (fatigue meter maxed, player-only): every action is slower.
    let exhausted = world
        .get::<&crate::components::Fatigue>(entity)
        .map(|f| f.is_exhausted())
        .unwrap_or(false);

    // Get actor component
    let mut actor = world
        .get::<&mut Actor>(entity)
        .map_err(|_| "Entity has no Actor component")?;

    if actor.current_action.is_some() {
        return Err("Entity is busy with another action");
    }

    // Calculate effective speed (base speed modified by effects)
    let mut effective_speed = if has_speed_boost {
        actor.speed * SPEED_BOOST_MULTIPLIER
    } else if has_slow {
        actor.speed * SLOW_MULTIPLIER
    } else {
        actor.speed
    };
    // Sneaking trades movement speed for stealth (only movement is slowed, so a
    // sneak attack itself isn't penalized).
    if is_sneaking && matches!(action_type, ActionType::Move { .. }) {
        effective_speed *= SNEAK_SPEED_MULT;
    }
    // Cursed-heavy gear: a total of 0.2 means all actions take 20% longer.
    if cursed_heavy > 0.0 {
        effective_speed /= 1.0 + cursed_heavy;
    }
    // Exhaustion drags every action out (0.75x speed).
    if exhausted {
        effective_speed *= EXHAUSTED_SPEED_MULT;
    }

    // Calculate completion time
    let duration = action_dispatch::calculate_action_duration(&action_type, effective_speed);
    let completion_time = clock.time + duration;

    // How tiring this action was. Nothing is spent and nothing is gated on it —
    // it only feeds the long-term fatigue meter below.
    let effort = action_type.effort_for_duration(duration);

    // Record action in progress
    actor.current_action = Some(ActionInProgress {
        action_type,
        start_time: clock.time,
        completion_time,
    });

    // Release the actor borrow before touching any other component.
    drop(actor);

    // Effort accumulates into fatigue, which is the only lasting cost of doing
    // things. It is deliberately a slow meter: a fight should tire you over the
    // course of a run, not throttle you in the middle of one. Entities without a
    // Fatigue meter (currently everything but the player) simply skip this — the
    // mechanism works unchanged if one is added.
    if effort > 0.0 {
        if let Ok(mut fatigue) = world.get::<&mut crate::components::Fatigue>(entity) {
            fatigue.value = (fatigue.value + effort * FATIGUE_PER_EFFORT).min(FATIGUE_MAX);
        }
    }

    // Schedule completion
    scheduler.schedule(entity, completion_time);

    let _ = events;
    Ok(())
}

/// Start an action and immediately apply its start-of-action effects.
///
/// Actions normally do everything at completion. Reactive abilities (Guard,
/// Bone Ward, Sacrifice) cannot: the blow they answer would land first. This
/// is the one place their effects go up, right after the action is scheduled,
/// via `systems::actions::apply_action_start_effects` (a no-op for every other
/// action, so any player-initiated action can come through here).
pub fn start_action_with_start_effects(
    ctx: &mut ActorCtx,
    entity: Entity,
    action_type: ActionType,
) -> Result<(), &'static str> {
    start_action(ctx.world, entity, action_type, ctx.clock, ctx.scheduler)?;
    actions::apply_action_start_effects(&mut ctx.effects(), entity, &action_type);
    Ok(())
}

// =============================================================================
// ACTION COMPLETION
// =============================================================================

/// Complete an action for an entity, applying its effects. Effects are applied
/// at `ctx.clock.time`, which is the completion time the caller just advanced to.
pub fn complete_action(ctx: &mut ActorCtx, entity: Entity) -> ActionResult {
    // Get the action to complete
    let action = {
        let Ok(actor) = ctx.world.get::<&Actor>(entity) else {
            return ActionResult::Invalid;
        };
        match actor.current_action {
            Some(action) => action,
            None => return ActionResult::Invalid,
        }
    };

    // An actor killed earlier in this same advance (by a companion, a burn
    // tick, ...) is not turned into bones until `remove_dead_entities` runs at
    // the end of the frame, so its completion is still queued. A dead actor's
    // action does nothing: in particular a swing it started cannot land.
    let dead = ctx
        .world
        .get::<&Health>(entity)
        .map(|h| h.current <= 0)
        .unwrap_or(false);
    if dead {
        if let Ok(mut actor) = ctx.world.get::<&mut Actor>(entity) {
            actor.current_action = None;
        }
        return ActionResult::Invalid;
    }

    // Check if this is a bow shot that needs recovery follow-up
    let needs_recovery = matches!(
        action.action_type,
        ActionType::ShootBow { .. } | ActionType::ShootCripplingShot { .. }
    );

    // Apply action effects
    let now = ctx.clock.time;
    let result = apply_action_effects(&mut ctx.effects(), entity, &action.action_type, now);

    // Clear action (energy regen is now time-based, not action-based)
    if let Ok(mut actor) = ctx.world.get::<&mut Actor>(entity) {
        actor.current_action = None;
    }

    // Auto-queue recovery action after bow shots
    if needs_recovery && matches!(result, ActionResult::Completed) {
        let _ = start_action(ctx.world, entity, ActionType::Recover, ctx.clock, ctx.scheduler);
    }

    result
}

/// Rebuild an `EffectCtx` from the unpacked locals, so the dispatch arms below
/// stay one line each.
fn effects<'a>(
    world: &'a mut World,
    grid: &'a mut Grid,
    spatial: &'a mut SpatialCache,
    events: &'a mut EventQueue,
    rng: &'a mut rand::rngs::StdRng,
) -> EffectCtx<'a> {
    EffectCtx { world, grid, spatial, events, rng }
}

/// Apply the effects of a completed action.
/// Dispatches to the appropriate action implementation in systems::actions.
fn apply_action_effects(
    ctx: &mut EffectCtx,
    entity: Entity,
    action_type: &ActionType,
    current_time: f32,
) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);
    match action_type {
        ActionType::Move { dx, dy, .. } => actions::apply_move(&mut effects(world, grid, spatial_cache, events, rng), entity, *dx, *dy),
        ActionType::Attack { target } => {
            actions::apply_attack(
                &mut effects(world, grid, spatial_cache, events, rng), entity, *target,
            )
        }
        ActionType::AttackDirection { dx, dy } => {
            actions::apply_attack_direction(
                &mut effects(world, grid, spatial_cache, events, rng), entity, *dx, *dy,
            )
        }
        ActionType::InteractDirection { dx, dy } => {
            actions::apply_interact_direction(
                &mut effects(world, grid, spatial_cache, events, rng), entity, *dx, *dy,
            )
        }
        ActionType::OpenDoor { door } => actions::apply_open_door(world, entity, *door, events),
        ActionType::OpenChest { chest } => actions::apply_open_chest(world, entity, *chest, events),
        ActionType::Wait => {
            actions::apply_wait(&mut effects(world, grid, spatial_cache, events, rng), entity)
        }
        ActionType::ShootBow { target_x, target_y } => {
            actions::apply_shoot_bow(world, grid, entity, *target_x, *target_y, events, current_time)
        }
        ActionType::UseStairs { x, y, direction } => {
            actions::apply_use_stairs(world, spatial_cache, entity, *x, *y, *direction, events)
        }
        ActionType::TalkTo { npc } => actions::apply_talk_to(entity, *npc, events),
        ActionType::ThrowPotion { potion_type, target_x, target_y } => {
            actions::apply_throw_potion(world, entity, *potion_type, *target_x, *target_y, events, current_time)
        }
        ActionType::Blink { target_x, target_y } => {
            actions::apply_blink(world, grid, entity, *target_x, *target_y, spatial_cache, events, rng)
        }
        ActionType::CastFireball { target_x, target_y } => {
            actions::apply_fireball(world, entity, *target_x, *target_y, events, rng)
        }
        ActionType::EquipWeapon { item_index } => {
            actions::apply_equip_weapon(world, entity, *item_index)
        }
        ActionType::UnequipWeapon => {
            actions::apply_unequip_weapon(world, entity)
        }
        ActionType::DropItem { item_index } => {
            actions::apply_drop_item(world, entity, *item_index, events)
        }
        ActionType::DropEquippedWeapon => {
            actions::apply_drop_equipped_weapon(world, entity, events)
        }
        ActionType::Cleave => {
            actions::apply_cleave(&mut effects(world, grid, spatial_cache, events, rng), entity)
        }
        ActionType::ActivateSprint => {
            actions::apply_activate_sprint(world, entity, events)
        }
        ActionType::StartTaming { target } => {
            actions::apply_start_taming(world, entity, *target, events)
        }
        ActionType::ActivateBarkskin => {
            actions::apply_activate_barkskin(world, entity, events)
        }
        ActionType::StartLifeDrain { target } => {
            actions::apply_start_life_drain(world, entity, *target, events)
        }
        ActionType::ActivateFear => {
            actions::apply_activate_fear(world, entity, events)
        }
        ActionType::ActivateStun => {
            actions::apply_activate_stun(world, entity, events)
        }
        ActionType::PlaceFireTrap { target_x, target_y } => {
            actions::apply_place_fire_trap(world, entity, *target_x, *target_y, events)
        }
        ActionType::Disengage => {
            actions::apply_disengage(world, grid, entity, spatial_cache, events, rng)
        }
        ActionType::Tumble { target_x, target_y } => {
            actions::apply_tumble(world, grid, entity, *target_x, *target_y, spatial_cache, events, rng)
        }
        ActionType::PlaceSnareTrap { target_x, target_y } => {
            actions::apply_place_snare_trap(world, entity, *target_x, *target_y, events)
        }
        ActionType::ShootCripplingShot { target_x, target_y } => {
            actions::apply_shoot_crippling_shot(world, grid, entity, *target_x, *target_y, events, current_time)
        }
        ActionType::CastLearnedSpell { ability, target_x, target_y } => {
            actions::apply_cast_learned_spell(
                world, grid, entity, *ability, *target_x, *target_y, spatial_cache, events, rng,
            )
        }
        ActionType::StartRaiseDead { target } => {
            actions::apply_start_raise_dead(world, entity, *target, events)
        }
        ActionType::Recover => {
            // Recovery is just a time delay, no effects
            ActionResult::Completed
        }
        ActionType::BossGroundSlam => actions::apply_boss_ground_slam(
            &mut effects(world, grid, spatial_cache, events, rng), entity,
        ),
        // Reactive kit abilities did their work when they started (see
        // `start_action_with_start_effects`); completion only tidies up.
        ActionType::Guard => {
            actions::apply_guard_complete(&mut effects(world, grid, spatial_cache, events, rng), entity)
        }
        ActionType::BoneWard | ActionType::Sacrifice { .. } => ActionResult::Completed,
        ActionType::CorpseExplosion { corpse } => actions::apply_corpse_explosion(
            &mut effects(world, grid, spatial_cache, events, rng), entity, *corpse,
        ),
        ActionType::ActivateThorns => {
            actions::apply_activate_thorns(&mut effects(world, grid, spatial_cache, events, rng), entity)
        }
        ActionType::Entangle { target_x, target_y } => actions::apply_entangle(
            &mut effects(world, grid, spatial_cache, events, rng), entity, *target_x, *target_y,
        ),
        ActionType::CallRain { target_x, target_y } => actions::apply_call_rain(
            &mut effects(world, grid, spatial_cache, events, rng), entity, *target_x, *target_y,
        ),
    }
}

// =============================================================================
// TIME-BASED REGENERATION
// =============================================================================

/// Process time-based health regeneration
pub fn tick_health_regen(world: &mut World, current_time: f32, events: Option<&mut EventQueue>) {
    use std::collections::HashSet;

    // First pass: collect entities with Regenerating effect (boosted regen)
    let regenerating: HashSet<Entity> = world
        .query::<(&Health, &StatusEffects)>()
        .iter()
        .filter_map(|(id, (_, status_effects))| {
            if effects::has_effect(status_effects, EffectType::Regenerating) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    // Collect regen info first to avoid borrow issues
    let mut regen_events: Vec<(Entity, i32)> = Vec::new();

    for (id, (health, hunger)) in
        world.query_mut::<(&mut Health, Option<&crate::components::Hunger>)>()
    {
        // Check if this entity has boosted regen from Regenerating effect
        let has_regen_boost = regenerating.contains(&id);

        // Hungry (player-only Hunger meter below threshold): natural HP regen
        // stops. Potion-boosted Regenerating still works — it's not "natural".
        if !has_regen_boost && hunger.map(|h| h.is_hungry()).unwrap_or(false) {
            continue;
        }

        // Determine regen parameters (boosted if Regenerating effect active)
        let (regen_amount, regen_interval) = if has_regen_boost {
            (REGENERATION_BOOST_AMOUNT, REGENERATION_BOOST_INTERVAL)
        } else {
            (health.regen_amount, health.regen_interval)
        };

        // Skip if no regen, already full, or dead
        if regen_interval <= 0.0 || health.current >= health.max || health.current <= 0 {
            continue;
        }

        // Calculate how many regen events have occurred
        let time_since_last = current_time - health.last_regen_time;
        if time_since_last >= regen_interval {
            let regen_ticks = (time_since_last / regen_interval) as i32;
            let amount = (regen_amount * regen_ticks).min(health.max - health.current);
            health.current += amount;
            // Update last regen time, accounting for partial intervals
            health.last_regen_time = current_time - (time_since_last % regen_interval);

            if amount > 0 {
                regen_events.push((id, amount));
            }
        }
    }

    // Emit events
    if let Some(events) = events {
        for (entity, amount) in regen_events {
            events.push(GameEvent::HealthRegenerated { entity, amount });
        }
    }
}

/// Process status effect duration ticks, removing expired effects
pub fn tick_status_effects(world: &mut World, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }

    for (_, effects) in world.query_mut::<&mut StatusEffects>() {
        effects.effects.retain_mut(|effect| {
            effect.remaining_duration -= elapsed;
            effect.remaining_duration > 0.0
        });
    }
}

/// Process ability cooldown ticks
pub fn tick_ability_cooldowns(world: &mut World, elapsed: f32) {
    use crate::components::{ClassAbility, ClassKit, LearnedAbilities, SecondaryAbility};

    if elapsed <= 0.0 {
        return;
    }

    for (_, ability) in world.query_mut::<&mut ClassAbility>() {
        if ability.cooldown_remaining > 0.0 {
            ability.cooldown_remaining = (ability.cooldown_remaining - elapsed).max(0.0);
        }
    }

    // Also tick secondary abilities (Druid's Barkskin)
    for (_, ability) in world.query_mut::<&mut SecondaryAbility>() {
        if ability.cooldown_remaining > 0.0 {
            ability.cooldown_remaining = (ability.cooldown_remaining - elapsed).max(0.0);
        }
    }

    // Also tick the per-class kit abilities
    for (_, kit) in world.query_mut::<&mut ClassKit>() {
        kit.tick(elapsed);
    }

    // Also tick learned spells (studied scrolls + Raise Dead)
    for (_, learned) in world.query_mut::<&mut LearnedAbilities>() {
        for spell in learned.spells.iter_mut() {
            if spell.cooldown_remaining > 0.0 {
                spell.cooldown_remaining = (spell.cooldown_remaining - elapsed).max(0.0);
            }
        }
    }
}

/// Process ranged attack cooldown ticks (for enemies with bows)
pub fn tick_ranged_cooldowns(world: &mut World, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }

    for (_, cooldown) in world.query_mut::<&mut RangedCooldown>() {
        if cooldown.remaining > 0.0 {
            cooldown.remaining = (cooldown.remaining - elapsed).max(0.0);
        }
    }
}

/// Process burn damage for entities with the Burning effect
pub fn tick_burn_damage(world: &mut World, current_time: f32, events: &mut EventQueue) {
    use crate::components::Position;

    // Collect entities that need to take burn damage
    let mut burn_events: Vec<(Entity, (f32, f32), i32)> = Vec::new();
    let mut deaths: Vec<Entity> = Vec::new();

    // First pass: find burning entities and check if they should take damage
    for (entity, (health, effects, pos)) in
        world.query_mut::<(&mut Health, &mut StatusEffects, &Position)>()
    {
        // Oil burns hot: an Oiled creature takes more from each burn tick.
        let oiled = effects.effects.iter().any(|e| e.effect_type == EffectType::Oiled);
        // Find the burning effect
        if let Some(burn_effect) = effects
            .effects
            .iter_mut()
            .find(|e| e.effect_type == EffectType::Burning)
        {
            let time_since_last = current_time - burn_effect.last_damage_tick;
            if time_since_last >= BURNING_DAMAGE_INTERVAL {
                // Deal damage
                let damage = if oiled {
                    (BURNING_DAMAGE_PER_SECOND as f32 * OILED_BURN_DAMAGE_MULT).round() as i32
                } else {
                    BURNING_DAMAGE_PER_SECOND
                };
                health.current = (health.current - damage).max(0);
                burn_effect.last_damage_tick = current_time;

                burn_events.push((entity, (pos.x as f32 + 0.5, pos.y as f32 + 0.5), damage));

                if health.current <= 0 {
                    deaths.push(entity);
                }
            }
        }
    }

    // Emit burn damage events and interrupt channeling
    for (entity, position, damage) in burn_events {
        // Interrupt life drain if entity was channeling
        actions::interrupt_life_drain_on_damage(world, entity, events);
        events.push(GameEvent::BurnDamage {
            entity,
            position,
            damage,
        });
    }

    // Handle deaths from burning
    for entity in deaths {
        if let Ok(pos) = world.get::<&Position>(entity) {
            events.push(GameEvent::EntityDied {
                entity,
                position: (pos.x as f32 + 0.5, pos.y as f32 + 0.5),
            });
        }
    }
}

/// Tick damage-over-time statuses (Poisoned, Bleeding) on game time.
///
/// Same shape as [`tick_burn_damage`]: each effect remembers the game time of
/// its last tick (`last_damage_tick`, 0 on a fresh application, so the first
/// tick lands at the next advancement) and deals its damage whenever its
/// interval has elapsed. Ticks stop when `tick_status_effects` expires the
/// effect. Damage goes through `combat::apply_damage_dot`, which bypasses
/// armor; the result is announced as a [`GameEvent::DotDamage`].
pub fn tick_dot_damage(
    world: &mut World,
    current_time: f32,
    rng: &mut impl rand::Rng,
    events: &mut EventQueue,
) {
    use crate::components::{Player, Position};
    use crate::events::DamageKind;

    let mut due: Vec<(Entity, DamageKind, i32)> = Vec::new();
    for (entity, (health, effects)) in world.query_mut::<(&Health, &mut StatusEffects)>() {
        if health.current <= 0 {
            continue;
        }
        for effect in effects.effects.iter_mut() {
            let (kind, damage, interval) = match effect.effect_type {
                EffectType::Poisoned => (DamageKind::Poison, POISON_DAMAGE, POISON_TICK_INTERVAL),
                EffectType::Bleeding => (DamageKind::Bleed, BLEED_DAMAGE, BLEED_TICK_INTERVAL),
                _ => continue,
            };
            if current_time - effect.last_damage_tick >= interval {
                effect.last_damage_tick = current_time;
                due.push((entity, kind, damage));
            }
        }
    }

    for (entity, kind, raw) in due {
        let alive = world.get::<&Health>(entity).map(|h| h.current > 0).unwrap_or(false);
        let Ok(position) = world
            .get::<&Position>(entity)
            .map(|p| (p.x as f32 + 0.5, p.y as f32 + 0.5))
        else {
            continue;
        };
        if !alive {
            continue;
        }
        let dealt = crate::systems::combat::apply_damage_dot(world, entity, raw, rng, events);
        if dealt <= 0 {
            continue;
        }
        actions::interrupt_life_drain_on_damage(world, entity, events);
        events.push(GameEvent::DotDamage { entity, position, damage: dealt, kind });

        // Dead enemies are swept (and announced) by `remove_dead_entities`;
        // the player is not, so announce a fatal tick here as burning does.
        let dead = world.get::<&Health>(entity).map(|h| h.current <= 0).unwrap_or(false);
        if dead && world.get::<&Player>(entity).is_ok() {
            events.push(GameEvent::EntityDied { entity, position });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Effort, Fatigue};

    /// Run a DoT victim through `seconds` of game time in `step`-sized
    /// advancements, the way the scheduler loop does (DoT tick, then status
    /// tick). Returns HP lost and the DoT events seen.
    fn bleed_out(effect: EffectType, duration: f32, seconds: f32, step: f32) -> (i32, Vec<GameEvent>) {
        use rand::SeedableRng;
        let mut world = World::new();
        let mut s = StatusEffects::new();
        effects::add_effect(&mut s, effect, duration);
        let victim = world.spawn((crate::components::Position::new(2, 2), Health::new(100), s));
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let mut events = EventQueue::new();
        // Start well into the run, so "last tick at t=0" is long past.
        let mut t = 100.0;
        let end = t + seconds;
        while t < end {
            t += step;
            tick_dot_damage(&mut world, t, &mut rng, &mut events);
            tick_status_effects(&mut world, step);
        }
        let lost = 100 - world.get::<&Health>(victim).unwrap().current;
        let seen: Vec<GameEvent> = events
            .drain()
            .filter(|e| matches!(e, GameEvent::DotDamage { .. }))
            .collect();
        (lost, seen)
    }

    /// Poison ticks `POISON_DAMAGE` every `POISON_TICK_INTERVAL` of game time
    /// and stops when the effect expires — however finely time is sliced.
    #[test]
    fn poison_ticks_on_game_time_and_stops_at_expiry() {
        let expected_ticks = (POISON_DURATION / POISON_TICK_INTERVAL).round() as i32;
        for step in [0.1, 0.25, 0.5] {
            let (lost, seen) = bleed_out(EffectType::Poisoned, POISON_DURATION, 30.0, step);
            assert_eq!(lost, expected_ticks * POISON_DAMAGE, "step {step}");
            assert_eq!(seen.len() as i32, expected_ticks, "step {step}");
            assert!(seen.iter().all(|e| matches!(e,
                GameEvent::DotDamage { kind: crate::events::DamageKind::Poison, .. })));
        }
        // No time passing, no damage.
        let (lost, _) = bleed_out(EffectType::Poisoned, POISON_DURATION, 0.0, 0.1);
        assert_eq!(lost, 0);
    }

    /// Bleeding works the same way at its own rate.
    #[test]
    fn bleeding_ticks_on_game_time_and_stops_at_expiry() {
        let expected_ticks = (BLEED_DURATION / BLEED_TICK_INTERVAL).round() as i32;
        let (lost, seen) = bleed_out(EffectType::Bleeding, BLEED_DURATION, 30.0, 0.25);
        assert_eq!(lost, expected_ticks * BLEED_DAMAGE);
        assert!(seen.iter().all(|e| matches!(e,
            GameEvent::DotDamage { kind: crate::events::DamageKind::Bleed, .. })));
    }

    fn actor(speed: f32) -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn((Actor::new(speed), Fatigue::new()));
        (world, entity)
    }

    fn fatigue_of(world: &World, entity: Entity) -> f32 {
        world.get::<&Fatigue>(entity).map(|f| f.value).unwrap_or(0.0)
    }

    fn start(world: &mut World, entity: Entity, action: ActionType, clock: &GameClock) {
        let mut scheduler = ActionScheduler::new();
        start_action(world, entity, action, clock, &mut scheduler).expect("should start");
    }

    /// Nothing gates an action but being busy. An idle actor can always act, in
    /// any state — this is what makes fighting feel consistent, and what makes
    /// "press the button and nothing happens" impossible.
    #[test]
    fn an_idle_actor_can_always_act() {
        let (mut world, entity) = actor(1.0);
        let clock = GameClock::new();

        // Exhausted: the worst state there is.
        if let Ok(mut f) = world.get::<&mut Fatigue>(entity) {
            f.value = FATIGUE_MAX;
        }
        assert!(world.get::<&Actor>(entity).map(|a| a.can_act()).unwrap_or(false));

        // And a long flurry never runs into a wall.
        for i in 0..50 {
            let mut scheduler = ActionScheduler::new();
            let r = start_action(
                &mut world,
                entity,
                ActionType::AttackDirection { dx: 1, dy: 0 },
                &clock,
                &mut scheduler,
            );
            assert!(r.is_ok(), "swing {i} was refused: {r:?}");
            if let Ok(mut a) = world.get::<&mut Actor>(entity) {
                a.current_action = None;
            }
        }
    }

    /// Being mid-action is the one thing that stops you starting another.
    #[test]
    fn an_actor_mid_action_cannot_start_another() {
        let (mut world, entity) = actor(1.0);
        let clock = GameClock::new();
        start(&mut world, entity, ActionType::Wait, &clock);

        let mut scheduler = ActionScheduler::new();
        assert!(start_action(&mut world, entity, ActionType::Wait, &clock, &mut scheduler).is_err());
    }

    /// Effort accrues per second of acting, so tiredness tracks the work done
    /// rather than the actor's speed. A flat amount per action would make a fast
    /// creature tire in proportion to being fast, which is backwards.
    #[test]
    fn effort_per_second_is_the_same_at_every_speed() {
        let walk = ActionType::Move { dx: 1, dy: 0, is_diagonal: false };
        let clock = GameClock::new();

        for speed in [0.55f32, 1.0, 2.2] {
            let (mut world, entity) = actor(speed);
            start(&mut world, entity, walk, &clock);
            let duration = world
                .get::<&Actor>(entity)
                .ok()
                .and_then(|a| a.current_action)
                .map(|a| a.completion_time - a.start_time)
                .expect("in progress");
            let rate = fatigue_of(&world, entity) / (duration * FATIGUE_PER_EFFORT);
            assert!(
                (rate - EXERTION_LIGHT).abs() < 1e-3,
                "speed {speed} tired at {rate}/s, expected {EXERTION_LIGHT}"
            );
        }
    }

    /// Fighting must tire you substantially faster than walking, or fatigue
    /// says nothing about what you have been doing.
    #[test]
    fn fighting_tires_much_faster_than_walking() {
        let clock = GameClock::new();

        let (mut w1, e1) = actor(1.0);
        start(&mut w1, e1, ActionType::Move { dx: 1, dy: 0, is_diagonal: false }, &clock);
        let walking = fatigue_of(&w1, e1);

        let (mut w2, e2) = actor(1.0);
        start(&mut w2, e2, ActionType::AttackDirection { dx: 1, dy: 0 }, &clock);
        let fighting = fatigue_of(&w2, e2);

        assert!(walking > 0.0 && fighting > 0.0);
        assert!(
            fighting > walking * 2.0,
            "a swing ({fighting}) should tire well beyond a step ({walking})"
        );
    }

    /// Waiting is free, so standing still never tires you.
    #[test]
    fn waiting_is_not_tiring() {
        let (mut world, entity) = actor(1.0);
        let clock = GameClock::new();
        start(&mut world, entity, ActionType::Wait, &clock);
        assert_eq!(fatigue_of(&world, entity), 0.0);
    }

    /// Abilities are a flat effort: casting one quickly does not make it less
    /// tiring.
    #[test]
    fn ability_effort_is_flat_and_speed_independent() {
        let clock = GameClock::new();
        let mut values = Vec::new();
        for speed in [0.5f32, 1.0, 2.0] {
            let (mut world, entity) = actor(speed);
            start(&mut world, entity, ActionType::Cleave, &clock);
            values.push(fatigue_of(&world, entity));
        }
        let expected = CLEAVE_ENERGY_COST * FATIGUE_PER_EFFORT;
        for v in &values {
            assert!((v - expected).abs() < 1e-4, "got {values:?}, expected {expected}");
        }
        assert!(matches!(ActionType::Cleave.effort(), Effort::Flat(_)));
    }

    /// Fatigue is a long meter: a single fight must not meaningfully move it.
    /// The whole point of the rework is that effort costs you over a run, not
    /// inside one exchange.
    #[test]
    fn a_short_fight_barely_dents_the_fatigue_meter() {
        let (mut world, entity) = actor(1.0);
        let clock = GameClock::new();

        for _ in 0..10 {
            start(&mut world, entity, ActionType::AttackDirection { dx: 1, dy: 0 }, &clock);
            if let Ok(mut a) = world.get::<&mut Actor>(entity) {
                a.current_action = None;
            }
        }

        let after = fatigue_of(&world, entity);
        assert!(
            after < FATIGUE_MAX * 0.1,
            "ten swings moved fatigue to {after}; it should stay a long-term meter"
        );
    }

    /// The core scheduling property: a short action resolves several times
    /// while a long one is still running, and the long actor is not touched in
    /// between.
    ///
    /// Three things in one test, because they are one mechanism:
    ///
    /// 1. Actions have durations, and the clock jumps to the next *completion*
    ///    rather than ticking at a fixed rate.
    /// 2. The actor with the shorter action gets every intervening turn.
    /// 3. The actor mid-long-action is never reconsidered — its
    ///    `ActionInProgress` is byte-for-byte the same throughout, and
    ///    `can_act()` stays false, which is exactly the condition
    ///    `ai::decide_action` returns on. Its AI does not re-run until its own
    ///    completion pops.
    #[test]
    fn a_short_action_takes_several_turns_while_a_long_one_runs_once() {
        let walk = ActionType::Move { dx: 1, dy: 0, is_diagonal: false };
        let mut world = World::new();
        // Base walk is 1.0s, scaled by 1/speed: 4.5s versus 1.0s.
        let slow = world.spawn((Actor::new(1.0 / 4.5),));
        let quick = world.spawn((Actor::new(1.0),));

        let mut clock = GameClock::new();
        let mut scheduler = ActionScheduler::new();
        start_action(&mut world, slow, walk, &clock, &mut scheduler).expect("slow starts");
        start_action(&mut world, quick, walk, &clock, &mut scheduler).expect("quick starts");

        let slow_action = world
            .get::<&Actor>(slow)
            .ok()
            .and_then(|a| a.current_action)
            .expect("slow is mid-action");
        assert!(
            (slow_action.completion_time - 4.5).abs() < 1e-3,
            "slow action should take 4.5s, got {}",
            slow_action.completion_time
        );

        // The quick actor should come round four times before the slow one lands.
        for turn in 1..=4 {
            let (entity, time) = scheduler.pop_next().expect("something pending");
            assert_eq!(entity, quick, "turn {turn} should belong to the quick actor");
            assert!(
                (time - turn as f32).abs() < 1e-3,
                "turn {turn} should complete at t={turn}, got {time}"
            );
            clock.advance_to(time);

            // Complete and restart only the actor that actually finished.
            if let Ok(mut a) = world.get::<&mut Actor>(entity) {
                a.current_action = None;
            }
            start_action(&mut world, entity, walk, &clock, &mut scheduler).expect("restart");

            // The slow actor has not been reconsidered: same action, still busy.
            {
                let a = world.get::<&Actor>(slow).expect("slow exists");
                let current = a.current_action.expect("still mid-action");
                assert_eq!(
                    current.start_time, slow_action.start_time,
                    "turn {turn}: the slow actor's action was restarted"
                );
                assert_eq!(
                    current.completion_time, slow_action.completion_time,
                    "turn {turn}: the slow actor's completion moved"
                );
                assert!(
                    !a.can_act(),
                    "turn {turn}: a busy actor must not be eligible to decide again"
                );
            }
        }

        // Only now does the long action land.
        let (entity, time) = scheduler.pop_next().expect("the slow completion");
        assert_eq!(entity, slow, "the slow actor should finally come round");
        assert!((time - 4.5).abs() < 1e-3, "it should land at 4.5s, got {time}");
    }

    /// The clock jumps to the next completion; it never advances past a pending
    /// one, so nothing is ever resolved out of order.
    #[test]
    fn the_clock_jumps_to_the_next_completion_in_time_order() {
        let wait = ActionType::Wait;
        let mut world = World::new();
        let mut scheduler = ActionScheduler::new();
        let clock = GameClock::new();

        // Three actors at wildly different speeds, started together.
        let a = world.spawn((Actor::new(0.25),)); // Wait is 0.5s base -> 2.0s
        let b = world.spawn((Actor::new(1.0),));  // -> 0.5s
        let c = world.spawn((Actor::new(0.5),));  // -> 1.0s
        for e in [a, b, c] {
            start_action(&mut world, e, wait, &clock, &mut scheduler).expect("starts");
        }

        let mut popped = Vec::new();
        while let Some((entity, time)) = scheduler.pop_next() {
            popped.push((entity, time));
        }

        assert_eq!(
            popped.iter().map(|(e, _)| *e).collect::<Vec<_>>(),
            vec![b, c, a],
            "completions must come out shortest-first, not in spawn order"
        );
        assert!(
            popped.windows(2).all(|w| w[0].1 <= w[1].1),
            "times must be non-decreasing, got {popped:?}"
        );
    }

    /// Actors with no Fatigue meter (everything but the player today) still act
    /// normally — the fatigue update is a lookup that has to degrade.
    #[test]
    fn actors_without_a_fatigue_meter_act_normally() {
        let mut world = World::new();
        let entity = world.spawn((Actor::new(1.0),));
        let clock = GameClock::new();
        let mut scheduler = ActionScheduler::new();
        assert!(start_action(
            &mut world,
            entity,
            ActionType::AttackDirection { dx: 1, dy: 0 },
            &clock,
            &mut scheduler
        )
        .is_ok());
    }
}
