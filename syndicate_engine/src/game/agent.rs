use crate::engine::{camera::CameraRig, iso::grid_to_iso, palette};
use macroquad::prelude::*;

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: &'static str,
    pub grid: Vec2,
    pub target: Vec2,
    pub selected: bool,
    pub color: Color,
}

impl Agent {
    pub fn squad() -> Vec<Self> {
        ["MILES", "KATE", "ZERO", "RIGG"]
            .iter()
            .enumerate()
            .map(|(i, &name)| Self {
                name,
                grid: vec2(5.0 + i as f32, 8.0 + i as f32 * 0.35),
                target: vec2(5.0 + i as f32, 8.0 + i as f32 * 0.35),
                selected: i == 0,
                color: [palette::NEON_GREEN, palette::CYBER_AMBER, SKYBLUE, MAGENTA][i],
            })
            .collect()
    }

    pub fn update(&mut self, dt: f32) {
        let delta = self.target - self.grid;
        if delta.length() > 0.03 {
            self.grid += delta.normalize() * dt * 3.2;
        }
    }

    pub fn draw(&self, camera: &CameraRig) {
        let base = camera.world_to_screen(grid_to_iso(self.grid.x, self.grid.y, 0.0));
        let bob = (get_time() as f32 * 6.0 + self.grid.x).sin() * 2.0;
        let p = vec2(base.x, base.y - 20.0 * camera.zoom + bob);
        draw_circle(p.x, p.y, 9.0 * camera.zoom, self.color);
        draw_circle_lines(
            p.x,
            p.y,
            12.0 * camera.zoom,
            2.0,
            if self.selected { WHITE } else { DARKGRAY },
        );
        draw_line(
            base.x,
            base.y,
            p.x,
            p.y + 9.0 * camera.zoom,
            2.0,
            self.color,
        );
        draw_text(self.name, p.x - 18.0, p.y - 18.0, 14.0 * camera.zoom, WHITE);
    }
}
