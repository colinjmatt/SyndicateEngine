use crate::game::pathfinding::GridPos;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weapon {
    pub name: &'static str,
    pub range_tiles: i32,
    pub damage: i32,
    pub cooldown_secs: f32,
}

impl Weapon {
    pub const UZI: Self = Self {
        name: "Uzi",
        range_tiles: 6,
        damage: 18,
        cooldown_secs: 0.35,
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct Combatant {
    pub name: &'static str,
    pub pos: GridPos,
    pub hp: i32,
    pub max_hp: i32,
    pub weapon: Weapon,
    pub cooldown: f32,
}

impl Combatant {
    pub fn guard(name: &'static str, pos: GridPos) -> Self {
        Self {
            name,
            pos,
            hp: 50,
            max_hp: 50,
            weapon: Weapon::UZI,
            cooldown: 0.0,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }

    pub fn tick(&mut self, dt: f32) {
        self.cooldown = (self.cooldown - dt).max(0.0);
    }

    pub fn distance_to(&self, pos: GridPos) -> i32 {
        self.pos.manhattan(pos)
    }

    pub fn can_fire_at(&self, pos: GridPos) -> bool {
        self.is_alive() && self.cooldown <= 0.0 && self.distance_to(pos) <= self.weapon.range_tiles
    }

    pub fn apply_damage(&mut self, damage: i32) {
        self.hp = (self.hp - damage).max(0);
    }
}

pub fn resolve_attack(
    attacker_pos: GridPos,
    weapon: Weapon,
    target: &mut Combatant,
) -> AttackResult {
    if !target.is_alive() {
        return AttackResult::TargetAlreadyDown;
    }

    if attacker_pos.manhattan(target.pos) > weapon.range_tiles {
        return AttackResult::OutOfRange;
    }

    target.apply_damage(weapon.damage);
    if target.is_alive() {
        AttackResult::Hit {
            remaining_hp: target.hp,
        }
    } else {
        AttackResult::Eliminated
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackResult {
    Hit { remaining_hp: i32 },
    Eliminated,
    OutOfRange,
    TargetAlreadyDown,
}

#[cfg(test)]
mod tests {
    use super::{AttackResult, Combatant, Weapon, resolve_attack};
    use crate::game::pathfinding::GridPos;

    #[test]
    fn resolves_in_range_damage() {
        let mut guard = Combatant::guard("GUARD", GridPos::new(4, 4));
        let result = resolve_attack(GridPos::new(4, 8), Weapon::UZI, &mut guard);
        assert_eq!(result, AttackResult::Hit { remaining_hp: 32 });
        assert_eq!(guard.hp, 32);
    }

    #[test]
    fn rejects_out_of_range_attack() {
        let mut guard = Combatant::guard("GUARD", GridPos::new(20, 20));
        let result = resolve_attack(GridPos::new(1, 1), Weapon::UZI, &mut guard);
        assert_eq!(result, AttackResult::OutOfRange);
        assert_eq!(guard.hp, guard.max_hp);
    }
}
