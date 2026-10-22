//! Dungeon-themed egui styling.
//!
//! Provides a cohesive visual style that integrates with the game world:
//! flat panels, hard borders, muted dungeon colors, monospace font.
//!
//! The flatness is deliberate and stays: `Rounding::ZERO`, 1px borders, muted
//! browns. What [`dungeon_window`] adds on top is detail density, so a panel
//! reads as carved stone rather than an unstyled rectangle — a hard offset
//! drop shadow, bevelled inner edges, a barely-there stone texture, corner
//! rivets and a short settle on open. All of it is painted here rather than
//! at the ten call sites, so the treatment lands everywhere at once; the
//! numbers behind it live in [`crate::constants`] (`src/constants/ui.rs`).

use crate::constants::*;
use crate::ui::icons::UiIcons;
use egui::emath::TSTransform;
use egui::epaint::Shadow;
use egui::style::{WidgetVisuals, Widgets};
use egui::{
    Color32, ColorImage, FontData, FontDefinitions, FontFamily, Frame, Margin, Painter, Pos2, Rect,
    Rounding, Stroke, Style, TextureHandle, TextureOptions, Visuals,
};

/// Dungeon color palette.
///
/// Everything the UI paints picks its colour from here. An inlined
/// `Color32::from_rgb` in a panel is a design decision nobody can find or
/// retune globally, so new shades belong in this module even when only one
/// call site wants them.
pub mod colors {
    use crate::constants::{PANEL_BEVEL_DARK_DELTA, PANEL_BEVEL_LIGHT_DELTA};
    use egui::Color32;

    // Panel backgrounds
    /// `PANEL_BG`'s channels on their own, so the bevel lines below can be
    /// derived from the background at compile time rather than drifting from
    /// it the next time the palette is retuned.
    const PANEL_BG_RGB: [u8; 3] = [25, 22, 20];
    pub const PANEL_BG: Color32 =
        Color32::from_rgb(PANEL_BG_RGB[0], PANEL_BG_RGB[1], PANEL_BG_RGB[2]);
    pub const PANEL_BORDER: Color32 = Color32::from_rgb(60, 52, 45);

    // Panel chrome
    /// Top/left inner bevel: `PANEL_BG` lightened by
    /// [`PANEL_BEVEL_LIGHT_DELTA`], which is what reads as a light source up
    /// and to the left of every panel.
    pub const BEVEL_LIGHT: Color32 = Color32::from_rgb(
        PANEL_BG_RGB[0].saturating_add(PANEL_BEVEL_LIGHT_DELTA),
        PANEL_BG_RGB[1].saturating_add(PANEL_BEVEL_LIGHT_DELTA),
        PANEL_BG_RGB[2].saturating_add(PANEL_BEVEL_LIGHT_DELTA),
    );
    /// Bottom/right inner bevel: `PANEL_BG` darkened by
    /// [`PANEL_BEVEL_DARK_DELTA`], the shaded side of the same light.
    pub const BEVEL_DARK: Color32 = Color32::from_rgb(
        PANEL_BG_RGB[0].saturating_sub(PANEL_BEVEL_DARK_DELTA),
        PANEL_BG_RGB[1].saturating_sub(PANEL_BEVEL_DARK_DELTA),
        PANEL_BG_RGB[2].saturating_sub(PANEL_BEVEL_DARK_DELTA),
    );
    /// The hard drop shadow panels cast. Near-black rather than pure black,
    /// and not quite opaque, so a panel over a torch-lit floor still shows a
    /// trace of what it is covering.
    pub const PANEL_SHADOW: Color32 = Color32::from_black_alpha(205);
    /// Multiply tint for the corner rivet sprite, which is a cold blue-grey
    /// buckler on the sheet. Warms it to dim brass so it belongs to the
    /// panel's palette instead of importing a second one.
    pub const PANEL_RIVET_TINT: Color32 = Color32::from_rgb(210, 170, 110);
    /// Hairline rule under a panel header. A touch brighter than
    /// `PANEL_BORDER` so it registers inside the panel rather than looking
    /// like a second edge.
    pub const PANEL_RULE: Color32 = Color32::from_rgb(74, 65, 56);

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
    /// "Asleep" in the hover panel: the fatigue bar's violet, lightened.
    pub const ASLEEP: Color32 = Color32::from_rgb(150, 140, 210);
    /// Light bar: warm firelight fill over a near-black background, so an
    /// empty bar reads as darkness.
    pub const LIGHT_BAR: Color32 = Color32::from_rgb(235, 185, 70);
    pub const LIGHT_BAR_BG: Color32 = Color32::from_rgb(30, 26, 20);

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
    /// Floating "miss" when a swing finds its target out of reach. A cool
    /// grey-blue so it reads as "nothing happened" next to the warm hit colours.
    pub const MISS_TEXT: Color32 = Color32::from_rgb(175, 190, 215);
    /// Outline behind a floating number, so it survives a light floor.
    pub const NUMBER_OUTLINE: Color32 = Color32::BLACK;

    // How a creature in the tile info panel stands toward the player. Muted
    // enough to sit in a panel next to TEXT_PRIMARY; the hostile red is
    // deliberately softer than DAMAGE_TAKEN so it reads as a label, not a hit.
    pub const RELATION_HOSTILE: Color32 = Color32::from_rgb(220, 110, 95);
    pub const RELATION_COMPANION: Color32 = Color32::from_rgb(120, 190, 120);
    pub const RELATION_FRIENDLY: Color32 = Color32::from_rgb(210, 180, 100);

