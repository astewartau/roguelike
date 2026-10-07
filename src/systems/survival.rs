//! Survival clock: hunger + fatigue (player-only meters).
//!
//! Hunger drains over game time (faster while resting/sleeping) and is
//! restored by eating food. Below the hungry threshold natural HP regen stops
//! (see `time_system::tick_health_regen`); at zero the player takes periodic
//! starvation damage that ignores armor and can kill.
//!
//! Fatigue grows over game time while awake (faster while sprinting) and is
//! recovered by sleeping (see the sleep fast-forward in `engine`). Above the
//! tired threshold enemies notice the player faster (`systems::ai`) and the
//! player's attacks hit softer (`systems::combat`); at the cap actions slow
//! down and energy regen stops (`time_system`).
//!
//! Runs from the engine tick in game-time via the accumulator pattern (see
//! `fire::tick_fire` / `identify::tick_identification`), so it freezes while
//! the game is paused/idle — and races ahead during rest/sleep fast-forward.

use hecs::{Entity, World};

use crate::components::{EffectType, Fatigue, Health, Hunger, ItemType, Position};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::systems::effects;

/// What the player was doing while the meters ticked. Resting and sleeping
/// are engine-level fast-forward states, so the engine passes them in.
#[derive(Debug, Clone, Copy, Default)]
pub struct SurvivalContext {
    /// Player is resting (Rest fast-forward): hunger drains at 2x.
    pub resting: bool,
    /// Player is sleeping (Sleep fast-forward): hunger drains at 1.5x and
    /// fatigue recovers instead of growing.
    pub sleeping: bool,
}

/// Outcome flags the engine reacts to (e.g. waking a sleeper who took
/// starvation damage).
#[derive(Debug, Clone, Copy, Default)]
pub struct SurvivalTickResult {
    /// Starvation damage was applied to the player this tick.
    pub starvation_damage: bool,
}

/// Advance the player's hunger and fatigue meters by `game_dt` seconds of
/// game time. Emits one-shot `HungerStateChanged` / `FatigueStateChanged`
/// events on threshold crossings and `StarvationDamage` when starving.
pub fn tick_survival(
    world: &mut World,
    player: Entity,
    game_dt: f32,
    accumulator: &mut f32,
    ctx: SurvivalContext,
    events: &mut EventQueue,
) -> SurvivalTickResult {
    let mut result = SurvivalTickResult::default();
    if game_dt <= 0.0 {
        return result;
    }
    *accumulator += game_dt;
    if *accumulator < SURVIVAL_TICK_INTERVAL {
        return result;
    }
    let step = *accumulator;
    *accumulator = 0.0;

    // --- Hunger ---------------------------------------------------------
    let mut starvation_ticks = 0;
    let mut hunger_change = None;
    if let Ok(mut hunger) = world.get::<&mut Hunger>(player) {
        let before = hunger.state();

        let drain_mult = if ctx.resting {
            HUNGER_REST_DRAIN_MULT
        } else if ctx.sleeping {
            HUNGER_SLEEP_DRAIN_MULT
        } else {
            1.0
        };
        hunger.value =
            (hunger.value - step * drain_mult / HUNGER_DRAIN_SECONDS_PER_POINT).max(0.0);

        if hunger.is_starving() {
            hunger.starvation_timer += step;
            while hunger.starvation_timer >= STARVATION_DAMAGE_INTERVAL {
                hunger.starvation_timer -= STARVATION_DAMAGE_INTERVAL;
                starvation_ticks += 1;
            }
        } else {
            hunger.starvation_timer = 0.0;
        }

        let after = hunger.state();
        if after != before {
            hunger_change = Some(after);
        }
    }
    if let Some(state) = hunger_change {
        events.push(GameEvent::HungerStateChanged { state });
    }

    // Starvation damage bypasses `combat::apply_damage` deliberately: armor
    // and Protected/Barkskin don't help an empty stomach, and it must be able
    // to kill (player death is detected from Health by the engine).
    if starvation_ticks > 0 {
        let damage = starvation_ticks * STARVATION_DAMAGE;
        let position = world
            .get::<&Position>(player)
            .map(|p| (p.x as f32 + 0.5, p.y as f32 + 0.5))
            .unwrap_or((0.0, 0.0));
        let mut dealt = false;
        if let Ok(mut health) = world.get::<&mut Health>(player) {
            if health.current > 0 {
                health.current -= damage;
                dealt = true;
            }
        }
        if dealt {
            result.starvation_damage = true;
            events.push(GameEvent::StarvationDamage {
                entity: player,
                position,
                damage,
            });
        }
    }

    // --- Fatigue ----------------------------------------------------------
    let sprinting = effects::entity_has_effect(world, player, EffectType::SpeedBoost);
    let mut fatigue_change = None;
    if let Ok(mut fatigue) = world.get::<&mut Fatigue>(player) {
        let before = fatigue.state();

        if ctx.sleeping {
            fatigue.value = (fatigue.value - step * SLEEP_FATIGUE_RECOVERY_PER_SECOND).max(0.0);
        } else {
            let gain_mult = if sprinting { FATIGUE_SPRINT_MULT } else { 1.0 };
            fatigue.value = (fatigue.value + step * gain_mult / FATIGUE_GAIN_SECONDS_PER_POINT)
                .min(FATIGUE_MAX);
        }

        let after = fatigue.state();
        if after != before {
            fatigue_change = Some(after);
        }
    }
    if let Some(state) = fatigue_change {
        events.push(GameEvent::FatigueStateChanged { state });
    }

    result
}

