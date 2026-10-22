//! Fire fading in as it catches and out as it goes out, rather than popping
//! on and off in a frame.
//!
//! Purely how fire looks. A fire is lit or not in the world (`OnFire`, the
//! Burning status, a fire trap's `FlameOverlay`); this remembers, in real
//! time, when each flame was first drawn, so it can fade in, and keeps
//! drawing one that has gone (its thing despawned, or no longer burning) at
//! its last place while it fades away. The light map does the same with a
//! fire's glow. Real time rather than game time: the game clock stands still
//! while it waits for you, and a fade would freeze halfway.

use std::collections::HashMap;

use hecs::Entity;

use crate::components::{AnimatedSprite, LightSource, Sprite};
use crate::constants::{FIRE_FADE_IN_SECONDS, FIRE_FADE_OUT_SECONDS, FIRE_FRAME_DURATION};
use crate::tile::{tile_ids, SpriteSheet};

use super::rendering::RenderEntity;

/// Which of a thing's flames: it can be fuel burning and a burning creature
/// at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlameKind {
    /// Fuel alight (`OnFire`): burning grass, oil, a dropped torch.
    Fuel,
    /// The Burning status on a creature.
    Burning,
    /// A flame drawn over something that asks for one (an armed fire trap).
    Overlay,
}

/// How a flame animates, so one that's going out keeps flickering as it
/// fades.
#[derive(Debug, Clone, Copy)]
pub enum FlameFrames {
    /// Its own animated sprite (a fuel's lit look).
    Animated(AnimatedSprite),
    /// The open-flames row, offset by `phase` seconds so neighbours don't
    /// flicker in step.
    Flames { phase: f32 },
}

impl FlameFrames {
    /// The frame to show at `real_time`.
    pub fn sprite(&self, real_time: f32) -> Sprite {
        match *self {
            FlameFrames::Animated(anim) => Sprite::new(anim.sheet, anim.current_tile_id(real_time)),
            FlameFrames::Flames { phase } => {
                let frame = ((real_time + phase) / FIRE_FRAME_DURATION) as u32 % 6;
                Sprite::new(SpriteSheet::AnimatedTiles, tile_ids::FIRE_EFFECT.1 + frame)
            }
        }
    }
}

/// A flame as last drawn, to go on drawing it while it fades out.
#[derive(Clone)]
struct Flame {
    /// Real time it was first drawn (lit) or went out (dying).
    since: f32,
    drawn: RenderEntity,
    frames: FlameFrames,
    /// Its glow, if it gives one, and where.
    light: Option<((i32, i32), LightSource)>,
}

/// The fade state of every flame (owned by the vfx manager).
#[derive(Default)]
pub struct FireFades {
    lit: HashMap<(Entity, FlameKind), Flame>,
    /// Those drawn this frame (the rest of `lit` have gone out).
    seen: Vec<(Entity, FlameKind)>,
    dying: Vec<Flame>,
}

/// 0 to 1 over `seconds`, easing out (quick to show, settling gently).
/// Shared with rain's fade (`ui::RainFades`).
pub(crate) fn ease(t: f32, seconds: f32) -> f32 {
    let x = (t / seconds).clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x)
}

impl FireFades {
    /// A flame drawn this frame: how far it has faded in (0 to 1), counting
    /// from the first frame it was drawn. Remembered as drawn, to fade out
    /// from if it goes.
    pub fn shown(
        &mut self,
        key: (Entity, FlameKind),
        real_time: f32,
        drawn: &RenderEntity,
        frames: FlameFrames,
        light: Option<((i32, i32), LightSource)>,
    ) -> f32 {
        self.seen.push(key);
        let flame = self.lit.entry(key).or_insert_with(|| Flame {
            since: real_time,
            drawn: drawn.clone(),
            frames,
            light,
        });
        flame.drawn = drawn.clone();
        flame.frames = frames;
        flame.light = light;
        ease(real_time - flame.since, FIRE_FADE_IN_SECONDS)
    }

    /// After the frame's flames are drawn: those not drawn this frame have
    /// gone out and start fading. Returns what's fading, to draw on top.
    ///
    /// A burning item picked up (`world` says it's held now) didn't go out:
    /// its fire went with it, so nothing is left fading on the floor.
    pub fn finish(&mut self, world: &hecs::World, real_time: f32) -> Vec<RenderEntity> {
        let seen = std::mem::take(&mut self.seen);
        let gone: Vec<(Entity, FlameKind)> = self.lit.keys().filter(|k| !seen.contains(k)).copied().collect();
        for key in gone {
            let carried_off = world.get::<&crate::components::ItemInstance>(key.0).is_ok()
                && world.get::<&crate::components::Position>(key.0).is_err();
            if carried_off {
                self.lit.remove(&key);
                continue;
            }
            if let Some(mut flame) = self.lit.remove(&key) {
                // It fades out from about as far as it had faded in (one
                // caught and gone at once barely shows at all).
                let shown = ease(real_time - flame.since, FIRE_FADE_IN_SECONDS);
                flame.since = real_time - (1.0 - shown) * FIRE_FADE_OUT_SECONDS;
                self.dying.push(flame);
            }
        }
        self.dying.retain(|f| real_time - f.since < FIRE_FADE_OUT_SECONDS);
        self.dying
            .iter()
            .map(|f| {
                let mut drawn = f.drawn.clone();
                drawn.sprite = f.frames.sprite(real_time);
                drawn.alpha *= 1.0 - ease(real_time - f.since, FIRE_FADE_OUT_SECONDS);
                drawn
            })
            .collect()
    }

