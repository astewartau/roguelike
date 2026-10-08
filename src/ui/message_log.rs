//! Scrolling combat/message log.
//!
//! In a real-time, energy-based game things happen fast — the log gives the
//! player a readable history of what just occurred ("Skeleton Archer hits you
//! for 6", "You reach level 3!"). It listens to [`GameEvent`]s, formats the
//! player-relevant ones into short lines, and draws them in the bottom-left
//! corner.

use std::collections::VecDeque;
use std::time::Instant;

use egui::Color32;
use hecs::{Entity, World};

use super::style::{self, colors};
use crate::constants::*;
use crate::ease;
use crate::components::{AbilityType, Equipment, Name};
use crate::events::{DamageKind, GameEvent, StairDirection};

/// Maximum number of distinct lines retained in the log.
const MAX_MESSAGES: usize = 100;
/// Number of lines shown on screen at once.
const VISIBLE_MESSAGES: usize = 6;

/// Colors used for log lines, themed to match the rest of the UI.
mod log_colors {
    use egui::Color32;

    /// Neutral combat / informational text.
    pub const INFO: Color32 = Color32::from_rgb(210, 200, 185);
    /// Damage dealt to the player.
    pub const HARM: Color32 = Color32::from_rgb(205, 90, 80);
    /// A kill or other notable victory.
    pub const KILL: Color32 = Color32::from_rgb(215, 185, 105);
    /// Beneficial events (loot, healing, level up).
    pub const GOOD: Color32 = Color32::from_rgb(110, 175, 110);
    /// System / navigation messages (floor changes, etc.).
    pub const SYSTEM: Color32 = Color32::from_rgb(150, 140, 125);
}

/// A single log line, with a repeat counter so spammy events collapse.
struct LogMessage {
    text: String,
    color: Color32,
    count: u32,
    /// When the line was first pushed, for the fade-and-slide entrance.
    /// Real time: the log is presentation, and a line frozen half-way in
    /// because the game clock stopped would be worse than no animation.
    arrived: Instant,
    /// When `count` last went up, for the badge flare.
    counted_at: Instant,
}

/// Rolling buffer of log messages.
pub struct MessageLog {
    messages: VecDeque<LogMessage>,
    /// The player entity, used to phrase messages from the player's POV.
    player_entity: Entity,
    /// Whether the one-time "you can close doors" tip has been shown.
    door_tip_shown: bool,
}

impl MessageLog {
    pub fn new(player_entity: Entity) -> Self {
        Self {
            messages: VecDeque::new(),
            player_entity,
            door_tip_shown: false,
        }
    }

    /// Push a system/informational line (e.g. resting feedback).
    pub fn system(&mut self, text: impl Into<String>) {
        self.push(text.into(), log_colors::INFO);
    }

    /// Test-only: the log lines in order, for asserting that a code path
    /// actually reported what it was supposed to.
    #[cfg(test)]
    pub fn lines(&self) -> Vec<String> {
        self.messages.iter().map(|m| m.text.clone()).collect()
    }

    /// Push a line. Consecutive identical lines collapse into a "(x2)" counter.
    fn push(&mut self, text: String, color: Color32) {
        if let Some(last) = self.messages.back_mut() {
            if last.text == text && last.color == color {
                last.count += 1;
                last.counted_at = Instant::now();
                return;
            }
        }
        let now = Instant::now();
        self.messages.push_back(LogMessage {
            text,
            color,
            count: 1,
            arrived: now,
            counted_at: now,
        });
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }

    /// Bare display name of an entity ("Skeleton", "Rat"), or a fallback.
    fn name(&self, world: &World, entity: Entity) -> String {
        world
            .get::<&Name>(entity)
            .map(|n| n.0.clone())
            .unwrap_or_else(|_| "something".to_string())
    }

    /// Entity as a sentence object: "you" for the player, else "the Skeleton".
    fn object(&self, world: &World, entity: Entity) -> String {
        if entity == self.player_entity {
            "you".to_string()
        } else {
            format!("the {}", self.name(world, entity))
        }
    }

    /// Entity as a capitalized sentence subject: "You" / "The Skeleton".
    fn subject(&self, world: &World, entity: Entity) -> String {
        capitalize(&self.object(world, entity))
    }

    /// Lowercase name of an entity's equipped melee weapon ("sword", "claws").
    fn melee_weapon(&self, world: &World, entity: Entity) -> Option<String> {
        world
            .get::<&Equipment>(entity)
            .ok()
            .and_then(|e| e.get_melee().map(|w| w.name.to_lowercase()))
    }

