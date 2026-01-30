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
    SwordUnsheathe,

    // Inventory
    CoinPickup,
    ItemPickup,
    PotionUse,
    ArmorEquip,

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
            SoundType::SwordUnsheathe,
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
            SoundType::PotionUse,
            Self::find_sounds(&base.join("inventory"), &["bottle.wav", "bubble.wav"]),
        );
        self.sounds.insert(
            SoundType::ArmorEquip,
            Self::find_sounds(&base.join("inventory"), &["armor-light.wav", "chainmail1.wav"]),
        );

        // Interface sounds
        self.sounds.insert(
            SoundType::UiClick,
            Self::find_sounds(
                &base.join("interface"),
                &["interface1.wav", "interface2.wav", "interface3.wav"],
            ),
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

    /// Play a random sound of the given type
    pub fn play(&self, sound_type: SoundType) {
        let Some(paths) = self.sounds.get(&sound_type) else {
            return;
        };

        if paths.is_empty() {
            return;
        }

        let mut rng = rand::thread_rng();
        let path = paths.choose(&mut rng).unwrap();

        self.play_file(path);
    }

    /// Play a specific sound file
    fn play_file(&self, path: &PathBuf) {
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

        sink.set_volume(self.volume);
        sink.append(source.amplify(self.volume));
        sink.detach(); // Let it play in background
    }

    /// Process game events and play appropriate sounds
    pub fn process_events(&self, events: &[GameEvent]) {
        for event in events {
            match event {
                GameEvent::AttackHit { .. } => {
                    self.play(SoundType::MeleeSwing);
                }
                GameEvent::EntityDied { .. } => {
                    self.play(SoundType::MonsterHit);
                }
                GameEvent::DoorOpened { .. } => {
                    self.play(SoundType::DoorOpen);
                }
                GameEvent::ContainerOpened { .. } => {
                    self.play(SoundType::DoorOpen);
                }
                GameEvent::ItemPickedUp { .. } => {
                    self.play(SoundType::ItemPickup);
                }
                GameEvent::GoldPickedUp { .. } => {
                    self.play(SoundType::CoinPickup);
                }
                GameEvent::LevelUp { .. } => {
                    self.play(SoundType::UiClick);
                }
                GameEvent::ProjectileHit { .. } => {
                    self.play(SoundType::MeleeSwing);
                }
                GameEvent::FireballExplosion { .. } => {
                    self.play(SoundType::Spell);
                }
                GameEvent::PotionSplash { .. } => {
                    self.play(SoundType::PotionUse);
                }
                GameEvent::CleavePerformed { .. } => {
                    self.play(SoundType::MeleeSwing);
                }
                GameEvent::BarkskinActivated { .. } => {
                    self.play(SoundType::Spell);
                }
                GameEvent::FearActivated { .. } => {
                    self.play(SoundType::ShadeSound);
                }
                GameEvent::ItemPurchased { .. } | GameEvent::ItemSold { .. } => {
                    self.play(SoundType::CoinPickup);
                }
                GameEvent::FireTrapTriggered { .. } => {
                    self.play(SoundType::Spell);
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
