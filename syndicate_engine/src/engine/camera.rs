use macroquad::prelude::*;

#[derive(Debug, Clone)]
pub struct CameraRig {
    pub offset: Vec2,
    pub zoom: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            offset: vec2(720.0, 160.0),
            zoom: 1.0,
        }
    }
}

impl CameraRig {
    pub fn update(&mut self, dt: f32) {
        let speed = 520.0 * dt / self.zoom.max(0.25);
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            self.offset.x += speed;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            self.offset.x -= speed;
        }
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            self.offset.y += speed;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            self.offset.y -= speed;
        }

        let (_x, wheel) = mouse_wheel();
        if wheel != 0.0 {
            self.zoom = (self.zoom + wheel * 0.08).clamp(0.45, 2.5);
        }
    }

    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        world * self.zoom + self.offset
    }
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        (screen - self.offset) / self.zoom
    }
}
