//! Scrolling combat/message log.
//!
//! In a real-time, energy-based game things happen fast — the log gives the
//! player a readable history of what just occurred ("Skeleton Archer hits you
//! for 6", "You reach level 3!"). It listens to [`GameEvent`]s, formats the
//! player-relevant ones into short lines, and draws them in the bottom-left
//! corner.

use std::collections::VecDeque;

use egui::Color32;
use hecs::{Entity, World};

use super::style::colors;
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
}

/// Rolling buffer of log messages.
pub struct MessageLog {
    messages: VecDeque<LogMessage>,
    /// The player entity, used to phrase messages from the player's POV.
    player_entity: Entity,
}

impl MessageLog {
    pub fn new(player_entity: Entity) -> Self {
        Self {
            messages: VecDeque::new(),
            player_entity,
        }
    }

    /// Push a line. Consecutive identical lines collapse into a "(x2)" counter.
    fn push(&mut self, text: String, color: Color32) {
        if let Some(last) = self.messages.back_mut() {
            if last.text == text && last.color == color {
                last.count += 1;
                return;
            }
        }
        self.messages.push_back(LogMessage {
            text,
            color,
            count: 1,
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
                target,
                damage,
                kind,
                ..
            } => {
                if let Some(target) = target {
                    self.record_projectile(world, *source, *target, *damage, *kind);
                }
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
            GameEvent::AbilityActivated { entity, ability } if *entity == me => {
                let text = match ability {
                    AbilityType::Sprint => "You break into a sprint.",
                    AbilityType::Disengage => "You disengage to safety.",
                    AbilityType::Tumble => "You tumble away.",
                    AbilityType::SnareTrap => "You set a snare trap.",
                    _ => return,
                };
                self.push(text.to_string(), log_colors::INFO);
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
                    ui.set_min_width(360.0);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    // Oldest of the visible window first, newest at the bottom.
                    let start = log.messages.len().saturating_sub(VISIBLE_MESSAGES);
                    for (i, msg) in log.messages.iter().enumerate().skip(start) {
                        // Fade older lines so the newest reads as most prominent.
                        let age = log.messages.len() - 1 - i;
                        let alpha = 1.0 - (age as f32) * 0.11;
                        let color = msg.color.gamma_multiply(alpha.max(0.35));
                        let text = if msg.count > 1 {
                            format!("{} (x{})", msg.text, msg.count)
                        } else {
                            msg.text.clone()
                        };
                        ui.label(egui::RichText::new(text).color(color).monospace());
                    }
                });
        });
}