    // Selection/Highlight
    pub const SELECTED: Color32 = Color32::from_rgb(70, 90, 110);

    // Accent colors
    pub const DUNGEON_GOLD: Color32 = Color32::from_rgb(210, 180, 100);
    pub const DUNGEON_GREEN: Color32 = Color32::from_rgb(80, 140, 80);

    // Item sockets (equipment slots, inventory cells): a recess darker than
    // the panel, so an empty socket reads as a hole waiting for something.
    pub const SOCKET_BG: Color32 = Color32::from_rgb(15, 13, 12);
    /// Faint warm wash over a socket the pointer is on.
    pub const SOCKET_HOVER_FILL: Color32 = Color32::from_rgb(34, 28, 22);
    /// Tint for the silhouette in an empty equipment socket (faded further by
    /// `CHARACTER_SILHOUETTE_ALPHA`).
    pub const SOCKET_SILHOUETTE: Color32 = Color32::from_rgb(200, 185, 160);

    // Character window
    /// A stat raised by gear.
    pub const STAT_BONUS: Color32 = Color32::from_rgb(140, 200, 115);
    /// A stat lowered by gear, and identified curse lines.
    pub const CURSE: Color32 = Color32::from_rgb(205, 90, 80);
    /// Carried weight, and the recess it fills.
    pub const WEIGHT_BAR: Color32 = Color32::from_rgb(150, 128, 92);
    pub const WEIGHT_BAR_BG: Color32 = Color32::from_rgb(34, 29, 22);
    /// An unlit pip in a stat row on the start screen.
    pub const PIP_EMPTY: Color32 = Color32::from_rgb(42, 37, 32);

    // Menus
    /// Behind the start screen's tiles, wherever they do not reach.
    pub const MENU_BACKDROP: Color32 = Color32::from_rgb(8, 7, 6);
    /// Multiplied into the start screen's tiles: dark, and warm, so the cold
    /// blue-grey stone reads as a room lit by fire rather than by daylight.
    pub const MENU_BACKDROP_TINT: Color32 = Color32::from_rgb(88, 72, 58);
    /// Firelight added over the start screen's floor around a torch.
    pub const TORCH_GLOW: Color32 = Color32::from_rgb(255, 160, 70);
    /// A rising ember.
    pub const EMBER: Color32 = Color32::from_rgb(255, 175, 80);
    /// The darkness the menu vignettes fade to at the screen edges.
    pub const VIGNETTE: Color32 = Color32::from_rgb(4, 3, 3);
    /// The hard shadow under a menu title.
    pub const TITLE_SHADOW: Color32 = Color32::from_black_alpha(230);
    /// Dims the frozen dungeon behind the pause menu.
    pub const PAUSE_DIM: Color32 = Color32::from_rgba_premultiplied(5, 4, 3, 190);
    /// Dims the frozen dungeon behind the game over screen, with a trace of
    /// red in it.
    pub const DEATH_DIM: Color32 = Color32::from_rgba_premultiplied(14, 3, 3, 215);
    /// "You Died".
    pub const DEATH_TITLE: Color32 = Color32::from_rgb(185, 45, 40);
    /// Fill of the primary button on a menu ("Enter the Dungeon", "Retry"):
    /// the ordinary button stone with brass worked into it.
    pub const BUTTON_PRIMARY: Color32 = Color32::from_rgb(58, 44, 28);
    pub const BUTTON_PRIMARY_HOVER: Color32 = Color32::from_rgb(78, 59, 34);

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
    pub const EFFECT_GUARDING: Color32 = Color32::from_rgb(200, 210, 230);
    pub const EFFECT_THORNS: Color32 = Color32::from_rgb(120, 200, 80);
    pub const EFFECT_BONE_WARD: Color32 = Color32::from_rgb(225, 225, 205);
    pub const EFFECT_WET: Color32 = Color32::from_rgb(90, 160, 230);
    pub const EFFECT_OILED: Color32 = Color32::from_rgb(150, 115, 70);
    pub const EFFECT_POISONED: Color32 = Color32::from_rgb(120, 200, 60);
    pub const EFFECT_BLEEDING: Color32 = Color32::from_rgb(200, 40, 40);
    /// Sickly zombie-flesh green.
    pub const EFFECT_GRABBED: Color32 = Color32::from_rgb(150, 170, 110);
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
        E::Guarding => colors::EFFECT_GUARDING,
        E::Thorns => colors::EFFECT_THORNS,
        E::BoneWard => colors::EFFECT_BONE_WARD,
        E::Wet => colors::EFFECT_WET,
        E::Oiled => colors::EFFECT_OILED,
        E::Poisoned => colors::EFFECT_POISONED,
        E::Bleeding => colors::EFFECT_BLEEDING,
        E::Grabbed => colors::EFFECT_GRABBED,
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

/// Mix `a` towards `b` by `t` (0 is all `a`), channel by channel including
/// alpha. For highlight fades, where both ends are palette colours.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()), ch(a.a(), b.a()))
}

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

    // The developer tools' modern looks: a clean sans and a heavier one for
    // headings, from assets/fonts (optional, see assets/README.md). Each
    // family ends with the ordinary proportional fonts, so a missing file,
    // or a glyph the font lacks (emoji), falls back rather than vanishing.
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, file) in [
        (super::devtools_look::SANS, "assets/fonts/AdwaitaSans-Regular.ttf"),
        (super::devtools_look::SANS_STRONG, "assets/fonts/NotoSans-Medium.ttf"),
    ] {
        let mut chain = Vec::new();
        if let Ok(data) = std::fs::read(file) {
            fonts.font_data.insert(family.to_owned(), FontData::from_owned(data));
            chain.push(family.to_owned());
        }
        chain.extend(fallback.iter().cloned());
        fonts.families.insert(FontFamily::Name(family.into()), chain);
    }

    fonts
}

