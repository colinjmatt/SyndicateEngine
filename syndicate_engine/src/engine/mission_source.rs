//! Runtime mission/map selection metadata from local original assets.
//!
//! This module intentionally reads only the map/palette selection surface needed
//! to choose the runtime MAP/HBLK palette plus aggregate render-planning
//! summaries for fixed GAME sections. It does not render objects, expose
//! per-object placements, or decode gameplay semantics.

use std::{collections::BTreeSet, fs, path::Path};

use crate::engine::rnc::{RncBlock, RncError};

const DEFAULT_CAMPAIGN_LABEL: &str = "West Europe";
const DEFAULT_MISSION_ID: u16 = 1;
const DEFAULT_PALETTE_ID: u8 = 2;
const MAP_INFO_OFFSET: usize = 113_960;
const MAP_INFO_BYTES: usize = 14;
const PEOPLE_OFFSET: usize = 32_776;
const CARS_OFFSET: usize = 56_328;
const STATICS_OFFSET: usize = 59_016;
const WEAPONS_OFFSET: usize = 71_016;
const SFX_OFFSET: usize = 89_448;
const SCENARIOS_OFFSET: usize = 97_128;

const ON_MAP_DESC: &[u8] = &[0x04];
const STATIC_DRAW_DESCS: &[u8] = &[0x04, 0x06, 0x07];

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
    pub render_diagnostics: OriginalMissionRenderDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionRenderDiagnostics {
    pub sections: Vec<OriginalMissionSectionSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMissionSectionSummary {
    pub label: String,
    pub record_count: usize,
    pub non_zero_records: usize,
    pub candidate_draw_records: usize,
    pub unique_type_values: usize,
    pub unique_subtype_values: usize,
    pub tile_x_range: Option<(u16, u16)>,
    pub tile_y_range: Option<(u16, u16)>,
    pub tile_z_range: Option<(u16, u16)>,
    pub draw_queue_stage: Option<String>,
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
        let render_diagnostics = OriginalMissionRenderDiagnostics::from_decoded_game_bytes(decoded);

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
            render_diagnostics,
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

impl OriginalMissionRenderDiagnostics {
    fn from_decoded_game_bytes(decoded: &[u8]) -> Self {
        let specs = [
            SectionSpec {
                label: "candidate people",
                start: PEOPLE_OFFSET,
                record_count: 256,
                record_size: 92,
                desc_offset: Some(10),
                active_descs: ON_MAP_DESC,
                type_offset: Some(20),
                subtype_offset: Some(21),
                position_offsets: Some((4, 6, 8)),
                draw_queue_stage: Some("people"),
            },
            SectionSpec {
                label: "candidate vehicles",
                start: CARS_OFFSET,
                record_count: 64,
                record_size: 42,
                desc_offset: Some(10),
                active_descs: ON_MAP_DESC,
                type_offset: Some(20),
                subtype_offset: Some(21),
                position_offsets: Some((4, 6, 8)),
                draw_queue_stage: Some("vehicles"),
            },
            SectionSpec {
                label: "candidate statics",
                start: STATICS_OFFSET,
                record_count: 400,
                record_size: 30,
                desc_offset: Some(10),
                active_descs: STATIC_DRAW_DESCS,
                type_offset: Some(24),
                subtype_offset: Some(25),
                position_offsets: Some((4, 6, 8)),
                draw_queue_stage: Some("statics"),
            },
            SectionSpec {
                label: "candidate weapons",
                start: WEAPONS_OFFSET,
                record_count: 512,
                record_size: 36,
                desc_offset: Some(10),
                active_descs: ON_MAP_DESC,
                type_offset: Some(24),
                subtype_offset: Some(25),
                position_offsets: Some((4, 6, 8)),
                draw_queue_stage: Some("weapons"),
            },
            SectionSpec {
                label: "candidate sfx",
                start: SFX_OFFSET,
                record_count: 256,
                record_size: 30,
                desc_offset: None,
                active_descs: &[],
                type_offset: None,
                subtype_offset: None,
                position_offsets: Some((4, 6, 8)),
                draw_queue_stage: Some("sfx"),
            },
            SectionSpec {
                label: "candidate scenarios",
                start: SCENARIOS_OFFSET,
                record_count: 2048,
                record_size: 8,
                desc_offset: None,
                active_descs: &[],
                type_offset: Some(7),
                subtype_offset: None,
                position_offsets: None,
                draw_queue_stage: None,
            },
        ];

        Self {
            sections: specs
                .into_iter()
                .map(|spec| summarize_section(decoded, spec))
                .collect(),
        }
    }

    pub fn object_queue_summary_label(&self) -> String {
        let (queued_records, queued_capacity, active_sections) = self.object_queue_counts();

        format!(
            "candidate object queue {queued_records}/{queued_capacity} records across {active_sections} sections; runtime-only"
        )
    }

    pub fn object_queue_panel_label(&self) -> String {
        let (queued_records, queued_capacity, active_sections) = self.object_queue_counts();
        format!("object queue cand {queued_records}/{queued_capacity}; {active_sections} sections")
    }

    pub fn object_queue_order_label(&self) -> &'static str {
        "draw order candidate: people > vehicles > weapons > statics > sfx after terrain"
    }

    pub fn object_queue_order_panel_label(&self) -> &'static str {
        "order cand: people>vehicles>weapons>statics>sfx"
    }

    fn object_queue_counts(&self) -> (usize, usize, usize) {
        let queued_records: usize = self
            .sections
            .iter()
            .filter(|section| section.draw_queue_stage.is_some())
            .map(|section| section.candidate_draw_records)
            .sum();
        let queued_capacity: usize = self
            .sections
            .iter()
            .filter(|section| section.draw_queue_stage.is_some())
            .map(|section| section.record_count)
            .sum();
        let active_sections = self
            .sections
            .iter()
            .filter(|section| section.draw_queue_stage.is_some())
            .filter(|section| section.candidate_draw_records > 0)
            .count();

        (queued_records, queued_capacity, active_sections)
    }
}