    /// Format a melee / cleave / fireball hit, from the player's POV.
    fn record_attack(
        &mut self,
        world: &World,
        attacker: Entity,
        target: Entity,
        damage: i32,
        kind: DamageKind,
        crit: bool,
    ) {
        let me = self.player_entity;
        // Only log hits the player is part of, to keep the log readable.
        if attacker != me && target != me {
            return;
        }
        let end = if crit { "!" } else { "." };
        let crit_word = if crit { "critically " } else { "" };

        match kind {
            DamageKind::Melee => {
                if attacker == me {
                    let weapon = self.melee_weapon(world, me);
                    let obj = self.object(world, target);
                    let line = match weapon {
                        Some(w) => format!("You {crit_word}hit {obj} with your {w} for {damage}{end}"),
                        None => format!("You {crit_word}hit {obj} for {damage}{end}"),
                    };
                    self.push(line, log_colors::INFO);
                } else {
                    let subj = self.subject(world, attacker);
                    let weapon = self.melee_weapon(world, attacker);
                    let line = match weapon {
                        Some(w) => format!("{subj} {crit_word}hits you with its {w} for {damage}{end}"),
                        None => format!("{subj} {crit_word}hits you for {damage}{end}"),
                    };
                    self.push(line, log_colors::HARM);
                }
            }
            DamageKind::Cleave => {
                // Cleave is a player ability; targets are always enemies.
                let obj = self.object(world, target);
                self.push(
                    format!("Your cleave {crit_word}tears into {obj} for {damage}{end}"),
                    log_colors::INFO,
                );
            }
            DamageKind::Fireball => {
                if target == me {
                    self.push(
                        format!("The fireball scorches you for {damage}."),
                        log_colors::HARM,
                    );
                } else {
                    let obj = self.object(world, target);
                    self.push(
                        format!("Your fireball scorches {obj} for {damage}."),
                        log_colors::INFO,
                    );
                }
            }
            DamageKind::Slam => {
                // Boss ground slam; the attacker is never the player.
                if target == me {
                    self.push(
                        format!("The ground slam crushes you for {damage}!"),
                        log_colors::HARM,
                    );
                } else {
                    let obj = self.object(world, target);
                    self.push(
                        format!("The ground slam crushes {obj} for {damage}!"),
                        log_colors::INFO,
                    );
                }
            }
            // Projectile kinds never arrive here.
            DamageKind::Arrow | DamageKind::CripplingShot | DamageKind::Potion => {}
        }
    }

    /// Format a projectile hit (arrow / crippling shot), from the player's POV.
    fn record_projectile(
        &mut self,
        world: &World,
        source: Entity,
        target: Entity,
        damage: i32,
        kind: DamageKind,
    ) {
        let me = self.player_entity;
        if damage <= 0 || (source != me && target != me) {
            return;
        }
        let weapon = match kind {
            DamageKind::Arrow => "arrow",
            DamageKind::CripplingShot => "crippling shot",
            _ => return,
        };

        if source == me {
            let obj = self.object(world, target);
            let mut line = format!("Your {weapon} hits {obj} for {damage}");
            if matches!(kind, DamageKind::CripplingShot) {
                line.push_str(", slowing it.");
            } else {
                line.push('.');
            }
            self.push(line, log_colors::INFO);
        } else {
            let subj = self.subject(world, source);
            self.push(
                format!("{subj}'s {weapon} hits you for {damage}."),
                log_colors::HARM,
            );
        }
    }

