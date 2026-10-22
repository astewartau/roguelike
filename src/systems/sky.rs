//! The time of day, and the light of the sky over outdoor maps.
//!
//! The time of day follows the game clock (`GameClock`), so it stands still
//! while the game waits on you and races ahead while you rest or sleep. A
//! day is `Sky::day_length` game seconds long. The clock never runs
//! backwards, so the developer tools set the time of day by moving
//! `Sky::offset`, where the day stood when the clock read 0.
//!
//! On a map that's outdoors (`Grid::outdoors`) the sky is a light over
//! every tile, counted wherever light is: the renderer adds it to the scene,
//! the per-tile light (`rendering::calculate_illumination`, what the light
//! overlay shows) includes it, it adds to how lit the player is
//! (`light::update_light_exposure`, so you're easier to spot by day and
//! harder on a moonless night), and like any light it shows you what it
//! lights that you have a line to (`sight_radius`). It lifts the fog of war
//! too, as ambient light (`fog_brightness`). Underground the time of day still
//! passes, unseen.

use crate::constants::{
    DAY_LENGTH, DAY_START_HOUR, FOV_RADIUS, LIGHT_FOG_SHARE, LIGHT_PLAYER_AMBIENT, SKY_COLOR_DAY, SKY_COLOR_NIGHT,
    SKY_COLOR_TWILIGHT, SKY_HORIZON_LEVEL, SKY_MOONLIGHT_LEVEL, SKY_MOON_DEFAULT,
    SKY_MOON_SIGHT_BONUS, SKY_NOON_LEVEL, SKY_STARLIGHT_LEVEL, SKY_TWILIGHT_HOURS, SKY_TWILIGHT_STRENGTH,
    SUNRISE_HOUR, SUNSET_HOUR,
};

/// The time of day and how the sky behaves: one for the whole world, kept
/// on `GameState`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// Game seconds from the first midnight to when the game clock read 0.
    /// The clock plus this is how long since that midnight.
    pub offset: f32,
    /// Game seconds in a day and night.
    pub day_length: f32,
    /// When the sun is half risen, and half set, in hours (0-24).
    pub sunrise: f32,
    pub sunset: f32,
    /// How bright the moon is, 0 (new) to 1 (full).
    pub moon: f32,
    /// How bright and what colour its light is, through the day.
    pub look: SkyLook,
}

impl Default for Sky {
    fn default() -> Self {
        Sky {
            offset: DAY_START_HOUR / 24.0 * DAY_LENGTH,
            day_length: DAY_LENGTH,
            sunrise: SUNRISE_HOUR,
            sunset: SUNSET_HOUR,
            moon: SKY_MOON_DEFAULT,
            look: SkyLook::default(),
        }
    }
}

/// How the sky lights the world: its brightness and colour by day, at dawn
/// and dusk, and by night, and the fog of war's. Starts from the `SKY_*` and
/// `LIGHT_FOG_*` constants; the developer tools tune it live.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLook {
    /// Sky light at noon (`SKY_NOON_LEVEL`).
    pub noon: f32,
    /// With the sun just up (`SKY_HORIZON_LEVEL`).
    pub horizon: f32,
    /// On a moonless night (`SKY_STARLIGHT_LEVEL`).
    pub starlight: f32,
    /// What a full moon adds (`SKY_MOONLIGHT_LEVEL`).
    pub moonlight: f32,
    /// Hours either side of sunrise and sunset that dawn and dusk last
    /// (`SKY_TWILIGHT_HOURS`).
    pub twilight_hours: f32,
    /// How strongly dawn and dusk tint the light (`SKY_TWILIGHT_STRENGTH`).
    pub twilight_strength: f32,
    /// Colours by day, at dawn and dusk, and by night (`SKY_COLOR_*`).
    pub day_color: (f32, f32, f32),
    pub twilight_color: (f32, f32, f32),
    pub night_color: (f32, f32, f32),
    /// Remembered tiles are lit this share of the ambient light, indoors
    /// and out (`LIGHT_FOG_SHARE`).
    pub fog: f32,
}

impl Default for SkyLook {
    fn default() -> Self {
        SkyLook {
            noon: SKY_NOON_LEVEL,
            horizon: SKY_HORIZON_LEVEL,
            starlight: SKY_STARLIGHT_LEVEL,
            moonlight: SKY_MOONLIGHT_LEVEL,
            twilight_hours: SKY_TWILIGHT_HOURS,
            twilight_strength: SKY_TWILIGHT_STRENGTH,
            day_color: SKY_COLOR_DAY,
            twilight_color: SKY_COLOR_TWILIGHT,
            night_color: SKY_COLOR_NIGHT,
            fog: LIGHT_FOG_SHARE,
        }
    }
}

