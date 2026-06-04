use crate::{
    engine::{camera::CameraRig, iso::grid_to_iso, palette},
    game::{combat::Weapon, pathfinding::GridPos},
};
use macroquad::prelude::*;

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: &'static str,
    pub grid: Vec2,
    pub target: Vec2,
    pub path: Vec<GridPos>,
    pub selected: bool,
    pub color: Color,
    pub last_order: OrderStatus,
    pub weapon: Weapon,
    pub cooldown: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderStatus {
    Idle,
    Moving { destination: GridPos, steps: usize },
    Invalid { destination: GridPos },
    Attacking { target: &'static str },
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
                path: Vec::new(),
                selected: i == 0,
                color: [palette::NEON_GREEN, palette::CYBER_AMBER, SKYBLUE, MAGENTA][i],
                last_order: OrderStatus::Idle,
                weapon: Weapon::UZI,
                cooldown: 0.0,
            })
            .collect()
    }

    pub fn update(&mut self, dt: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
        if let Some(next) = self.path.first().copied() {
            self.target = vec2(next.x as f32, next.y as f32);
            if (self.grid - self.target).length() < 0.08 {
                self.grid = self.target;
                self.path.remove(0);
                if self.path.is_empty() {
                    self.last_order = OrderStatus::Idle;
                }
            }
        }

        let delta = self.target - self.grid;
        if delta.length() > 0.03 {
            let step = dt * 4.0;
            self.grid += delta.normalize() * step.min(delta.length());
        }
    }

    pub fn grid_pos(&self) -> GridPos {
        GridPos::new(self.grid.x.round() as i32, self.grid.y.round() as i32)
    }

    pub fn set_path(&mut self, path: Vec<GridPos>) {
        let destination = path.last().copied();
        self.path = path.into_iter().skip(1).collect();
        self.last_order = destination
            .filter(|_| !self.path.is_empty())
            .map(|destination| OrderStatus::Moving {
                destination,
                steps: self.path.len(),
            })
            .unwrap_or(OrderStatus::Idle);
    }

    pub fn reject_order(&mut self, destination: GridPos) {
        self.path.clear();
        self.last_order = OrderStatus::Invalid { destination };
    }

    pub fn can_fire(&self) -> bool {
        self.cooldown <= 0.0
    }

    pub fn mark_fired_at(&mut self, target: &'static str) {
        self.cooldown = self.weapon.cooldown_secs;
        self.last_order = OrderStatus::Attacking { target };
    }

    pub fn destination(&self) -> Option<GridPos> {
        self.path.last().copied()
    }

    pub fn order_summary(&self) -> String {
        match self.last_order {
            OrderStatus::Idle => "idle".to_string(),
            OrderStatus::Moving { destination, steps } => {
                format!(
                    "moving to {},{} via {} steps",
                    destination.x, destination.y, steps
                )
            }
            OrderStatus::Invalid { destination } => {
                format!("invalid order {},{}", destination.x, destination.y)
            }
            OrderStatus::Attacking { target } => format!("attacking {target}"),
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

#[cfg(test)]
mod tests {
    use super::{Agent, OrderStatus};
    use crate::game::pathfinding::GridPos;

    #[test]
    fn set_path_tracks_destination_and_steps() {
        let mut agent = Agent::squad().remove(0);
        agent.set_path(vec![
            GridPos::new(5, 8),
            GridPos::new(6, 8),
            GridPos::new(7, 8),
        ]);
        assert_eq!(agent.destination(), Some(GridPos::new(7, 8)));
        assert_eq!(
            agent.last_order,
            OrderStatus::Moving {
                destination: GridPos::new(7, 8),
                steps: 2
            }
        );
    }

    #[test]
    fn invalid_order_clears_path() {
        let mut agent = Agent::squad().remove(0);
        agent.set_path(vec![GridPos::new(5, 8), GridPos::new(6, 8)]);
        agent.reject_order(GridPos::new(20, 22));
        assert!(agent.path.is_empty());
        assert_eq!(
            agent.last_order,
            OrderStatus::Invalid {
                destination: GridPos::new(20, 22)
            }
        );
    }
}