    /// How far the glow of `entity`'s `kind` of flame has faded in (1 for
    /// one this hasn't seen drawn, such as a fire out of sight).
    pub fn light_fade(&self, entity: Entity, kind: FlameKind, real_time: f32) -> f32 {
        self.lit
            .get(&(entity, kind))
            .map(|f| ease(real_time - f.since, FIRE_FADE_IN_SECONDS))
            .unwrap_or(1.0)
    }

    /// The glow of flames going out, dimmed as far as they've faded.
    pub fn dying_lights(&self, real_time: f32) -> Vec<((i32, i32), LightSource)> {
        self.dying
            .iter()
            .filter_map(|f| {
                let (pos, mut light) = f.light?;
                light.intensity *= 1.0 - ease(real_time - f.since, FIRE_FADE_OUT_SECONDS);
                Some((pos, light))
            })
            .collect()
    }

    /// Forget every flame (a new floor: nothing there was burning here).
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::rendering::RenderEntity;

    fn flame() -> RenderEntity {
        RenderEntity {
            x: 1.0,
            y: 1.0,
            sprite: Sprite::new(SpriteSheet::AnimatedTiles, tile_ids::FIRE_EFFECT.1),
            brightness: 1.0,
            alpha: 1.0,
            effects: 0,
            overlays: Vec::new(),
            tint: (1.0, 1.0, 1.0),
        }
    }

    /// A burning torch picked up off the floor took its fire with it: no
    /// flame is left fading where it lay. One that burns out there does fade.
    #[test]
    fn a_torch_picked_up_leaves_no_fading_flame() {
        use crate::components::{ItemInstance, ItemType};
        let mut world = crate::world::World::new();
        let frames = FlameFrames::Flames { phase: 0.0 };
        let torch = crate::systems::held::drop_new(&mut world, 1, 1, ItemInstance::plain(ItemType::Torch));
        let mut fades = FireFades::default();
        fades.shown((torch, FlameKind::Fuel), 10.0, &flame(), frames, None);
        assert!(fades.finish(&world, 10.0).is_empty());
        crate::systems::held::lift(&mut world, torch);
        assert!(fades.finish(&world, 11.0).is_empty(), "carried off, not gone out");

        let burnt = crate::systems::held::drop_new(&mut world, 2, 2, ItemInstance::plain(ItemType::Torch));
        fades.shown((burnt, FlameKind::Fuel), 12.0, &flame(), frames, None);
        fades.finish(&world, 12.0);
        crate::systems::torches::item_goes_out(&mut world, burnt, ItemType::UnlitTorch);
        assert_eq!(fades.finish(&world, 13.0).len(), 1, "burnt out where it lay: it fades");
    }

    /// A flame fades in from nothing over the fade-in time, and when it
    /// goes out it's drawn on where it was, fading to nothing.
    #[test]
    fn a_flame_fades_in_and_out() {
        let mut fades = FireFades::default();
        let e = hecs::World::new().spawn(());
        let key = (e, FlameKind::Fuel);
        let frames = FlameFrames::Flames { phase: 0.0 };
        assert_eq!(fades.shown(key, 10.0, &flame(), frames, None), 0.0, "caught this frame: unseen");
        assert!(fades.finish(&hecs::World::new(), 10.0).is_empty());
        let half = fades.shown(key, 10.0 + FIRE_FADE_IN_SECONDS / 2.0, &flame(), frames, None);
        assert!(half > 0.3 && half < 1.0, "on its way in: {half}");
        fades.finish(&hecs::World::new(), 11.0);
        assert_eq!(fades.shown(key, 11.0, &flame(), frames, None), 1.0, "fully in");
        assert!(fades.finish(&hecs::World::new(), 11.0).is_empty(), "drawn this frame, so not going out");

        // Not drawn this frame: it's gone out, and fades from where it was.
        let fading = fades.finish(&hecs::World::new(), 12.0);
        assert_eq!(fading.len(), 1);
        assert!(fading[0].alpha > 0.99 && fading[0].x == 1.0, "fades from full, in its place");
        let later = fades.finish(&hecs::World::new(), 12.0 + FIRE_FADE_OUT_SECONDS / 2.0);
        assert!(later[0].alpha > 0.0 && later[0].alpha < 1.0);
        assert!(fades.finish(&hecs::World::new(), 12.01 + FIRE_FADE_OUT_SECONDS).is_empty(), "and then it's gone");
    }
}