/// Padding between a panel's border and its contents
pub const PANEL_INNER_MARGIN: f32 = 8.0;

/// The hard drop shadow panels cast.
///
/// Shadows are disabled globally in [`dungeon_visuals`] and stay that way;
/// this is the one place one is wanted. No blur and no spread: a hard offset
/// shadow is what a pixel-art panel casts, and it separates a panel from the
/// dungeon behind it far better than the 1px border can on its own. A soft
/// blurred shadow here would immediately read as a modern UI.
fn panel_shadow() -> Shadow {
    Shadow {
        offset: egui::vec2(PANEL_SHADOW_OFFSET, PANEL_SHADOW_OFFSET),
        blur: 0.0,
        spread: 0.0,
        color: colors::PANEL_SHADOW,
    }
}

/// Create a dungeon-themed window frame
pub fn dungeon_window_frame() -> Frame {
    Frame::none()
        .fill(colors::PANEL_BG)
        .stroke(Stroke::new(BORDER_WIDTH, colors::PANEL_BORDER))
        .inner_margin(Margin::same(PANEL_INNER_MARGIN))
        .shadow(panel_shadow())
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

// =============================================================================
// PANEL CHROME
// =============================================================================

/// Show a window with the full dungeon panel treatment.
///
/// Wraps [`egui::Window`] so that every panel in the game picks up the same
/// chrome — the frame and its hard shadow, bevelled inner edges, the stone
/// texture, corner rivets, a header-weight title and the entry animation —
/// without ten call sites each having to remember all of it. `configure` gets
/// the window to position and size; everything visual is applied here.
///
/// `title` is both the window's title and, as with a bare `egui::Window`, the
/// id its position and size are remembered under, so it has to stay stable.
pub fn dungeon_window<'w>(
    ctx: &egui::Context,
    icons: &UiIcons,
    title: &str,
    configure: impl FnOnce(egui::Window<'w>) -> egui::Window<'w>,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> Option<egui::Response> {
    let entry = entry_progress(ctx, title);
    // Alpha on a plain decelerating curve so the panel is legible almost at
    // once; scale on `out_back` so it overshoots a hair and settles, which is
    // the part that reads as weight rather than as a fade.
    let alpha = crate::ease::out_cubic(entry);
    let scale = PANEL_ENTRY_SCALE + (1.0 - PANEL_ENTRY_SCALE) * crate::ease::out_back(entry);

    // The title is drawn by egui outside `add_contents`, so it has to be faded
    // by hand rather than through the content ui's opacity.
    let title_text = egui::RichText::new(title)
        .strong()
        .color(colors::TEXT_PRIMARY.gamma_multiply(alpha));

    let inner = configure(egui::Window::new(title_text))
        .frame(dungeon_window_frame().multiply_with_opacity(alpha))
        .show(ctx, |ui| {
            ui.multiply_opacity(alpha);
            // Claim a slot for the stone texture before any widget is added,
            // so it is painted behind the contents. It can only be filled in
            // once the window's final size is known, which is after `show`.
            let backdrop = ui.painter().add(egui::Shape::Noop);
            // Where the contents begin, so the texture can start below the
            // title bar instead of over the top of it.
            let content_top = ui.max_rect().top() - PANEL_INNER_MARGIN;
            add_contents(ui);
            (backdrop, content_top)
        })?;

    let rect = inner.response.rect;
    let layer_id = inner.response.layer_id;
    let mut painter = ctx.layer_painter(layer_id);
    painter.multiply_opacity(alpha);

    if let Some((backdrop, content_top)) = inner.inner {
        painter.set(backdrop, panel_texture_shape(ctx, panel_texture_rect(rect, content_top)));
    }
    paint_panel_edges(&painter, icons, rect);

    // Scale about the panel's own centre: `p -> scale * p + centre * (1 -
    // scale)` fixes the centre and scales everything towards it. This moves
    // the shapes only, not the interaction rects, which for a sub-pixel
    // mismatch lasting `PANEL_ENTRY_DURATION` is not worth untangling.
    //
    // Tested against 1.0 rather than `< 1.0`: `out_back` overshoots, so the
    // scale passes a hair above full size on its way in, and that overshoot is
    // the whole reason for using that curve.
    if scale != 1.0 {
        let centre = rect.center();
        ctx.transform_layer_shapes(
            layer_id,
            TSTransform {
                scaling: scale,
                translation: centre.to_vec2() * (1.0 - scale),
            },
        );
    }

    Some(inner.response)
}

/// The part of a panel the stone texture covers: everything below the title
/// bar. The title bar keeps the flat `PANEL_BG` behind it, which reads as a
/// separate band rather than as a miss.
fn panel_texture_rect(window: Rect, content_top: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(window.left(), content_top.max(window.top())),
        window.max,
    )
}

/// Draw a section header inside a panel: the title a step up in weight, then a
/// hairline rule across the panel.
///
/// Panel titles used to be set in the same weight as the text under them, which
/// left a panel as one undifferentiated column of content. The rule is what
/// actually creates the hierarchy — it gives each section a visible top edge.
pub fn panel_header(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).heading().strong());
    panel_rule(ui);
}

