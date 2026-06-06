use std::path::Path;

use crate::engine::block_texture::IndexedBlockGraphics;
use macroquad::prelude::*;

pub struct RuntimeOriginalGraphics {
    bank: IndexedBlockGraphics,
    texture: Texture2D,
}

impl RuntimeOriginalGraphics {
    pub fn from_root(root: impl AsRef<Path>) -> Option<Self> {
        Self::from_root_with_palette_id(root, None)
    }

    pub fn from_root_with_palette_id(
        root: impl AsRef<Path>,
        palette_id: Option<u8>,
    ) -> Option<Self> {
        let bank = IndexedBlockGraphics::from_root_with_palette_id(root, palette_id).ok()?;
        let (width, height) = bank.texture_size_u16();
        let texture = Texture2D::from_rgba8(width, height, bank.rgba());
        texture.set_filter(FilterMode::Nearest);
        Some(Self { bank, texture })
    }

    pub fn bank(&self) -> &IndexedBlockGraphics {
        &self.bank
    }

    pub fn draw_record(
        &self,
        record_index: usize,
        top_left: Vec2,
        size: Vec2,
        tint: Color,
    ) -> bool {
        let Some((x, y, w, h)) = self.bank.source_rect(record_index) else {
            return false;
        };
        draw_texture_ex(
            &self.texture,
            top_left.x,
            top_left.y,
            tint,
            DrawTextureParams {
                dest_size: Some(size),
                source: Some(Rect::new(x, y, w, h)),
                ..Default::default()
            },
        );
        true
    }

    pub fn draw_atlas_preview(&self, origin: Vec2, columns: usize, rows: usize, tile_size: Vec2) {
        let visible = columns.saturating_mul(rows).min(self.bank.record_count);
        for record_index in 0..visible {
            let x = origin.x + (record_index % columns) as f32 * tile_size.x;
            let y = origin.y + (record_index / columns) as f32 * tile_size.y;
            self.draw_record(record_index, vec2(x, y), tile_size, WHITE);
        }

        draw_rectangle_lines(
            origin.x,
            origin.y,
            columns as f32 * tile_size.x,
            rows as f32 * tile_size.y,
            1.0,
            SKYBLUE,
        );
    }
}
