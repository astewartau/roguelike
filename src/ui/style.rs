//! Dungeon-themed egui styling.
//!
//! Provides a cohesive visual style that integrates with the game world:
//! flat panels, hard borders, muted dungeon colors, monospace font.

use egui::epaint::Shadow;
use egui::style::{WidgetVisuals, Widgets};
use egui::{Color32, FontData, FontDefinitions, FontFamily, Frame, Margin, Rounding, Stroke, Style, Visuals};

/// Dungeon color palette
pub mod colors {
    use egui::Color32;

    // Panel backgrounds
    pub const PANEL_BG: Color32 = Color32::from_rgb(25, 22, 20);
    pub const PANEL_BORDER: Color32 = Color32::from_rgb(60, 52, 45);

    // Interactive elements
    pub const BUTTON_BG: Color32 = Color32::from_rgb(35, 30, 28);
    pub const BUTTON_HOVER: Color32 = Color32::from_rgb(50, 43, 38);
    pub const BUTTON_ACTIVE: Color32 = Color32::from_rgb(65, 55, 48);
    pub const BUTTON_BORDER: Color32 = Color32::from_rgb(80, 70, 60);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 210, 195);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(150, 140, 125);
    pub const TEXT_ACCENT: Color32 = Color32::from_rgb(210, 180, 100);

    // Progress bars
    pub const HP_BAR: Color32 = Color32::from_rgb(140, 35, 35);
    pub const HP_BAR_BG: Color32 = Color32::from_rgb(40, 20, 20);
    pub const XP_BAR: Color32 = Color32::from_rgb(70, 100, 140);
    pub const XP_BAR_BG: Color32 = Color32::from_rgb(25, 35, 50);

    // Selection/Highlight
    pub const SELECTED: Color32 = Color32::from_rgb(70, 90, 110);
    pub const HOVERED: Color32 = Color32::from_rgb(45, 40, 35);

    // Accent colors
    pub const DUNGEON_GOLD: Color32 = Color32::from_rgb(210, 180, 100);
    pub const DUNGEON_GREEN: Color32 = Color32::from_rgb(80, 140, 80);
}

/// Border width for panels and buttons
pub const BORDER_WIDTH: f32 = 1.0;

/// Create the dungeon-themed visuals
pub fn dungeon_visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    // Zero rounding everywhere
    visuals.window_rounding = Rounding::ZERO;
    visuals.menu_rounding = Rounding::ZERO;

    // Disable shadows
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;

    // Window styling
    visuals.window_fill = colors::PANEL_BG;
    visuals.window_stroke = Stroke::new(BORDER_WIDTH, colors::PANEL_BORDER);

    // Panel/frame backgrounds
    visuals.panel_fill = colors::PANEL_BG;
    visuals.extreme_bg_color = colors::PANEL_BG;
    visuals.faint_bg_color = Color32::from_rgb(30, 27, 24);

    // Widget styling
    visuals.widgets = dungeon_widgets();

    // Selection
    visuals.selection.bg_fill = colors::SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, colors::TEXT_ACCENT);

    // Text colors
    visuals.override_text_color = Some(colors::TEXT_PRIMARY);

    visuals
}

/// Widget visuals for the dungeon theme
fn dungeon_widgets() -> Widgets {
    Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: colors::PANEL_BG,
            weak_bg_fill: colors::PANEL_BG,
            bg_stroke: Stroke::new(BORDER_WIDTH, colors::PANEL_BORDER),
            rounding: Rounding::ZERO,
            fg_stroke: Stroke::new(1.0, colors::TEXT_MUTED),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: colors::BUTTON_BG,
            weak_bg_fill: colors::BUTTON_BG,
            bg_stroke: Stroke::new(BORDER_WIDTH, colors::BUTTON_BORDER),
            rounding: Rounding::ZERO,
            fg_stroke: Stroke::new(1.0, colors::TEXT_PRIMARY),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: colors::BUTTON_HOVER,
            weak_bg_fill: colors::BUTTON_HOVER,
            bg_stroke: Stroke::new(BORDER_WIDTH, colors::TEXT_ACCENT),
            rounding: Rounding::ZERO,
            fg_stroke: Stroke::new(1.0, colors::TEXT_PRIMARY),
            expansion: 0.0,
        },
        active: WidgetVisuals {
            bg_fill: colors::BUTTON_ACTIVE,
            weak_bg_fill: colors::BUTTON_ACTIVE,
            bg_stroke: Stroke::new(2.0, colors::TEXT_ACCENT),
            rounding: Rounding::ZERO,
            fg_stroke: Stroke::new(1.0, colors::TEXT_PRIMARY),
            expansion: 0.0,
        },
        open: WidgetVisuals {
            bg_fill: colors::BUTTON_ACTIVE,
            weak_bg_fill: colors::BUTTON_ACTIVE,
            bg_stroke: Stroke::new(BORDER_WIDTH, colors::BUTTON_BORDER),
            rounding: Rounding::ZERO,
            fg_stroke: Stroke::new(1.0, colors::TEXT_PRIMARY),
            expansion: 0.0,
        },
    }
}

/// Load Hack monospace font and set as default
pub fn load_fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    // Load Hack font from system
    if let Ok(font_data) = std::fs::read("/usr/share/fonts/TTF/Hack-Regular.ttf") {
        fonts
            .font_data
            .insert("hack".to_owned(), FontData::from_owned(font_data));

        // Set Hack as the primary proportional and monospace font
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "hack".to_owned());

        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "hack".to_owned());
    }

    fonts
}

/// Create a dungeon-themed window frame
pub fn dungeon_window_frame() -> Frame {
    Frame::none()
        .fill(colors::PANEL_BG)
        .stroke(Stroke::new(BORDER_WIDTH, colors::PANEL_BORDER))
        .inner_margin(Margin::same(8.0))
}

/// Create the dungeon-themed style with immediate tooltips
pub fn dungeon_style() -> Style {
    let mut style = Style::default();
    style.visuals = dungeon_visuals();
    // Show tooltips immediately on hover, even while mouse is moving
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    style
}