/// The hairline rule from [`panel_header`] on its own, across the panel, for
/// dividing sections that need no title.
pub fn panel_rule(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, PANEL_HEADER_RULE_HEIGHT), egui::Sense::hover());
    let painter = ui.painter();
    let y = painter.round_to_pixel_center(rect.center().y);
    painter.hline(rect.x_range(), y, Stroke::new(BORDER_WIDTH, colors::PANEL_RULE));
}

/// Progress through a panel's entry animation: 0.0 on the pass it appears,
/// 1.0 once settled.
///
/// Keyed on the panel's title in egui's own data store, alongside the pass it
/// was last drawn on. A panel that was not drawn on the previous pass counts
/// as newly opened, which is what makes a window animate again every time it
/// is reopened. `Context::animate_bool` cannot do that job: it would have to
/// be ticked on the frames where the window is closed and draws nothing, and
/// these windows are simply not called then.
fn entry_progress(ctx: &egui::Context, title: &str) -> f32 {
    /// When a panel opened, and the last pass it was seen on.
    #[derive(Clone, Copy)]
    struct Entry {
        opened_at: f64,
        last_pass: u64,
    }

    let id = egui::Id::new(("dungeon_panel_entry", title));
    let now = ctx.input(|i| i.time);
    let pass = ctx.cumulative_pass_nr();

    let mut entry = ctx
        .data(|d| d.get_temp::<Entry>(id))
        .filter(|previous| previous.last_pass + 1 >= pass)
        .unwrap_or(Entry { opened_at: now, last_pass: pass });
    entry.last_pass = pass;
    ctx.data_mut(|d| d.insert_temp(id, entry));

    let progress = ((now - entry.opened_at) as f32 / PANEL_ENTRY_DURATION).clamp(0.0, 1.0);
    if progress < 1.0 {
        // Nothing else is necessarily moving, so the animation has to ask for
        // the frames it needs.
        ctx.request_repaint();
    }
    progress
}

/// The tiled stone texture for a panel, as one shape.
///
/// One uv unit is one tile, so the mottling keeps the same scale whatever size
/// the panel is; the texture wraps, so a uv rect larger than one unit is what
/// does the tiling.
fn panel_texture_shape(ctx: &egui::Context, rect: Rect) -> egui::Shape {
    let texture = panel_texture(ctx);
    let tiles = rect.size() / PANEL_TEXTURE_SIZE as f32;
    let uv = Rect::from_min_size(Pos2::ZERO, tiles);
    egui::Shape::image(texture.id(), rect, uv, Color32::WHITE)
}

/// Paint a panel's bevel and corner rivets over the panel at `rect`. These go
/// on top of the contents, unlike the texture, because they belong to the
/// panel's edge where no content is drawn.
pub fn paint_panel_edges(painter: &Painter, icons: &UiIcons, rect: Rect) {
    // Bevel: a lighter line down the top and left inner edges, a darker one
    // down the bottom and right. Rounded onto pixel centres, or a 1px line at
    // an arbitrary sub-pixel position is drawn as two half-lit pixels and the
    // whole effect goes soft.
    let inner = painter.round_rect_to_pixels(rect.shrink(BORDER_WIDTH));
    let light = Stroke::new(BORDER_WIDTH, colors::BEVEL_LIGHT);
    let dark = Stroke::new(BORDER_WIDTH, colors::BEVEL_DARK);
    let centre = |pos| painter.round_pos_to_pixel_center(pos);
    painter.line_segment([centre(inner.left_top()), centre(inner.right_top())], light);
    painter.line_segment([centre(inner.left_top()), centre(inner.left_bottom())], light);
    painter.line_segment([centre(inner.left_bottom()), centre(inner.right_bottom())], dark);
    painter.line_segment([centre(inner.right_top()), centre(inner.right_bottom())], dark);

    // Corner rivets: a brass-tinted sheet sprite at each corner, the detail
    // that makes a panel read as a built thing rather than a window. Pinned to
    // whole pixels so the 2:1 reduction stays on the pixel grid.
    let seats = inner.shrink(PANEL_RIVET_INSET + PANEL_RIVET_SIZE / 2.0);
    let rivet = egui::vec2(PANEL_RIVET_SIZE, PANEL_RIVET_SIZE);
    for seat in [
        seats.left_top(),
        seats.right_top(),
        seats.left_bottom(),
        seats.right_bottom(),
    ] {
        let seat = painter.round_pos_to_pixels(seat);
        painter.add(egui::Shape::image(
            icons.items_texture_id,
            Rect::from_center_size(seat, rivet),
            icons.panel_rivet_uv,
            colors::PANEL_RIVET_TINT,
        ));
    }
}

