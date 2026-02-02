//! Audio system for playing sound effects.
//!
//! Plays sounds in response to game events.

use rand::seq::SliceRandom;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::events::GameEvent;

/// Sound categories for organizing effects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundType {
    // Battle
    MeleeSwing,
    Spell,
    WeaponEquip,

    // Inventory
    CoinPickup,
    ItemPickup,
    PotionPickup,
    PotionDrink,
    PotionThrow,

    // Interface
    UiClick,

    // World
    DoorOpen,

    // Enemies
    MonsterHit,
    SlimeSound,
    ShadeSound,
}

/// Audio manager that handles sound playback
pub struct AudioManager {
    /// Keep the stream alive
    _stream: OutputStream,
    /// Handle for creating sinks
    stream_handle: OutputStreamHandle,
    /// Cached sound file paths by type
    sounds: HashMap<SoundType, Vec<PathBuf>>,
    /// Master volume (0.0 - 1.0)
    volume: f32,
}

impl AudioManager {
    /// Create a new audio manager and load sounds
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = OutputStream::try_default().ok()?;

        let mut manager = Self {
            _stream: stream,
            stream_handle,
            sounds: HashMap::new(),
            volume: 0.5,
        };

        manager.load_sounds();
        Some(manager)
    }

    /// Load all sound file paths
    fn load_sounds(&mut self) {
        let base = PathBuf::from("assets/sounds");

        // Battle sounds
        self.sounds.insert(
            SoundType::MeleeSwing,
            Self::find_sounds(&base.join("battle"), &["swing.wav", "swing2.wav", "swing3.wav"]),
        );
        self.sounds.insert(
            SoundType::Spell,
            Self::find_sounds(&base.join("battle"), &["spell.wav", "magic1.wav"]),
        );
        self.sounds.insert(
            SoundType::WeaponEquip,
            Self::find_sounds(
                &base.join("battle"),
                &[
                    "sword-unsheathe.wav",
                    "sword-unsheathe2.wav",
                    "sword-unsheathe3.wav",
                ],
            ),
        );

        // Inventory sounds
        self.sounds.insert(
            SoundType::CoinPickup,
            Self::find_sounds(&base.join("inventory"), &["coin.wav", "coin2.wav", "coin3.wav"]),
        );
        self.sounds.insert(
            SoundType::ItemPickup,
            Self::find_sounds(
                &base.join("inventory"),
                &["metal-small1.wav", "metal-small2.wav", "metal-small3.wav"],
            ),
        );
        self.sounds.insert(
            SoundType::PotionPickup,
            Self::find_sounds(&base.join("inventory"), &["bubble.wav"]),
        );
        self.sounds.insert(
            SoundType::PotionDrink,
            Self::find_sounds(&base.join("inventory"), &["bubble3.wav"]),
        );
        self.sounds.insert(
            SoundType::PotionThrow,
            Self::find_sounds(&base.join("inventory"), &["bottle.wav"]),
        );

        // Interface sounds
        self.sounds.insert(
            SoundType::UiClick,
            Self::find_sounds(&base.join("interface"), &["interface1.wav"]),
        );

        // World sounds
        self.sounds.insert(
            SoundType::DoorOpen,
            Self::find_sounds(&base.join("world"), &["door.wav"]),
        );

        // Enemy sounds
        self.sounds.insert(
            SoundType::MonsterHit,
            Self::find_sounds(
                &base.join("enemies"),
                &["mnstr1.wav", "mnstr2.wav", "mnstr3.wav", "mnstr4.wav"],
            ),
        );
        self.sounds.insert(
            SoundType::SlimeSound,
            Self::find_sounds(
                &base.join("enemies"),
                &["slime1.wav", "slime2.wav", "slime3.wav"],
            ),
        );
        self.sounds.insert(
            SoundType::ShadeSound,
            Self::find_sounds(
                &base.join("enemies"),
                &["shade1.wav", "shade2.wav", "shade3.wav"],
            ),
        );
    }

    /// Find sound files that exist
    fn find_sounds(dir: &PathBuf, filenames: &[&str]) -> Vec<PathBuf> {
        filenames
            .iter()
            .map(|f| dir.join(f))
            .filter(|p| p.exists())
            .collect()
    }

    /// Maximum distance at which sounds can be heard (in tiles)
    const HEARING_RANGE: f32 = 15.0;

    /// Play a random sound of the given type at full volume (for UI sounds)
    pub fn play(&self, sound_type: SoundType) {
        self.play_with_volume(sound_type, 1.0);
    }

    /// Play a sound with distance-based volume attenuation
    /// Returns false if the sound is out of range
    pub fn play_at_distance(&self, sound_type: SoundType, distance: f32) -> bool {
        if distance > Self::HEARING_RANGE {
            return false;
        }

        // Calculate volume falloff: full volume at distance 0, zero at HEARING_RANGE
        // Use inverse square-ish falloff for more natural sound
        let normalized_distance = distance / Self::HEARING_RANGE;
        let volume_multiplier = (1.0 - normalized_distance).powi(2);

        self.play_with_volume(sound_type, volume_multiplier);
        true
    }

    /// Play a sound with a specific volume multiplier
    fn play_with_volume(&self, sound_type: SoundType, volume_multiplier: f32) {
        let Some(paths) = self.sounds.get(&sound_type) else {
            return;
        };

        if paths.is_empty() {
            return;
        }

        let mut rng = rand::thread_rng();
        let path = paths.choose(&mut rng).unwrap();

        self.play_file_with_volume(path, volume_multiplier);
    }

    /// Play a specific sound file with volume multiplier
    fn play_file_with_volume(&self, path: &PathBuf, volume_multiplier: f32) {
        let Ok(file) = File::open(path) else {
            return;
        };

        let reader = BufReader::new(file);
        let Ok(source) = Decoder::new(reader) else {
            return;
        };

        let Ok(sink) = Sink::try_new(&self.stream_handle) else {
            return;
        };

        let final_volume = self.volume * volume_multiplier;
        sink.set_volume(final_volume);
        sink.append(source.amplify(final_volume));
        sink.detach(); // Let it play in background
    }

    /// Calculate distance between two positions
    fn distance(p1: (i32, i32), p2: (i32, i32)) -> f32 {
        let dx = (p1.0 - p2.0) as f32;
        let dy = (p1.1 - p2.1) as f32;
        (dx * dx + dy * dy).sqrt()
    }

    /// Process game events and play appropriate sounds
    /// player_pos is the player's current position for distance-based audio
    pub fn process_events(&self, events: &[GameEvent], player_pos: (i32, i32)) {
        use crate::components::ItemType;

        for event in events {
            match event {
                GameEvent::AttackHit { target_pos, .. } => {
                    let pos = (target_pos.0 as i32, target_pos.1 as i32);
                    let dist = Self::distance(player_pos, pos);
                    self.play_at_distance(SoundType::MeleeSwing, dist);
                }
                GameEvent::EntityDied { position, .. } => {
                    let pos = (position.0 as i32, position.1 as i32);
                    let dist = Self::distance(player_pos, pos);
                    self.play_at_distance(SoundType::MonsterHit, dist);
                }
                GameEvent::DoorOpened { position, .. } => {
                    let dist = Self::distance(player_pos, *position);
                    self.play_at_distance(SoundType::DoorOpen, dist);
                }
                GameEvent::ContainerOpened { container_type, position, .. } => {
                    // Only play sound for actual chests, not bodies or ground items
                    if *container_type == Some(crate::components::ContainerType::Chest) {
                        let dist = Self::distance(player_pos, *position);
                        self.play_at_distance(SoundType::DoorOpen, dist);
                    }
                }
                // Player-only sounds (always full volume since they're at player position)
                GameEvent::ItemPickedUp { item, .. } => {
                    // Potions get a different sound
                    if matches!(
                        item,
                        ItemType::HealthPotion
                            | ItemType::RegenerationPotion
                            | ItemType::StrengthPotion
                            | ItemType::ConfusionPotion
                    ) {
                        self.play(SoundType::PotionPickup);
                    } else {
                        self.play(SoundType::ItemPickup);
                    }
                }
                GameEvent::GoldPickedUp { .. } => {
                    self.play(SoundType::CoinPickup);
                }
                GameEvent::LevelUp { .. } => {
                    self.play(SoundType::UiClick);
                }
                GameEvent::ProjectileHit { position, .. } => {
                    let dist = Self::distance(player_pos, *position);
                    self.play_at_distance(SoundType::MeleeSwing, dist);
                }
                GameEvent::FireballExplosion { x, y, .. } => {
                    let dist = Self::distance(player_pos, (*x, *y));
                    self.play_at_distance(SoundType::Spell, dist);
                }
                GameEvent::PotionSplash { x, y, .. } => {
                    let dist = Self::distance(player_pos, (*x, *y));
                    self.play_at_distance(SoundType::PotionThrow, dist);
                }
                GameEvent::PotionDrunk { .. } => {
                    self.play(SoundType::PotionDrink);
                }
                GameEvent::WeaponEquipped { .. } => {
                    self.play(SoundType::WeaponEquip);
                }
                GameEvent::CleavePerformed { center, .. } => {
                    let dist = Self::distance(player_pos, *center);
                    self.play_at_distance(SoundType::MeleeSwing, dist);
                }
                GameEvent::BarkskinActivated { .. } => {
                    self.play(SoundType::Spell);
                }
                GameEvent::FearActivated { position, .. } => {
                    let dist = Self::distance(player_pos, *position);
                    self.play_at_distance(SoundType::ShadeSound, dist);
                }
                GameEvent::ItemPurchased { .. } | GameEvent::ItemSold { .. } => {
                    self.play(SoundType::CoinPickup);
                }
                GameEvent::FireTrapTriggered { position, .. } => {
                    let dist = Self::distance(player_pos, *position);
                    self.play_at_distance(SoundType::Spell, dist);
                }
                GameEvent::LifeDrainStarted { .. } => {
                    self.play(SoundType::Spell);
                }
                _ => {}
            }
        }
    }

    /// Set the master volume (0.0 - 1.0)
    #[allow(dead_code)]
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }
}