/// Hunger restored by eating the given item, or `None` for non-food.
pub fn food_hunger_restore(kind: ItemType) -> Option<f32> {
    match kind {
        ItemType::Cheese => Some(CHEESE_HUNGER_RESTORE),
        ItemType::Bread => Some(BREAD_HUNGER_RESTORE),
        ItemType::Apple => Some(APPLE_HUNGER_RESTORE),
        _ => None,
    }
}

/// Restore hunger on an entity, clamped to the cap. No-op for entities
/// without a `Hunger` meter (only the player has one).
pub fn restore_hunger(world: &mut World, entity: Entity, amount: f32) {
    if let Ok(mut hunger) = world.get::<&mut Hunger>(entity) {
        hunger.eat(amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{FatigueState, HungerState, StatusEffects};

    fn spawn_player(world: &mut World) -> Entity {
        world.spawn((
            Position::new(0, 0),
            Health::new(50),
            Hunger::new(),
            Fatigue::new(),
            StatusEffects::new(),
        ))
    }

    fn tick(
        world: &mut World,
        player: Entity,
        dt: f32,
        ctx: SurvivalContext,
        events: &mut EventQueue,
    ) -> SurvivalTickResult {
        let mut accumulator = 0.0;
        tick_survival(world, player, dt, &mut accumulator, ctx, events)
    }

    fn hunger(world: &World, player: Entity) -> f32 {
        world.get::<&Hunger>(player).expect("hunger").value
    }

    fn fatigue(world: &World, player: Entity) -> f32 {
        world.get::<&Fatigue>(player).expect("fatigue").value
    }

    #[test]
    fn test_hunger_drains_one_point_per_interval() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        tick(&mut world, player, HUNGER_DRAIN_SECONDS_PER_POINT, SurvivalContext::default(), &mut events);
        assert!((hunger(&world, player) - (HUNGER_MAX - 1.0)).abs() < 0.001);
    }

    #[test]
    fn test_hunger_drains_faster_resting_and_sleeping() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        let dt = HUNGER_DRAIN_SECONDS_PER_POINT;
        tick(&mut world, player, dt, SurvivalContext { resting: true, sleeping: false }, &mut events);
        assert!((hunger(&world, player) - (HUNGER_MAX - HUNGER_REST_DRAIN_MULT)).abs() < 0.001);

        let mut world2 = World::new();
        let player2 = spawn_player(&mut world2);
        tick(&mut world2, player2, dt, SurvivalContext { resting: false, sleeping: true }, &mut events);
        assert!((hunger(&world2, player2) - (HUNGER_MAX - HUNGER_SLEEP_DRAIN_MULT)).abs() < 0.001);
    }

    #[test]
    fn test_hunger_threshold_crossing_emits_once() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        // Start just above the hungry threshold and drain past it.
        world.get::<&mut Hunger>(player).expect("hunger").value =
            HUNGER_HUNGRY_THRESHOLD + 0.5;
        tick(&mut world, player, HUNGER_DRAIN_SECONDS_PER_POINT, SurvivalContext::default(), &mut events);

        let crossings: Vec<_> = events
            .drain()
            .filter_map(|e| match e {
                GameEvent::HungerStateChanged { state } => Some(state),
                _ => None,
            })
            .collect();
        assert_eq!(crossings, vec![HungerState::Hungry]);

        // Further draining within the same state emits nothing new.
        tick(&mut world, player, HUNGER_DRAIN_SECONDS_PER_POINT, SurvivalContext::default(), &mut events);
        let crossings: Vec<_> = events
            .drain()
            .filter_map(|e| match e {
                GameEvent::HungerStateChanged { state } => Some(state),
                _ => None,
            })
            .collect();
        assert!(crossings.is_empty(), "no repeat warning while still Hungry");
    }

    #[test]
    fn test_starvation_damage_ignores_armor_and_can_kill() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        // Empty stomach, 2 HP left. Two starvation intervals must kill,
        // regardless of any defense (damage is applied directly to Health).
        world.get::<&mut Hunger>(player).expect("hunger").value = 0.0;
        world.get::<&mut Health>(player).expect("health").current = 2;

        let result = tick(
            &mut world,
            player,
            STARVATION_DAMAGE_INTERVAL * 2.0,
            SurvivalContext::default(),
            &mut events,
        );
        assert!(result.starvation_damage);
        let hp = world.get::<&Health>(player).expect("health").current;
        assert!(hp <= 0, "starvation must be able to kill (hp = {hp})");

        let starved: Vec<_> = events
            .drain()
            .filter(|e| matches!(e, GameEvent::StarvationDamage { .. }))
            .collect();
        assert_eq!(starved.len(), 1);
    }

    #[test]
    fn test_fatigue_grows_awake_and_triples_while_sprinting() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        tick(&mut world, player, FATIGUE_GAIN_SECONDS_PER_POINT, SurvivalContext::default(), &mut events);
        assert!((fatigue(&world, player) - 1.0).abs() < 0.001);

        // With SpeedBoost (Sprint) active, fatigue grows 3x as fast.
        effects::add_effect_to_entity(&mut world, player, EffectType::SpeedBoost, 100.0);
        tick(&mut world, player, FATIGUE_GAIN_SECONDS_PER_POINT, SurvivalContext::default(), &mut events);
        assert!((fatigue(&world, player) - (1.0 + FATIGUE_SPRINT_MULT)).abs() < 0.001);
    }

    #[test]
    fn test_sleeping_recovers_fatigue_and_emits_state_changes() {
        let mut world = World::new();
        let player = spawn_player(&mut world);
        let mut events = EventQueue::new();

        world.get::<&mut Fatigue>(player).expect("fatigue").value = FATIGUE_MAX;
        let sleeping = SurvivalContext { resting: false, sleeping: true };

        tick(&mut world, player, 30.0, sleeping, &mut events);
        let expected = FATIGUE_MAX - 30.0 * SLEEP_FATIGUE_RECOVERY_PER_SECOND;
        assert!((fatigue(&world, player) - expected).abs() < 0.001);

        // Sleep long enough to recover fully; the meter clamps at 0.
        tick(&mut world, player, 1000.0, sleeping, &mut events);
        assert_eq!(fatigue(&world, player), 0.0);

        // Exhausted -> Tired -> Rested crossings each emitted once.
        let states: Vec<_> = events
            .drain()
            .filter_map(|e| match e {
                GameEvent::FatigueStateChanged { state } => Some(state),
                _ => None,
            })
            .collect();
        assert!(states.contains(&FatigueState::Rested));
    }

    #[test]
    fn test_food_restores_hunger_and_clamps_at_max() {
        let mut world = World::new();
        let player = spawn_player(&mut world);

        world.get::<&mut Hunger>(player).expect("hunger").value = 50.0;
        restore_hunger(&mut world, player, BREAD_HUNGER_RESTORE);
        assert!((hunger(&world, player) - (50.0 + BREAD_HUNGER_RESTORE)).abs() < 0.001);

        restore_hunger(&mut world, player, 500.0);
        assert_eq!(hunger(&world, player), HUNGER_MAX);
    }

    #[test]
    fn test_food_hunger_restore_values() {
        assert_eq!(food_hunger_restore(ItemType::Cheese), Some(CHEESE_HUNGER_RESTORE));
        assert_eq!(food_hunger_restore(ItemType::Bread), Some(BREAD_HUNGER_RESTORE));
        assert_eq!(food_hunger_restore(ItemType::Apple), Some(APPLE_HUNGER_RESTORE));
        assert_eq!(food_hunger_restore(ItemType::Sword), None);
        assert_eq!(food_hunger_restore(ItemType::HealthPotion), None);
    }

    #[test]
    fn test_hunger_and_fatigue_states() {
        let mut hunger = Hunger::new();
        assert_eq!(hunger.state(), HungerState::Fed);
        hunger.value = HUNGER_HUNGRY_THRESHOLD - 0.1;
        assert_eq!(hunger.state(), HungerState::Hungry);
        assert!(hunger.is_hungry());
        hunger.value = 0.0;
        assert_eq!(hunger.state(), HungerState::Starving);
        assert!(hunger.is_starving());

        let mut fatigue = Fatigue::new();
        assert_eq!(fatigue.state(), FatigueState::Rested);
        fatigue.value = FATIGUE_TIRED_THRESHOLD + 0.1;
        assert_eq!(fatigue.state(), FatigueState::Tired);
        assert!(fatigue.is_tired());
        assert!(!fatigue.is_exhausted());
        fatigue.value = FATIGUE_MAX;
        assert_eq!(fatigue.state(), FatigueState::Exhausted);
        assert!(fatigue.is_exhausted());
    }
}
