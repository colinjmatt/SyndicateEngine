//! Runtime mission/map selection metadata from local original assets.
//!
//! This module intentionally reads only the small map/palette selection surface
//! needed to choose the runtime MAP/HBLK palette. It does not decode objectives,
//! people, vehicles, objects, or gameplay semantics.

use std::{fs, path::Path};

use crate::engine::rnc::{RncBlock, RncError};

const DEFAULT_CAMPAIGN_LABEL: &str = "West Europe";
const DEFAULT_MISSION_ID: u16 = 1;
const DEFAULT_PALETTE_ID: u8 = 2;
const MAP_INFO_OFFSET: usize = 113_960;
const MAP_INFO_BYTES: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionSelection {
    pub campaign_label: String,
    pub mission_id: u16,
    pub palette_id: u8,
    pub mission_label: String,
    pub map_id: u16,
    pub map_label: String,
    pub palette_label: String,
    pub min_scroll_tile: (u16, u16),
    pub max_scroll_tile: (u16, u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalMissionSelectionError {
    NoMissionCandidate,
    Decode(String),
    TruncatedMapInfo,
    InvalidMapId,
}

impl OriginalMissionSelection {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, OriginalMissionSelectionError> {
        Self::from_root_for_campaign_block(
            root,
            DEFAULT_CAMPAIGN_LABEL,
            DEFAULT_MISSION_ID,
            DEFAULT_PALETTE_ID,
        )
    }

    pub fn from_root_for_campaign_block(
        root: impl AsRef<Path>,
        campaign_label: &str,
        mission_id: u16,
        palette_id: u8,
    ) -> Result<Self, OriginalMissionSelectionError> {
        let root = root.as_ref();
        for relative in game_candidates(mission_id) {
            let path = root.join(&relative);
            let Ok(data) = fs::read(&path) else {
                continue;
            };
            let decoded = decode_maybe_rnc(&data)
                .map_err(|err| OriginalMissionSelectionError::Decode(format!("{err:?}")))?;
            return Self::from_decoded_game_bytes(
                campaign_label.to_string(),
                mission_id,
                palette_id,
                relative,
                &decoded,
                root,
            );
        }

        Err(OriginalMissionSelectionError::NoMissionCandidate)
    }

    pub fn from_decoded_game_bytes(
        campaign_label: String,
        mission_id: u16,
        palette_id: u8,
        mission_label: String,
        decoded: &[u8],
        root: &Path,
    ) -> Result<Self, OriginalMissionSelectionError> {
        let map_info = decoded
            .get(MAP_INFO_OFFSET..MAP_INFO_OFFSET + MAP_INFO_BYTES)
            .ok_or(OriginalMissionSelectionError::TruncatedMapInfo)?;
        let map_id = read_le_u16(map_info, 0);
        if map_id == 0 {
            return Err(OriginalMissionSelectionError::InvalidMapId);
        }

        let min_scroll_tile = (read_le_u16(map_info, 2) / 2, read_le_u16(map_info, 4) / 2);
        let max_scroll_tile = (read_le_u16(map_info, 6) / 2, read_le_u16(map_info, 8) / 2);
        let map_label = first_existing_label(root, map_candidates(map_id))
            .unwrap_or_else(|| format!("SYNDICAT/DATA/{}", map_file_name(map_id)));
        let palette_label = first_existing_label(root, palette_candidates(palette_id))
            .unwrap_or_else(|| format!("SYNDICAT/DATA/{}", palette_file_name(palette_id)));

        Ok(Self {
            campaign_label,
            mission_id,
            palette_id,
            mission_label,
            map_id,
            map_label,
            palette_label,
            min_scroll_tile,
            max_scroll_tile,
        })
    }

    pub fn panel_label(&self) -> String {
        format!(
            "{} mission {} -> {}",
            self.campaign_label, self.mission_id, self.map_label
        )
    }

    pub fn status_label(&self) -> String {
        format!(
            "Map metadata {} + {}; runtime metadata only",
            compact_asset_label(&self.map_label),
            compact_asset_label(&self.palette_label)
        )
    }
}

pub fn map_candidates(map_id: u16) -> Vec<String> {
    let file_name = map_file_name(map_id);
    data_file_candidates(&file_name)
}

pub fn palette_candidates(palette_id: u8) -> Vec<String> {
    let file_name = palette_file_name(palette_id);
    data_file_candidates(&file_name)
}

fn game_candidates(mission_id: u16) -> Vec<String> {
    let file_name = format!("GAME{mission_id:02}.DAT");
    data_file_candidates(&file_name)
}

fn data_file_candidates(file_name: &str) -> Vec<String> {
    ["SYNDICAT/DATA", "DATADISK/DATA"]
        .into_iter()
        .map(|prefix| format!("{prefix}/{file_name}"))
        .collect()
}

fn map_file_name(map_id: u16) -> String {
    format!("MAP{map_id:02}.DAT")
}

fn palette_file_name(palette_id: u8) -> String {
    format!("HPAL{palette_id:02}.DAT")
}

fn first_existing_label(root: &Path, candidates: Vec<String>) -> Option<String> {
    candidates
        .into_iter()
        .find(|relative| root.join(relative).is_file())
}

fn compact_asset_label(label: &str) -> &str {
    label.rsplit('/').next().unwrap_or(label)
}

fn read_le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn decode_maybe_rnc(data: &[u8]) -> Result<Vec<u8>, RncError> {
    if let Some(block) = RncBlock::parse(data) {
        block.decompress()
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAP_INFO_OFFSET, OriginalMissionSelection, map_candidates, palette_candidates};
    use std::path::Path;

    #[test]
    fn parses_map_selection_metadata_without_gameplay_semantics() {
        let mut decoded = vec![0u8; MAP_INFO_OFFSET + 14];
        decoded[MAP_INFO_OFFSET..MAP_INFO_OFFSET + 2].copy_from_slice(&7u16.to_le_bytes());
        decoded[MAP_INFO_OFFSET + 2..MAP_INFO_OFFSET + 4].copy_from_slice(&20u16.to_le_bytes());
        decoded[MAP_INFO_OFFSET + 4..MAP_INFO_OFFSET + 6].copy_from_slice(&40u16.to_le_bytes());
        decoded[MAP_INFO_OFFSET + 6..MAP_INFO_OFFSET + 8].copy_from_slice(&100u16.to_le_bytes());
        decoded[MAP_INFO_OFFSET + 8..MAP_INFO_OFFSET + 10].copy_from_slice(&140u16.to_le_bytes());

        let selection = OriginalMissionSelection::from_decoded_game_bytes(
            "synthetic campaign".to_string(),
            3,
            4,
            "synthetic/GAME03.DAT".to_string(),
            &decoded,
            Path::new("/nonexistent"),
        )
        .unwrap();

        assert_eq!(selection.map_id, 7);
        assert_eq!(selection.palette_id, 4);
        assert_eq!(selection.min_scroll_tile, (10, 20));
        assert_eq!(selection.max_scroll_tile, (50, 70));
        assert!(selection.status_label().contains("runtime metadata only"));
        assert!(!selection.status_label().contains("objective"));
    }

    #[test]
    fn formats_data_file_candidates_with_two_digit_ids() {
        assert_eq!(
            map_candidates(1),
            vec![
                "SYNDICAT/DATA/MAP01.DAT".to_string(),
                "DATADISK/DATA/MAP01.DAT".to_string()
            ]
        );
        assert_eq!(
            palette_candidates(2),
            vec![
                "SYNDICAT/DATA/HPAL02.DAT".to_string(),
                "DATADISK/DATA/HPAL02.DAT".to_string()
            ]
        );
    }
}
