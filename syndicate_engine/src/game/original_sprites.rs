use std::path::Path;

use crate::engine::{
    mission_scene::{OriginalMissionObjectCandidate, OriginalMissionObjectKind, OriginalTilePoint},
    original_sprites::{OriginalObjectSpriteRenderAssets, OriginalSpriteAtlasRect},
};
use macroquad::prelude::*;

pub struct RuntimeOriginalObjectGraphics {
    assets: OriginalObjectSpriteRenderAssets,
    texture: Texture2D,
}

impl RuntimeOriginalObjectGraphics {
    pub fn from_root_with_palette_id(
        root: impl AsRef<Path>,
        palette_id: Option<u8>,
    ) -> Option<Self> {
        let assets =
            OriginalObjectSpriteRenderAssets::from_root_with_palette_id(root, palette_id).ok()?;
        let (width, height) = assets.sprite_atlas.texture_size_u16()?;
        let texture = Texture2D::from_rgba8(width, height, assets.sprite_atlas.rgba());
        texture.set_filter(FilterMode::Nearest);
        Some(Self { assets, texture })
    }

    pub fn assets(&self) -> &OriginalObjectSpriteRenderAssets {
        &self.assets
    }

    pub fn draw_static_object(
        &self,
        object: &OriginalMissionObjectCandidate,
        tile_screen_top_left: Vec2,
        tile_size: Vec2,
        zoom: f32,
    ) -> bool {
        self.draw_mission_object(object, tile_screen_top_left, tile_size, zoom, 0)
    }

    pub fn draw_mission_object(
        &self,
        object: &OriginalMissionObjectCandidate,
        tile_screen_top_left: Vec2,
        tile_size: Vec2,
        zoom: f32,
        animation_frame: u16,
    ) -> bool {
        if !matches!(
            object.kind,
            OriginalMissionObjectKind::Static
                | OriginalMissionObjectKind::Ped
                | OriginalMissionObjectKind::Vehicle
                | OriginalMissionObjectKind::Weapon
        ) || !object.candidate_draw
        {
            return false;
        }
        let Some(tile) = object.tile else {
            return false;
        };
        let Ok(assembly) = self
            .assets
            .assemble_object_frame(object.object_frame_refs(animation_frame))
        else {
            return false;
        };
        let base = object_screen_base(object.kind, tile, tile_screen_top_left, tile_size);
        let mut drew_any = false;
        for element in assembly.elements {
            let Some(rect) = self
                .assets
                .sprite_atlas
                .source_rect(element.sprite_id as usize)
            else {
                continue;
            };
            let sprite_top_left =
                base + vec2(element.offset_x as f32, element.offset_y as f32) * zoom;
            if !sprite_intersects_screen(sprite_top_left, rect, zoom) {
                continue;
            }
            draw_texture_ex(
                &self.texture,
                sprite_top_left.x,
                sprite_top_left.y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(rect.width as f32 * zoom, rect.height as f32 * zoom)),
                    source: Some(Rect::new(
                        rect.x as f32,
                        rect.y as f32,
                        rect.width as f32,
                        rect.height as f32,
                    )),
                    flip_x: element.flipped,
                    ..Default::default()
                },
            );
            drew_any = true;
        }

        drew_any
    }

    pub fn draw_sprite_id(&self, sprite_id: usize, top_left: Vec2, scale: f32) -> bool {
        let Some(rect) = self.assets.sprite_atlas.source_rect(sprite_id) else {
            return false;
        };
        let scale = scale.max(0.1);
        draw_texture_ex(
            &self.texture,
            top_left.x,
            top_left.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(rect.width as f32 * scale, rect.height as f32 * scale)),
                source: Some(Rect::new(
                    rect.x as f32,
                    rect.y as f32,
                    rect.width as f32,
                    rect.height as f32,
                )),
                ..Default::default()
            },
        );
        true
    }
}

fn object_screen_base(
    kind: OriginalMissionObjectKind,
    tile: OriginalTilePoint,
    tile_screen_top_left: Vec2,
    tile_size: Vec2,
) -> Vec2 {
    let mut tile_mid_bottom =
        tile_screen_top_left + vec2(tile_size.x * 0.5, tile_size.y * (2.0 / 3.0));
    if kind == OriginalMissionObjectKind::Vehicle {
        tile_mid_bottom.y += tile_size.y / 3.0;
    }
    let offset_x = ((tile.off_x as f32 - tile.off_y as f32) * (tile_size.x * 0.5)) / 256.0;
    let offset_y = ((tile.off_x as f32 + tile.off_y as f32) * (tile_size.y / 3.0)) / 256.0
        - (tile.off_z as f32 * (tile_size.y / 3.0)) / 128.0;
    tile_mid_bottom + vec2(offset_x, offset_y)
}

#[cfg(test)]
fn static_screen_base(
    tile: OriginalTilePoint,
    tile_screen_top_left: Vec2,
    tile_size: Vec2,
) -> Vec2 {
    object_screen_base(
        OriginalMissionObjectKind::Static,
        tile,
        tile_screen_top_left,
        tile_size,
    )
}

fn sprite_intersects_screen(top_left: Vec2, rect: OriginalSpriteAtlasRect, zoom: f32) -> bool {
    let size = vec2(rect.width as f32 * zoom, rect.height as f32 * zoom);
    top_left.x <= screen_width() + size.x
        && top_left.y <= screen_height() + size.y
        && top_left.x + size.x >= -size.x
        && top_left.y + size.y >= -size.y
}

#[cfg(test)]
mod tests {
    use super::{object_screen_base, static_screen_base};
    use crate::engine::mission_scene::{OriginalMissionObjectKind, OriginalTilePoint};
    use macroquad::prelude::*;

    #[test]
    fn static_screen_base_applies_freesynd_subtile_offsets() {
        let base = static_screen_base(
            OriginalTilePoint {
                tile_x: 0,
                tile_y: 0,
                tile_z: 0,
                off_x: 128,
                off_y: 0,
                off_z: 64,
            },
            vec2(100.0, 50.0),
            vec2(64.0, 48.0),
        );

        assert_eq!(base.x, 148.0);
        assert_eq!(base.y, 82.0);
    }

    #[test]
    fn vehicle_screen_base_applies_freesynd_vertical_adjustment() {
        let base = object_screen_base(
            OriginalMissionObjectKind::Vehicle,
            OriginalTilePoint {
                tile_x: 0,
                tile_y: 0,
                tile_z: 0,
                off_x: 0,
                off_y: 0,
                off_z: 0,
            },
            vec2(100.0, 50.0),
            vec2(64.0, 48.0),
        );

        assert_eq!(base.x, 132.0);
        assert_eq!(base.y, 98.0);
    }
}