/// The panel stone texture, generated on first use and then cached in egui's
/// data store for the rest of the run.
///
/// Generated once into a texture rather than evaluated per frame: the noise is
/// fixed art, and sampling it per pixel per panel per frame would be paying
/// for the same answer sixty times a second.
fn panel_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::new("dungeon_panel_texture");
    if let Some(cached) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return cached;
    }
    let texture = ctx.load_texture(
        "dungeon_panel_noise",
        panel_noise_image(),
        // Nearest, because this is a pixel-art game and a smoothed tile would
        // be the only blurred thing on screen. Repeat, because the tile is
        // laid across panels of every size.
        TextureOptions::NEAREST_REPEAT,
    );
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

/// Build the panel noise tile: `PANEL_BG` with faint mottling worked into it,
/// so the large flat areas of a panel are not perfectly flat.
///
/// The pixels are opaque, and they carry the background colour rather than an
/// overlay on top of it. A translucent overlay would be the obvious way to do
/// this and it does not work here: `PANEL_BG` is dark enough that the faintest
/// alpha 8 bits can express already brightens it by roughly half, which is
/// nowhere near "barely perceptible". Writing the colour directly means what
/// is computed here is what reaches the screen.
///
/// Sampled as a slice of 4D Perlin noise taken around a torus — a circle per
/// axis — so that the tile is seamless. A flat 2D slice would not join up with
/// itself, and the mismatch would show as a grid every [`PANEL_TEXTURE_SIZE`]
/// pixels across every panel.
fn panel_noise_image() -> ColorImage {
    use noise::NoiseFn;

    let perlin = noise::Perlin::new(PANEL_TEXTURE_SEED);
    let size = PANEL_TEXTURE_SIZE;
    // One trip around each circle is one trip across the tile, so the circle's
    // radius in noise space is what sets the frequency.
    let radius = PANEL_TEXTURE_FREQUENCY / std::f64::consts::TAU;

    let sample = |x: usize, y: usize| {
        let u = x as f64 / size as f64 * std::f64::consts::TAU;
        let v = y as f64 / size as f64 * std::f64::consts::TAU;
        perlin.get([
            radius * u.cos(),
            radius * u.sin(),
            radius * v.cos(),
            radius * v.sin(),
        ]) as f32
    };

    // Normalise against the tile's own extremes. Perlin only approaches its
    // nominal ±1 range, and 4D slices stay well short of it, so without this
    // `PANEL_TEXTURE_AMPLITUDE` would quietly mean a third of what it says.
    let values: Vec<f32> = (0..size * size).map(|i| sample(i % size, i / size)).collect();
    let extreme = values.iter().fold(0.0f32, |peak, v| peak.max(v.abs())).max(f32::EPSILON);
    let amplitude = PANEL_TEXTURE_AMPLITUDE as f32;

    let base = colors::PANEL_BG;
    let shift = |channel: u8, delta: f32| {
        (channel as f32 + delta).round().clamp(0.0, 255.0) as u8
    };
    let pixels = values
        .iter()
        .map(|value| {
            let delta = value / extreme * amplitude;
            Color32::from_rgb(
                shift(base.r(), delta),
                shift(base.g(), delta),
                shift(base.b(), delta),
            )
        })
        .collect();

    ColorImage { size: [size, size], pixels }
}


// =============================================================================
// SOCKETS, GLOWS AND MENU PIECES
// =============================================================================
// Painted pieces shared by the menus and the character window. Same rules as
// the panel chrome above: square corners, 1px lines on pixel centres, hard
// shadows, nothing blurred.

/// Shadow, stone and border for a slab at `rect`, as one shape, so it can fill
/// a placeholder laid down before the slab's contents.
pub fn stone_shape(ctx: &egui::Context, rect: Rect, border: Color32) -> egui::Shape {
    let shadow = egui::vec2(PANEL_SHADOW_OFFSET, PANEL_SHADOW_OFFSET);
    egui::Shape::Vec(vec![
        egui::Shape::rect_filled(rect.translate(shadow), Rounding::ZERO, colors::PANEL_SHADOW),
        panel_texture_shape(ctx, rect),
        egui::Shape::rect_stroke(rect, Rounding::ZERO, Stroke::new(BORDER_WIDTH, border)),
    ])
}

/// Lay `add_contents` out on a stone slab: the shadow, texture, bevel and
/// rivets of a [`dungeon_window`], for a panel that sits inside a layout
/// (the start screen's class details, the pause menu) rather than floating as
/// a window of its own.
pub fn stone_panel<R>(
    ui: &mut egui::Ui,
    icons: &UiIcons,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let backdrop = ui.painter().add(egui::Shape::Noop);
    // Top to bottom whatever the parent's layout is: a panel dropped into a
    // row would otherwise lay its own contents out sideways.
    let inner = Frame::none()
        .inner_margin(Margin::same(PANEL_INNER_MARGIN))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add_contents).inner
        });
    let rect = ui.painter().round_rect_to_pixels(inner.response.rect);
    ui.painter().set(backdrop, stone_shape(ui.ctx(), rect, colors::PANEL_BORDER));
    paint_panel_edges(ui.painter(), icons, rect);
    inner
}

