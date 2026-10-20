//! How the developer tools panel looks: a modern dark style with the game's
//! amber fire as its accent ("Ember").
//!
//! Everything here is scoped to the panel: `apply` sets the style of the
//! panel's own `Ui`, so the game's HUD and windows keep the dungeon look.
//! The panel's widgets take their colours from `PALETTE` (through `pal`)
//! rather than the dungeon palette.

use egui::{Color32, FontFamily, FontId, Rounding, Stroke};

/// The colours the panel is drawn in.
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

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const PALETTE: Palette = Palette {
    bg: Color32::from_rgba_premultiplied(21, 16, 13, 244),
    card: hex(0x241B16),
    border: hex(0x3A2D24),
    text: hex(0xF3E8DC),
    muted: hex(0xA8957F),
    accent: hex(0xF2A33A),
    on_accent: hex(0x1E1309),
    hover: hex(0x2F241D),
    warning: hex(0xF2C94C),
    danger: hex(0xF07A5A),
};

/// The font families `style::load_fonts` registers for the modern looks:
/// a clean sans for text, a heavier one for headings. Both fall back to
/// the game's own fonts (and its emoji) when the files aren't there.
pub const SANS: &str = "dev-sans";
pub const SANS_STRONG: &str = "dev-sans-strong";

/// The panel's colours. (Takes the `Ui` so callers read naturally, and so a
/// look could vary by context later without touching them.)
pub fn pal(_ui: &egui::Ui) -> Palette {
    PALETTE
}

/// Style the panel's `Ui`.
pub fn apply(ui: &mut egui::Ui) {
    let p = PALETTE;
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
    v.dark_mode = true;
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
/// shadow.
pub fn panel_frame() -> egui::Frame {
    let p = PALETTE;
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
            color: Color32::from_black_alpha(110),
        })
}

/// A card around a group of controls.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
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

/// A section label: small, quiet capitals with room above.
pub fn section(ui: &mut egui::Ui, text: &str) {
    let p = pal(ui);
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(FontId::new(11.5, FontFamily::Name(SANS_STRONG.into())))
            .color(p.muted),
    );
    ui.add_space(2.0);
}

/// A switch for an on/off setting. Clicking the label toggles it too.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: impl Into<egui::WidgetText>) -> egui::Response {
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
    let mut picked = None;
    let per_row = 4;
    let gap = 4.0;
    let width = (ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32;
    for row in tabs.chunks(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for &(tab, icon, name) in row {
                let selected = tab == current_tab;
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

/// A filled accent button for the main action in a place.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal(ui);
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(p.on_accent).font(FontId::new(14.0, FontFamily::Name(SANS_STRONG.into()))))
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(8.0)),
    )
}

/// A small rounded chip (a building block to add): outlined, filling on
/// hover.
pub fn chip(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal(ui);
    let font = FontId::new(12.5, FontFamily::Name(SANS.into()));
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, p.text);
    let size = galley.size() + egui::vec2(20.0, 10.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hovered = response.hovered();
    let fill = if hovered { p.accent } else { p.card };
    ui.painter().rect(rect, Rounding::same(rect.height() / 2.0), fill, Stroke::new(1.0_f32, if hovered { p.accent } else { p.border }));
    let ink = if hovered { p.on_accent } else { p.text };
    ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, ink);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A full-width search box with a magnifier and a clear button.
pub fn search_box(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    let p = pal(ui);
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").color(p.muted));
                let clear = !text.is_empty();
                let width = ui.available_width() - if clear { 22.0 } else { 0.0 };
                let edit = ui.add(egui::TextEdit::singleline(text).hint_text(hint).frame(false).desired_width(width));
                if clear && ui.add(egui::Button::new(egui::RichText::new("✕").color(p.muted)).frame(false)).clicked() {
                    text.clear();
                }
                edit
            })
            .inner
        })
        .inner
}

/// One entry in a list to add from: its name, what it does underneath,
/// and a "+" on the right; the whole row highlights on hover and is the
/// thing to click.
pub fn add_row(ui: &mut egui::Ui, name: &str, summary: &str) -> egui::Response {
    let p = pal(ui);
    let width = ui.available_width();
    let pad = egui::vec2(10.0, 7.0);
    let plus_w = 26.0;
    let name_galley = ui.painter().layout_no_wrap(
        name.to_owned(),
        FontId::new(14.0, FontFamily::Name(SANS_STRONG.into())),
        p.text,
    );
    let summary_galley = ui.painter().layout(
        summary.to_owned(),
        FontId::new(12.0, FontFamily::Name(SANS.into())),
        p.muted,
        (width - pad.x * 2.0 - plus_w).max(40.0),
    );
    let height = pad.y * 2.0 + name_galley.size().y + 2.0 + summary_galley.size().y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, Rounding::same(8.0), p.hover);
    }
    let text_at = rect.min + pad;
    let name_h = name_galley.size().y;
    ui.painter().galley(text_at, name_galley, p.text);
    ui.painter().galley(text_at + egui::vec2(0.0, name_h + 2.0), summary_galley, p.muted);
    let plus = egui::pos2(rect.right() - pad.x - plus_w / 2.0, rect.center().y);
    if hovered {
        ui.painter().circle_filled(plus, 11.0, p.accent);
    }
    ui.painter().text(
        plus,
        egui::Align2::CENTER_CENTER,
        "+",
        FontId::new(17.0, FontFamily::Name(SANS_STRONG.into())),
        if hovered { p.on_accent } else { p.accent },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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

    /// Body text reads comfortably; muted text and text on the accent at
    /// least passably.
    #[test]
    fn the_palette_is_readable() {
        let p = PALETTE;
        let bg = Color32::from_rgb(p.bg.r(), p.bg.g(), p.bg.b());
        assert!(contrast(p.text, bg) >= 7.0, "text {}", contrast(p.text, bg));
        assert!(contrast(p.muted, bg) >= 3.0, "muted {}", contrast(p.muted, bg));
        assert!(contrast(p.on_accent, p.accent) >= 3.0, "on accent {}", contrast(p.on_accent, p.accent));
    }
}
