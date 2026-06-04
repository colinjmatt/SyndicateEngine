#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimClock {
    paused: bool,
    speed: f32,
    step_once: bool,
}

impl Default for SimClock {
    fn default() -> Self {
        Self {
            paused: false,
            speed: 1.0,
            step_once: false,
        }
    }
}

impl SimClock {
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }
    pub fn step_once(&mut self) {
        self.step_once = true;
        self.paused = true;
    }
    pub fn faster(&mut self) {
        self.speed = (self.speed * 2.0).min(4.0);
    }
    pub fn slower(&mut self) {
        self.speed = (self.speed * 0.5).max(0.25);
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn advance_dt(&mut self, real_dt: f32) -> f32 {
        if self.step_once {
            self.step_once = false;
            return (1.0 / 30.0) * self.speed;
        }
        if self.paused {
            0.0
        } else {
            real_dt * self.speed
        }
    }

    pub fn label(&self) -> String {
        if self.paused {
            format!("paused @ {:.2}x", self.speed)
        } else {
            format!("running @ {:.2}x", self.speed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SimClock;

    #[test]
    fn pause_stops_time_until_step() {
        let mut clock = SimClock::default();
        clock.toggle_pause();
        assert_eq!(clock.advance_dt(1.0), 0.0);
        clock.step_once();
        assert!(clock.advance_dt(1.0) > 0.0);
        assert_eq!(clock.advance_dt(1.0), 0.0);
    }

    #[test]
    fn clamps_speed() {
        let mut clock = SimClock::default();
        for _ in 0..10 {
            clock.faster();
        }
        assert_eq!(clock.speed(), 4.0);
        for _ in 0..10 {
            clock.slower();
        }
        assert_eq!(clock.speed(), 0.25);
    }
}
