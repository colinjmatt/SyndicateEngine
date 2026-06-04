use macroquad::prelude::*;

pub const TILE_W: f32 = 64.0;
pub const TILE_H: f32 = 32.0;

pub fn grid_to_iso(x: f32, y: f32, z: f32) -> Vec2 {
    vec2((x - y) * TILE_W * 0.5, (x + y) * TILE_H * 0.5 - z * TILE_H)
}

pub fn iso_to_grid(p: Vec2) -> Vec2 {
    let x = p.x / (TILE_W * 0.5);
    let y = p.y / (TILE_H * 0.5);
    vec2((y + x) * 0.5, (y - x) * 0.5)
}

pub fn draw_iso_tile(center: Vec2, color: Color, outline: Color) {
    let hw = TILE_W * 0.5;
    let hh = TILE_H * 0.5;
    draw_triangle(
        vec2(center.x, center.y - hh),
        vec2(center.x + hw, center.y),
        vec2(center.x, center.y + hh),
        color,
    );
    draw_triangle(
        vec2(center.x, center.y - hh),
        vec2(center.x, center.y + hh),
        vec2(center.x - hw, center.y),
        color,
    );
    draw_line(
        center.x,
        center.y - hh,
        center.x + hw,
        center.y,
        1.0,
        outline,
    );
    draw_line(
        center.x + hw,
        center.y,
        center.x,
        center.y + hh,
        1.0,
        outline,
    );
    draw_line(
        center.x,
        center.y + hh,
        center.x - hw,
        center.y,
        1.0,
        outline,
    );
    draw_line(
        center.x - hw,
        center.y,
        center.x,
        center.y - hh,
        1.0,
        outline,
    );
}
