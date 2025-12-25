//! Camera-related constants.

/// Default zoom level (pixels per grid cell)
pub const CAMERA_DEFAULT_ZOOM: f32 = 32.0;
/// Minimum zoom level
pub const CAMERA_MIN_ZOOM: f32 = 4.0;
/// Maximum zoom level
pub const CAMERA_MAX_ZOOM: f32 = 128.0;
/// Zoom speed multiplier per scroll unit
pub const CAMERA_ZOOM_FACTOR: f32 = 1.1;
/// Smoothing factor for camera tracking (lower = smoother)
pub const CAMERA_TRACKING_SMOOTHING: f32 = 0.85;
/// Velocity damping factor (lower = more friction)
pub const CAMERA_VELOCITY_DAMPING: f32 = 0.90;
/// Velocity threshold below which camera stops
pub const CAMERA_VELOCITY_THRESHOLD: f32 = 0.001;
/// Zoom difference threshold for snapping
pub const CAMERA_ZOOM_SNAP_THRESHOLD: f32 = 0.01;
/// Momentum multiplier when releasing pan
pub const CAMERA_MOMENTUM_SCALE: f32 = 2.0;