    /// Translate a game event into a log line, if it is player-relevant.
    pub fn record_event(&mut self, event: &GameEvent, world: &World) {
        let me = self.player_entity;
        match event {
            GameEvent::AttackHit {
                attacker,
                target,
                damage,
                kind,
                crit,
                ..
            } => {
                self.record_attack(world, *attacker, *target, *damage, *kind, *crit);
            }
            GameEvent::ProjectileHit {
                source,
                target: Some(target),
                damage,
                kind,
                ..
            } => {
                self.record_projectile(world, *source, *target, *damage, *kind);
            }
            GameEvent::EntityDied { entity, .. } => {
                if *entity == me {
                    self.push("You die.".to_string(), log_colors::HARM);
                } else {
                    let who = self.subject(world, *entity);
                    self.push(format!("{who} dies."), log_colors::KILL);
                }
            }
            GameEvent::BurnDamage { entity, damage, .. } => {
                if *entity == me {
                    self.push(format!("You burn for {damage}."), log_colors::HARM);
                } else {
                    let who = self.subject(world, *entity);
                    self.push(format!("{who} burns for {damage}."), log_colors::INFO);
                }
            }
            GameEvent::CaughtFire { entity, .. } => {
                if *entity == me {
                    self.push("You catch fire!".to_string(), log_colors::HARM);
                } else {
                    let who = self.subject(world, *entity);
                    self.push(format!("{who} catches fire!"), log_colors::INFO);
                }
            }
            GameEvent::BarrelExploded { .. } => {
                self.push("An oil barrel explodes!".to_string(), log_colors::HARM);
            }
            GameEvent::BrazierToppled { .. } => {
                self.push(
                    "The brazier topples, spilling burning coals!".to_string(),
                    log_colors::INFO,
                );
            }
            GameEvent::FlaskFilled { entity } if *entity == me => {
                self.push("You fill the flask.".to_string(), log_colors::INFO);
            }
            GameEvent::FlaskFillFailed { entity } if *entity == me => {
                self.push(
                    "There is no water within reach to fill the flask.".to_string(),
                    log_colors::INFO,
                );
            }
            GameEvent::SnareTrapTriggered { victim, .. } => {
                if *victim == me {
                    self.push("You are caught in a snare!".to_string(), log_colors::HARM);
                } else {
                    let who = self.subject(world, *victim);
                    self.push(format!("{who} is caught in a snare!"), log_colors::INFO);
                }
            }
            GameEvent::FireTrapTriggered { victim, .. } => {
                if *victim == me {
                    self.push("You trigger a fire trap!".to_string(), log_colors::HARM);
                } else {
                    let who = self.subject(world, *victim);
                    self.push(format!("{who} triggers a fire trap!"), log_colors::INFO);
                }
            }
            GameEvent::TrapSpotted { .. } => {
                self.push("You spot a trap!".to_string(), log_colors::KILL);
            }
            GameEvent::DungeonTrapTriggered { kind, victim, damage, .. } => {
                use crate::components::DungeonTrapKind;
                let (text, color) = match (kind, *victim == me) {
                    (DungeonTrapKind::Spike, true) => (
                        format!("Hidden spikes stab you for {damage}!"),
                        log_colors::HARM,
                    ),
                    (DungeonTrapKind::Spike, false) => (
                        format!("Hidden spikes stab {}!", self.object(world, *victim)),
                        log_colors::INFO,
                    ),
                    (DungeonTrapKind::Fire, true) => (
                        format!("A fire trap erupts beneath you for {damage}!"),
                        log_colors::HARM,
                    ),
                    (DungeonTrapKind::Fire, false) => (
                        format!("A fire trap erupts beneath {}!", self.object(world, *victim)),
                        log_colors::INFO,
                    ),
                    (DungeonTrapKind::Snare, true) => (
                        "You are caught in a hidden snare!".to_string(),
                        log_colors::HARM,
                    ),
                    (DungeonTrapKind::Snare, false) => (
                        format!("{} is caught in a hidden snare!", self.subject(world, *victim)),
                        log_colors::INFO,
                    ),
                    (DungeonTrapKind::Alarm, true) => (
                        "You step on an alarm plate — a shrill ringing echoes through the dungeon!"
                            .to_string(),
                        log_colors::HARM,
                    ),
                    (DungeonTrapKind::Alarm, false) => (
                        format!("{} sets off an alarm!", self.subject(world, *victim)),
                        log_colors::INFO,
                    ),
                };
                self.push(text, color);
            }
            GameEvent::SecretDoorFound { .. } => {
                self.push("You notice a hidden passage!".to_string(), log_colors::GOOD);
            }
            GameEvent::FountainUsed { entity, outcome } if *entity == me => {
                use crate::events::FountainOutcome;
                let (text, color) = match outcome {
                    FountainOutcome::Heal => (
                        "You drink from the fountain. Cool relief washes over you — fully healed!",
                        log_colors::GOOD,
                    ),
                    FountainOutcome::Food => (
                        "You drink deep from the fountain. Your hunger fades.",
                        log_colors::GOOD,
                    ),
                    FountainOutcome::Buff(effect) => {
                        let text = match effect {
                            crate::components::EffectType::Regenerating => {
                                "The water tingles with magic — your wounds begin to knit."
                            }
                            crate::components::EffectType::Protected => {
                                "The water tingles with magic — a ward settles over you."
                            }
                            _ => "The water tingles with magic — strength floods your limbs.",
                        };
                        (text, log_colors::GOOD)
                    }
                    FountainOutcome::Bad(effect) => {
                        let text = match effect {
                            crate::components::EffectType::Confused => {
                                "The water tastes foul — your head swims!"
                            }
                            _ => "The water tastes foul — your limbs grow sluggish!",
                        };
                        (text, log_colors::HARM)
                    }
                    FountainOutcome::Dry => ("The fountain is dry.", log_colors::SYSTEM),
                };
                self.push(text.to_string(), color);
            }
            GameEvent::AltarSacrificed { item_name, blessed, detail } => {
                let color = if *blessed { log_colors::GOOD } else { log_colors::HARM };
                self.push(format!("You sacrifice {item_name}. {detail}"), color);
            }
            GameEvent::ShrineUsed { entity, fresh } if *entity == me => {
                if *fresh {
                    self.push(
                        "The shrine hums — your possessions are revealed and a ward surrounds you."
                            .to_string(),
                        log_colors::GOOD,
                    );
                } else {
                    self.push("The shrine is cold and inert.".to_string(), log_colors::SYSTEM);
                }
            }
            GameEvent::AbilityActivated { entity, ability } if *entity == me => {
                let text = match ability {
                    AbilityType::Sprint => "You break into a sprint.",
                    AbilityType::Disengage => "You disengage to safety.",
                    AbilityType::Tumble => "You tumble away.",
                    AbilityType::SnareTrap => "You set a snare trap.",
                    AbilityType::LearnedBlink
                    | AbilityType::LearnedFireball
                    | AbilityType::LearnedFear
                    | AbilityType::LearnedSlow
                    | AbilityType::LearnedProtection
                    | AbilityType::LearnedSpeed
                    | AbilityType::LearnedInvisibility => {
                        self.push(format!("You cast {}.", ability.name()), log_colors::INFO);
                        return;
                    }
                    _ => return,
                };
                self.push(text.to_string(), log_colors::INFO);
            }
            GameEvent::SpellLearned { ability } => {
                self.push(
                    format!("You study the scroll and learn {}!", ability.name()),
                    log_colors::GOOD,
                );
            }
            GameEvent::SpellStudyFailed {
                scroll,
                required_int,
                current_int,
            } => {
                self.push(
                    format!(
                        "The {} eludes you — studying it requires {} Intelligence (you have {}).",
                        crate::systems::item_name(*scroll),
                        required_int,
                        current_int
                    ),
                    log_colors::SYSTEM,
                );
            }
            GameEvent::SpellAlreadyKnown { ability } => {
                self.push(
                    format!("You already know {}.", ability.name()),
                    log_colors::SYSTEM,
                );
            }
            GameEvent::RaiseDeadStarted { caster, .. } if *caster == me => {
                self.push(
                    "You begin chanting over the bones...".to_string(),
                    log_colors::INFO,
                );
            }
            GameEvent::RaiseDeadFailed { caster } if *caster == me => {
                self.push("The ritual fizzles.".to_string(), log_colors::SYSTEM);
            }
            GameEvent::SkeletonRaised { owner, .. } if *owner == me => {
                self.push(
                    "A skeleton claws free of the bones and rises to serve you!".to_string(),
                    log_colors::GOOD,
                );
            }
            GameEvent::ItemPickedUp { entity, item } if *entity == me => {
                self.push(
                    format!("You pick up {}.", crate::systems::item_name(*item)),
                    log_colors::GOOD,
                );
            }
            GameEvent::GoldPickedUp { entity, amount } if *entity == me => {
                self.push(format!("You pick up {amount} gold."), log_colors::GOOD);
            }
            GameEvent::PotionDrunk { entity, potion_type } if *entity == me => {
                self.push(
                    format!("You drink {}.", crate::systems::item_name(*potion_type)),
                    log_colors::INFO,
                );
            }
            GameEvent::WeaponEquipped { entity, weapon_type } if *entity == me => {
                self.push(
                    format!("You equip {}.", crate::systems::item_name(*weapon_type)),
                    log_colors::INFO,
                );
            }
            GameEvent::DoorOpened { opener, .. } if *opener == me && !self.door_tip_shown => {
                self.door_tip_shown = true;
                self.system(
                    "Tip: hold Ctrl and press a direction to close a door — useful for shaking pursuers.",
                );
            }
            GameEvent::ItemPurchased { item, price, .. } => {
                self.push(
                    format!("You buy {} for {price} gold.", crate::systems::item_name(*item)),
                    log_colors::GOOD,
                );
            }
            GameEvent::ItemSold { item, value, .. } => {
                self.push(
                    format!("You sell {} for {value} gold.", crate::systems::item_name(*item)),
                    log_colors::GOOD,
                );
            }
            GameEvent::ItemIdentified { name, cursed } => {
                self.push(format!("You recognize: {name}."), log_colors::GOOD);
                if *cursed {
                    self.push("It bears a curse!".to_string(), log_colors::HARM);
                }
            }
            GameEvent::LevelUp { new_level } => {
                self.push(format!("You reach level {new_level}!"), log_colors::GOOD);
            }
            GameEvent::TamingCompleted { tamer, target } if *tamer == me => {
                let who = self.name(world, *target);
                self.push(format!("You tame the {who}."), log_colors::GOOD);
            }
            GameEvent::TamingFailed { tamer, .. } if *tamer == me => {
                self.push("The taming fails.".to_string(), log_colors::SYSTEM);
            }
            GameEvent::BarkskinActivated { entity } if *entity == me => {
                self.push("Your skin hardens into bark.".to_string(), log_colors::INFO);
            }
            GameEvent::StunActivated { entity, .. } if *entity == me => {
                self.push("You unleash a stunning blow.".to_string(), log_colors::INFO);
            }
            GameEvent::FearActivated { entity, .. } if *entity == me => {
                self.push("You let out a terrifying shriek.".to_string(), log_colors::INFO);
            }
            GameEvent::HungerStateChanged { state } => {
                let (text, color) = match state {
                    crate::components::HungerState::Hungry => {
                        ("You are getting hungry.", log_colors::SYSTEM)
                    }
                    crate::components::HungerState::Starving => {
                        ("You are starving!", log_colors::HARM)
                    }
                    crate::components::HungerState::Fed => {
                        ("You no longer feel hungry.", log_colors::GOOD)
                    }
                };
                self.push(text.to_string(), color);
            }
            GameEvent::FatigueStateChanged { state } => {
                let (text, color) = match state {
                    crate::components::FatigueState::Tired => {
                        ("You feel tired.", log_colors::SYSTEM)
                    }
                    crate::components::FatigueState::Exhausted => {
                        ("You are exhausted.", log_colors::HARM)
                    }
                    crate::components::FatigueState::Rested => {
                        ("You feel rested.", log_colors::GOOD)
                    }
                };
                self.push(text.to_string(), color);
            }
            GameEvent::StarvationDamage { entity, damage, .. } if *entity == me => {
                // Consecutive identical lines collapse into "(xN)", so this
                // won't flood the log during fast-forwarded rest.
                self.push(
                    format!("You are wasting away from hunger ({damage})."),
                    log_colors::HARM,
                );
            }
            GameEvent::EnemyHealed { healer, target, amount, .. } => {
                let who = self.subject(world, *healer);
                let obj = self.object(world, *target);
                self.push(
                    format!("{who} chants — a green glow mends {obj} (+{amount})."),
                    log_colors::INFO,
                );
            }
            GameEvent::EnemyHasted { healer, target, .. } => {
                let who = self.subject(world, *healer);
                let obj = self.object(world, *target);
                self.push(format!("{who} shrieks — {obj} speeds up!"), log_colors::INFO);
            }
            GameEvent::WebTouched { victim, .. } => {
                if *victim == me {
                    self.push(
                        "You are tangled in a sticky web!".to_string(),
                        log_colors::HARM,
                    );
                } else {
                    let who = self.subject(world, *victim);
                    self.push(format!("{who} is tangled in a web."), log_colors::INFO);
                }
            }
            GameEvent::BossSighted { name, .. } => {
                self.push(format!("{name} glares at you!"), log_colors::HARM);
            }
            GameEvent::BossDefeated { name } => {
                self.push(format!("{name} is defeated!"), log_colors::KILL);
                self.push(
                    "The dungeon falls silent. A great evil has been vanquished."
                        .to_string(),
                    log_colors::GOOD,
                );
            }
            GameEvent::BossAbilityUsed { boss, ability, .. } => {
                let name = self.name(world, *boss);
                let (text, color) = match ability {
                    crate::components::BossAbility::GroundSlam => {
                        (format!("{name} slams the ground!"), log_colors::HARM)
                    }
                    crate::components::BossAbility::SummonSpiders => (
                        format!("{name} shrieks — spiderlings skitter from the shadows!"),
                        log_colors::HARM,
                    ),
                    crate::components::BossAbility::RaiseDead => (
                        format!("{name} drags a skeleton up from old bones!"),
                        log_colors::HARM,
                    ),
                };
                self.push(text, color);
            }
            GameEvent::FloorTransition {
                direction,
                from_floor,
            } => {
                let (verb, floor) = match direction {
                    StairDirection::Down => ("descend", from_floor + 1),
                    StairDirection::Up => ("ascend", from_floor.saturating_sub(1)),
                };
                self.push(format!("You {verb} to floor {floor}."), log_colors::SYSTEM);
            }
            _ => {}
        }
    }
}

