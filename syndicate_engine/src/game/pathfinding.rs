use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

use crate::game::map::TacticalMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

impl GridPos {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, other: Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenNode {
    pos: GridPos,
    cost: i32,
    estimate: i32,
}

impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .cmp(&self.estimate)
            .then_with(|| other.cost.cmp(&self.cost))
    }
}

impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn find_path(map: &TacticalMap, start: GridPos, goal: GridPos) -> Option<Vec<GridPos>> {
    if !map.is_walkable_pos(start) || !map.is_walkable_pos(goal) {
        return None;
    }

    let mut open = BinaryHeap::new();
    let mut came_from = HashMap::<GridPos, GridPos>::new();
    let mut cost_so_far = HashMap::<GridPos, i32>::new();

    open.push(OpenNode {
        pos: start,
        cost: 0,
        estimate: start.manhattan(goal),
    });
    cost_so_far.insert(start, 0);

    while let Some(current) = open.pop() {
        if current.pos == goal {
            return Some(reconstruct_path(start, goal, came_from));
        }

        for next in map.walkable_neighbors(current.pos) {
            let new_cost = current.cost + movement_cost(map, next);
            if new_cost < *cost_so_far.get(&next).unwrap_or(&i32::MAX) {
                cost_so_far.insert(next, new_cost);
                came_from.insert(next, current.pos);
                open.push(OpenNode {
                    pos: next,
                    cost: new_cost,
                    estimate: new_cost + next.manhattan(goal),
                });
            }
        }
    }

    None
}

fn movement_cost(map: &TacticalMap, pos: GridPos) -> i32 {
    if map.is_road_pos(pos) { 8 } else { 10 }
}

fn reconstruct_path(
    start: GridPos,
    goal: GridPos,
    came_from: HashMap<GridPos, GridPos>,
) -> Vec<GridPos> {
    let mut current = goal;
    let mut path = vec![current];
    while current != start {
        let Some(previous) = came_from.get(&current).copied() else {
            break;
        };
        current = previous;
        path.push(current);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::{GridPos, find_path};
    use crate::game::map::TacticalMap;

    #[test]
    fn finds_path_across_demo_city() {
        let map = TacticalMap::demo_city();
        let path = find_path(&map, GridPos::new(4, 4), GridPos::new(16, 16)).unwrap();
        assert_eq!(path.first().copied(), Some(GridPos::new(4, 4)));
        assert_eq!(path.last().copied(), Some(GridPos::new(16, 16)));
        assert!(path.iter().all(|&pos| map.is_walkable_pos(pos)));
    }

    #[test]
    fn rejects_water_goal() {
        let map = TacticalMap::demo_city();
        assert!(find_path(&map, GridPos::new(4, 4), GridPos::new(20, 22)).is_none());
    }
}
