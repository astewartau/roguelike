//! How the developer tools panel looks: switchable styles, so they can be
//! compared in the game (the picker is in the panel's header).
//!
//! Everything here is scoped to the panel: `apply` sets the style of the
//! panel's own `Ui`, so the game's HUD and windows keep the dungeon look.
//! The panel's widgets ask this module for colours (`pal`) instead of using
//! the dungeon palette directly, so every look reads well, light ones
//! included.

use egui::{Color32, FontFamily, FontId, Rounding, Stroke};

use super::style::colors;

/// A look for the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DevLook {
    /// The dungeon style the rest of the UI uses.
    Classic,
    /// Modern dark: cool greys, an indigo accent, rounded cards.
    #[default]
    Slate,
    /// Modern light, like a web app.
    Daylight,
    /// Modern dark with the game's amber fire as its accent.
    Ember,
}

impl DevLook {
    pub const ALL: [DevLook; 4] = [DevLook::Classic, DevLook::Slate, DevLook::Daylight, DevLook::Ember];

    pub fn name(self) -> &'static str {
        match self {
            DevLook::Classic => "Classic",
            DevLook::Slate => "Slate",
            DevLook::Daylight => "Daylight",
            DevLook::Ember => "Ember",
        }
    }

    fn modern(self) -> bool {
        self != DevLook::Classic
    }
}

/// The colours a look is drawn in.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Behind the panel.
    pub bg: Color32,
    /// Behind cards, inputs and buttons.
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    /// Text on an accent fill.
    pub on_accent: Color32,
    pub hover: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

pub fn palette(look: DevLook) -> Palette {
    let hex = |rgb: u32| Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    match look {
        DevLook::Classic => Palette {
            bg: colors::PANEL_BG,
            card: colors::BUTTON_BG,
            border: colors::PANEL_BORDER,
            text: colors::TEXT_PRIMARY,
            muted: colors::TEXT_MUTED,
            accent: colors::TEXT_ACCENT,
            on_accent: Color32::BLACK,
            hover: colors::HOVERED,
            warning: Color32::from_rgb(225, 170, 80),
            danger: colors::RELATION_HOSTILE,
        },
        DevLook::Slate => Palette {
            bg: Color32::from_rgba_unmultiplied(17, 20, 27, 244),
            card: hex(0x1B2030),
            border: hex(0x2B3245),
            text: hex(0xE7EAF2),
            muted: hex(0x8C94A9),
            accent: hex(0x7C8CFF),
            on_accent: hex(0x0E1020),
            hover: hex(0x252B3C),
            warning: hex(0xF0B357),
            danger: hex(0xF0716A),
        },
        DevLook::Daylight => Palette {
            bg: Color32::from_rgba_unmultiplied(248, 249, 251, 248),
            card: hex(0xFFFFFF),
            border: hex(0xE2E5EC),
            text: hex(0x1B2130),
            muted: hex(0x6A7286),
            accent: hex(0x2F6BFF),
            on_accent: hex(0xFFFFFF),
            hover: hex(0xEEF1F7),
            warning: hex(0xB76E00),
            danger: hex(0xD8443B),
        },
        DevLook::Ember => Palette {
            bg: Color32::from_rgba_unmultiplied(22, 17, 14, 244),
            card: hex(0x241B16),
            border: hex(0x3A2D24),
            text: hex(0xF3E8DC),
            muted: hex(0xA8957F),
            accent: hex(0xF2A33A),
            on_accent: hex(0x1E1309),
            hover: hex(0x2F241D),
            warning: hex(0xF2C94C),
            danger: hex(0xF07A5A),
        },
    }
}

/// The font families `style::load_fonts` registers for the modern looks:
/// a clean sans for text, a heavier one for headings. Both fall back to
/// the game's own fonts (and its emoji) when the files aren't there.
pub const SANS: &str = "dev-sans";
pub const SANS_STRONG: &str = "dev-sans-strong";

fn look_id() -> egui::Id {
    egui::Id::new("devtools_look")
}

/// The look the panel is being drawn in.
pub fn current(ui: &egui::Ui) -> DevLook {
    ui.ctx().data(|d| d.get_temp(look_id())).unwrap_or_default()
}

/// The current look's colours.
pub fn pal(ui: &egui::Ui) -> Palette {
    palette(current(ui))
}

/// Style the panel's `Ui` for `look`, and remember it for the helpers below.
pub fn apply(ui: &mut egui::Ui, look: DevLook) {
    ui.ctx().data_mut(|d| d.insert_temp(look_id(), look));
    if !look.modern() {
        return;
    }
    let p = palette(look);
    let style = ui.style_mut();
    let sans = |size: f32| FontId::new(size, FontFamily::Name(SANS.into()));
    style.text_styles.insert(egui::TextStyle::Body, sans(14.0));
    style.text_styles.insert(egui::TextStyle::Button, sans(14.0));
    style.text_styles.insert(egui::TextStyle::Small, sans(11.5));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::new(19.0, FontFamily::Name(SANS_STRONG.into())));
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.slider_rail_height = 5.0;

    let v = &mut style.visuals;
    v.dark_mode = look != DevLook::Daylight;
    v.override_text_color = Some(p.text);
    v.panel_fill = p.bg;
    v.window_fill = p.bg;
    v.extreme_bg_color = p.card;
    v.faint_bg_color = p.hover;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0_f32, p.on_accent);
    v.hyperlink_color = p.accent;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    let round = Rounding::same(8.0);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, p.bg, p.border),
        (&mut v.widgets.inactive, p.card, p.border),
        (&mut v.widgets.hovered, p.hover, p.accent),
        (&mut v.widgets.active, p.accent, p.accent),
        (&mut v.widgets.open, p.hover, p.border),
    ] {
        w.rounding = round;
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.fg_stroke = Stroke::new(1.0_f32, p.text);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, p.on_accent);
}

