//! Easing curves for animation.
//!
//! Before this existed, everything animated was either linear
//! (`VisualEffect::progress`) or an exponential lerp (`VISUAL_LERP_SPEED`),
//! both of which read as "programmer animation". These functions reshape a
//! normalised time into something with weight to it.
//!
//! Every function takes `t` in `0.0..=1.0` and clamps it, so a caller that
//! overshoots its duration gets the end of the curve rather than a curve that
//! keeps going. `out_*` means the motion decelerates into its resting place,
//! which is what almost everything wants; `in_out_quad` is for motion that
//! starts and ends at rest.
//!
//! The coefficients here are part of the curve definitions (the standard
//! Penner easing constants), not gameplay tuning, so they live beside the
//! functions rather than in `src/constants/`. [`POP_PEAK`] is the exception
//! and is called out below.

/// How far past 1.0 [`out_back`] travels before settling. The standard value;
/// larger exaggerates the overshoot, 0.0 degenerates to a plain cubic.
const BACK_OVERSHOOT: f32 = 1.701_58;

/// Period of the [`out_elastic`] oscillation as a fraction of the duration.
/// Smaller means more wobbles over the same time.
#[allow(dead_code)] // Used only by out_elastic, which has no caller yet.
const ELASTIC_PERIOD: f32 = 0.3;

/// Decay rate of the [`out_elastic`] envelope. Higher settles sooner.
#[allow(dead_code)] // Used only by out_elastic, which has no caller yet.
const ELASTIC_DECAY: f32 = 10.0;

/// Starting scale for [`pop`], i.e. how oversized a popping thing begins.
/// This one *is* a tuning value — it sets how hard a pop reads — but it lives
/// here because it defines what `pop` means for every caller; change it to
/// retune every pop in the game at once.
const POP_PEAK: f32 = 1.6;

/// Decelerating cubic: leaves fast, lands soft. The default choice for
/// anything travelling to a resting place.
pub fn out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv
}

/// Decelerating cubic that overshoots past 1.0 and settles back, for motion
/// that should feel like it has mass behind it.
pub fn out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let inv = t - 1.0;
    1.0 + (BACK_OVERSHOOT + 1.0) * inv * inv * inv + BACK_OVERSHOOT * inv * inv
}

/// Overshoots and oscillates around 1.0 before settling — a spring. Loud;
/// reserve it for things that should draw the eye.
#[allow(dead_code)] // No caller yet; part of the curve palette (see module docs).
pub fn out_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    // The endpoints are exact rather than nearly-exact, so a caller can rely
    // on pinning to 0.0 and 1.0.
    if t == 0.0 || t == 1.0 {
        return t;
    }
    let phase = (t - ELASTIC_PERIOD / 4.0) * std::f32::consts::TAU / ELASTIC_PERIOD;
    2.0f32.powf(-ELASTIC_DECAY * t) * phase.sin() + 1.0
}

/// Symmetric quadratic: accelerates out of rest, decelerates back into it.
/// For motion with a still point at each end, like a window sliding open.
pub fn in_out_quad(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        2.0 * t * t
    } else {
        let inv = -2.0 * t + 2.0;
        1.0 - inv * inv / 2.0
    }
}

/// Scale multiplier for a "pop": starts [`POP_PEAK`]x oversized and settles to
/// exactly 1.0, dipping a hair under on the way thanks to [`out_back`]'s
/// overshoot. Multiply a size by this to make something punch out and settle.
pub fn pop(t: f32) -> f32 {
    POP_PEAK + (1.0 - POP_PEAK) * out_back(t)
}

/// Smooth 0 -> 1 -> 0 ping-pong over one unit of `phase`, for a pulse that has
/// to loop forever without a visible seam.
///
/// Unlike the `out_*` curves this takes an unbounded phase and wraps it, so a
/// caller can hand it `elapsed_seconds * rate` directly. The underlying shape
/// is a triangle wave put through [`in_out_quad`], which is flat at both
/// turning points — a raw triangle would visibly kink there.
pub fn ping_pong(phase: f32) -> f32 {
    let wrapped = phase.rem_euclid(1.0);
    let triangle = if wrapped < 0.5 {
        wrapped * 2.0
    } else {
        (1.0 - wrapped) * 2.0
    };
    in_out_quad(triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Floating point comparison for curve values.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn curves_pin_to_their_endpoints() {
        for (name, f) in [
            ("out_cubic", out_cubic as fn(f32) -> f32),
            ("out_back", out_back),
            ("out_elastic", out_elastic),
            ("in_out_quad", in_out_quad),
        ] {
            assert!(close(f(0.0), 0.0), "{name}(0.0) = {} , want 0.0", f(0.0));
            assert!(close(f(1.0), 1.0), "{name}(1.0) = {} , want 1.0", f(1.0));
        }
    }

    #[test]
    fn curves_clamp_outside_the_unit_interval() {
        for (name, f) in [
            ("out_cubic", out_cubic as fn(f32) -> f32),
            ("out_back", out_back),
            ("out_elastic", out_elastic),
            ("in_out_quad", in_out_quad),
        ] {
            assert!(close(f(-1.0), 0.0), "{name} did not clamp below 0.0");
            assert!(close(f(2.0), 1.0), "{name} did not clamp above 1.0");
        }
    }

    #[test]
    fn out_cubic_decelerates() {
        // Past the halfway point in value well before the halfway point in
        // time is what "decelerating" means.
        assert!(out_cubic(0.5) > 0.5);
        // Monotonic.
        let mut previous = 0.0;
        for step in 0..=20 {
            let value = out_cubic(step as f32 / 20.0);
            assert!(value >= previous, "out_cubic went backwards at {step}");
            previous = value;
        }
    }

    #[test]
    fn out_back_overshoots_before_settling() {
        // Somewhere in the back half it goes past its destination.
        let peak = (10..20)
            .map(|step| out_back(step as f32 / 20.0))
            .fold(f32::MIN, f32::max);
        assert!(peak > 1.0, "out_back never overshot (peak {peak})");
    }

    #[test]
    fn in_out_quad_is_symmetric_about_its_midpoint() {
        assert!(close(in_out_quad(0.5), 0.5));
        for step in 0..=10 {
            let t = step as f32 / 10.0;
            assert!(
                close(in_out_quad(t), 1.0 - in_out_quad(1.0 - t)),
                "in_out_quad is not symmetric at {t}"
            );
        }
    }

    #[test]
    fn ping_pong_loops_seamlessly() {
        // Rests at 0 at every integer phase and peaks at 1 halfway between.
        for cycle in 0..4 {
            let base = cycle as f32;
            assert!(close(ping_pong(base), 0.0), "ping_pong({base}) left its floor");
            assert!(
                close(ping_pong(base + 0.5), 1.0),
                "ping_pong({}) missed its peak",
                base + 0.5
            );
        }
        // Negative phases wrap the same way, so a caller need not clamp.
        assert!(close(ping_pong(-0.5), 1.0));
        // Symmetric about the peak.
        for step in 0..=10 {
            let t = step as f32 / 20.0;
            assert!(
                close(ping_pong(t), ping_pong(1.0 - t)),
                "ping_pong is not symmetric at {t}"
            );
        }
    }

    #[test]
    fn pop_starts_oversized_and_settles_at_one() {
        assert!(close(pop(1.0), 1.0));
        assert!(pop(0.0) > 1.0, "pop did not start oversized");
        // Shrinking, not growing.
        assert!(pop(0.0) > pop(0.5));
    }
}