/// Paint a recessed socket at `rect`: `fill`, a 1px inset bevel (dark along
/// the top and left, light along the bottom and right) and a hairline frame.
/// The same recess the HUD bars sit in, so an item slot reads as cut into the
/// panel rather than pasted onto it.
pub fn paint_recess(painter: &Painter, rect: Rect, fill: Color32) {
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_filled(rect, Rounding::ZERO, fill);
    let inner = rect.shrink(BORDER_WIDTH);
    let centre = |pos| painter.round_pos_to_pixel_center(pos);
    let dark = Stroke::new(BORDER_WIDTH, colors::BAR_BEVEL_DARK);
    let light = Stroke::new(BORDER_WIDTH, colors::BAR_BEVEL_LIGHT);
    painter.line_segment([centre(inner.left_top()), centre(inner.right_top())], dark);
    painter.line_segment([centre(inner.left_top()), centre(inner.left_bottom())], dark);
    painter.line_segment([centre(inner.left_bottom()), centre(inner.right_bottom())], light);
    painter.line_segment([centre(inner.right_top()), centre(inner.right_bottom())], light);
    painter.rect_stroke(rect, Rounding::ZERO, Stroke::new(BORDER_WIDTH, colors::BAR_FRAME));
}

/// Light added by a glow of `color` at `strength` (0-1): the colour scaled
/// down, with zero alpha. Under egui's premultiplied blending a zero-alpha
/// colour is added to what is underneath rather than covering it, so a glow
/// over a dark floor lights the floor instead of fogging it.
fn additive(color: Color32, strength: f32) -> Color32 {
    let scale = |c: u8| (c as f32 * strength.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_premultiplied(scale(color.r()), scale(color.g()), scale(color.b()), 0)
}

/// Add a round glow of `color` centred on `centre`, at full `strength` in the
/// middle and gone by `radius`. The falloff is steeper than linear, which is
/// what makes it read as a light source rather than a coloured disc.
pub fn paint_glow(painter: &Painter, centre: Pos2, radius: f32, color: Color32, strength: f32) {
    // (fraction of the radius, fraction of the strength): roughly a squared
    // falloff, in few enough rings to stay cheap.
    const RINGS: [(f32, f32); 4] = [(0.0, 1.0), (0.3, 0.55), (0.65, 0.18), (1.0, 0.0)];
    let mut mesh = egui::Mesh::default();
    let segments = UI_GLOW_SEGMENTS as u32;
    for (fraction, falloff) in RINGS {
        let vertex_color = additive(color, strength * falloff);
        for i in 0..segments {
            let angle = i as f32 / segments as f32 * std::f32::consts::TAU;
            let offset = egui::vec2(angle.cos(), angle.sin()) * radius * fraction;
            mesh.colored_vertex(centre + offset, vertex_color);
        }
    }
    for ring in 0..RINGS.len() as u32 - 1 {
        let (inner, outer) = (ring * segments, (ring + 1) * segments);
        for i in 0..segments {
            let j = (i + 1) % segments;
            mesh.add_triangle(inner + i, outer + i, outer + j);
            mesh.add_triangle(inner + i, outer + j, inner + j);
        }
    }
    painter.add(mesh);
}

/// Darken the edges of `rect` towards `color` at `max_alpha`, clear in an
/// ellipse covering the middle `1 - reach` of it. Pulls the eye to the centre
/// of a menu screen and lets whatever is behind it fall away into the dark.
///
/// A grid of vertices, each shaded by how far out along the ellipse it sits,
/// so the darkness reaches right into the corners with no seams.
pub fn paint_vignette(painter: &Painter, rect: Rect, color: Color32, max_alpha: u8, reach: f32) {
    let cells = UI_VIGNETTE_GRID as u32;
    let clear = 1.0 - reach.clamp(0.01, 1.0);
    let half = rect.size() / 2.0;
    let mut mesh = egui::Mesh::default();
    for row in 0..=cells {
        for column in 0..=cells {
            let pos = rect.min + egui::vec2(column as f32, row as f32) / cells as f32 * rect.size();
            let from_centre = (pos - rect.center()) / half;
            let t = ((from_centre.length() - clear) / (1.0 - clear)).clamp(0.0, 1.0);
            // Eased, so the dark gathers at the edges instead of starting as a
            // visible ring.
            let alpha = (max_alpha as f32 * t * t * (3.0 - 2.0 * t)).round() as u8;
            mesh.colored_vertex(pos, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha));
        }
    }
    let stride = cells + 1;
    for row in 0..cells {
        for column in 0..cells {
            let a = row * stride + column;
            mesh.add_triangle(a, a + 1, a + stride + 1);
            mesh.add_triangle(a, a + stride + 1, a + stride);
        }
    }
    painter.add(mesh);
}

/// A menu title: spaced capitals in `color` over a hard drop shadow, centred
/// in the available width.
pub fn menu_title(ui: &mut egui::Ui, text: &str, color: Color32) {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat {
            font_id: egui::FontId::proportional(MENU_TITLE_SIZE),
            extra_letter_spacing: MENU_TITLE_LETTER_SPACING,
            color,
            ..Default::default()
        },
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), galley.size().y),
        egui::Sense::hover(),
    );
    // Letter spacing also follows the last letter, so the text is a spacing
    // wider than it looks; shift by half of that to centre what is seen.
    let x = rect.center().x - (galley.size().x - MENU_TITLE_LETTER_SPACING) / 2.0;
    let pos = ui.painter().round_pos_to_pixels(Pos2::new(x, rect.top()));
    let shadow = egui::vec2(MENU_TITLE_SHADOW_OFFSET, MENU_TITLE_SHADOW_OFFSET);
    let painter = ui.painter();
    painter.galley_with_override_text_color(pos + shadow, galley.clone(), colors::TITLE_SHADOW);
    painter.galley(pos, galley, color);
}