/// The sky's light at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLight {
    /// How much of it is day, 0 (night) to 1 (the sun well up).
    pub daylight: f32,
    /// How bright, on the light map's scale.
    pub level: f32,
    /// Its colour, with one channel at 1.0.
    pub color: (f32, f32, f32),
    /// How much of a full moon's light is in it, 0 to 1 (none by day).
    pub moonlit: f32,
}

impl SkyLight {
    /// The light it adds to every tile, colour and all.
    pub fn rgb(&self) -> (f32, f32, f32) {
        (self.color.0 * self.level, self.color.1 * self.level, self.color.2 * self.level)
    }
}

impl Sky {
    fn since_midnight(&self, clock: f32) -> f32 {
        (self.offset + clock).max(0.0)
    }

    /// Which day it is at game time `clock`, counting from 1.
    pub fn day(&self, clock: f32) -> u32 {
        (self.since_midnight(clock) / self.day_length) as u32 + 1
    }

    /// The hour at game time `clock`, 0 up to 24.
    pub fn hour(&self, clock: f32) -> f32 {
        self.since_midnight(clock).rem_euclid(self.day_length) / self.day_length * 24.0
    }

    /// Make it `hour` o'clock at game time `clock`, the same day.
    pub fn set_hour(&mut self, clock: f32, hour: f32) {
        self.offset += (hour.rem_euclid(24.0) - self.hour(clock)) / 24.0 * self.day_length;
    }

    /// Make a day `length` game seconds long, keeping the day and the hour
    /// it is at game time `clock`.
    pub fn set_day_length(&mut self, clock: f32, length: f32) {
        let (day, hour) = (self.day(clock), self.hour(clock));
        self.day_length = length.max(1.0);
        self.offset = ((day - 1) as f32 + hour / 24.0) * self.day_length - clock;
    }

    /// "Day 2, 14:05".
    pub fn clock_text(&self, clock: f32) -> String {
        let minutes = (self.hour(clock) * 60.0) as u32;
        format!("Day {}, {:02}:{:02}", self.day(clock), minutes / 60, minutes % 60)
    }

    /// What part of the day it is, in a word.
    pub fn part_of_day(&self, clock: f32) -> &'static str {
        let h = self.hour(clock);
        let above = self.hours_of_sun(h);
        let twilight = self.look.twilight_hours.max(0.01);
        if above.abs() < twilight {
            if (h - self.sunrise).abs() < (h - self.sunset).abs() { "Dawn" } else { "Dusk" }
        } else if above < 0.0 {
            if h > 12.0 && h < self.sunset + 3.0 * twilight { "Evening" } else { "Night" }
        } else {
            let noon = (self.sunrise + self.sunset) / 2.0;
            if (h - noon).abs() < 1.5 {
                "Midday"
            } else if h < noon {
                "Morning"
            } else {
                "Afternoon"
            }
        }
    }

    /// Hours the sun has been up, or until it sets, whichever is less, at
    /// hour `h`: how far it is above the horizon. Negative at night, by the
    /// hours to the nearer of sunrise and sunset.
    fn hours_of_sun(&self, h: f32) -> f32 {
        let (rise, set) = (self.sunrise, self.sunset.max(self.sunrise + 0.5));
        if (rise..=set).contains(&h) {
            (h - rise).min(set - h)
        } else {
            let after_set = (h - set).rem_euclid(24.0);
            let before_rise = (rise - h).rem_euclid(24.0);
            -after_set.min(before_rise)
        }
    }

    /// The sky's light at game time `clock`.
    pub fn light(&self, clock: f32) -> SkyLight {
        let h = self.hour(clock);
        let above = self.hours_of_sun(h);
        let look = &self.look;
        let twilight = look.twilight_hours.max(0.01);
        let daylight = smoothstep(-twilight, twilight, above);

        // By day it climbs from the horizon's light to noon's as the sun
        // does; by night it's starlight, and as much moonlight as the moon.
        let (rise, set) = (self.sunrise, self.sunset.max(self.sunrise + 0.5));
        let elevation = if (rise..=set).contains(&h) {
            (std::f32::consts::PI * (h - rise) / (set - rise)).sin().max(0.0)
        } else {
            0.0
        };
        let day_level = look.horizon + (look.noon - look.horizon) * elevation.sqrt();
        let moon = self.moon.clamp(0.0, 1.0);
        let night_level = look.starlight + look.moonlight * moon;
        let level = night_level + (day_level - night_level) * daylight;

        // Cold by night, warm white by day, and amber round the horizon.
        let glow = (-(above / twilight).powi(2)).exp() * look.twilight_strength;
        let color = mix(mix(look.night_color, look.day_color, daylight), look.twilight_color, glow);
        SkyLight { daylight, level, color: peg(color), moonlit: moon * (1.0 - daylight) }
    }

    /// How bright remembered tiles (explored, out of sight) are, with `sky`
    /// the light over this map now (None underground): `look.fog` of the
    /// ambient light, the sky's and the player's soft light in view, as the
    /// tile shader lights them. Lamps and fires don't count: they're not seen
    /// from out of sight.
    pub fn fog_brightness(&self, sky: Option<SkyLight>) -> f32 {
        let daylight = sky.map_or(0.0, |s| s.daylight);
        let ambient = LIGHT_PLAYER_AMBIENT * (1.0 - daylight) + sky.map_or(0.0, |s| s.level);
        self.look.fog.max(0.0) * ambient
    }
}

