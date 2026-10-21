//! Picking a picture from a sprite sheet, for any setting that names one:
//! a `Sprite`, a fire's lit look, a door's open and closed sprites, a hidden
//! trap's revealed look, an overlay. The settings editor recognises the
//! shape (a sheet plus a tile number) and shows the picture with a Pick…
//! button that opens the whole sheet to click on.

use serde_json::Value;

use crate::tile::SpriteSheet;

use super::devtools::DevArt;
use super::devtools_look as look;

/// Side of one tile in the picker window, in points.
const PICKER_TILE: f32 = 28.0;

/// The sheets, in the order the picker offers them.
const SHEETS: [(SpriteSheet, &str); 5] = [
    (SpriteSheet::Monsters, "Monsters"),
    (SpriteSheet::Rogues, "People"),
    (SpriteSheet::Items, "Items"),
    (SpriteSheet::Tiles, "Tiles"),
    (SpriteSheet::AnimatedTiles, "Animated"),
];

/// The keys a picture's tile number goes by.
const TILE_KEYS: [&str; 2] = ["tile_id", "base_tile_id"];

/// The picture `value` names, if it is one: `{sheet, tile_id}` (a `Sprite`),
/// `{sheet, base_tile_id, ...}` (an `AnimatedSprite`), or `[sheet, tile]` (a
/// door's sprites).
pub fn as_tile(value: &Value) -> Option<(SpriteSheet, u32)> {
    let sheet = |v: &Value| serde_json::from_value::<SpriteSheet>(v.clone()).ok();
    match value {
        Value::Object(fields) => {
            let id = TILE_KEYS.iter().find_map(|k| fields.get(*k)).and_then(Value::as_u64)?;
            Some((sheet(fields.get("sheet")?)?, id as u32))
        }
        Value::Array(items) if items.len() == 2 => Some((sheet(&items[0])?, items[1].as_u64()? as u32)),
        _ => None,
    }
}

/// Whether `key` is one of the parts of a picture the picker sets, so the
/// settings grid can leave it out.
pub fn is_tile_key(key: &str) -> bool {
    key == "sheet" || TILE_KEYS.contains(&key)
}

/// Write `sheet` and `id` back into `value`, in whatever shape it has.
fn set_tile(value: &mut Value, sheet: SpriteSheet, id: u32) {
    let Ok(sheet) = serde_json::to_value(sheet) else { return };
    match value {
        Value::Object(fields) => {
            let key = TILE_KEYS.iter().find(|k| fields.contains_key(**k)).copied().unwrap_or("tile_id");
            fields.insert("sheet".into(), sheet);
            fields.insert(key.into(), Value::from(id));
        }
        Value::Array(items) => *items = vec![sheet, Value::from(id)],
        _ => {}
    }
}

fn sheet_name(sheet: SpriteSheet) -> &'static str {
    SHEETS.iter().find(|(s, _)| *s == sheet).map(|(_, n)| *n).unwrap_or("?")
}

/// The picture, where it's from, and a Pick… button; picking opens a window
/// of the whole sheet. Returns whether a new picture was chosen.
pub fn tile_field(ui: &mut egui::Ui, art: &DevArt, value: &mut Value) -> bool {
    let Some((sheet, id)) = as_tile(value) else { return false };
    let p = look::pal(ui);
    let open_id = ui.id().with("tile_picker");
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);

    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::click());
        ui.painter().rect_filled(rect, 6.0, egui::Color32::BLACK);
        let (tex, uv) = art.icon((sheet, id));
        ui.painter().image(tex, rect.shrink(2.0), uv, egui::Color32::WHITE);
        if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            open = !open;
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(format!("{} #{id}", sheet_name(sheet))).color(p.muted));
            if ui.selectable_label(open, "Pick…").clicked() {
                open = !open;
            }
        });
    });

    let mut chosen = None;
    if open {
        let sheet_id = open_id.with("sheet");
        let mut showing = ui.ctx().data(|d| d.get_temp::<SpriteSheet>(sheet_id)).unwrap_or(sheet);
        egui::Window::new("Pick a picture")
            .id(open_id.with("window"))
            .open(&mut open)
            .default_size(egui::vec2(420.0, 480.0))
            .collapsible(false)
            .frame(look::panel_frame())
            .show(ui.ctx(), |ui| {
                look::apply(ui);
                chosen = sheet_grid(ui, art, &mut showing, (sheet, id));
            });
        ui.ctx().data_mut(|d| d.insert_temp(sheet_id, showing));
    }
    if let Some((sheet, id)) = chosen {
        set_tile(value, sheet, id);
        open = false;
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
    chosen.is_some()
}

/// Tabs for the sheets and the chosen sheet as a grid of tiles; the current
/// picture is outlined. Returns the tile clicked, if any.
fn sheet_grid(
    ui: &mut egui::Ui,
    art: &DevArt,
    showing: &mut SpriteSheet,
    current: (SpriteSheet, u32),
) -> Option<(SpriteSheet, u32)> {
    let p = look::pal(ui);
    ui.horizontal_wrapped(|ui| {
        for (sheet, label) in SHEETS {
            ui.selectable_value(showing, sheet, label);
        }
    });
    let sheet = *showing;
    let (columns, rows) = art.tileset.grid_size(sheet);
    if columns == 0 {
        ui.colored_label(p.muted, "That sheet isn't loaded.");
        return None;
    }
    let tex = art.icons.texture_for_sheet(sheet);
    let mut chosen = None;
    egui::ScrollArea::both().show_rows(ui, PICKER_TILE, rows as usize, |ui, visible| {
        for r in visible {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for c in 0..columns {
                    let id = r as u32 * columns + c;
                    let uv = art.tileset.get_egui_uv(sheet, id);
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(PICKER_TILE, PICKER_TILE), egui::Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 4.0, p.hover);
                    }
                    ui.painter().image(tex, rect.shrink(2.0), uv, egui::Color32::WHITE);
                    if (sheet, id) == current {
                        ui.painter().rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5_f32, p.accent));
                    }
                    if response.on_hover_text(format!("#{id}")).clicked() {
                        chosen = Some((sheet, id));
                    }
                }
            });
        }
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pictures_are_recognised_and_written_back_in_their_own_shape() {
        let mut sprite = json!({"sheet": "Monsters", "tile_id": 4});
        let mut animated = json!({"sheet": "AnimatedTiles", "base_tile_id": 9, "frame_count": 6});
        let mut door = json!(["Tiles", 12]);
        assert_eq!(as_tile(&sprite), Some((SpriteSheet::Monsters, 4)));
        assert_eq!(as_tile(&animated), Some((SpriteSheet::AnimatedTiles, 9)));
        assert_eq!(as_tile(&door), Some((SpriteSheet::Tiles, 12)));
        assert_eq!(as_tile(&json!({"x": 1, "y": 2})), None);

        set_tile(&mut sprite, SpriteSheet::Items, 7);
        set_tile(&mut animated, SpriteSheet::AnimatedTiles, 30);
        set_tile(&mut door, SpriteSheet::Tiles, 40);
        assert_eq!(sprite, json!({"sheet": "Items", "tile_id": 7}));
        assert_eq!(animated["base_tile_id"], 30);
        assert_eq!(animated["frame_count"], 6, "the rest is left alone");
        assert_eq!(door, json!(["Tiles", 40]));
    }
}