/// An ornamental rule `width` wide, centred: a hairline that fades out
/// towards both ends, with a small brass diamond at its middle. Goes under a
/// menu title or between sections of a menu.
pub fn ornament_rule(ui: &mut egui::Ui, width: f32) {
    let d = MENU_RULE_DIAMOND;
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), d * 2.0 + 2.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let centre = painter.round_pos_to_pixel_center(row.center());
    let half = width / 2.0;
    // Each half of the line is a 1px quad, solid by the diamond and
    // transparent at the far end.
    let line = colors::BUTTON_BORDER;
    let mut mesh = egui::Mesh::default();
    for (near, far) in [(centre.x - d * 2.0, centre.x - half), (centre.x + d * 2.0, centre.x + half)] {
        let base = mesh.vertices.len() as u32;
        let (top, bottom) = (centre.y - 0.5, centre.y + 0.5);
        mesh.colored_vertex(Pos2::new(near, top), line);
        mesh.colored_vertex(Pos2::new(far, top), Color32::TRANSPARENT);
        mesh.colored_vertex(Pos2::new(far, bottom), Color32::TRANSPARENT);
        mesh.colored_vertex(Pos2::new(near, bottom), line);
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(mesh);
    painter.add(egui::Shape::convex_polygon(
        vec![
            centre + egui::vec2(0.0, -d),
            centre + egui::vec2(d, 0.0),
            centre + egui::vec2(0.0, d),
            centre + egui::vec2(-d, 0.0),
        ],
        colors::TEXT_ACCENT,
        Stroke::NONE,
    ));
}

/// How much a menu button stands out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonWeight {
    /// The one thing the screen is for ("Enter the Dungeon", "Retry"): brass
    /// worked into the stone, gold lettering.
    Primary,
    /// An ordinary full-size menu entry.
    Normal,
    /// A smaller secondary action beside the main ones.
    Small,
}

