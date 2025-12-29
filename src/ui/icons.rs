//! UI icon texture management.
//!
//! Contains pre-computed UV coordinates for UI icons from the tileset.

use crate::components::ItemType;
use crate::multi_tileset::MultiTileset;
use crate::tile::tile_ids;
use crate::tile::SpriteSheet;

/// Helper struct containing pre-computed UV coordinates for UI icons
pub struct UiIcons {
    /// Texture ID for Tiles sheet (terrain, UI elements)
    pub tiles_texture_id: egui::TextureId,
    /// Texture ID for Rogues sheet (player, NPCs)
    pub rogues_texture_id: egui::TextureId,
    /// Texture ID for Monsters sheet (enemies)
    pub monsters_texture_id: egui::TextureId,
    /// Texture ID for Items sheet (weapons, potions, scrolls)
    pub items_texture_id: egui::TextureId,
    /// Texture ID for AnimatedTiles sheet (fire pits, torches, etc.)
    pub animated_tiles_texture_id: egui::TextureId,
    // Items sheet UVs
    pub sword_uv: egui::Rect,
    pub bow_uv: egui::Rect,
    pub dagger_uv: egui::Rect,
    pub staff_uv: egui::Rect,
    pub red_potion_uv: egui::Rect,
    pub green_potion_uv: egui::Rect,
    pub amber_potion_uv: egui::Rect,
    pub blue_potion_uv: egui::Rect,
    pub scroll_uv: egui::Rect,
    pub coins_uv: egui::Rect,
    pub heart_uv: egui::Rect,
    pub diamond_uv: egui::Rect,
    pub cheese_uv: egui::Rect,
    pub bread_uv: egui::Rect,
    pub apple_uv: egui::Rect,
    // Ability icons
    pub cleave_uv: egui::Rect,
    pub sprint_uv: egui::Rect,
    pub barkskin_uv: egui::Rect,
}

impl UiIcons {
    pub fn new(
        tileset: &MultiTileset,
        tiles_egui_id: egui::TextureId,
        rogues_egui_id: egui::TextureId,
        monsters_egui_id: egui::TextureId,
        items_egui_id: egui::TextureId,
        animated_tiles_egui_id: egui::TextureId,
    ) -> Self {
        Self {
            tiles_texture_id: tiles_egui_id,
            rogues_texture_id: rogues_egui_id,
            monsters_texture_id: monsters_egui_id,
            items_texture_id: items_egui_id,
            animated_tiles_texture_id: animated_tiles_egui_id,
            sword_uv: tileset.get_egui_uv(tile_ids::SWORD.0, tile_ids::SWORD.1),
            bow_uv: tileset.get_egui_uv(tile_ids::BOW.0, tile_ids::BOW.1),
            dagger_uv: tileset.get_egui_uv(tile_ids::DAGGER.0, tile_ids::DAGGER.1),
            staff_uv: tileset.get_egui_uv(tile_ids::STAFF.0, tile_ids::STAFF.1),
            red_potion_uv: tileset.get_egui_uv(tile_ids::RED_POTION.0, tile_ids::RED_POTION.1),
            green_potion_uv: tileset.get_egui_uv(tile_ids::GREEN_POTION.0, tile_ids::GREEN_POTION.1),
            amber_potion_uv: tileset.get_egui_uv(tile_ids::AMBER_POTION.0, tile_ids::AMBER_POTION.1),
            blue_potion_uv: tileset.get_egui_uv(tile_ids::BLUE_POTION.0, tile_ids::BLUE_POTION.1),
            scroll_uv: tileset.get_egui_uv(tile_ids::SCROLL.0, tile_ids::SCROLL.1),
            coins_uv: tileset.get_egui_uv(tile_ids::COINS.0, tile_ids::COINS.1),
            heart_uv: tileset.get_egui_uv(tile_ids::HEART.0, tile_ids::HEART.1),
            diamond_uv: tileset.get_egui_uv(tile_ids::DIAMOND.0, tile_ids::DIAMOND.1),
            cheese_uv: tileset.get_egui_uv(tile_ids::CHEESE.0, tile_ids::CHEESE.1),
            bread_uv: tileset.get_egui_uv(tile_ids::BREAD.0, tile_ids::BREAD.1),
            apple_uv: tileset.get_egui_uv(tile_ids::APPLE.0, tile_ids::APPLE.1),
            // Ability icons: AXE for Cleave, BLUE_POTION for Sprint, AMBER_POTION for Barkskin (brown)
            cleave_uv: tileset.get_egui_uv(tile_ids::AXE.0, tile_ids::AXE.1),
            sprint_uv: tileset.get_egui_uv(tile_ids::BLUE_POTION.0, tile_ids::BLUE_POTION.1),
            barkskin_uv: tileset.get_egui_uv(tile_ids::AMBER_POTION.0, tile_ids::AMBER_POTION.1),
        }
    }

    /// Get the texture ID for a specific sprite sheet
    pub fn texture_for_sheet(&self, sheet: SpriteSheet) -> egui::TextureId {
        match sheet {
            SpriteSheet::Tiles => self.tiles_texture_id,
            SpriteSheet::Rogues => self.rogues_texture_id,
            SpriteSheet::Monsters => self.monsters_texture_id,
            SpriteSheet::Items => self.items_texture_id,
            SpriteSheet::AnimatedTiles => self.animated_tiles_texture_id,
        }
    }

    /// Get the UV for a specific item type
    pub fn get_item_uv(&self, item_type: ItemType) -> egui::Rect {
        match item_type {
            ItemType::Sword => self.sword_uv,
            ItemType::Bow => self.bow_uv,
            ItemType::Dagger => self.dagger_uv,
            ItemType::Staff => self.staff_uv,
            ItemType::HealthPotion => self.red_potion_uv,
            ItemType::RegenerationPotion => self.green_potion_uv,
            ItemType::StrengthPotion => self.amber_potion_uv,
            ItemType::ConfusionPotion => self.blue_potion_uv,
            ItemType::ScrollOfInvisibility
            | ItemType::ScrollOfSpeed
            | ItemType::ScrollOfProtection
            | ItemType::ScrollOfBlink
            | ItemType::ScrollOfFear
            | ItemType::ScrollOfFireball
            | ItemType::ScrollOfReveal
            | ItemType::ScrollOfMapping
            | ItemType::ScrollOfSlow => self.scroll_uv,
            ItemType::Cheese => self.cheese_uv,
            ItemType::Bread => self.bread_uv,
            ItemType::Apple => self.apple_uv,
        }
    }

    /// Get the texture ID for items (weapons, potions, scrolls)
    pub fn items_texture(&self) -> egui::TextureId {
        self.items_texture_id
    }
}
