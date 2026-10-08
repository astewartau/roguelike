//! Dungeon-themed egui styling.
//!
//! Provides a cohesive visual style that integrates with the game world:
//! flat panels, hard borders, muted dungeon colors, monospace font.

use egui::epaint::Shadow;
use egui::style::{WidgetVisuals, Widgets};
use egui::{Color32, FontData, FontDefinitions, FontFamily, Frame, Margin, Rounding, Stroke, Style, Visuals};

/// Dungeon color palette.
///
/// Everything the UI paints picks its colour from here. An inlined
/// `Color32::from_rgb` in a panel is a design decision nobody can find or
/// retune globally, so new shades belong in this module even when only one
/// call site wants them.
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

    // Resource bars. Each is a (fill, recess) pair: the recess is the dark
    // inset the fill sits in, tinted toward the fill so an empty bar still
    // says which resource it is.
    pub const HP_BAR: Color32 = Color32::from_rgb(140, 35, 35);
    pub const HP_BAR_BG: Color32 = Color32::from_rgb(40, 20, 20);
    /// Pale ghost drained behind the HP fill after a hit, so the lost chunk is
    /// visible for a moment as an event rather than just a smaller number.
    pub const HP_CHIP: Color32 = Color32::from_rgb(205, 125, 110);
    pub const XP_BAR: Color32 = Color32::from_rgb(70, 100, 140);
    pub const XP_BAR_BG: Color32 = Color32::from_rgb(25, 35, 50);
    pub const HUNGER_BAR: Color32 = Color32::from_rgb(190, 130, 55);
    pub const HUNGER_BAR_BG: Color32 = Color32::from_rgb(45, 32, 16);
    pub const FATIGUE_BAR: Color32 = Color32::from_rgb(110, 100, 185);
    pub const FATIGUE_BAR_BG: Color32 = Color32::from_rgb(28, 25, 48);
    /// The "Zz" glyph standing in for a fatigue icon.
    pub const FATIGUE_ICON: Color32 = Color32::from_rgb(150, 140, 210);

    // Bar frame. The recess is drawn with a 1px bevel: dark along the top and
    // left, light along the bottom and right, which is what makes it read as
    // sunk into the panel rather than painted on it.
    pub const BAR_BEVEL_DARK: Color32 = Color32::from_rgb(14, 12, 11);
    pub const BAR_BEVEL_LIGHT: Color32 = Color32::from_rgb(68, 60, 52);
    /// Hairline border around a bar, between the recess and the panel.
    pub const BAR_FRAME: Color32 = Color32::from_rgb(52, 45, 39);
    /// Segment notches scored across a bar so it can be counted at a glance.
    pub const BAR_NOTCH: Color32 = Color32::from_rgb(10, 9, 8);

    // Survival and stealth state labels
    pub const HUNGER_WARNING: Color32 = Color32::from_rgb(220, 160, 70);
    pub const HUNGER_CRITICAL: Color32 = Color32::from_rgb(230, 80, 70);
    pub const FATIGUE_WARNING: Color32 = Color32::from_rgb(200, 190, 110);
    pub const FATIGUE_CRITICAL: Color32 = Color32::from_rgb(170, 120, 220);
    pub const CONCEALED: Color32 = Color32::from_rgb(120, 200, 120);
    pub const SNEAKING: Color32 = Color32::from_rgb(150, 120, 210);

    // Hotbar feedback
    /// Flash over a slot whose ability just came off cooldown.
    pub const HOTBAR_READY_FLASH: Color32 = Color32::from_rgb(255, 250, 235);
    /// Flash over a slot the player pressed but cannot use right now.
    pub const HOTBAR_DENIED_FLASH: Color32 = Color32::from_rgb(205, 45, 40);
    /// Sweep over the part of a cooldown still to run.
    pub const HOTBAR_COOLDOWN_SWEEP: Color32 = Color32::from_rgb(6, 5, 5);

    // Floating damage and heal numbers
    /// An ordinary hit the player landed.
    pub const DAMAGE_DEALT: Color32 = Color32::from_rgb(255, 228, 198);
    /// A heavy hit the player landed.
    pub const DAMAGE_BIG: Color32 = Color32::from_rgb(255, 172, 62);
    /// A critical hit, which also gets a bigger punch and a `!`.
    pub const DAMAGE_CRIT: Color32 = Color32::from_rgb(255, 242, 115);
    /// Damage the player takes. Deliberately the one red in the set, so a hit
    /// on you is never confused with a hit you landed.
    pub const DAMAGE_TAKEN: Color32 = Color32::from_rgb(255, 64, 56);
    pub const HEAL_NUMBER: Color32 = Color32::from_rgb(105, 255, 105);
    /// Outline behind a floating number, so it survives a light floor.
    pub const NUMBER_OUTLINE: Color32 = Color32::BLACK;

    // Selection/Highlight
    pub const SELECTED: Color32 = Color32::from_rgb(70, 90, 110);
    pub const HOVERED: Color32 = Color32::from_rgb(45, 40, 35);

    // Accent colors
    pub const DUNGEON_GOLD: Color32 = Color32::from_rgb(210, 180, 100);
    pub const DUNGEON_GREEN: Color32 = Color32::from_rgb(80, 140, 80);

    // Item rarity tiers
    pub const RARITY_COMMON: Color32 = TEXT_PRIMARY;
    pub const RARITY_MAGIC: Color32 = Color32::from_rgb(110, 160, 230);
    pub const RARITY_RARE: Color32 = Color32::from_rgb(230, 200, 90);
    pub const RARITY_LEGENDARY: Color32 = Color32::from_rgb(255, 145, 40);

    // Status effects. Used for the effect pips and for the text fallback of
    // any effect that has no icon.
    pub const EFFECT_INVISIBLE: Color32 = Color32::from_rgb(180, 180, 255);
    pub const EFFECT_SPEED: Color32 = Color32::from_rgb(255, 220, 100);
    pub const EFFECT_REGEN: Color32 = Color32::from_rgb(100, 255, 100);
    pub const EFFECT_STRENGTH: Color32 = Color32::from_rgb(255, 150, 50);
    pub const EFFECT_PROTECTED: Color32 = Color32::from_rgb(150, 150, 255);
    pub const EFFECT_BARKSKIN: Color32 = Color32::from_rgb(139, 90, 43);
    pub const EFFECT_CONFUSED: Color32 = Color32::from_rgb(200, 100, 200);
    pub const EFFECT_FEARED: Color32 = Color32::from_rgb(255, 100, 100);
    pub const EFFECT_SLOWED: Color32 = Color32::from_rgb(100, 150, 200);
    pub const EFFECT_BURNING: Color32 = Color32::from_rgb(255, 100, 50);
    pub const EFFECT_ROOTED: Color32 = Color32::from_rgb(139, 90, 43);
    pub const EFFECT_INVULNERABLE: Color32 = Color32::from_rgb(255, 215, 0);
    pub const EFFECT_STUNNED: Color32 = Color32::from_rgb(255, 230, 120);
}

