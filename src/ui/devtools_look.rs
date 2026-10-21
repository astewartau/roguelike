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
    // Labels are words to read, not text to select (a drag across one
    // painted it in the selection colour).
    style.interaction.selectable_labels = false;

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

/// A square icon button for the tool rail: the icon alone, its name on
/// hover. Filled with the accent while `selected` (its panel is open).
pub fn rail_button(ui: &mut egui::Ui, icon: &str, name: &str, selected: bool) -> egui::Response {
    let p = pal(ui);
    let size = crate::constants::DEVTOOLS_RAIL_BUTTON;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let fill = if selected {
        p.accent
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(10.0), fill);
    let ink = if selected { p.on_accent } else if response.hovered() { p.text } else { p.muted };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, FontId::new(20.0, FontFamily::Name(SANS.into())), ink);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_ui_at_pointer(|ui| {
        apply(ui);
        ui.label(name);
    })
}

/// The frame of a bar fixed to a screen edge (the toolbar, the rail): the
/// panel's colours with a hairline border, square to the screen.
pub fn bar_frame(margin: egui::Margin) -> egui::Frame {
    let p = PALETTE;
    egui::Frame::none().fill(p.bg).stroke(Stroke::new(1.0_f32, p.border)).inner_margin(margin)
}

/// Text in the heavier face and full text colour, for titles. (egui's own
/// "strong" text takes the colour of a pressed button, which here is the
/// dark ink used on amber.)
pub fn strong(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .font(FontId::new(14.0, FontFamily::Name(SANS_STRONG.into())))
        .color(PALETTE.text)
}

/// A small round button with a drawn cross: close, remove, clear. (The
/// "✕" glyph isn't in the panel's font.)
pub fn cross_button(ui: &mut egui::Ui) -> egui::Response {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().circle_filled(rect.center(), 10.0, p.hover);
    }
    let ink = if hovered { p.danger } else { p.muted };
    let r = 4.0;
    let c = rect.center();
    let stroke = Stroke::new(1.6_f32, ink);
    ui.painter().line_segment([c + egui::vec2(-r, -r), c + egui::vec2(r, r)], stroke);
    ui.painter().line_segment([c + egui::vec2(r, -r), c + egui::vec2(-r, r)], stroke);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small "?" that explains something on hover.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
    let ink = if response.hovered() { p.accent } else { p.muted };
    ui.painter().circle_stroke(rect.center(), 6.5, Stroke::new(1.0_f32, ink));
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "?", FontId::new(10.0, FontFamily::Name(SANS_STRONG.into())), ink);
    response.on_hover_ui(|ui| {
        apply(ui);
        ui.set_max_width(260.0);
        ui.label(text);
    });
}

/// A group that folds away: a header row with a chevron, the group's name
/// and how many things are in it; click it to open or close. Remembers its
/// state by `id`. `force_open` shows the body regardless (while searching,
/// so results are never hidden behind a closed group).
pub fn accordion<R>(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    title: &str,
    count: usize,
    force_open: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let p = pal(ui);
    let id = ui.make_persistent_id(id);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
    let open = force_open || state.is_open();

    ui.add_space(4.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
    if response.clicked() && !force_open {
        state.toggle(ui);
    }
    let hovered = response.hovered();
    ui.painter().rect_filled(rect, Rounding::same(8.0), if hovered { p.hover } else { p.card });

    // Chevron: points right when closed, down when open, turning between.
    let t = ui.ctx().animate_bool_responsive(id.with("chevron"), open);
    let c = egui::pos2(rect.left() + 16.0, rect.center().y);
    let angle = t * std::f32::consts::FRAC_PI_2;
    let rot = |v: egui::Vec2| egui::vec2(v.x * angle.cos() - v.y * angle.sin(), v.x * angle.sin() + v.y * angle.cos());
    let ink = if hovered || open { p.accent } else { p.muted };
    let points = [c + rot(egui::vec2(-2.5, -4.5)), c + rot(egui::vec2(3.0, 0.0)), c + rot(egui::vec2(-2.5, 4.5))];
    ui.painter().add(egui::Shape::line(points.to_vec(), Stroke::new(1.8_f32, ink)));

    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        FontId::new(12.0, FontFamily::Name(SANS_STRONG.into())),
        if hovered || open { p.text } else { p.muted },
    );

    // How many are inside, as a small pill on the right.
    let badge_font = FontId::new(11.0, FontFamily::Name(SANS_STRONG.into()));
    let galley = ui.painter().layout_no_wrap(count.to_string(), badge_font, p.muted);
    let badge = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 12.0 - galley.size().x / 2.0, rect.center().y),
        galley.size() + egui::vec2(12.0, 4.0),
    );
    ui.painter().rect_filled(badge, Rounding::same(badge.height() / 2.0), p.bg);
    ui.painter().galley(badge.center() - galley.size() / 2.0, galley, p.muted);
    let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);

    if force_open {
        ui.add_space(4.0);
        return Some(add(ui));
    }
    state.show_body_unindented(ui, |ui| {
        ui.add_space(4.0);
        add(ui)
    })
    .map(|body| body.inner)
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
                if clear && cross_button(ui).clicked() {
                    text.clear();
                }
                edit
            })
            .inner
        })
        .inner
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
