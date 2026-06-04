use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::game::pathfinding::GridPos;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub selected_agent: usize,
    pub agents: Vec<AgentSave>,
    pub hostiles: Vec<HostileSave>,
    pub combat_log: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSave {
    pub name: String,
    pub grid_x: f32,
    pub grid_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub path: Vec<GridPos>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostileSave {
    pub name: String,
    pub pos: GridPos,
    pub hp: i32,
    pub cooldown: f32,
}

pub fn write_save(path: impl AsRef<Path>, save: &SaveGame) -> anyhow::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(save)?)?;
    Ok(())
}

pub fn read_save(path: impl AsRef<Path>) -> anyhow::Result<SaveGame> {
    let data = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

#[cfg(test)]
mod tests {
    use super::{AgentSave, HostileSave, SaveGame};
    use crate::game::pathfinding::GridPos;

    #[test]
    fn save_game_roundtrips_json() {
        let save = SaveGame {
            version: 1,
            selected_agent: 2,
            agents: vec![AgentSave {
                name: "ZERO".to_string(),
                grid_x: 4.0,
                grid_y: 5.0,
                target_x: 6.0,
                target_y: 7.0,
                path: vec![GridPos::new(6, 7)],
            }],
            hostiles: vec![HostileSave {
                name: "GUARD".to_string(),
                pos: GridPos::new(8, 9),
                hp: 12,
                cooldown: 0.5,
            }],
            combat_log: "test".to_string(),
        };

        let json = serde_json::to_string(&save).unwrap();
        let restored: SaveGame = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, save);
    }
}
