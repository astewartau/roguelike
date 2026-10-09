//! Hit-stop: a brief freeze of visual animation when a heavy blow lands.
//!
//! Purely presentation. The simulation is event-driven and never sees it; the
//! engine feeds this timer the frame's [`GameEvent`]s and then passes the
//! frame's *real* time through [`HitStop::visual_dt`] before handing it to the
//! animation systems (visual lerp, lunges, hit flashes, floating numbers and
//! other vfx). While the timer runs they get `HITSTOP_TIME_SCALE` of real time
//! (a dead freeze at 0.0); the camera shake keeps its real time, so the knock
//! still rattles during the freeze.
//!
//! What triggers it, from the player's point of view:
//! - a critical hit the player lands: `HITSTOP_CRIT`;
//! - a kill by the player or one of their companions (melee, cleave,
//!   spells, arrows): `HITSTOP_KILL`;
//! - a boss's ground slam connecting with the player: `HITSTOP_HEAVY`.
//!
//! Triggers never stack: a new one only extends the freeze up to its own
//! length, so a cleave that crits and kills three things freezes once, for
//! the longest of them.

use hecs::{Entity, World};

use crate::components::{CompanionAI, TamedBy};
use crate::constants::*;
use crate::events::{DamageKind, GameEvent};

/// The hit-stop timer, in real seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HitStop {
    remaining: f32,
}

impl HitStop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Real seconds of freeze left.
    #[cfg(test)]
    pub fn remaining(&self) -> f32 {
        self.remaining
    }

    /// Freeze for `duration` real seconds from now, unless a longer freeze is
    /// already running. Never adds to what is left.
    pub fn trigger(&mut self, duration: f32) {
        self.remaining = self.remaining.max(duration);
    }

    /// The animation time for a frame of `real_dt` real seconds, consuming
    /// the freeze: the frozen part runs at `HITSTOP_TIME_SCALE`, the rest at
    /// full speed, so a freeze ending mid-frame loses only what it covered.
    pub fn visual_dt(&mut self, real_dt: f32) -> f32 {
        let real_dt = real_dt.max(0.0);
        let frozen = self.remaining.min(real_dt);
        self.remaining -= frozen;
        (real_dt - frozen) + frozen * HITSTOP_TIME_SCALE
    }

    /// How long a freeze `event` asks for, if any.
    pub fn duration_for(world: &World, player: Entity, event: &GameEvent) -> Option<f32> {
        // An arrow carries no kill flag; by the time its event is handled the
        // victim is either at 0 HP or already bones (no Health left).
        if let GameEvent::ProjectileHit { source, target: Some(t), damage, .. } = event {
            let ours = *source == player || is_companion_of(world, *source, player);
            let dead = world.get::<&crate::components::Health>(*t).map(|h| h.current <= 0).unwrap_or(true);
            return (ours && *damage > 0 && dead).then_some(HITSTOP_KILL);
        }
        let GameEvent::AttackHit { attacker, target, damage, kind, crit, killed, .. } = event else {
            return None;
        };
        if *kind == DamageKind::Slam && *target == player && *damage > 0 {
            return Some(HITSTOP_HEAVY);
        }
        let ours = *attacker == player || is_companion_of(world, *attacker, player);
        if ours && *killed {
            return Some(HITSTOP_KILL);
        }
        if *attacker == player && *crit {
            return Some(HITSTOP_CRIT);
        }
        None
    }

    /// Feed one event: trigger a freeze if it asks for one.
    pub fn on_event(&mut self, world: &World, player: Entity, event: &GameEvent) {
        if let Some(d) = Self::duration_for(world, player, event) {
            self.trigger(d);
        }
    }
}