/// Color for a status effect, used by both the HUD pips and the text fallback
/// for effects with no icon.
pub fn effect_color(effect: crate::components::EffectType) -> Color32 {
    use crate::components::EffectType as E;
    match effect {
        E::Invisible => colors::EFFECT_INVISIBLE,
        E::SpeedBoost => colors::EFFECT_SPEED,
        E::Regenerating => colors::EFFECT_REGEN,
        E::Strengthened => colors::EFFECT_STRENGTH,
        E::Protected => colors::EFFECT_PROTECTED,
        E::Barkskin => colors::EFFECT_BARKSKIN,
        E::Confused => colors::EFFECT_CONFUSED,
        E::Feared => colors::EFFECT_FEARED,
        E::Slowed => colors::EFFECT_SLOWED,
        E::Burning => colors::EFFECT_BURNING,
        E::Rooted => colors::EFFECT_ROOTED,
        E::Invulnerable => colors::EFFECT_INVULNERABLE,
        E::Stunned => colors::EFFECT_STUNNED,
    }
}

/// Lift a colour's brightness by `factor`, saturating at white and leaving
/// alpha alone. Used for the bright leading edge on a resource bar and for the
/// flare on a freshly arrived log line.
pub fn brighten(color: Color32, factor: f32) -> Color32 {
    let lift = |c: u8| ((c as f32 * factor).round() as i32).clamp(0, 255) as u8;
    Color32::from_rgba_unmultiplied(
        lift(color.r()),
        lift(color.g()),
        lift(color.b()),
        color.a(),
    )
}

/// Color for an item rarity tier (for tooltips and item names).
pub fn rarity_color(rarity: crate::components::Rarity) -> Color32 {
    use crate::components::Rarity;
    match rarity {
        Rarity::Common => colors::RARITY_COMMON,
        Rarity::Magic => colors::RARITY_MAGIC,
        Rarity::Rare => colors::RARITY_RARE,
        Rarity::Legendary => colors::RARITY_LEGENDARY,
    }
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
    let mut style = Style {
        visuals: dungeon_visuals(),
        ..Default::default()
    };
    // Show tooltips immediately on hover, even while mouse is moving
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    style
}
