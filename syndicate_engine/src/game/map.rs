use crate::engine::{
    camera::CameraRig,
    iso::{draw_iso_tile, grid_to_iso},
    palette,
};
use macroquad::prelude::*;

#[derive(Debug, Clone, Copy)]
pub enum TileKind {
    Road,
    Pavement,
    Roof,
    Water,
}

#[derive(Debug, Clone)]
pub struct TacticalMap {
    pub width: usize,
    pub height: usize,
    tiles: Vec<TileKind>,
}

impl TacticalMap {
    pub fn demo_city() -> Self {
        let width = 28;
        let height = 28;
        let mut tiles = vec![TileKind::Pavement; width * height];
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                tiles[idx] = if x == 13 || y == 14 || (x > 2 && x < 25 && y == 5) {
                    TileKind::Road
                } else if (x > 17 && y > 18) || (x < 4 && y > 20) {
                    TileKind::Water
                } else if (x + y) % 9 == 0
                    || (x > 6 && x < 11 && y > 7 && y < 12)
                    || (x > 16 && x < 22 && y > 8 && y < 13)
                {
                    TileKind::Roof
                } else {
                    TileKind::Pavement
                };
            }
        }
        Self {
            width,
            height,
            tiles,
        }
    }

    pub fn tile(&self, x: usize, y: usize) -> TileKind {
        self.tiles[y * self.width + x]
    }

    pub fn draw(&self, camera: &CameraRig) {
        for y in 0..self.height {
            for x in 0..self.width {
                let kind = self.tile(x, y);
                let z = if matches!(kind, TileKind::Roof) {
                    0.65
                } else {
                    0.0
                };
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                let color = match kind {
                    TileKind::Road => palette::ROAD,
                    TileKind::Pavement => palette::PAVEMENT,
                    TileKind::Roof => palette::ROOF,
                    TileKind::Water => palette::WATER,
                };
                draw_iso_tile(center, color, Color::new(0.02, 0.025, 0.03, 0.75));
            }
        }
    }
}
