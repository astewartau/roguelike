//! What the prefab builder knows about each component: which group it sits
//! in, what it does in a sentence, a few words for the preview card, and
//! what else it needs before it does anything. Also the building blocks:
//! sets of components that make one recognisable thing ("a creature", "a
//! light") in one click.
//!
//! Every component in the registry has an entry here; the test at the
//! bottom fails if one is added without it.

/// Groups, by what the components in them do, in the order the picker shows
/// them.
pub const GROUPS: [&str; 8] = ["Looks", "Body", "Mind", "Attacks", "Fire & light", "Things", "Allies & people", "Internal"];

/// One component, as the builder describes it.
pub struct Info {
    pub name: &'static str,
    pub group: &'static str,
    /// A few words for the preview card ("hunts you", "burns"); empty if it
    /// says nothing worth listing there.
    pub phrase: &'static str,
    /// What it does, in a sentence.
    pub summary: &'static str,
    /// What it does nothing without, as a hint: "A|B" means either will do.
    /// Hard requirements (one right answer) aren't hints; they're
    /// `component_registry::REQUIRES`, and the world adds them itself.
    pub needs: &'static [&'static str],
}

const fn info(
    name: &'static str,
    group: &'static str,
    phrase: &'static str,
    summary: &'static str,
    needs: &'static [&'static str],
) -> Info {
    Info { name, group, phrase, summary, needs }
}