struct SectionSpec {
    label: &'static str,
    start: usize,
    record_count: usize,
    record_size: usize,
    desc_offset: Option<usize>,
    active_descs: &'static [u8],
    type_offset: Option<usize>,
    subtype_offset: Option<usize>,
    position_offsets: Option<(usize, usize, usize)>,
    draw_queue_stage: Option<&'static str>,
}

fn summarize_section(decoded: &[u8], spec: SectionSpec) -> OriginalMissionSectionSummary {
    let available_records = decoded
        .get(spec.start..)
        .map(|tail| tail.len().min(spec.record_count * spec.record_size) / spec.record_size)
        .unwrap_or_default();
    let mut non_zero_records = 0;
    let mut candidate_draw_records = 0;
    let mut type_values = BTreeSet::new();
    let mut subtype_values = BTreeSet::new();
    let mut tile_x_range = None;
    let mut tile_y_range = None;
    let mut tile_z_range = None;

    for index in 0..available_records {
        let start = spec.start + index * spec.record_size;
        let Some(record) = decoded.get(start..start + spec.record_size) else {
            continue;
        };
        let non_zero = record.iter().any(|&byte| byte != 0);
        if !non_zero {
            continue;
        }

        non_zero_records += 1;
        if let Some(offset) = spec
            .type_offset
            .and_then(|offset| record.get(offset).copied())
        {
            type_values.insert(offset);
        }
        if let Some(offset) = spec
            .subtype_offset
            .and_then(|offset| record.get(offset).copied())
        {
            subtype_values.insert(offset);
        }

        let candidate_draw = if let Some(desc_offset) = spec.desc_offset {
            record
                .get(desc_offset)
                .is_some_and(|desc| spec.active_descs.contains(desc))
        } else if spec.type_offset.is_some() && spec.draw_queue_stage.is_none() {
            spec.type_offset
                .and_then(|offset| record.get(offset))
                .is_some_and(|scenario_type| *scenario_type != 0)
        } else {
            non_zero
        };

        if candidate_draw {
            candidate_draw_records += 1;
            if let Some((x_offset, y_offset, z_offset)) = spec.position_offsets {
                if let (Some(x), Some(y), Some(z)) = (
                    read_record_u16(record, x_offset).map(|value| value >> 8),
                    read_record_u16(record, y_offset).map(|value| value >> 8),
                    read_record_u16(record, z_offset).map(|value| value / 128),
                ) {
                    merge_range(&mut tile_x_range, x);
                    merge_range(&mut tile_y_range, y);
                    merge_range(&mut tile_z_range, z);
                }
            }
        }
    }

    OriginalMissionSectionSummary {
        label: spec.label.to_string(),
        record_count: available_records,
        non_zero_records,
        candidate_draw_records,
        unique_type_values: type_values.len(),
        unique_subtype_values: subtype_values.len(),
        tile_x_range,
        tile_y_range,
        tile_z_range,
        draw_queue_stage: spec.draw_queue_stage.map(str::to_string),
    }
}

