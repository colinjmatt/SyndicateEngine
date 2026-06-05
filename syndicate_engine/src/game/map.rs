use crate::engine::{
    camera::CameraRig,
    iso::{draw_iso_tile, grid_to_iso},
    map_decode::{MapInferredLayerPreview, MapSignaturePreview},
    palette,
};
use crate::game::pathfinding::GridPos;
use macroquad::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn contains(&self, pos: GridPos) -> bool {
        pos.x >= 0 && pos.y >= 0 && (pos.x as usize) < self.width && (pos.y as usize) < self.height
    }

    pub fn tile_pos(&self, pos: GridPos) -> Option<TileKind> {
        self.contains(pos)
            .then(|| self.tile(pos.x as usize, pos.y as usize))
    }

    pub fn is_walkable_pos(&self, pos: GridPos) -> bool {
        self.tile_pos(pos)
            .is_some_and(|tile| !matches!(tile, TileKind::Water | TileKind::Roof))
    }

    pub fn is_road_pos(&self, pos: GridPos) -> bool {
        self.tile_pos(pos)
            .is_some_and(|tile| tile == TileKind::Road)
    }

    pub fn walkable_neighbors(&self, pos: GridPos) -> Vec<GridPos> {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|(dx, dy)| GridPos::new(pos.x + dx, pos.y + dy))
            .filter(|&next| self.is_walkable_pos(next))
            .collect()
    }

    pub fn draw_path(&self, camera: &CameraRig, path: &[GridPos], color: Color) {
        for window in path.windows(2) {
            let a =
                camera.world_to_screen(grid_to_iso(window[0].x as f32, window[0].y as f32, 0.05));
            let b =
                camera.world_to_screen(grid_to_iso(window[1].x as f32, window[1].y as f32, 0.05));
            draw_line(a.x, a.y, b.x, b.y, 3.0, color);
        }
    }

    pub fn draw_marker(&self, camera: &CameraRig, pos: GridPos, color: Color) {
        if !self.contains(pos) {
            return;
        }
        let center = camera.world_to_screen(grid_to_iso(pos.x as f32, pos.y as f32, 0.1));
        draw_circle_lines(center.x, center.y, 14.0 * camera.zoom, 3.0, color);
        draw_line(
            center.x - 18.0,
            center.y,
            center.x + 18.0,
            center.y,
            2.0,
            color,
        );
        draw_line(
            center.x,
            center.y - 10.0,
            center.x,
            center.y + 10.0,
            2.0,
            color,
        );
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

    pub fn draw_signature_preview(&self, camera: &CameraRig, preview: &MapSignaturePreview) {
        for y in 0..preview.height {
            for x in 0..preview.width {
                let class = preview.cell(x, y).unwrap_or(0);
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, 0.0));
                draw_iso_tile(
                    center,
                    signature_tile_color(class),
                    Color::new(0.01, 0.012, 0.016, 0.55),
                );
            }
        }
    }

    pub fn draw_inferred_layer_preview(
        &self,
        camera: &CameraRig,
        preview: &MapInferredLayerPreview,
    ) {
        for y in 0..preview.height {
            for x in 0..preview.width {
                let Some(cell) = preview.cell(x, y) else {
                    continue;
                };
                let z = cell.height_class as f32 * 0.06;
                let center = camera.world_to_screen(grid_to_iso(x as f32, y as f32, z));
                draw_iso_tile(
                    center,
                    inferred_tile_color(cell.visual_class, cell.height_class),
                    Color::new(0.01, 0.012, 0.016, 0.58),
                );
            }
        }
    }
}

fn inferred_tile_color(class: u8, height_class: u8) -> Color {
    let base = match class {
        0 => Color::from_rgba(42, 50, 56, 255),
        1 => Color::from_rgba(82, 124, 89, 255),
        2 => Color::from_rgba(126, 116, 76, 255),
        3 => Color::from_rgba(77, 112, 152, 255),
        4 => Color::from_rgba(134, 102, 153, 255),
        5 => Color::from_rgba(165, 107, 70, 255),
        _ => Color::from_rgba(205, 205, 205, 255),
    };
    brighten(base, height_class as f32 * 0.035)
}

fn brighten(color: Color, amount: f32) -> Color {
    Color::new(
        (color.r + amount).min(1.0),
        (color.g + amount).min(1.0),
        (color.b + amount).min(1.0),
        color.a,
    )
}

fn signature_tile_color(class: u8) -> Color {
    match class {
        0 => Color::from_rgba(18, 20, 27, 255),
        1 => Color::from_rgba(58, 105, 147, 255),
        2 => Color::from_rgba(74, 137, 92, 255),
        3 => Color::from_rgba(157, 126, 62, 255),
        4 => Color::from_rgba(130, 86, 156, 255),
        5 => Color::from_rgba(158, 79, 80, 255),
        6 => Color::from_rgba(64, 150, 150, 255),
        7 => Color::from_rgba(180, 180, 92, 255),
        8 => Color::from_rgba(99, 105, 190, 255),
        9 => Color::from_rgba(190, 120, 70, 255),
        10 => Color::from_rgba(120, 170, 105, 255),
        11 => Color::from_rgba(170, 105, 150, 255),
        12 => Color::from_rgba(95, 145, 190, 255),
        13 => Color::from_rgba(190, 95, 120, 255),
        14 => Color::from_rgba(130, 150, 80, 255),
        _ => Color::from_rgba(205, 205, 205, 255),
    }
}