pub const INFO: &[Info] = &[
    // Looks
    info("Name", "Looks", "", "What it's called in tooltips and the message log.", &[]),
    info("Sprite", "Looks", "", "The picture it's drawn with.", &[]),
    info("AnimatedSprite", "Looks", "", "An animated picture, drawn over its sprite (flames, water).", &[]),
    info("SpriteTint", "Looks", "", "Tints its picture (multiplies the colours; white is untinted).", &["Sprite|AnimatedSprite"]),
    info("OverlaySprite", "Looks", "", "A second picture drawn on top, like an archer's bow.", &[]),
    info("FlameOverlay", "Looks", "", "Animated flames drawn over its picture.", &["Sprite"]),
    // Body
    info("Health", "Body", "", "Can be hurt (by blows, blasts, fire): hit points, and whether you'd attack it freely, only on purpose, or never.", &[]),
    info("Stats", "Body", "", "Strength, intelligence and agility: damage, spell power, dodging.", &[]),
    info("StatusEffects", "Body", "", "Can be burning, poisoned, slowed, wet and so on.", &[]),
    info("BlocksMovement", "Body", "blocks the way", "Nothing can stand on its tile.", &[]),
    info("BlocksVision", "Body", "blocks sight", "Can't be seen past.", &[]),
    info("Pushable", "Body", "can be shoved", "Can be shoved a tile at a time.", &[]),
    info("Flying", "Body", "flies", "Hovers over water, traps and webs.", &[]),
    info("Equipment", "Body", "", "What it wields and wears; its weapon sets its melee damage.", &["Actor"]),
    info("XpMultiplier", "Body", "worth extra XP", "Worth this many times the usual XP when killed.", &["Health"]),
    // Mind
    info("Actor", "Mind", "", "Takes turns. Speed is how fast it acts.", &["ChaseAI|CompanionAI"]),
    info("ChaseAI", "Mind", "hunts you", "Hostile: wanders, notices you, chases and attacks.", &[]),
    info("Asleep", "Mind", "asleep", "Asleep until something wakes it.", &[]),
    info("CanOpenDoors", "Mind", "opens doors", "Opens doors, and shouts to wake its allies.", &["ChaseAI|CompanionAI"]),
    info("SeesInDark", "Mind", "sees in the dark", "Sees you in the dark.", &[]),
    info("FearImmune", "Mind", "fearless", "Never flees and can't be feared.", &["ChaseAI"]),
    info("SupportAI", "Mind", "heals allies", "Keeps its distance and heals or hastes its allies.", &[]),
    info("Boss", "Mind", "a boss", "Uses its boss ability on a cooldown.", &["ChaseAI"]),
    info("Flanker", "Mind", "flanks", "Circles round to attack from the side.", &[]),
    info("PackHunter", "Mind", "hunts in packs", "Braver near packmates; hurting one alerts the rest.", &[]),
    info("Charger", "Mind", "charges", "Charges in a straight line.", &[]),
    info("HitAndRun", "Mind", "hits and runs", "Breaks off after each attack.", &[]),
    info("Tameable", "Mind", "tameable", "A druid can tame it.", &["ChaseAI"]),
    info("Faction", "Mind", "takes a side", "Which side it's on: it fights the other sides and leaves its own be.", &["ChaseAI|CompanionAI"]),
    info("Disposition", "Mind", "", "How it meets its enemies: goes for them, fights only once hurt, or runs.", &[]),
    info("DevFrozen", "Mind", "frozen", "Developer tools: it waits forever.", &["Actor"]),
    // Attacks
    info("Venomous", "Attacks", "venomous", "Its hits slow and poison.", &["ChaseAI|CompanionAI"]),
    info("Lacerating", "Attacks", "makes you bleed", "Its hits may cause bleeding.", &["ChaseAI|CompanionAI"]),
    info("Grabber", "Attacks", "grabs", "Its hits can grab and hold you.", &["ChaseAI"]),
    info("Splits", "Attacks", "splits when hurt", "Splits in two when badly hurt.", &[]),
    // Fire & light
    info("LightSource", "Fire & light", "glows", "Gives off light: radius, brightness, colour, flicker.", &[]),
    info("CarriedTorch", "Fire & light", "carries a torch", "Carries a torch: a walking light that sets oil and webs alight.", &["Actor"]),
    info("Combustible", "Fire & light", "catches fire", "Can catch fire (as a creature does: it starts Burning).", &[]),
    info("Fuel", "Fire & light", "burns", "Lies there and can burn: how readily, how long, what it looks like lit.", &[]),
    info("OnFire", "Fire & light", "on fire", "Burning right now (fuel only).", &[]),
    info("CausesBurning", "Fire & light", "burns what steps in", "Sets alight whatever steps onto it.", &[]),
    info("Topples", "Fire & light", "topples", "Knocked into while lit, it tips over and spills its fire.", &["Fuel"]),
    info("Shatters", "Fire & light", "shatters", "Breaks at 0 HP into a puddle of burning oil.", &[]),
    info("Explosive", "Fire & light", "explodes", "Burning or broken open, it lights a fuse and blows up.", &[]),
    info("BarrelFuse", "Fire & light", "fuse lit", "A lit fuse; it goes off when this runs out.", &[]),
    info("OilPuddle", "Fire & light", "oily", "Oil: coats whoever walks through it, and burns hot.", &["Fuel"]),
    info("WetTile", "Fire & light", "wet", "A soaked tile: damp fuel on it won't catch.", &[]),
    info("RainCloud", "Fire & light", "rains", "Rain over its area, soaking it again and again.", &[]),
    // Things
    info("Container", "Things", "holds loot", "Holds items and gold.", &[]),
    info("GroundItemPile", "Things", "", "Loose items lying on the floor: picked up, not opened.", &[]),
    info("Raisable", "Things", "raisable", "The dead can be raised from it, or it detonated.", &[]),
    info("Door", "Things", "", "Opens and closes.", &["BlocksMovement"]),
    info("SecretDoor", "Things", "", "Looks like wall until found; then it's a door.", &["BlocksMovement"]),
    info("Trap", "Things", "a trap", "Goes off under whoever steps on it, and is used up.", &[]),
    info("Concealed", "Things", "hidden", "Invisible until the player spots it, then shows this look.", &[]),
    info("Furniture", "Things", "", "A fountain, altar or shrine to use.", &["BlocksMovement"]),
    info("Web", "Things", "sticky", "Roots whoever walks into it, and is used up.", &[]),
    info("WebSpinner", "Things", "spins webs", "Lays webs on its own tile every few seconds.", &[]),
    info("WebWalker", "Things", "walks on webs", "Crosses webs without being caught.", &[]),
    // Allies & people
    info("Follower", "Allies & people", "on your side", "On someone's side (you, if placed from a prefab).", &[]),
    info("CompanionAI", "Allies & people", "follows you", "Follows its leader and fights whoever threatens them.", &[]),
    info("Summoned", "Allies & people", "summoned", "Counts against its summoner's limit.", &[]),
    info("FriendlyNPC", "Allies & people", "friendly", "Can be talked to, never attacked by bumping.", &["Name"]),
    info("Vendor", "Allies & people", "sells things", "Buys and sells: its stock and gold.", &[]),
    info("Dialogue", "Allies & people", "", "What it says when talked to.", &["FriendlyNPC"]),
    // Internal: state the game keeps for itself, hidden from the picker by
    // default.
    info("Position", "Internal", "", "Where it is (set by where it's placed).", &[]),
    info("VisualPosition", "Internal", "", "Where it's drawn while it moves.", &[]),
    info("Channel", "Internal", "", "Something it's in the middle of channelling.", &[]),
    info("LightExposure", "Internal", "", "How lit it is right now.", &[]),
    info("RootStruggleNoticed", "Internal", "", "Rooted-struggle bookkeeping.", &[]),
    info("RangedCooldown", "Internal", "", "Time until its next shot.", &["ChaseAI"]),
    info("GrabbedBy", "Internal", "", "Who is holding it.", &[]),
];

