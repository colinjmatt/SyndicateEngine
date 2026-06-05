//! Conservative diagnostics for decompressed `MAP*.DAT` city files.
//!
//! The original map format is not fully decoded yet. This module only claims the
//! stable structure that is observable across local samples: an RNC-wrapped
//! payload with a `64 * 64 * 12` primary cell section followed by a variable-size
//! tail that is also aligned to 12-byte records.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::rnc::{RncBlock, RncError};

pub const MAP_WIDTH_CANDIDATE: usize = 64;
pub const MAP_HEIGHT_CANDIDATE: usize = 64;
pub const MAP_CELL_BYTES: usize = 12;
pub const MAP_CELL_COUNT: usize = MAP_WIDTH_CANDIDATE * MAP_HEIGHT_CANDIDATE;
pub const MAP_PRIMARY_SECTION_LEN: usize = MAP_CELL_COUNT * MAP_CELL_BYTES;
pub const MAP_SIGNATURE_PREVIEW_CLASSES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDatAnalysis {
    pub container: MapDatContainer,
    pub payload: MapPayloadAnalysis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapDatContainer {
    Plain,
    RncVerified {
        method: u8,
        packed_len: u32,
        unpacked_len: u32,
        block_count: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPayloadAnalysis {
    pub len: usize,
    pub primary_grid: Option<MapPrimaryGridAnalysis>,
    pub tail: MapTailAnalysis,
    pub signature_preview: Option<MapSignaturePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPrimaryGridAnalysis {
    pub width: usize,
    pub height: usize,
    pub cell_count: usize,
    pub bytes_per_cell: usize,
    pub unique_cells: usize,
    pub empty_cells: usize,
    pub word_stats: [WordStats; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordStats {
    pub min: u32,
    pub max: u32,
    pub unique_values: usize,
    pub zero_values: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapTailAnalysis {
    pub len: usize,
    pub aligned_to_cell_record: bool,
    pub record_count_12: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapCellRecord {
    pub words: [u32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSignaturePreview {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u8>,
    pub visual_classes: usize,
    pub unique_signatures: usize,
    pub dominant_signature_cells: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapDecodeError {
    Rnc(RncError),
}

impl MapDatAnalysis {
    pub fn analyze_file_bytes(data: &[u8]) -> Result<Self, MapDecodeError> {
        if let Some(block) = RncBlock::parse(data) {
            let decoded = block.decompress().map_err(MapDecodeError::Rnc)?;
            Ok(Self {
                container: MapDatContainer::RncVerified {
                    method: block.header.method,
                    packed_len: block.header.packed_len,
                    unpacked_len: block.header.unpacked_len,
                    block_count: block.header.block_count,
                },
                payload: analyze_payload(&decoded),
            })
        } else {
            Ok(Self {
                container: MapDatContainer::Plain,
                payload: analyze_payload(data),
            })
        }
    }

    pub fn short_label(&self) -> String {
        let container = match self.container {
            MapDatContainer::Plain => "plain".to_string(),
            MapDatContainer::RncVerified {
                method,
                packed_len,
                unpacked_len,
                block_count,
            } => format!(
                "RNC method {method} verified, packed {packed_len}, unpacked {unpacked_len}, {block_count} blocks"
            ),
        };

        format!("{container}; {}", self.payload.short_label())
    }
}

impl MapPayloadAnalysis {
    pub fn short_label(&self) -> String {
        if let Some(grid) = &self.primary_grid {
            let preview = self
                .signature_preview
                .as_ref()
                .map(|preview| {
                    format!(
                        "; preview {} classes, dominant {}%",
                        preview.visual_classes,
                        preview.dominant_coverage_percent()
                    )
                })
                .unwrap_or_default();
            format!(
                "{}x{}x{} primary cells, {} unique, {} empty; tail {} bytes ({} x 12-byte records){}",
                grid.width,
                grid.height,
                grid.bytes_per_cell,
                grid.unique_cells,
                grid.empty_cells,
                self.tail.len,
                self.tail.record_count_12,
                preview
            )
        } else {
            format!(
                "{} bytes; below {}-byte 64x64x12 primary-section candidate",
                self.len, MAP_PRIMARY_SECTION_LEN
            )
        }
    }
}

impl MapSignaturePreview {
    pub fn cell(&self, x: usize, y: usize) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y * self.width + x).copied()
    }

    pub fn dominant_coverage_percent(&self) -> u8 {
        if self.cells.is_empty() {
            return 0;
        }
        ((self.dominant_signature_cells * 100 + self.cells.len() / 2) / self.cells.len()) as u8
    }
}

impl MapCellRecord {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != MAP_CELL_BYTES {
            return None;
        }

        Some(Self {
            words: [
                u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
                u32::from_le_bytes([data[4], data[5], data[6], data[7]]),
                u32::from_le_bytes([data[8], data[9], data[10], data[11]]),
            ],
        })
    }
}

pub fn analyze_payload(data: &[u8]) -> MapPayloadAnalysis {
    if data.len() < MAP_PRIMARY_SECTION_LEN {
        return MapPayloadAnalysis {
            len: data.len(),
            primary_grid: None,
            tail: MapTailAnalysis {
                len: 0,
                aligned_to_cell_record: false,
                record_count_12: 0,
            },
            signature_preview: None,
        };
    }

    let primary = &data[..MAP_PRIMARY_SECTION_LEN];
    let tail_len = data.len() - MAP_PRIMARY_SECTION_LEN;
    let mut unique_cells = BTreeSet::new();
    let mut empty_cells = 0;
    let mut word_values = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut zero_values = [0usize; 3];

    for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
        let mut record_bytes = [0; MAP_CELL_BYTES];
        record_bytes.copy_from_slice(chunk);
        if record_bytes.iter().all(|&byte| byte == 0) {
            empty_cells += 1;
        }
        unique_cells.insert(record_bytes);

        if let Some(record) = MapCellRecord::parse(chunk) {
            for (index, value) in record.words.into_iter().enumerate() {
                if value == 0 {
                    zero_values[index] += 1;
                }
                word_values[index].insert(value);
            }
        }
    }

    let word_stats = std::array::from_fn(|index| WordStats {
        min: word_values[index].first().copied().unwrap_or_default(),
        max: word_values[index].last().copied().unwrap_or_default(),
        unique_values: word_values[index].len(),
        zero_values: zero_values[index],
    });

    MapPayloadAnalysis {
        len: data.len(),
        primary_grid: Some(MapPrimaryGridAnalysis {
            width: MAP_WIDTH_CANDIDATE,
            height: MAP_HEIGHT_CANDIDATE,
            cell_count: MAP_CELL_COUNT,
            bytes_per_cell: MAP_CELL_BYTES,
            unique_cells: unique_cells.len(),
            empty_cells,
            word_stats,
        }),
        tail: MapTailAnalysis {
            len: tail_len,
            aligned_to_cell_record: tail_len % MAP_CELL_BYTES == 0,
            record_count_12: tail_len / MAP_CELL_BYTES,
        },
        signature_preview: Some(build_signature_preview(primary)),
    }
}

fn build_signature_preview(primary: &[u8]) -> MapSignaturePreview {
    let mut frequency = BTreeMap::<[u8; MAP_CELL_BYTES], usize>::new();
    let mut records = Vec::with_capacity(MAP_CELL_COUNT);

    for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
        let mut record = [0; MAP_CELL_BYTES];
        record.copy_from_slice(chunk);
        *frequency.entry(record).or_insert(0) += 1;
        records.push(record);
    }

    let mut ranked = frequency
        .iter()
        .map(|(&record, &count)| (record, count))
        .collect::<Vec<_>>();
    ranked.sort_by(|(left_record, left_count), (right_record, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_record.cmp(right_record))
    });

    let mut class_by_record = BTreeMap::new();
    for (index, (record, _)) in ranked
        .iter()
        .take(MAP_SIGNATURE_PREVIEW_CLASSES.saturating_sub(1))
        .enumerate()
    {
        class_by_record.insert(*record, index as u8 + 1);
    }

    let cells = records
        .iter()
        .map(|record| class_by_record.get(record).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    let dominant_signature_cells = ranked.first().map(|(_, count)| *count).unwrap_or(0);
    let visual_classes = ranked
        .len()
        .min(MAP_SIGNATURE_PREVIEW_CLASSES.saturating_sub(1))
        + usize::from(ranked.len() >= MAP_SIGNATURE_PREVIEW_CLASSES);

    MapSignaturePreview {
        width: MAP_WIDTH_CANDIDATE,
        height: MAP_HEIGHT_CANDIDATE,
        cells,
        visual_classes,
        unique_signatures: ranked.len(),
        dominant_signature_cells,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAP_CELL_BYTES, MAP_CELL_COUNT, MAP_PRIMARY_SECTION_LEN, MAP_SIGNATURE_PREVIEW_CLASSES,
        MapCellRecord, analyze_payload,
    };

    #[test]
    fn parses_map_cell_record_as_three_little_endian_words() {
        let record = MapCellRecord::parse(&[1, 0, 0, 0, 2, 0, 0, 0, 0xff, 0, 0, 0]).unwrap();
        assert_eq!(record.words, [1, 2, 255]);
        assert!(MapCellRecord::parse(&[0; MAP_CELL_BYTES - 1]).is_none());
    }

    #[test]
    fn analyzes_synthetic_64x64x12_payload() {
        let mut payload = vec![0; MAP_PRIMARY_SECTION_LEN + MAP_CELL_BYTES * 2];
        payload[0..4].copy_from_slice(&1u32.to_le_bytes());
        payload[4..8].copy_from_slice(&2u32.to_le_bytes());
        payload[8..12].copy_from_slice(&3u32.to_le_bytes());

        let analysis = analyze_payload(&payload);
        let grid = analysis.primary_grid.unwrap();
        assert_eq!(analysis.len, payload.len());
        assert_eq!(grid.cell_count, MAP_CELL_COUNT);
        assert_eq!(grid.unique_cells, 2);
        assert_eq!(grid.empty_cells, MAP_CELL_COUNT - 1);
        assert_eq!(grid.word_stats[0].unique_values, 2);
        assert_eq!(grid.word_stats[0].zero_values, MAP_CELL_COUNT - 1);
        assert_eq!(analysis.tail.len, MAP_CELL_BYTES * 2);
        assert!(analysis.tail.aligned_to_cell_record);
        assert_eq!(analysis.tail.record_count_12, 2);
        let preview = analysis.signature_preview.unwrap();
        assert_eq!(preview.width, 64);
        assert_eq!(preview.height, 64);
        assert_eq!(preview.unique_signatures, 2);
        assert_eq!(preview.visual_classes, 2);
        assert_eq!(preview.dominant_signature_cells, MAP_CELL_COUNT - 1);
        assert_eq!(preview.cell(1, 0), Some(1));
        assert!(preview.cell(64, 0).is_none());
    }

    #[test]
    fn flags_payloads_below_primary_section_candidate() {
        let analysis = analyze_payload(&[0; 128]);
        assert!(analysis.primary_grid.is_none());
        assert_eq!(analysis.tail.len, 0);
        assert!(analysis.signature_preview.is_none());
    }

    #[test]
    fn collapses_rare_records_into_preview_class_zero() {
        let mut payload = vec![0; MAP_PRIMARY_SECTION_LEN];
        for index in 0..MAP_SIGNATURE_PREVIEW_CLASSES {
            let start = index * MAP_CELL_BYTES;
            payload[start..start + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
        }

        let analysis = analyze_payload(&payload);
        let preview = analysis.signature_preview.unwrap();
        assert_eq!(preview.unique_signatures, MAP_SIGNATURE_PREVIEW_CLASSES + 1);
        assert_eq!(preview.visual_classes, MAP_SIGNATURE_PREVIEW_CLASSES);
        assert_eq!(preview.cell(20, 0), Some(1));
        assert_eq!(preview.cell(MAP_SIGNATURE_PREVIEW_CLASSES - 1, 0), Some(0));
    }
}