/// How far the player sees by `sky` (None underground) on a map `extent`
/// tiles across: their own light's reach at night, a little more under a
/// bright moon, and everything they have a line to in full day, since the
/// sky lights it all.
pub fn sight_radius(sky: Option<SkyLight>, extent: i32) -> i32 {
    let Some(sky) = sky else {
        return FOV_RADIUS;
    };
    let moonlit = SKY_MOON_SIGHT_BONUS as f32 * sky.moonlit;
    let by_night = FOV_RADIUS as f32 + moonlit;
    let by_day = extent.max(FOV_RADIUS) as f32;
    (by_night + (by_day - by_night) * sky.daylight).round() as i32
}


fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> (f32, f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// Scale a colour so its brightest channel is 1.0, as every light colour
/// is, so brightness lives in the level alone.
fn peg(c: (f32, f32, f32)) -> (f32, f32, f32) {
    let top = c.0.max(c.1).max(c.2).max(f32::EPSILON);
    (c.0 / top, c.1 / top, c.2 / top)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_hour(sky: &Sky, hour: f32) -> SkyLight {
        let mut sky = *sky;
        sky.set_hour(0.0, hour);
        sky.light(0.0)
    }

    #[test]
    fn the_time_of_day_follows_the_clock() {
        let sky = Sky::default();
        assert_eq!(sky.day(0.0), 1);
        assert!((sky.hour(0.0) - DAY_START_HOUR).abs() < 1e-3);
        // A quarter of a day later it's six hours on.
        assert!((sky.hour(sky.day_length / 4.0) - (DAY_START_HOUR + 6.0)).abs() < 1e-3);
        // And past midnight it's the next day.
        let tomorrow = sky.day_length * (1.0 - DAY_START_HOUR / 24.0) + 1.0;
        assert_eq!(sky.day(tomorrow), 2);
        assert!(sky.hour(tomorrow) < 0.1);
        assert_eq!(sky.clock_text(0.0), format!("Day 1, {:02}:00", DAY_START_HOUR as u32));
    }

    #[test]
    fn setting_the_hour_or_the_day_length_keeps_the_day() {
        let mut sky = Sky::default();
        let clock = 5000.0;
        let day = sky.day(clock);
        for hour in [0.0, 5.5, 12.0, 23.75] {
            sky.set_hour(clock, hour);
            assert!((sky.hour(clock) - hour).abs() < 1e-2, "{hour}");
            assert_eq!(sky.day(clock), day, "{hour}");
        }
        sky.set_hour(clock, 15.0);
        sky.set_day_length(clock, 600.0);
        assert!((sky.hour(clock) - 15.0).abs() < 1e-2);
        assert_eq!(sky.day(clock), day);
        assert!((sky.hour(clock + 150.0) - 21.0).abs() < 1e-2, "a quarter of the new day on");
    }

    #[test]
    fn noon_is_brighter_than_dawn_and_dawn_than_midnight() {
        let sky = Sky::default();
        let (midnight, dawn, morning, noon) =
            (at_hour(&sky, 0.0), at_hour(&sky, sky.sunrise), at_hour(&sky, 9.0), at_hour(&sky, 12.5));
        assert!(midnight.level < dawn.level && dawn.level < morning.level && morning.level <= noon.level);
        assert!((noon.level - SKY_NOON_LEVEL).abs() < 0.01);
        assert!(midnight.daylight < 0.01 && noon.daylight > 0.99);
        assert!((dawn.daylight - 0.5).abs() < 0.01, "the sun is half risen at sunrise");
    }

    #[test]
    fn dusk_glows_warm_and_night_is_cold() {
        let sky = Sky::default();
        let (dusk, night, noon) = (at_hour(&sky, sky.sunset), at_hour(&sky, 1.0), at_hour(&sky, 12.5));
        assert!(dusk.color.0 > dusk.color.2 + 0.3, "dusk is amber: {:?}", dusk.color);
        assert!(night.color.2 > night.color.0, "night is blue: {:?}", night.color);
        for light in [dusk, night, noon] {
            let top = light.color.0.max(light.color.1).max(light.color.2);
            assert!((top - 1.0).abs() < 1e-5, "one channel pegged at 1: {:?}", light.color);
        }
    }

    #[test]
    fn the_moon_lights_the_night_and_not_the_day() {
        let dark = Sky { moon: 0.0, ..Sky::default() };
        let full = Sky { moon: 1.0, ..Sky::default() };
        assert!((at_hour(&dark, 1.0).level - SKY_STARLIGHT_LEVEL).abs() < 1e-4);
        assert!((at_hour(&full, 1.0).level - SKY_STARLIGHT_LEVEL - SKY_MOONLIGHT_LEVEL).abs() < 1e-4);
        assert!((at_hour(&dark, 12.5).level - at_hour(&full, 12.5).level).abs() < 1e-4);
    }

    #[test]
    fn daylight_lets_you_see_as_far_as_the_land_does() {
        let dark = Sky { moon: 0.0, ..Sky::default() };
        let full = Sky { moon: 1.0, ..Sky::default() };
        assert_eq!(sight_radius(None, 80), FOV_RADIUS);
        assert_eq!(sight_radius(Some(at_hour(&dark, 1.0)), 80), FOV_RADIUS, "a moonless night: your own light");
        assert_eq!(sight_radius(Some(at_hour(&full, 1.0)), 80), FOV_RADIUS + SKY_MOON_SIGHT_BONUS);
        assert_eq!(sight_radius(Some(at_hour(&dark, 12.5)), 80), 80, "noon: the whole map");
        let dusk = sight_radius(Some(at_hour(&dark, dark.sunset)), 80);
        assert!(dusk > FOV_RADIUS && dusk < 80, "dusk draws in: {dusk}");
    }

    /// The developer tools' light and colour settings are what the sky uses.
    #[test]
    fn the_sky_follows_its_tuning() {
        let mut sky = Sky::default();
        sky.look.noon = 1.2;
        sky.look.starlight = 0.2;
        sky.look.night_color = (1.0, 0.0, 0.0);
        sky.look.fog = 0.5;
        sky.moon = 0.0;
        assert!((at_hour(&sky, 12.5).level - 1.2).abs() < 0.01);
        let night = at_hour(&sky, 1.0);
        assert!((night.level - 0.2).abs() < 1e-4);
        assert!(night.color.0 == 1.0 && night.color.1 < 1e-6 && night.color.2 < 1e-6, "{:?}", night.color);
        assert!((sky.fog_brightness(None) - 0.5 * LIGHT_PLAYER_AMBIENT).abs() < 1e-6);
        assert!((sky.fog_brightness(Some(at_hour(&sky, 12.5))) - 0.5 * 1.2).abs() < 0.01, "half the noon sky");
    }

    #[test]
    fn the_fog_of_war_is_a_share_of_the_ambient_light() {
        let sky = Sky::default();
        let fog_brightness = |light| sky.fog_brightness(light);
        let (night, noon) = (at_hour(&sky, 1.0), at_hour(&sky, 12.5));
        assert!((fog_brightness(None) - LIGHT_FOG_SHARE * LIGHT_PLAYER_AMBIENT).abs() < 1e-6, "underground: your own light's");
        assert!((fog_brightness(Some(noon)) - LIGHT_FOG_SHARE * noon.level).abs() < 1e-4, "noon: the sky's");
        assert!(fog_brightness(Some(night)) < fog_brightness(Some(noon)));
        assert!(fog_brightness(Some(noon)) < noon.level, "still darker than what you see");
    }

    #[test]
    fn the_parts_of_the_day_come_in_order() {
        let sky = Sky::default();
        let part = |h: f32| {
            let mut s = sky;
            s.set_hour(0.0, h);
            s.part_of_day(0.0)
        };
        assert_eq!(part(2.0), "Night");
        assert_eq!(part(sky.sunrise), "Dawn");
        assert_eq!(part(9.0), "Morning");
        assert_eq!(part(12.5), "Midday");
        assert_eq!(part(16.0), "Afternoon");
        assert_eq!(part(sky.sunset), "Dusk");
        assert_eq!(part(21.0), "Evening");
    }
}
