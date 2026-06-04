use crate::{
    engine::{assets::AssetIndex, camera::CameraRig, iso::iso_to_grid},
    game::{agent::Agent, map::TacticalMap, ui},
};
use macroquad::prelude::*;

pub struct WorldState {
    assets: AssetIndex,
    camera: CameraRig,
    map: TacticalMap,
    agents: Vec<Agent>,
    selected: usize,
}

impl WorldState {
    pub fn new(assets: AssetIndex) -> Self {
        Self {
            assets,
            camera: CameraRig::default(),
            map: TacticalMap::demo_city(),
            agents: Agent::squad(),
            selected: 0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        if is_key_pressed(KeyCode::Escape) {
            std::process::exit(0);
        }
        self.camera.update(dt);
        for (key, idx) in [
            (KeyCode::Key1, 0),
            (KeyCode::Key2, 1),
            (KeyCode::Key3, 2),
            (KeyCode::Key4, 3),
        ] {
            if is_key_pressed(key) && idx < self.agents.len() {
                self.select(idx);
            }
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            let mouse = vec2(mouse_position().0, mouse_position().1);
            let grid = iso_to_grid(self.camera.screen_to_world(mouse));
            self.agents[self.selected].target = vec2(
                grid.x.clamp(0.0, self.map.width as f32 - 1.0),
                grid.y.clamp(0.0, self.map.height as f32 - 1.0),
            );
        }
        for agent in &mut self.agents {
            agent.update(dt);
        }
    }

    fn select(&mut self, idx: usize) {
        self.selected = idx;
        for (i, agent) in self.agents.iter_mut().enumerate() {
            agent.selected = i == idx;
        }
    }

    pub fn draw(&self) {
        self.map.draw(&self.camera);
        for agent in &self.agents {
            agent.draw(&self.camera);
        }
        ui::draw_hud(&self.assets, self.agents[self.selected].name);
        draw_minimap(&self.agents);
    }
}

fn draw_minimap(agents: &[Agent]) {
    let x = screen_width() - 188.0;
    let y = 22.0;
    draw_rectangle(x, y, 166.0, 166.0, Color::new(0.0, 0.0, 0.0, 0.56));
    draw_rectangle_lines(x, y, 166.0, 166.0, 2.0, GREEN);
    draw_text("CITY GRID", x + 18.0, y + 24.0, 18.0, GREEN);
    for agent in agents {
        let px = x + 18.0 + agent.grid.x / 28.0 * 130.0;
        let py = y + 38.0 + agent.grid.y / 28.0 * 124.0;
        draw_circle(px, py, if agent.selected { 5.0 } else { 3.5 }, agent.color);
    }
}