/// The frame the whole panel sits in: a floating rounded card with a soft
/// shadow for the modern looks, the dungeon panel for Classic.
pub fn panel_frame(look: DevLook) -> egui::Frame {
    let p = palette(look);
    if !look.modern() {
        return egui::Frame::none()
            .fill(p.bg)
            .stroke(Stroke::new(super::style::BORDER_WIDTH, p.border))
            .inner_margin(egui::Margin::same(8.0));
    }
    egui::Frame::none()
        .fill(p.bg)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(14.0))
        .inner_margin(egui::Margin::same(14.0))
        .outer_margin(egui::Margin::same(8.0))
        .shadow(egui::epaint::Shadow {
            offset: egui::vec2(0.0, 6.0),
            blur: 24.0,
            spread: 0.0,
            color: Color32::from_black_alpha(if look == DevLook::Daylight { 40 } else { 110 }),
        })
}

/// A card around a group of controls (a plain group for Classic).
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
    if !current(ui).modern() {
        return add(ui);
    }
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A section label: small, quiet capitals with room above, in the modern
/// looks; the dungeon's ruled header in Classic.
pub fn section(ui: &mut egui::Ui, text: &str) {
    if !current(ui).modern() {
        super::style::panel_header(ui, text);
        return;
    }
    let p = pal(ui);
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(FontId::new(11.5, FontFamily::Name(SANS_STRONG.into())))
            .color(p.muted),
    );
    ui.add_space(2.0);
}

/// A switch for an on/off setting in the modern looks (a checkbox in
/// Classic). Clicking the label toggles it too.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: impl Into<egui::WidgetText>) -> egui::Response {
    if !current(ui).modern() {
        return ui.checkbox(on, label);
    }
    let p = pal(ui);
    ui.horizontal(|ui| {
        let size = egui::vec2(34.0, 19.0);
        let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
        let label = ui.add(egui::Label::new(label).sense(egui::Sense::click()));
        if response.clicked() || label.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        let t = ui.ctx().animate_bool_responsive(response.id, *on);
        let track = if *on { p.accent } else { p.border };
        ui.painter().rect_filled(rect, Rounding::same(rect.height() / 2.0), track);
        let r = rect.height() / 2.0 - 2.5;
        let x = egui::lerp((rect.left() + r + 2.5)..=(rect.right() - r - 2.5), t);
        let knob = if *on { p.on_accent } else { p.text };
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), r, knob);
        response | label
    })
    .inner
}

/// A row of pill tabs, each an icon and a name, filling the width in as
/// many rows as it takes. Returns the tab clicked, if any.
pub fn pill_tabs<T: Copy + PartialEq>(ui: &mut egui::Ui, current_tab: T, tabs: &[(T, &str, &str)]) -> Option<T> {
    let p = pal(ui);
    let modern = current(ui).modern();
    let mut picked = None;
    let per_row = 4;
    let gap = 4.0;
    let width = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
    for row in tabs.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(tab, icon, name) in row {
                let selected = tab == current_tab;
                if !modern {
                    if ui.selectable_label(selected, name).clicked() {
                        picked = Some(tab);
                    }
                    continue;
                }
                let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 46.0), egui::Sense::click());
                let fill = if selected {
                    p.accent
                } else if response.hovered() {
                    p.hover
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, Rounding::same(10.0), fill);
                let ink = if selected { p.on_accent } else if response.hovered() { p.text } else { p.muted };
                let sans = |size: f32| FontId::new(size, FontFamily::Name(SANS.into()));
                ui.painter().text(rect.center() - egui::vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, icon, sans(16.0), ink);
                ui.painter().text(rect.center() + egui::vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, name, sans(11.5), ink);
                if response.clicked() {
                    picked = Some(tab);
                }
            }
        });
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f32 {
        let lin = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Text reads on every look: body text comfortably, muted text and text
    /// on the accent at least passably.
    #[test]
    fn every_look_is_readable() {
        for look in DevLook::ALL {
            let p = palette(look);
            let bg = Color32::from_rgb(p.bg.r(), p.bg.g(), p.bg.b());
            assert!(contrast(p.text, bg) >= 7.0, "{}: text {}", look.name(), contrast(p.text, bg));
            assert!(contrast(p.muted, bg) >= 3.0, "{}: muted {}", look.name(), contrast(p.muted, bg));
            assert!(contrast(p.on_accent, p.accent) >= 3.0, "{}: on accent {}", look.name(), contrast(p.on_accent, p.accent));
        }
    }
}