/// A menu button: a stone slab with a bevel and a hard shadow, warming to
/// brass with a pointer either side while `highlighted` (the keyboard
/// selection) or hovered.
///
/// Painted rather than an `egui::Button` so the highlight can fade, which a
/// stock button cannot do, and so every menu shares exactly one look.
pub fn menu_button(
    ui: &mut egui::Ui,
    label: &str,
    weight: ButtonWeight,
    highlighted: bool,
) -> egui::Response {
    let (size, font_size) = match weight {
        ButtonWeight::Small => (MENU_BUTTON_SMALL_SIZE, MENU_BUTTON_SMALL_FONT_SIZE),
        _ => (MENU_BUTTON_SIZE, MENU_BUTTON_FONT_SIZE),
    };
    let (rect, response) = ui.allocate_exact_size(size.into(), egui::Sense::click());
    let enabled = ui.is_enabled();
    let lit = enabled && (highlighted || response.hovered());
    let t = ui
        .ctx()
        .animate_bool_with_time(response.id.with("menu_button_lit"), lit, MENU_HIGHLIGHT_FADE);

    let (base, hover, text, text_lit) = match weight {
        ButtonWeight::Primary => (
            colors::BUTTON_PRIMARY,
            colors::BUTTON_PRIMARY_HOVER,
            colors::DUNGEON_GOLD,
            brighten(colors::DUNGEON_GOLD, 1.15),
        ),
        _ => (colors::BUTTON_BG, colors::BUTTON_HOVER, colors::TEXT_PRIMARY, colors::DUNGEON_GOLD),
    };
    let fill = if response.is_pointer_button_down_on() {
        colors::BUTTON_ACTIVE
    } else {
        mix(base, hover, t)
    };
    let text_color = if enabled { mix(text, text_lit, t) } else { colors::TEXT_MUTED.gamma_multiply(0.6) };
    let border = mix(colors::BUTTON_BORDER, colors::TEXT_ACCENT, t);

    let painter = ui.painter();
    let rect = painter.round_rect_to_pixels(rect);
    let shadow = egui::vec2(MENU_BUTTON_SHADOW_OFFSET, MENU_BUTTON_SHADOW_OFFSET);
    painter.rect_filled(rect.translate(shadow), Rounding::ZERO, colors::PANEL_SHADOW);
    painter.rect_filled(rect, Rounding::ZERO, fill);
    // Raised bevel: lit along the top and left, shaded along the bottom and
    // right, the opposite of a socket's recess.
    let inner = rect.shrink(BORDER_WIDTH);
    let centre = |pos| painter.round_pos_to_pixel_center(pos);
    let light = Stroke::new(BORDER_WIDTH, brighten(fill, 1.45));
    let dark = Stroke::new(BORDER_WIDTH, mix(fill, Color32::BLACK, 0.4));
    painter.line_segment([centre(inner.left_top()), centre(inner.right_top())], light);
    painter.line_segment([centre(inner.left_top()), centre(inner.left_bottom())], light);
    painter.line_segment([centre(inner.left_bottom()), centre(inner.right_bottom())], dark);
    painter.line_segment([centre(inner.right_top()), centre(inner.right_bottom())], dark);
    painter.rect_stroke(rect, Rounding::ZERO, Stroke::new(BORDER_WIDTH, border));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(font_size),
        text_color,
    );

    // Brass pointers either side, pointing in, fading with the highlight.
    if t > 0.0 && weight != ButtonWeight::Small {
        let s = MENU_POINTER_SIZE;
        let colour = colors::TEXT_ACCENT.gamma_multiply(t);
        let y = rect.center().y;
        for (tip, back) in [
            (rect.left() - s * 1.5, rect.left() - s * 3.0),
            (rect.right() + s * 1.5, rect.right() + s * 3.0),
        ] {
            painter.add(egui::Shape::convex_polygon(
                vec![Pos2::new(back, y - s), Pos2::new(tip, y), Pos2::new(back, y + s)],
                colour,
                Stroke::NONE,
            ));
        }
    }

    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A small toggle for picking one of a few options (the start screen's seed
/// mode): a recess when off, a lit brass-edged slab when `selected`.
pub fn menu_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(MENU_CHIP_SIZE.into(), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    let rect = painter.round_rect_to_pixels(rect);
    if selected {
        painter.rect_filled(rect, Rounding::ZERO, colors::BUTTON_PRIMARY);
        painter.rect_stroke(rect, Rounding::ZERO, Stroke::new(BORDER_WIDTH, colors::TEXT_ACCENT));
    } else {
        paint_recess(painter, rect, if hovered { colors::SOCKET_HOVER_FILL } else { colors::SOCKET_BG });
    }
    let text = if selected {
        colors::DUNGEON_GOLD
    } else if hovered {
        colors::TEXT_PRIMARY
    } else {
        colors::TEXT_MUTED
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(MENU_BUTTON_SMALL_FONT_SIZE - 1.0),
        text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Lay out a dimmed, vignetted full-screen overlay with a centred stone panel
/// on it: the shared shape of the pause and game over screens. `dim` covers
/// the frozen dungeon; the panel is [`MENU_PANEL_WIDTH`] wide and holds
/// whatever `add_contents` puts in it.
pub fn menu_overlay(
    ctx: &egui::Context,
    icons: &UiIcons,
    dim: Color32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::CentralPanel::default()
        .frame(Frame::none().fill(dim))
        .show(ctx, |ui| {
            let screen = ui.max_rect();
            paint_vignette(ui.painter(), screen, colors::VIGNETTE, MENU_VIGNETTE_ALPHA, MENU_VIGNETTE_REACH);
        });
    egui::Area::new(egui::Id::new("menu_overlay_panel"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            stone_panel(ui, icons, |ui| {
                ui.set_width(MENU_PANEL_WIDTH);
                ui.vertical_centered(add_contents);
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bevel has to brighten one edge and darken the other, or it is not a
    /// bevel. Easy to break by retuning `PANEL_BG` towards either end.
    #[test]
    fn bevel_lines_straddle_the_panel_background() {
        let (bg, light, dark) = (colors::PANEL_BG, colors::BEVEL_LIGHT, colors::BEVEL_DARK);
        assert!(light.r() > bg.r() && light.g() > bg.g() && light.b() > bg.b());
        assert!(dark.r() < bg.r() && dark.g() < bg.g() && dark.b() < bg.b());
    }

    /// A hard shadow, not a soft one: any blur or spread and it stops being
    /// period-appropriate for a pixel-art game.
    #[test]
    fn panel_shadow_is_hard_and_offset() {
        let shadow = panel_shadow();
        assert_eq!(shadow.blur, 0.0);
        assert_eq!(shadow.spread, 0.0);
        assert!(shadow.offset.x > 0.0 && shadow.offset.y > 0.0);
    }

    /// "Barely perceptible" is the whole specification for the texture, so
    /// its deviation from `PANEL_BG` is worth pinning: a few levels, never
    /// enough to be seen as noise.
    #[test]
    fn panel_texture_stays_within_a_few_levels_of_the_background() {
        let image = panel_noise_image();
        assert_eq!(image.size, [PANEL_TEXTURE_SIZE, PANEL_TEXTURE_SIZE]);
        let base = colors::PANEL_BG;
        let deviation = image
            .pixels
            .iter()
            .map(|pixel| pixel.r().abs_diff(base.r()))
            .max()
            .unwrap();
        assert_eq!(
            deviation, PANEL_TEXTURE_AMPLITUDE,
            "texture deviates by {deviation} levels, want {PANEL_TEXTURE_AMPLITUDE}"
        );
        // Opaque: the tile is the background, not something laid over it.
        assert!(image.pixels.iter().all(|pixel| pixel.a() == 255));
    }

    /// The tile is laid down repeating, so opposite edges have to match or the
    /// seam shows as a grid across every panel.
    #[test]
    fn panel_noise_tile_is_seamless() {
        let image = panel_noise_image();
        let size = PANEL_TEXTURE_SIZE;
        let at = |x: usize, y: usize| image.pixels[y * size + x];
        for i in 0..size {
            // Columns 0 and `size - 1` are a step apart, not identical; what
            // matters is that they are adjacent samples of the same field.
            assert!(at(0, i).r().abs_diff(at(size - 1, i).r()) <= 1);
            assert!(at(i, 0).r().abs_diff(at(i, size - 1).r()) <= 1);
        }
    }
}
