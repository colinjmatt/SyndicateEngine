use crate::{
    engine::{
        camera::CameraRig,
        map_tiles::{OriginalMapTiles, OriginalTileTypes},
        mission_source::OriginalMissionSelection,
    },
    game::{map::original_map_tile_world_top_left, original_graphics::RuntimeOriginalGraphics},
};
use macroquad::prelude::*;

const ORIGINAL_MAP_START_Z: f32 = 1.0;
const ORIGINAL_MAP_START_SCREEN_X: f32 = 720.0;
const ORIGINAL_MAP_START_SCREEN_Y: f32 = 430.0;
const ORIGINAL_MAP_START_ZOOM: f32 = 0.82;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OriginalMapViewState {
    geometry: OriginalMapViewGeometry,
    scroll_bounds: Option<OriginalMapScrollBounds>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OriginalMapViewGeometry {
    map_width: f32,
    stack_height: f32,
    tile_width: f32,
    tile_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalMapScrollBounds {
    min_tile: (u16, u16),
    max_tile: (u16, u16),
}

impl OriginalMapViewState {
    pub fn from_runtime_assets(
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
        mission_selection: Option<&OriginalMissionSelection>,
    ) -> Self {
        Self {
            geometry: OriginalMapViewGeometry::from_runtime_assets(map_tiles, graphics),
            scroll_bounds: mission_selection.and_then(OriginalMapScrollBounds::from_selection),
        }
    }

    pub fn starting_camera(
        &self,
        map_tiles: &OriginalMapTiles,
        tile_types: Option<&OriginalTileTypes>,
    ) -> CameraRig {
        let mut camera = CameraRig::default();
        camera.zoom = ORIGINAL_MAP_START_ZOOM;

        let focus_tile = map_tiles
            .primary_runtime_region(tile_types)
            .map(|region| {
                let (center_x, center_y) = region.center();
                vec2(center_x, center_y)
            })
            .or_else(|| self.scroll_bounds.map(|bounds| bounds.center_tile()))
            .unwrap_or_else(|| vec2(map_tiles.width as f32 * 0.5, map_tiles.depth as f32 * 0.5));

        let focus = original_map_tile_world_top_left(
            map_tiles,
            focus_tile.x,
            focus_tile.y,
            ORIGINAL_MAP_START_Z,
            self.geometry.tile_width,
            self.geometry.tile_height,
        );
        camera.offset =
            vec2(ORIGINAL_MAP_START_SCREEN_X, ORIGINAL_MAP_START_SCREEN_Y) - focus * camera.zoom;
        self.clamp_camera(&mut camera);
        camera
    }

    pub fn clamp_camera(&self, camera: &mut CameraRig) {
        let Some(bounds) = self.scroll_bounds else {
            return;
        };

        let viewport_origin = camera.screen_to_world(vec2(0.0, 0.0));
        let scroll_tile = self
            .geometry
            .viewport_origin_to_scroll_tile(viewport_origin);
        let clamped_tile = bounds.clamp_tile(scroll_tile);
        if (clamped_tile - scroll_tile).length_squared() <= f32::EPSILON {
            return;
        }

        let clamped_origin = self.geometry.scroll_tile_to_viewport_origin(clamped_tile);
        camera.offset = -clamped_origin * camera.zoom;
    }

    pub fn scroll_summary_label(&self) -> String {
        self.scroll_bounds
            .map(|bounds| {
                format!(
                    "viewport scroll clamp {:?}->{:?}",
                    bounds.min_tile, bounds.max_tile
                )
            })
            .unwrap_or_else(|| "viewport scroll clamp unavailable".to_string())
    }
}

impl OriginalMapViewGeometry {
    pub fn from_runtime_assets(
        map_tiles: &OriginalMapTiles,
        graphics: &RuntimeOriginalGraphics,
    ) -> Self {
        Self {
            map_width: map_tiles.width as f32,
            stack_height: map_tiles.height as f32,
            tile_width: graphics.bank().record_width as f32,
            tile_height: graphics.bank().record_height as f32,
        }
    }

    fn viewport_origin_to_scroll_tile(self, origin: Vec2) -> Vec2 {
        let dx =
            (origin.x - (self.map_width + 1.0) * self.tile_width * 0.5) / (self.tile_width * 0.5);
        let dy = (origin.y - (self.stack_height + 2.0) * self.tile_height / 3.0)
            / (self.tile_height / 3.0);
        vec2((dx + dy) * 0.5, (dy - dx) * 0.5)
    }

    fn scroll_tile_to_viewport_origin(self, tile: Vec2) -> Vec2 {
        vec2(
            (self.map_width + 1.0 + tile.x - tile.y) * self.tile_width * 0.5,
            (self.stack_height + 2.0 + tile.x + tile.y) * self.tile_height / 3.0,
        )
    }
}

impl OriginalMapScrollBounds {
    pub fn from_selection(selection: &OriginalMissionSelection) -> Option<Self> {
        if selection.max_scroll_tile.0 < selection.min_scroll_tile.0
            || selection.max_scroll_tile.1 < selection.min_scroll_tile.1
        {
            return None;
        }

        Some(Self {
            min_tile: selection.min_scroll_tile,
            max_tile: selection.max_scroll_tile,
        })
    }

    fn center_tile(self) -> Vec2 {
        vec2(
            (self.min_tile.0 as f32 + self.max_tile.0 as f32) * 0.5,
            (self.min_tile.1 as f32 + self.max_tile.1 as f32) * 0.5,
        )
    }

    fn clamp_tile(self, tile: Vec2) -> Vec2 {
        vec2(
            tile.x.clamp(self.min_tile.0 as f32, self.max_tile.0 as f32),
            tile.y.clamp(self.min_tile.1 as f32, self.max_tile.1 as f32),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{OriginalMapScrollBounds, OriginalMapViewGeometry, OriginalMapViewState};
    use crate::engine::camera::CameraRig;
    use macroquad::prelude::*;

    #[test]
    fn scroll_tile_projection_roundtrips_viewport_origin() {
        let geometry = OriginalMapViewGeometry {
            map_width: 128.0,
            stack_height: 12.0,
            tile_width: 64.0,
            tile_height: 48.0,
        };
        let tile = vec2(42.25, 33.5);
        let origin = geometry.scroll_tile_to_viewport_origin(tile);
        let projected = geometry.viewport_origin_to_scroll_tile(origin);

        assert!((projected.x - tile.x).abs() < 0.001);
        assert!((projected.y - tile.y).abs() < 0.001);
    }

    #[test]
    fn clamp_camera_projects_view_origin_back_inside_scroll_tiles() {
        let view = OriginalMapViewState {
            geometry: OriginalMapViewGeometry {
                map_width: 128.0,
                stack_height: 12.0,
                tile_width: 64.0,
                tile_height: 48.0,
            },
            scroll_bounds: Some(OriginalMapScrollBounds {
                min_tile: (10, 20),
                max_tile: (50, 70),
            }),
        };
        let outside_origin = view
            .geometry
            .scroll_tile_to_viewport_origin(vec2(-8.0, 95.0));
        let mut camera = CameraRig {
            offset: -outside_origin,
            zoom: 1.0,
        };

        view.clamp_camera(&mut camera);

        let clamped_tile = view
            .geometry
            .viewport_origin_to_scroll_tile(camera.screen_to_world(vec2(0.0, 0.0)));
        assert!((clamped_tile.x - 10.0).abs() < 0.001);
        assert!((clamped_tile.y - 70.0).abs() < 0.001);
    }
}
