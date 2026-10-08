use crate::constants::*;
use glam::{Mat4, Vec2};

/// A camera shake asked for by a game event.
///
/// Only "how hard, from where, which way" — the camera owns the oscillation,
/// the decay and the clamping, so event handlers never have to know what a
/// shake actually looks like.
#[derive(Debug, Clone, Copy)]
pub struct ShakeRequest {
    /// Peak offset in tiles, before distance falloff.
    pub amplitude: f32,
    /// Where in the world it happened. `Some` applies a falloff with distance
    /// from the camera centre, so a far-off explosion barely registers;
    /// `None` means it happened to the player and always lands at full
    /// strength.
    pub origin: Option<Vec2>,
    /// Direction for a one-axis kick (a shove, like the player's own swing),
    /// or `None` for an untargeted rattle. Need not be normalised.
    pub direction: Option<Vec2>,
}

pub struct Camera {
    pub position: Vec2,
    pub zoom: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    // Smooth movement
    velocity: Vec2,
    target_zoom: f32,
    last_mouse_world_pos: Option<Vec2>,
    // Auto-tracking
    tracking_target: Option<Vec2>,
    manual_control: bool,
    // Drag anchor - world position that should stay under cursor while dragging
    drag_anchor: Option<Vec2>,
    last_drag_pos: Vec2,
    // Screen shake. Applied in `projection_matrix` and nowhere else, so
    // `position` stays the true camera centre and panning, `screen_to_world`,
    // `world_to_screen` and `get_visible_bounds` are all unaffected by it.
    shake_amplitude: f32,
    /// Seconds since the current shake started, driving its oscillation phase.
    shake_elapsed: f32,
    /// Set for a directional kick, `None` for a rattle.
    shake_direction: Option<Vec2>,
}