/// Capitalize the first character of a string ("the Skeleton" -> "The Skeleton").
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Draw the message log in the bottom-left corner.
///
/// Rendered as a non-interactive background area so it never steals clicks from
/// the world or other UI.
pub fn draw_message_log(ctx: &egui::Context, log: &MessageLog) {
    if log.messages.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::new("message_log"))
        .order(egui::Order::Background)
        .interactable(false)
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(colors::PANEL_BG.gamma_multiply(0.78))
                .stroke(egui::Stroke::new(super::style::BORDER_WIDTH, colors::PANEL_BORDER))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.set_min_width(LOG_WIDTH);
                    ui.spacing_mut().item_spacing.y = 1.0;

                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    let line_height = ui.fonts(|f| f.row_height(&font));

                    // Oldest of the visible window first, newest at the bottom.
                    let start = log.messages.len().saturating_sub(VISIBLE_MESSAGES);
                    for (i, msg) in log.messages.iter().enumerate().skip(start) {
                        let age = log.messages.len() - 1 - i;
                        draw_line(ui, msg, age, &font, line_height);
                    }
                });
        });
}

/// Draw one log line: faded by how many lines sit below it, and — if it only
/// just arrived — sliding up into place, brighter than it will settle at.
fn draw_line(
    ui: &mut egui::Ui,
    msg: &LogMessage,
    age: usize,
    font: &egui::FontId,
    line_height: f32,
) {
    // How far through its entrance the line is.
    let t = (msg.arrived.elapsed().as_secs_f32() / LOG_LINE_ARRIVE_DURATION).clamp(0.0, 1.0);
    let settled = ease::out_cubic(t);

    // Older lines dim, so the newest always reads as the newest.
    let age_alpha =
        (1.0 - age as f32 * LOG_LINE_AGE_FADE).max(LOG_LINE_MIN_ALPHA);
    // An arriving line flares above its resting colour and fades back down.
    let color = style::brighten(msg.color, 1.0 + LOG_LINE_ARRIVE_LIFT * (1.0 - settled))
        .gamma_multiply(age_alpha * settled);

    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), line_height),
        egui::Sense::hover(),
    );
    // Clipped to its own row so the slide never spills onto the line above.
    let painter = ui.painter().with_clip_rect(row);
    let rise = (1.0 - settled) * LOG_LINE_SLIDE_DISTANCE;
    let origin = row.left_top() + egui::vec2(0.0, rise);

    let galley = painter.layout_no_wrap(msg.text.clone(), font.clone(), color);
    let text_width = galley.size().x;
    painter.galley(origin, galley, color);

    if msg.count > 1 {
        draw_count_badge(&painter, origin + egui::vec2(text_width, 0.0), msg, font, color);
    }
}

/// Draw the "(xN)" repeat badge, hopping and flaring for a moment each time
/// the counter ticks.
///
/// The pop is in brightness and position rather than font size: egui caches
/// rasterized glyphs per distinct size, and a badge that scaled smoothly would
/// feed the atlas a new set every frame for no visual gain over this.
fn draw_count_badge(
    painter: &egui::Painter,
    origin: egui::Pos2,
    msg: &LogMessage,
    font: &egui::FontId,
    base: egui::Color32,
) {
    let t = (msg.counted_at.elapsed().as_secs_f32() / LOG_COUNT_POP_DURATION).clamp(0.0, 1.0);
    let pop = 1.0 - ease::out_cubic(t);
    let color = style::brighten(base, 1.0 + LOG_COUNT_POP_LIFT * pop);
    painter.text(
        origin + egui::vec2(LOG_COUNT_GAP, -LOG_COUNT_POP_RISE * pop),
        egui::Align2::LEFT_TOP,
        format!("(x{})", msg.count),
        font.clone(),
        color,
    );
}
