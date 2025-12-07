use crate::constants::*;
use glam::{Mat4, Vec2};

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
        }
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        // Direct pan - moves exactly with cursor (no momentum while dragging)
        let world_dx = -dx / self.zoom;
        let world_dy = dy / self.zoom;

        self.position.x += world_dx;
        self.position.y += world_dy;

        // Track velocity for momentum on release
        self.velocity.x = world_dx;
        self.velocity.y = world_dy;

        // Enable manual control mode
        self.manual_control = true;
    }

    pub fn release_pan(&mut self) {
        // Apply momentum scaling when mouse is released
        self.velocity *= CAMERA_MOMENTUM_SCALE;
    }

    pub fn set_tracking_target(&mut self, target: Vec2) {
        self.tracking_target = Some(target);
        // When player moves, return to auto-tracking
        self.manual_control = false;
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

    pub fn update(&mut self, dt: f32, is_dragging: bool) {
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

    pub fn projection_matrix(&self) -> Mat4 {
        let half_width = self.viewport_width / (2.0 * self.zoom);
        let half_height = self.viewport_height / (2.0 * self.zoom);

        let left = self.position.x - half_width;
        let right = self.position.x + half_width;
        let bottom = self.position.y - half_height;
        let top = self.position.y + half_height;

        Mat4::orthographic_rh(left, right, bottom, top, -1.0, 1.0)
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::IDENTITY
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