/// Components that only adjust another one (their host): the builder shows
/// each as a switch inside its host's card, with its own few settings
/// under it, rather than as a card of its own. They stay separate
/// components (systems ask for them directly); this is only how they're
/// edited. Each is (modifier, host or "A|B" hosts, the switch's label).
pub const MODIFIERS: &[(&str, &str, &str)] = &[
    ("Asleep", "ChaseAI", "asleep"),
    ("SeesInDark", "ChaseAI", "sees in the dark"),
    ("Tameable", "ChaseAI", "can be tamed"),
    ("SupportAI", "ChaseAI", "heals and hastes allies"),
    ("Flanker", "ChaseAI", "flanks"),
    ("PackHunter", "ChaseAI", "hunts in packs"),
    ("Charger", "ChaseAI", "charges"),
    ("HitAndRun", "ChaseAI", "hits and runs"),
    ("WebSpinner", "ChaseAI", "spins webs"),
    ("Disposition", "ChaseAI", "temperament"),
    ("CanOpenDoors", "ChaseAI|CompanionAI", "opens doors"),
    ("FearImmune", "ChaseAI|CompanionAI", "fearless"),
    ("Venomous", "ChaseAI|CompanionAI", "venomous bite"),
    ("Lacerating", "ChaseAI|CompanionAI", "makes wounds bleed"),
    ("Grabber", "ChaseAI|CompanionAI", "grabs"),
    ("DevFrozen", "Actor", "frozen (dev)"),
    ("SpriteTint", "Sprite|AnimatedSprite", "tinted"),
    ("OverlaySprite", "Sprite", "second picture on top"),
    ("FlameOverlay", "Sprite", "flames on top"),
];

/// The host `modifier` sits in, among those `has` (the first present), if
/// it's a modifier and one of its hosts is there.
pub fn host_of(modifier: &str, has: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    MODIFIERS
        .iter()
        .find(|(m, _, _)| *m == modifier)
        .and_then(|(_, hosts, _)| hosts.split('|').find(|h| has(h)))
}

/// Reactions: what sets each one off, and what each trigger needs on the
/// entity to be possible ("" for nothing more). The builder shows them as
/// a "goes off when" line, ticked or crossed.
pub const TRIGGERS: &[(&str, &[(&str, &str)])] = &[
    ("Explosive", &[("hurt", "Health"), ("set burning", "Combustible")]),
    ("Shatters", &[("broken (0 HP)", "Health"), ("knocked into", "")]),
    ("Splits", &[("badly hurt", "Health")]),
    ("Topples", &[("knocked into while lit", "Fuel")]),
    ("Trap", &[("stepped on", "")]),
    ("Web", &[("walked into", "")]),
];

/// What sets `name` off, if it's a reaction.
pub fn triggers_of(name: &str) -> &'static [(&'static str, &'static str)] {
    TRIGGERS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).unwrap_or(&[])
}

/// What the builder knows about `name`, if anything.
pub fn info_of(name: &str) -> Option<&'static Info> {
    INFO.iter().find(|i| i.name == name)
}

/// Whether `need` ("A" or "A|B") is met by `have`.
pub fn need_met(need: &str, have: &[&str]) -> bool {
    need.split('|').any(|n| have.contains(&n))
}

/// A set of components that together make one recognisable thing.
pub struct Block {
    pub name: &'static str,
    pub summary: &'static str,
    pub parts: &'static [&'static str],
}