/// Whether `entity` fights for `player` (tamed or raised companion).
fn is_companion_of(world: &World, entity: Entity, player: Entity) -> bool {
    world.get::<&TamedBy>(entity).map(|t| t.owner == player).unwrap_or(false)
        || world.get::<&CompanionAI>(entity).map(|c| c.owner == player).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(attacker: Entity, target: Entity, kind: DamageKind, crit: bool, killed: bool) -> GameEvent {
        GameEvent::AttackHit {
            attacker,
            target,
            target_pos: (0.5, 0.5),
            damage: 5,
            kind,
            crit,
            flanked: false,
            killed,
        }
    }

    #[test]
    fn the_freeze_eats_animation_time_then_releases_it() {
        let mut h = HitStop::new();
        assert_eq!(h.visual_dt(0.016), 0.016, "no freeze, full speed");

        h.trigger(0.05);
        let frame = 0.016;
        let mut animated = 0.0;
        let mut real = 0.0;
        while real < 0.05 - 1e-6 {
            animated += h.visual_dt(frame);
            real += frame;
        }
        // 0.05 of real time was frozen; anything past it ran at full speed.
        let expected = (real - 0.05) + 0.05 * HITSTOP_TIME_SCALE;
        assert!((animated - expected).abs() < 1e-5, "{animated} vs {expected}");
        assert_eq!(h.remaining(), 0.0);
        assert_eq!(h.visual_dt(frame), frame, "and then resumes");
    }

    #[test]
    fn triggers_never_stack_past_the_longest_single_freeze() {
        let mut h = HitStop::new();
        h.trigger(HITSTOP_CRIT);
        h.trigger(HITSTOP_KILL);
        h.trigger(HITSTOP_CRIT);
        assert_eq!(h.remaining(), HITSTOP_KILL);
        h.visual_dt(0.03);
        h.trigger(HITSTOP_CRIT);
        assert!((h.remaining() - HITSTOP_CRIT.max(HITSTOP_KILL - 0.03)).abs() < 1e-6);
    }

    #[test]
    fn a_long_frame_is_only_partly_frozen() {
        let mut h = HitStop::new();
        h.trigger(0.08);
        let dt = h.visual_dt(0.1);
        assert!((dt - (0.02 + 0.08 * HITSTOP_TIME_SCALE)).abs() < 1e-6);
    }

    #[test]
    fn which_events_freeze() {
        let mut world = World::new();
        let player = world.spawn(());
        let pet = world.spawn((TamedBy { owner: player },));
        let orc = world.spawn(());
        let d = |e: GameEvent| HitStop::duration_for(&world, player, &e);

        assert_eq!(d(hit(player, orc, DamageKind::Melee, true, false)), Some(HITSTOP_CRIT));
        assert_eq!(d(hit(player, orc, DamageKind::Melee, false, true)), Some(HITSTOP_KILL));
        assert_eq!(d(hit(player, orc, DamageKind::Cleave, true, true)), Some(HITSTOP_KILL));
        assert_eq!(d(hit(pet, orc, DamageKind::Melee, false, true)), Some(HITSTOP_KILL));
        assert_eq!(d(hit(orc, player, DamageKind::Slam, false, false)), Some(HITSTOP_HEAVY));
        assert_eq!(d(hit(player, orc, DamageKind::Melee, false, false)), None);
        assert_eq!(d(hit(orc, player, DamageKind::Melee, true, false)), None, "an enemy crit does not freeze");
        assert_eq!(d(hit(orc, pet, DamageKind::Melee, false, true)), None, "an enemy kill does not");
        assert_eq!(d(GameEvent::RootStruggle { entity: player }), None);

        // An arrow that leaves its target dead (or already bones).
        let arrow = |target| GameEvent::ProjectileHit {
            projectile: player,
            source: player,
            target: Some(target),
            position: (0, 0),
            damage: 4,
            kind: DamageKind::Arrow,
        };
        let live = world.spawn((crate::components::Health::new(10),));
        assert_eq!(HitStop::duration_for(&world, player, &arrow(live)), None);
        world.get::<&mut crate::components::Health>(live).unwrap().current = 0;
        assert_eq!(HitStop::duration_for(&world, player, &arrow(live)), Some(HITSTOP_KILL));
    }
}