impl Camera {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            position: Vec2::ZERO,
            zoom: CAMERA_DEFAULT_ZOOM,
            viewport_width,
            viewport_height,
            velocity: Vec2::ZERO,
            target_zoom: CAMERA_DEFAULT_ZOOM,
            last_mouse_world_pos: None,
            tracking_target: None,
            manual_control: false,
            drag_anchor: None,
            last_drag_pos: Vec2::ZERO,
            shake_amplitude: 0.0,
            shake_elapsed: 0.0,
            shake_direction: None,
        }
    }

    /// Start panning - records the world position under the cursor as anchor
    pub fn start_pan(&mut self, screen_x: f32, screen_y: f32) {
        self.drag_anchor = Some(self.screen_to_world(screen_x, screen_y));
        self.last_drag_pos = Vec2::new(screen_x, screen_y);
        self.velocity = Vec2::ZERO;
    }

    /// Continue panning - moves camera so the anchor stays under the cursor
    pub fn pan(&mut self, screen_x: f32, screen_y: f32) {
        if let Some(anchor) = self.drag_anchor {
            // Calculate where the anchor point currently appears on screen
            // and adjust camera so it appears at the cursor position
            let current_world = self.screen_to_world(screen_x, screen_y);
            let delta = anchor - current_world;

            self.position += delta;

            // Track velocity for momentum (based on world-space movement)
            let screen_delta = Vec2::new(screen_x, screen_y) - self.last_drag_pos;
            self.velocity = Vec2::new(-screen_delta.x, screen_delta.y) / self.zoom;
            self.last_drag_pos = Vec2::new(screen_x, screen_y);

            // Enable manual control mode
            self.manual_control = true;
        }
    }

    pub fn release_pan(&mut self) {
        // Apply momentum scaling when mouse is released
        self.velocity *= CAMERA_MOMENTUM_SCALE;
        self.drag_anchor = None;
    }

    pub fn set_tracking_target(&mut self, target: Vec2) {
        // Only reset manual control if the target actually moved
        // (otherwise every frame would reset manual_control and break panning)
        let target_moved = self.tracking_target
            .map(|old| (old - target).length() > 0.01)
            .unwrap_or(true);

        self.tracking_target = Some(target);

        // When player moves, return to auto-tracking
        if target_moved {
            self.manual_control = false;
        }
    }

    pub fn add_zoom_impulse(&mut self, delta: f32, mouse_x: f32, mouse_y: f32) {
        // Zoom towards player if following, otherwise zoom towards mouse cursor
        if !self.manual_control {
            // Following player - zoom towards tracking target (player position)
            self.last_mouse_world_pos = self.tracking_target;
        } else {
            // Manual control - zoom towards mouse cursor
            self.last_mouse_world_pos = Some(self.screen_to_world(mouse_x, mouse_y));
        }

        // Apply zoom
        let zoom_factor = CAMERA_ZOOM_FACTOR.powf(delta);
        self.target_zoom = (self.target_zoom * zoom_factor).clamp(CAMERA_MIN_ZOOM, CAMERA_MAX_ZOOM);
    }

    /// Apply an event's shake request, scaled down by how far its origin is
    /// from what the player is looking at.
    pub fn apply_shake_request(&mut self, request: &ShakeRequest) {
        let falloff = match request.origin {
            Some(origin) => {
                let distance = (origin - self.position).length();
                (1.0 - distance / CAMERA_SHAKE_FALLOFF_DISTANCE).clamp(0.0, 1.0)
            }
            // No origin: it happened to the player, so distance is moot.
            None => 1.0,
        };

        self.add_shake(request.amplitude * falloff, request.direction);
    }

    /// Start a shake of `amplitude` tiles.
    ///
    /// Shakes do not accumulate — the strongest one wins and restarts the
    /// decay — so a volley of small hits cannot stack into a screen-wrecking
    /// rattle, and a scratch landing during an explosion cannot cut the
    /// explosion short.
    pub fn add_shake(&mut self, amplitude: f32, direction: Option<Vec2>) {
        let amplitude = amplitude.min(CAMERA_SHAKE_MAX_AMPLITUDE);
        if amplitude <= self.shake_amplitude {
            return;
        }

        self.shake_amplitude = amplitude;
        self.shake_elapsed = 0.0;
        // A zero-length direction would normalise to nothing; fall back to a
        // rattle rather than silently dropping the shake.
        self.shake_direction = direction.and_then(|d| d.try_normalize());
    }

    /// The current shake offset in world units (tiles), or zero when nothing
    /// is shaking. Only `projection_matrix` should use this.
    fn shake_offset(&self) -> Vec2 {
        if self.shake_amplitude <= 0.0 {
            return Vec2::ZERO;
        }

        let phase = self.shake_elapsed * CAMERA_SHAKE_FREQUENCY * std::f32::consts::TAU;

        match self.shake_direction {
            // Directional: a shove along one axis.
            Some(direction) => direction * (self.shake_amplitude * phase.sin()),
            // Untargeted: x and y on different frequencies, so the offset
            // traces a messy path instead of sliding along one diagonal.
            None => {
                Vec2::new(phase.sin(), (phase * CAMERA_SHAKE_FREQUENCY_Y_RATIO).sin())
                    * self.shake_amplitude
            }
        }
    }

    /// Advance the shake decay. Split out of [`Camera::update`] so it can be
    /// stepped on its own, without also running tracking, momentum and zoom;
    /// `update` calls it first thing every frame.
    fn update_shake(&mut self, dt: f32) {
        if self.shake_amplitude <= 0.0 {
            return;
        }

        self.shake_elapsed += dt;
        self.shake_amplitude *= CAMERA_SHAKE_DECAY.powf(dt);

        if self.shake_amplitude < CAMERA_SHAKE_CUTOFF {
            self.shake_amplitude = 0.0;
            self.shake_elapsed = 0.0;
            self.shake_direction = None;
        }
    }

    pub fn update(&mut self, dt: f32, is_dragging: bool) {
        // `dt` is real frame time here, which is what the shake wants - see
        // the comment at the `camera.update` call site in `engine::tick`.
        self.update_shake(dt);

        // Auto-track target if not in manual control mode
        if !self.manual_control && !is_dragging {
            if let Some(target) = self.tracking_target {
                // Smooth interpolation to target position
                let t = 1.0 - CAMERA_TRACKING_SMOOTHING.powf(dt * 60.0);
                self.position = self.position + (target - self.position) * t;
            }
        }

        // Only apply momentum when not dragging and in manual mode
        if !is_dragging && self.manual_control {
            // Apply velocity with damping (smooth deceleration)
            let damping = CAMERA_VELOCITY_DAMPING.powf(dt * 60.0);

            self.position += self.velocity * dt * 60.0;
            self.velocity *= damping;

            // Stop completely when velocity is very small
            if self.velocity.length() < CAMERA_VELOCITY_THRESHOLD {
                self.velocity = Vec2::ZERO;
            }
        }

        // Smooth zoom interpolation
        if (self.zoom - self.target_zoom).abs() > CAMERA_ZOOM_SNAP_THRESHOLD {
            let zoom_before = self.zoom;

            // Smooth interpolation
            let t = 1.0 - CAMERA_TRACKING_SMOOTHING.powf(dt * 60.0);
            self.zoom = self.zoom + (self.target_zoom - self.zoom) * t;

            // Adjust position to zoom towards last mouse position
            if let Some(world_pos) = self.last_mouse_world_pos {
                // Keep the world point stationary during zoom
                self.position = world_pos + (self.position - world_pos) * (zoom_before / self.zoom);
            }
        } else {
            self.zoom = self.target_zoom;
            self.last_mouse_world_pos = None;
        }
    }

    pub fn screen_to_world(&self, screen_x: f32, screen_y: f32) -> Vec2 {
        let ndc_x = (screen_x / self.viewport_width) * 2.0 - 1.0;
        let ndc_y = 1.0 - (screen_y / self.viewport_height) * 2.0;

        let world_x = (ndc_x * self.viewport_width) / (2.0 * self.zoom) + self.position.x;
        let world_y = (ndc_y * self.viewport_height) / (2.0 * self.zoom) + self.position.y;

        Vec2::new(world_x, world_y)
    }

    pub fn world_to_screen(&self, world_x: f32, world_y: f32) -> (f32, f32) {
        // Use same calculations as projection_matrix for consistency
        let half_width = self.viewport_width / (2.0 * self.zoom);
        let half_height = self.viewport_height / (2.0 * self.zoom);

        let left = self.position.x - half_width;
        let bottom = self.position.y - half_height;

        // Convert world to NDC using orthographic projection
        let ndc_x = (world_x - left) / (2.0 * half_width) * 2.0 - 1.0;
        let ndc_y = (world_y - bottom) / (2.0 * half_height) * 2.0 - 1.0;

        // Convert NDC to screen (y is flipped for screen coordinates)
        let screen_x = (ndc_x + 1.0) * 0.5 * self.viewport_width;
        let screen_y = (1.0 - ndc_y) * 0.5 * self.viewport_height;

        (screen_x, screen_y)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        let half_width = self.viewport_width / (2.0 * self.zoom);
        let half_height = self.viewport_height / (2.0 * self.zoom);

        // Shake lives here and only here. `position` remains the real camera
        // centre, so a shake can never corrupt panning, the momentum
        // velocity, coordinate round-tripping or tile culling - it just moves
        // the view for a few frames.
        let centre = self.position + self.shake_offset();

        let left = centre.x - half_width;
        let right = centre.x + half_width;
        let bottom = centre.y - half_height;
        let top = centre.y + half_height;

        Mat4::orthographic_rh(left, right, bottom, top, -1.0, 1.0)
    }

    pub fn get_visible_bounds(&self) -> (i32, i32, i32, i32) {
        let half_width = self.viewport_width / (2.0 * self.zoom);
        let half_height = self.viewport_height / (2.0 * self.zoom);

        let min_x = (self.position.x - half_width).floor() as i32 - 1;
        let max_x = (self.position.x + half_width).ceil() as i32 + 1;
        let min_y = (self.position.y - half_height).floor() as i32 - 1;
        let max_y = (self.position.y + half_height).ceil() as i32 + 1;

        (min_x, max_x, min_y, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_camera() -> Camera {
        let mut camera = Camera::new(800.0, 600.0);
        camera.position = Vec2::new(10.0, 10.0);
        camera
    }

    #[test]
    fn a_shake_never_touches_position_or_coordinate_mapping() {
        let mut camera = test_camera();
        let position_before = camera.position;
        let round_trip_before = camera.screen_to_world(400.0, 300.0);
        let bounds_before = camera.get_visible_bounds();

        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE, None);
        camera.update_shake(1.0 / 60.0);

        assert!(camera.shake_offset() != Vec2::ZERO, "the shake did not take");
        assert_eq!(camera.position, position_before, "shake moved position");
        assert_eq!(camera.screen_to_world(400.0, 300.0), round_trip_before);
        assert_eq!(camera.get_visible_bounds(), bounds_before);
    }

    #[test]
    fn a_shake_moves_the_projection() {
        let mut camera = test_camera();
        let calm = camera.projection_matrix();

        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE, None);
        camera.update_shake(1.0 / 60.0);

        assert_ne!(camera.projection_matrix(), calm, "projection did not shake");
    }

    #[test]
    fn a_shake_decays_to_nothing_and_stays_there() {
        let mut camera = test_camera();
        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE, None);

        // A second of real time is far longer than any shake should outlive.
        for _ in 0..60 {
            camera.update_shake(1.0 / 60.0);
        }

        assert_eq!(camera.shake_amplitude, 0.0, "shake never settled");
        assert_eq!(camera.shake_offset(), Vec2::ZERO);
        assert_eq!(camera.projection_matrix(), test_camera().projection_matrix());
    }

    #[test]
    fn amplitude_is_clamped_and_the_strongest_shake_wins() {
        let mut camera = test_camera();

        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE * 100.0, None);
        assert_eq!(camera.shake_amplitude, CAMERA_SHAKE_MAX_AMPLITUDE);

        // A weaker shake arriving mid-rattle must not cut it short.
        camera.add_shake(0.001, None);
        assert_eq!(camera.shake_amplitude, CAMERA_SHAKE_MAX_AMPLITUDE);
    }

    #[test]
    fn distance_falloff_mutes_a_far_off_explosion() {
        let mut near = test_camera();
        near.apply_shake_request(&ShakeRequest {
            amplitude: CAMERA_SHAKE_EXPLOSION,
            origin: Some(near.position),
            direction: None,
        });

        let mut far = test_camera();
        far.apply_shake_request(&ShakeRequest {
            amplitude: CAMERA_SHAKE_EXPLOSION,
            origin: Some(far.position + Vec2::new(CAMERA_SHAKE_FALLOFF_DISTANCE * 2.0, 0.0)),
            direction: None,
        });

        assert!(near.shake_amplitude > 0.0, "an explosion underfoot did not shake");
        assert_eq!(far.shake_amplitude, 0.0, "an explosion past the falloff still shook");
    }

    #[test]
    fn a_directional_kick_stays_on_its_axis() {
        let mut camera = test_camera();
        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE, Some(Vec2::new(3.0, 0.0)));
        camera.update_shake(1.0 / 240.0);

        let offset = camera.shake_offset();
        assert!(offset.x.abs() > 0.0, "the kick produced no movement");
        assert_eq!(offset.y, 0.0, "a horizontal kick moved the view vertically");
    }

    #[test]
    fn a_degenerate_direction_falls_back_to_a_rattle() {
        let mut camera = test_camera();
        camera.add_shake(CAMERA_SHAKE_MAX_AMPLITUDE, Some(Vec2::ZERO));
        camera.update_shake(1.0 / 240.0);

        assert!(camera.shake_direction.is_none());
        assert_ne!(camera.shake_offset(), Vec2::ZERO, "the shake was dropped");
    }
}