fn read_record_u16(record: &[u8], offset: usize) -> Option<u16> {
    record
        .get(offset..offset + 2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn merge_range(range: &mut Option<(u16, u16)>, value: u16) {
    match range {
        Some((min, max)) => {
            *min = (*min).min(value);
            *max = (*max).max(value);
        }
        None => *range = Some((value, value)),
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
    use super::{
        CARS_OFFSET, MAP_INFO_OFFSET, OriginalMissionRenderDiagnostics, OriginalMissionSelection,
        PEOPLE_OFFSET, SCENARIOS_OFFSET, STATICS_OFFSET, WEAPONS_OFFSET, map_candidates,
        palette_candidates,
    };
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
        assert!(
            selection
                .render_diagnostics
                .object_queue_summary_label()
                .contains("runtime-only")
        );
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

    #[test]
    fn summarizes_candidate_object_sections_without_asset_bytes_or_semantics() {
        let mut decoded = vec![0u8; SCENARIOS_OFFSET + 16];
        write_position_record(&mut decoded[PEOPLE_OFFSET..PEOPLE_OFFSET + 92], 12, 34, 2);
        decoded[PEOPLE_OFFSET + 10] = 0x04;
        decoded[PEOPLE_OFFSET + 20] = 0x01;
        decoded[PEOPLE_OFFSET + 21] = 0x02;

        write_position_record(&mut decoded[CARS_OFFSET..CARS_OFFSET + 42], 88, 7, 1);
        decoded[CARS_OFFSET + 10] = 0x05;
        decoded[CARS_OFFSET + 20] = 0x02;

        write_position_record(&mut decoded[STATICS_OFFSET..STATICS_OFFSET + 30], 40, 44, 3);
        decoded[STATICS_OFFSET + 10] = 0x06;
        decoded[STATICS_OFFSET + 24] = 0x05;
        decoded[STATICS_OFFSET + 25] = 0x09;

        write_position_record(&mut decoded[WEAPONS_OFFSET..WEAPONS_OFFSET + 36], 20, 21, 0);
        decoded[WEAPONS_OFFSET + 10] = 0x04;
        decoded[WEAPONS_OFFSET + 24] = 0x04;
        decoded[WEAPONS_OFFSET + 25] = 0x0A;

        decoded[SCENARIOS_OFFSET + 7] = 0x08;

        let diagnostics = OriginalMissionRenderDiagnostics::from_decoded_game_bytes(&decoded);
        let people = diagnostics
            .sections
            .iter()
            .find(|section| section.label == "candidate people")
            .unwrap();
        let vehicles = diagnostics
            .sections
            .iter()
            .find(|section| section.label == "candidate vehicles")
            .unwrap();
        let statics = diagnostics
            .sections
            .iter()
            .find(|section| section.label == "candidate statics")
            .unwrap();

        assert_eq!(people.candidate_draw_records, 1);
        assert_eq!(people.tile_x_range, Some((12, 12)));
        assert_eq!(vehicles.candidate_draw_records, 0);
        assert_eq!(statics.candidate_draw_records, 1);
        assert!(
            diagnostics
                .object_queue_summary_label()
                .contains("candidate object queue")
        );
        assert!(
            diagnostics
                .object_queue_order_label()
                .contains("after terrain")
        );
        assert!(!diagnostics.object_queue_summary_label().contains("0x"));
    }

    fn write_position_record(record: &mut [u8], tile_x: u16, tile_y: u16, tile_z: u16) {
        record[4..6].copy_from_slice(&(tile_x << 8).to_le_bytes());
        record[6..8].copy_from_slice(&(tile_y << 8).to_le_bytes());
        record[8..10].copy_from_slice(&(tile_z * 128).to_le_bytes());
    }
}