pub const BLOCKS: &[Block] = &[
    Block {
        name: "Creature",
        summary: "A body and a mind: hostile, takes turns, can be hit, blocks its tile.",
        parts: &["Name", "Sprite", "Health", "Stats", "StatusEffects", "BlocksMovement", "Actor", "ChaseAI", "Equipment"],
    },
    Block {
        name: "Companion",
        summary: "On your side and follows you (turns a hostile mind friendly).",
        parts: &["Actor", "Follower", "CompanionAI"],
    },
    Block {
        name: "Breakable",
        summary: "Has hit points: can be hit, blasted and burned.",
        parts: &["Health", "StatusEffects"],
    },
    Block { name: "Obstacle", summary: "Stands in the way.", parts: &["BlocksMovement"] },
    Block { name: "Wall-like", summary: "Stands in the way and blocks sight.", parts: &["BlocksMovement", "BlocksVision"] },
    Block { name: "Light", summary: "Gives off light.", parts: &["LightSource"] },
    Block { name: "Flammable thing", summary: "Lies there and can burn.", parts: &["Fuel"] },
    Block { name: "Flammable creature", summary: "Can catch fire and burn.", parts: &["Combustible", "StatusEffects"] },
    Block { name: "Loot", summary: "Holds items and gold, and stands in the way until emptied.", parts: &["Name", "Container", "BlocksMovement"] },
    Block {
        name: "Shopkeeper",
        summary: "Talks, and buys and sells.",
        parts: &["Name", "Sprite", "FriendlyNPC", "Vendor", "Dialogue", "BlocksMovement"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_component_is_described() {
        for entry in crate::component_registry::entries() {
            let info = info_of(entry.name).unwrap_or_else(|| panic!("{} has no catalogue entry", entry.name));
            assert!(GROUPS.contains(&info.group), "{}: unknown group {}", entry.name, info.group);
            for need in info.needs {
                for n in need.split('|') {
                    assert!(crate::component_registry::find(n).is_some(), "{} needs unknown {n}", entry.name);
                }
            }
        }
        for info in INFO {
            for need in info.needs {
                assert!(
                    !crate::component_registry::all_requirements(info.name).contains(need),
                    "{} lists {need} as a hint, but it's a hard requirement the world adds itself",
                    info.name
                );
            }
            assert!(crate::component_registry::find(info.name).is_some(), "{} is described but not registered", info.name);
        }
        for (modifier, hosts, _) in MODIFIERS {
            assert!(crate::component_registry::find(modifier).is_some(), "modifier {modifier} isn't registered");
            for host in hosts.split('|') {
                assert!(crate::component_registry::find(host).is_some(), "{modifier}'s host {host} isn't registered");
            }
        }
        for (reaction, triggers) in TRIGGERS {
            assert!(crate::component_registry::find(reaction).is_some(), "reaction {reaction} isn't registered");
            for (_, need) in *triggers {
                assert!(need.is_empty() || crate::component_registry::find(need).is_some(), "{reaction} trigger needs {need}");
            }
        }
        for block in BLOCKS {
            for part in block.parts {
                assert!(crate::component_registry::find(part).is_some(), "{}: unknown part {part}", block.name);
            }
        }
    }

    /// A modifier lives in its host's card when the host is there (and is
    /// an ordinary entry otherwise); a reaction knows what each of its
    /// triggers needs.
    #[test]
    fn modifiers_find_their_host_and_reactions_their_triggers() {
        let hostile = |n: &str| ["Actor", "ChaseAI", "Sprite"].contains(&n);
        assert_eq!(host_of("Charger", &hostile), Some("ChaseAI"));
        assert_eq!(host_of("Venomous", &hostile), Some("ChaseAI"));
        assert_eq!(host_of("SpriteTint", &hostile), Some("Sprite"));
        let companion = |n: &str| ["Actor", "CompanionAI", "Follower"].contains(&n);
        assert_eq!(host_of("Venomous", &companion), Some("CompanionAI"), "either mind hosts it");
        assert_eq!(host_of("Charger", &companion), None, "only a hostile mind charges");
        assert_eq!(host_of("Explosive", &hostile), None, "not a modifier");

        let explosive: Vec<_> = triggers_of("Explosive").iter().map(|(_, need)| *need).collect();
        assert_eq!(explosive, vec!["Health", "Combustible"]);
        assert!(triggers_of("Name").is_empty());
    }
}

