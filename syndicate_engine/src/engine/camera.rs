use macroquad::prelude::*;

const WHEEL_ZOOM_STEP: f32 = 0.025;
const MIN_ZOOM_FACTOR_PER_FRAME: f32 = 0.92;
const MAX_ZOOM_FACTOR_PER_FRAME: f32 = 1.08;
const MIN_ZOOM: f32 = 0.45;
const MAX_ZOOM: f32 = 2.5;

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
            self.zoom_around(vec2(mouse_position().0, mouse_position().1), wheel);
        }
    }

    pub fn zoom_around(&mut self, screen_anchor: Vec2, wheel_delta: f32) {
        if wheel_delta == 0.0 {
            return;
        }
        let world_anchor = self.screen_to_world(screen_anchor);
        let factor = (1.0 + wheel_delta * WHEEL_ZOOM_STEP)
            .clamp(MIN_ZOOM_FACTOR_PER_FRAME, MAX_ZOOM_FACTOR_PER_FRAME);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = screen_anchor - world_anchor * self.zoom;
    }

    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        world * self.zoom + self.offset
    }
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        (screen - self.offset) / self.zoom
    }
}

#[cfg(test)]
mod tests {
    use super::CameraRig;
    use macroquad::prelude::*;

    #[test]
    fn zoom_around_preserves_world_point_under_cursor() {
        let mut camera = CameraRig {
            offset: vec2(120.0, -45.0),
            zoom: 1.0,
        };
        let cursor = vec2(420.0, 260.0);
        let before = camera.screen_to_world(cursor);

        camera.zoom_around(cursor, 1.0);

        let after = camera.screen_to_world(cursor);
        assert!((before.x - after.x).abs() < 0.001);
        assert!((before.y - after.y).abs() < 0.001);
        assert!(camera.zoom < 1.08);
    }
}
