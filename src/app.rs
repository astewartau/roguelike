//! Application window and OpenGL context management.
//!
//! This module handles window creation, OpenGL context setup, and the winit
//! event loop integration. It separates platform/graphics concerns from game logic.

use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::Arc;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

use egui_glow::EguiGlow;

use crate::constants::*;

/// Result of window and GL context creation.
pub struct WindowContext {
    pub window: Window,
    pub gl_surface: glutin::surface::Surface<WindowSurface>,
    pub gl_context: glutin::context::PossiblyCurrentContext,
    pub gl: Arc<glow::Context>,
    pub egui_glow: EguiGlow,
}

/// Create a window with OpenGL context and egui integration.
pub fn create_window(event_loop: &ActiveEventLoop) -> WindowContext {
    // Create window
    let window_attrs = WindowAttributes::default()
        .with_title("Grid Roguelike")
        .with_inner_size(PhysicalSize::new(WINDOW_DEFAULT_WIDTH, WINDOW_DEFAULT_HEIGHT))
        .with_resizable(true);

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .reduce(|accum, config| {
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .expect("Failed to create window");

    let window = window.expect("Failed to create window");
    let window_handle = window.window_handle().unwrap();
    let gl_display = gl_config.display();

    let context_attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(window_handle.as_raw()));

    let gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attrs)
            .expect("Failed to create OpenGL context")
    };

    let size = window.inner_size();
    let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        window_handle.as_raw(),
        NonZeroU32::new(size.width).unwrap(),
        NonZeroU32::new(size.height).unwrap(),
    );

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &surface_attrs)
            .expect("Failed to create surface")
    };

    let gl_context = gl_context
        .make_current(&gl_surface)
        .expect("Failed to make context current");

    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function(|s| {
            let s = CString::new(s).unwrap();
            gl_display.get_proc_address(&s) as *const _
        })
    });

    // Initialize egui
    let egui_glow = EguiGlow::new(event_loop, gl.clone(), None, None, false);

    // Apply dungeon-themed styling
    {
        let ctx = &egui_glow.egui_ctx;
        ctx.set_fonts(crate::ui::style::load_fonts());
        ctx.set_style(crate::ui::style::dungeon_style());
    }

    WindowContext {
        window,
        gl_surface,
        gl_context,
        gl,
        egui_glow,
    }
}

/// Resize the GL surface to match the window size.
pub fn resize_surface(
    gl_surface: &glutin::surface::Surface<WindowSurface>,
    gl_context: &glutin::context::PossiblyCurrentContext,
    width: u32,
    height: u32,
) {
    if width > 0 && height > 0 {
        gl_surface.resize(
            gl_context,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );
    }
}
