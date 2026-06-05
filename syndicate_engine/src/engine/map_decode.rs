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
pub const MAP_INFERRED_LAYER_CLASSES: usize = 6;
pub const MAP_INFERRED_CLASS_BASELINE: u8 = 0;
pub const MAP_INFERRED_CLASS_SURFACE: u8 = 1;
pub const MAP_INFERRED_CLASS_DETAIL: u8 = 2;
pub const MAP_INFERRED_CLASS_REFERENCE: u8 = 3;
pub const MAP_INFERRED_CLASS_HEIGHT: u8 = 4;
pub const MAP_INFERRED_CLASS_MIXED: u8 = 5;
pub const MAP_CANDIDATE_SURFACE_LANE: usize = 0;
pub const MAP_CANDIDATE_DETAIL_LANE: usize = 4;
pub const MAP_CANDIDATE_REFERENCE_LANE: usize = 8;
pub const MAP_SPATIAL_TOP_TRANSITIONS: usize = 6;
pub const MAP_SPATIAL_TOP_BLOCKS: usize = 4;

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
    pub inferred_layer_preview: Option<MapInferredLayerPreview>,
    pub spatial_correlation: Option<MapSpatialCorrelationAnalysis>,
    pub substrate_candidate: Option<MapPrimarySubstrateCandidate>,
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
    pub byte_stats: [ByteLaneStats; MAP_CELL_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordStats {
    pub min: u32,
    pub max: u32,
    pub unique_values: usize,
    pub zero_values: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteLaneStats {
    pub min: u8,
    pub max: u8,
    pub unique_values: usize,
    pub zero_values: usize,
    pub top_values: Vec<ByteFrequency>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteFrequency {
    pub value: u8,
    pub count: usize,
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
pub struct MapInferredLayerPreview {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u8>,
    pub height_classes: Vec<u8>,
    pub surface_values: Vec<u8>,
    pub detail_values: Vec<u8>,
    pub reference_values: Vec<u8>,
    pub height_values: Vec<u8>,
    pub visual_classes: usize,
    pub class_counts: [usize; MAP_INFERRED_LAYER_CLASSES],
    pub surface_baseline: u8,
    pub detail_baseline: u8,
    pub reference_baseline: u8,
    pub height_lane: usize,
    pub height_baseline: u8,
    pub surface_unique: usize,
    pub detail_unique: usize,
    pub reference_unique: usize,
    pub height_unique: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapInferredLayerCell {
    pub visual_class: u8,
    pub height_class: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPrimarySubstrateCandidate {
    pub width: usize,
    pub height: usize,
    pub surface_index_candidate: Vec<u8>,
    pub detail_index_candidate: Vec<u8>,
    pub reference_candidate: Vec<u8>,
    pub height_candidate: Vec<u8>,
    pub field_evidence: [MapCandidateFieldEvidence; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapCandidateFieldEvidence {
    pub field: MapCandidateField,
    pub lane: usize,
    pub baseline: u8,
    pub unique_values: usize,
    pub continuity_percent: u8,
    pub repeated_2x2_percent: u8,
    pub gentle_gradient_percent: u8,
    pub confidence: MapCandidateEvidenceConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapCandidateEvidenceConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapCandidateField {
    SurfaceIndex,
    DetailIndex,
    Reference,
    Height,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSpatialCorrelationAnalysis {
    pub byte_lanes: [ByteLaneSpatialStats; MAP_CELL_BYTES],
    pub height_candidate_lane: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteLaneSpatialStats {
    pub lane: usize,
    pub right_pairs: usize,
    pub down_pairs: usize,
    pub right_same_pairs: usize,
    pub down_same_pairs: usize,
    pub total_2x2_blocks: usize,
    pub uniform_2x2_blocks: usize,
    pub repeated_2x2_patterns: usize,
    pub repeated_2x2_blocks: usize,
    pub top_transitions: Vec<ByteTransitionFrequency>,
    pub top_2x2_patterns: Vec<ByteBlockPatternFrequency>,
    pub gentle_gradient_pairs: usize,
    pub moderate_gradient_pairs: usize,
    pub max_abs_gradient: u8,
    pub mean_abs_gradient_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteTransitionFrequency {
    pub from: u8,
    pub to: u8,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteBlockPatternFrequency {
    pub values: [u8; 4],
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapGlobalCorrelationAnalysis {
    pub map_count: usize,
    pub total_cells: usize,
    pub unique_cells: usize,
    pub word_stats: [WordStats; 3],
    pub byte_stats: [ByteLaneStats; MAP_CELL_BYTES],
    pub spatial_correlation: MapSpatialCorrelationAnalysis,
    pub substrate_evidence: [MapCandidateFieldEvidence; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapDecodeError {
    Rnc(RncError),
}

impl MapDatAnalysis {
    pub fn analyze_file_bytes(data: &[u8]) -> Result<Self, MapDecodeError> {
        let (container, payload) = decode_map_payload_bytes(data)?;
        Ok(Self {
            container,
            payload: analyze_payload(&payload),
        })
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
            let inferred = self
                .inferred_layer_preview
                .as_ref()
                .map(|preview| format!("; inferred {}", preview.summary_label()))
                .unwrap_or_default();
            format!(
                "{}x{}x{} primary cells, {} unique, {} empty; tail {} bytes ({} x 12-byte records){}{}",
                grid.width,
                grid.height,
                grid.bytes_per_cell,
                grid.unique_cells,
                grid.empty_cells,
                self.tail.len,
                self.tail.record_count_12,
                preview,
                inferred
            )
        } else {
            format!(
                "{} bytes; below {}-byte 64x64x12 primary-section candidate",
                self.len, MAP_PRIMARY_SECTION_LEN
            )
        }
    }
}

impl MapInferredLayerPreview {
    pub fn cell(&self, x: usize, y: usize) -> Option<MapInferredLayerCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = y * self.width + x;
        Some(MapInferredLayerCell {
            visual_class: *self.cells.get(index)?,
            height_class: *self.height_classes.get(index)?,
        })
    }

    pub fn dominant_class_percent(&self) -> u8 {
        if self.cells.is_empty() {
            return 0;
        }
        let dominant = self.class_counts.iter().copied().max().unwrap_or(0);
        ((dominant * 100 + self.cells.len() / 2) / self.cells.len()) as u8
    }

    pub fn summary_label(&self) -> String {
        format!(
            "{} classes, dominant {}%, baselines w0:b0=0x{:02x} w1:b4=0x{:02x} w2:b8=0x{:02x}, height candidate b{}=0x{:02x}",
            self.visual_classes,
            self.dominant_class_percent(),
            self.surface_baseline,
            self.detail_baseline,
            self.reference_baseline,
            self.height_lane,
            self.height_baseline
        )
    }

    pub fn class_label(class: u8) -> &'static str {
        match class {
            MAP_INFERRED_CLASS_BASELINE => "baseline candidate",
            MAP_INFERRED_CLASS_SURFACE => "word0 surface candidate",
            MAP_INFERRED_CLASS_DETAIL => "word1 detail candidate",
            MAP_INFERRED_CLASS_REFERENCE => "word2 reference candidate",
            MAP_INFERRED_CLASS_HEIGHT => "height-lane candidate",
            MAP_INFERRED_CLASS_MIXED => "mixed candidate",
            _ => "unknown candidate",
        }
    }

    pub fn field_value(&self, field: MapCandidateField, x: usize, y: usize) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let index = y * self.width + x;
        match field {
            MapCandidateField::SurfaceIndex => self.surface_values.get(index),
            MapCandidateField::DetailIndex => self.detail_values.get(index),
            MapCandidateField::Reference => self.reference_values.get(index),
            MapCandidateField::Height => self.height_values.get(index),
        }
        .copied()
    }

    pub fn field_lane(&self, field: MapCandidateField) -> usize {
        match field {
            MapCandidateField::SurfaceIndex => MAP_CANDIDATE_SURFACE_LANE,
            MapCandidateField::DetailIndex => MAP_CANDIDATE_DETAIL_LANE,
            MapCandidateField::Reference => MAP_CANDIDATE_REFERENCE_LANE,
            MapCandidateField::Height => self.height_lane,
        }
    }

    pub fn field_baseline(&self, field: MapCandidateField) -> u8 {
        match field {
            MapCandidateField::SurfaceIndex => self.surface_baseline,
            MapCandidateField::DetailIndex => self.detail_baseline,
            MapCandidateField::Reference => self.reference_baseline,
            MapCandidateField::Height => self.height_baseline,
        }
    }

    pub fn field_unique_values(&self, field: MapCandidateField) -> usize {
        match field {
            MapCandidateField::SurfaceIndex => self.surface_unique,
            MapCandidateField::DetailIndex => self.detail_unique,
            MapCandidateField::Reference => self.reference_unique,
            MapCandidateField::Height => self.height_unique,
        }
    }

    pub fn field_label(&self, field: MapCandidateField) -> String {
        format!("{} b{}", field.provisional_label(), self.field_lane(field))
    }
}

impl MapCandidateField {
    pub const ALL: [Self; 4] = [
        Self::SurfaceIndex,
        Self::DetailIndex,
        Self::Reference,
        Self::Height,
    ];

    pub fn provisional_label(self) -> &'static str {
        match self {
            Self::SurfaceIndex => "surface_index_candidate",
            Self::DetailIndex => "detail_index_candidate",
            Self::Reference => "reference_candidate",
            Self::Height => "height_candidate",
        }
    }
}

impl MapPrimarySubstrateCandidate {
    pub fn field_value(&self, field: MapCandidateField, x: usize, y: usize) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }

        let index = y * self.width + x;
        match field {
            MapCandidateField::SurfaceIndex => self.surface_index_candidate.get(index),
            MapCandidateField::DetailIndex => self.detail_index_candidate.get(index),
            MapCandidateField::Reference => self.reference_candidate.get(index),
            MapCandidateField::Height => self.height_candidate.get(index),
        }
        .copied()
    }

    pub fn evidence_for(&self, field: MapCandidateField) -> Option<MapCandidateFieldEvidence> {
        self.field_evidence
            .iter()
            .copied()
            .find(|evidence| evidence.field == field)
    }
}

impl MapCandidateFieldEvidence {
    pub fn evidence_label(self) -> String {
        format!(
            "{} evidence: {} (b{}, baseline 0x{:02x}, unique {}, continuity {}%, repeated 2x2 {}%, gentle Δ<=1 {}%)",
            self.field.provisional_label(),
            self.confidence.label(),
            self.lane,
            self.baseline,
            self.unique_values,
            self.continuity_percent,
            self.repeated_2x2_percent,
            self.gentle_gradient_percent
        )
    }
}

impl MapCandidateEvidenceConfidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl ByteLaneSpatialStats {
    pub fn neighbour_pairs(&self) -> usize {
        self.right_pairs + self.down_pairs
    }

    pub fn same_neighbour_pairs(&self) -> usize {
        self.right_same_pairs + self.down_same_pairs
    }

    pub fn continuity_percent(&self) -> u8 {
        percent(self.same_neighbour_pairs(), self.neighbour_pairs())
    }

    pub fn right_continuity_percent(&self) -> u8 {
        percent(self.right_same_pairs, self.right_pairs)
    }

    pub fn down_continuity_percent(&self) -> u8 {
        percent(self.down_same_pairs, self.down_pairs)
    }

    pub fn uniform_2x2_percent(&self) -> u8 {
        percent(self.uniform_2x2_blocks, self.total_2x2_blocks)
    }

    pub fn repeated_2x2_percent(&self) -> u8 {
        percent(self.repeated_2x2_blocks, self.total_2x2_blocks)
    }

    pub fn gentle_gradient_percent(&self) -> u8 {
        percent(self.gentle_gradient_pairs, self.neighbour_pairs())
    }

    pub fn moderate_gradient_percent(&self) -> u8 {
        percent(self.moderate_gradient_pairs, self.neighbour_pairs())
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

pub fn decode_map_payload_bytes(data: &[u8]) -> Result<(MapDatContainer, Vec<u8>), MapDecodeError> {
    if let Some(block) = RncBlock::parse(data) {
        let decoded = block.decompress().map_err(MapDecodeError::Rnc)?;
        Ok((
            MapDatContainer::RncVerified {
                method: block.header.method,
                packed_len: block.header.packed_len,
                unpacked_len: block.header.unpacked_len,
                block_count: block.header.block_count,
            },
            decoded,
        ))
    } else {
        Ok((MapDatContainer::Plain, data.to_vec()))
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
            inferred_layer_preview: None,
            spatial_correlation: None,
            substrate_candidate: None,
        };
    }

    let primary = &data[..MAP_PRIMARY_SECTION_LEN];
    let tail_len = data.len() - MAP_PRIMARY_SECTION_LEN;
    let mut unique_cells = BTreeSet::new();
    let mut empty_cells = 0;
    let mut word_values = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut zero_values = [0usize; 3];
    let mut byte_values: [BTreeSet<u8>; MAP_CELL_BYTES] = Default::default();
    let mut byte_frequencies: [BTreeMap<u8, usize>; MAP_CELL_BYTES] = Default::default();
    let mut byte_zero_values = [0usize; MAP_CELL_BYTES];

    for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
        let mut record_bytes = [0; MAP_CELL_BYTES];
        record_bytes.copy_from_slice(chunk);
        if record_bytes.iter().all(|&byte| byte == 0) {
            empty_cells += 1;
        }
        unique_cells.insert(record_bytes);
        for (index, &byte) in record_bytes.iter().enumerate() {
            if byte == 0 {
                byte_zero_values[index] += 1;
            }
            byte_values[index].insert(byte);
            *byte_frequencies[index].entry(byte).or_insert(0) += 1;
        }

        if let Some(record) = MapCellRecord::parse(chunk) {
            for (index, value) in record.words.into_iter().enumerate() {
                if value == 0 {
                    zero_values[index] += 1;
                }
                word_values[index].insert(value);
            }
        }
    }

    let word_stats = build_word_stats(&word_values, &zero_values);
    let byte_stats = build_byte_stats(&byte_values, &byte_frequencies, &byte_zero_values);
    let height_lane = select_candidate_height_lane(&byte_frequencies);
    let inferred_layer_preview =
        build_inferred_layer_preview(primary, &byte_frequencies, height_lane);
    let spatial_correlation = build_spatial_correlation(primary, height_lane);
    let substrate_candidate = build_substrate_candidate(
        primary,
        &byte_frequencies,
        &byte_stats,
        &spatial_correlation,
        height_lane,
    );

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
            byte_stats,
        }),
        tail: MapTailAnalysis {
            len: tail_len,
            aligned_to_cell_record: tail_len % MAP_CELL_BYTES == 0,
            record_count_12: tail_len / MAP_CELL_BYTES,
        },
        signature_preview: Some(build_signature_preview(primary)),
        inferred_layer_preview: Some(inferred_layer_preview),
        spatial_correlation: Some(spatial_correlation),
        substrate_candidate: Some(substrate_candidate),
    }
}

pub fn analyze_primary_sections<'a, I>(sections: I) -> Option<MapGlobalCorrelationAnalysis>
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let sections = sections
        .into_iter()
        .filter(|section| section.len() >= MAP_PRIMARY_SECTION_LEN)
        .map(|section| &section[..MAP_PRIMARY_SECTION_LEN])
        .collect::<Vec<_>>();

    if sections.is_empty() {
        return None;
    }

    let mut unique_cells = BTreeSet::new();
    let mut word_values = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    let mut zero_values = [0usize; 3];
    let mut byte_values: [BTreeSet<u8>; MAP_CELL_BYTES] = Default::default();
    let mut byte_frequencies: [BTreeMap<u8, usize>; MAP_CELL_BYTES] = Default::default();
    let mut byte_zero_values = [0usize; MAP_CELL_BYTES];

    for primary in &sections {
        for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
            let mut record_bytes = [0; MAP_CELL_BYTES];
            record_bytes.copy_from_slice(chunk);
            unique_cells.insert(record_bytes);

            for (index, &byte) in record_bytes.iter().enumerate() {
                if byte == 0 {
                    byte_zero_values[index] += 1;
                }
                byte_values[index].insert(byte);
                *byte_frequencies[index].entry(byte).or_insert(0) += 1;
            }

            if let Some(record) = MapCellRecord::parse(chunk) {
                for (index, value) in record.words.into_iter().enumerate() {
                    if value == 0 {
                        zero_values[index] += 1;
                    }
                    word_values[index].insert(value);
                }
            }
        }
    }

    let height_lane = select_candidate_height_lane(&byte_frequencies);

    let map_count = sections.len();
    let byte_stats = build_byte_stats(&byte_values, &byte_frequencies, &byte_zero_values);
    let spatial_correlation = build_spatial_correlation_for_sections(sections, height_lane);
    let substrate_evidence =
        build_substrate_evidence(&byte_frequencies, &byte_stats, &spatial_correlation);

    Some(MapGlobalCorrelationAnalysis {
        map_count,
        total_cells: map_count * MAP_CELL_COUNT,
        unique_cells: unique_cells.len(),
        word_stats: build_word_stats(&word_values, &zero_values),
        byte_stats,
        spatial_correlation,
        substrate_evidence,
    })
}

fn build_word_stats(word_values: &[BTreeSet<u32>; 3], zero_values: &[usize; 3]) -> [WordStats; 3] {
    std::array::from_fn(|index| WordStats {
        min: word_values[index].first().copied().unwrap_or_default(),
        max: word_values[index].last().copied().unwrap_or_default(),
        unique_values: word_values[index].len(),
        zero_values: zero_values[index],
    })
}

fn build_byte_stats(
    byte_values: &[BTreeSet<u8>; MAP_CELL_BYTES],
    byte_frequencies: &[BTreeMap<u8, usize>; MAP_CELL_BYTES],
    byte_zero_values: &[usize; MAP_CELL_BYTES],
) -> [ByteLaneStats; MAP_CELL_BYTES] {
    std::array::from_fn(|index| ByteLaneStats {
        min: byte_values[index].first().copied().unwrap_or_default(),
        max: byte_values[index].last().copied().unwrap_or_default(),
        unique_values: byte_values[index].len(),
        zero_values: byte_zero_values[index],
        top_values: top_byte_values(&byte_frequencies[index], 4),
    })
}

fn top_byte_values(frequencies: &BTreeMap<u8, usize>, limit: usize) -> Vec<ByteFrequency> {
    let mut ranked = frequencies
        .iter()
        .map(|(&value, &count)| ByteFrequency { value, count })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    ranked.truncate(limit);
    ranked
}

fn dominant_byte(frequencies: &BTreeMap<u8, usize>) -> u8 {
    top_byte_values(frequencies, 1)
        .first()
        .map(|entry| entry.value)
        .unwrap_or_default()
}

fn select_candidate_height_lane(byte_frequencies: &[BTreeMap<u8, usize>; MAP_CELL_BYTES]) -> usize {
    const CANDIDATE_LANES: [usize; 9] = [1, 2, 3, 5, 6, 7, 9, 10, 11];

    CANDIDATE_LANES
        .into_iter()
        .min_by_key(|&lane| {
            let unique = byte_frequencies[lane].len();
            let max = byte_frequencies[lane]
                .keys()
                .next_back()
                .copied()
                .unwrap_or_default();
            let constant_penalty = if unique <= 1 { 1 } else { 0 };
            let broad_range_penalty = if max <= 31 { 0 } else { 1 };
            (constant_penalty, broad_range_penalty, unique, max, lane)
        })
        .unwrap_or(1)
}

fn build_inferred_layer_preview(
    primary: &[u8],
    byte_frequencies: &[BTreeMap<u8, usize>; MAP_CELL_BYTES],
    height_lane: usize,
) -> MapInferredLayerPreview {
    let surface_baseline = dominant_byte(&byte_frequencies[0]);
    let detail_baseline = dominant_byte(&byte_frequencies[4]);
    let reference_baseline = dominant_byte(&byte_frequencies[8]);
    let height_baseline = dominant_byte(&byte_frequencies[height_lane]);
    let mut cells = Vec::with_capacity(MAP_CELL_COUNT);
    let mut height_classes = Vec::with_capacity(MAP_CELL_COUNT);
    let mut surface_values = Vec::with_capacity(MAP_CELL_COUNT);
    let mut detail_values = Vec::with_capacity(MAP_CELL_COUNT);
    let mut reference_values = Vec::with_capacity(MAP_CELL_COUNT);
    let mut height_values = Vec::with_capacity(MAP_CELL_COUNT);
    let mut class_counts = [0usize; MAP_INFERRED_LAYER_CLASSES];

    for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
        let surface_value = chunk[0];
        let detail_value = chunk[4];
        let reference_value = chunk[8];
        let height_value = chunk[height_lane];
        let surface_changed = chunk[0] != surface_baseline;
        let detail_changed = chunk[4] != detail_baseline;
        let reference_changed = chunk[8] != reference_baseline;
        let height_changed = height_value != height_baseline;
        let changed_channels = [
            surface_changed,
            detail_changed,
            reference_changed,
            height_changed,
        ]
        .into_iter()
        .filter(|changed| *changed)
        .count();

        let visual_class = if changed_channels == 0 {
            MAP_INFERRED_CLASS_BASELINE
        } else if changed_channels > 1 {
            MAP_INFERRED_CLASS_MIXED
        } else if surface_changed {
            MAP_INFERRED_CLASS_SURFACE
        } else if detail_changed {
            MAP_INFERRED_CLASS_DETAIL
        } else if reference_changed {
            MAP_INFERRED_CLASS_REFERENCE
        } else {
            MAP_INFERRED_CLASS_HEIGHT
        };
        let height_class = height_value.abs_diff(height_baseline).min(15);

        class_counts[visual_class as usize] += 1;
        cells.push(visual_class);
        height_classes.push(height_class);
        surface_values.push(surface_value);
        detail_values.push(detail_value);
        reference_values.push(reference_value);
        height_values.push(height_value);
    }

    let visual_classes = class_counts.iter().filter(|&&count| count > 0).count();

    MapInferredLayerPreview {
        width: MAP_WIDTH_CANDIDATE,
        height: MAP_HEIGHT_CANDIDATE,
        cells,
        height_classes,
        surface_values,
        detail_values,
        reference_values,
        height_values,
        visual_classes,
        class_counts,
        surface_baseline,
        detail_baseline,
        reference_baseline,
        height_lane,
        height_baseline,
        surface_unique: byte_frequencies[0].len(),
        detail_unique: byte_frequencies[4].len(),
        reference_unique: byte_frequencies[8].len(),
        height_unique: byte_frequencies[height_lane].len(),
    }
}

fn build_substrate_candidate(
    primary: &[u8],
    byte_frequencies: &[BTreeMap<u8, usize>; MAP_CELL_BYTES],
    byte_stats: &[ByteLaneStats; MAP_CELL_BYTES],
    spatial_correlation: &MapSpatialCorrelationAnalysis,
    height_lane: usize,
) -> MapPrimarySubstrateCandidate {
    let mut surface_index_candidate = Vec::with_capacity(MAP_CELL_COUNT);
    let mut detail_index_candidate = Vec::with_capacity(MAP_CELL_COUNT);
    let mut reference_candidate = Vec::with_capacity(MAP_CELL_COUNT);
    let mut height_candidate = Vec::with_capacity(MAP_CELL_COUNT);

    for chunk in primary.chunks_exact(MAP_CELL_BYTES) {
        surface_index_candidate.push(chunk[MAP_CANDIDATE_SURFACE_LANE]);
        detail_index_candidate.push(chunk[MAP_CANDIDATE_DETAIL_LANE]);
        reference_candidate.push(chunk[MAP_CANDIDATE_REFERENCE_LANE]);
        height_candidate.push(chunk[height_lane]);
    }

    MapPrimarySubstrateCandidate {
        width: MAP_WIDTH_CANDIDATE,
        height: MAP_HEIGHT_CANDIDATE,
        surface_index_candidate,
        detail_index_candidate,
        reference_candidate,
        height_candidate,
        field_evidence: build_substrate_evidence(byte_frequencies, byte_stats, spatial_correlation),
    }
}

fn build_substrate_evidence(
    byte_frequencies: &[BTreeMap<u8, usize>; MAP_CELL_BYTES],
    byte_stats: &[ByteLaneStats; MAP_CELL_BYTES],
    spatial_correlation: &MapSpatialCorrelationAnalysis,
) -> [MapCandidateFieldEvidence; 4] {
    std::array::from_fn(|index| {
        let field = MapCandidateField::ALL[index];
        let lane = match field {
            MapCandidateField::SurfaceIndex => MAP_CANDIDATE_SURFACE_LANE,
            MapCandidateField::DetailIndex => MAP_CANDIDATE_DETAIL_LANE,
            MapCandidateField::Reference => MAP_CANDIDATE_REFERENCE_LANE,
            MapCandidateField::Height => spatial_correlation.height_candidate_lane,
        };
        let spatial = &spatial_correlation.byte_lanes[lane];
        MapCandidateFieldEvidence {
            field,
            lane,
            baseline: dominant_byte(&byte_frequencies[lane]),
            unique_values: byte_stats[lane].unique_values,
            continuity_percent: spatial.continuity_percent(),
            repeated_2x2_percent: spatial.repeated_2x2_percent(),
            gentle_gradient_percent: spatial.gentle_gradient_percent(),
            confidence: classify_candidate_evidence(field, byte_stats[lane].unique_values, spatial),
        }
    })
}

fn classify_candidate_evidence(
    field: MapCandidateField,
    unique_values: usize,
    spatial: &ByteLaneSpatialStats,
) -> MapCandidateEvidenceConfidence {
    let continuity = spatial.continuity_percent();
    let repeated = spatial.repeated_2x2_percent();
    let gentle = spatial.gentle_gradient_percent();

    match field {
        MapCandidateField::Height => {
            if unique_values > 1 && continuity >= 80 && gentle >= 95 {
                MapCandidateEvidenceConfidence::High
            } else if unique_values > 1 && continuity >= 50 && gentle >= 65 {
                MapCandidateEvidenceConfidence::Medium
            } else {
                MapCandidateEvidenceConfidence::Low
            }
        }
        _ => {
            if unique_values > 1 && continuity >= 50 && repeated >= 60 {
                MapCandidateEvidenceConfidence::High
            } else if unique_values > 1 && continuity >= 20 && repeated >= 30 {
                MapCandidateEvidenceConfidence::Medium
            } else {
                MapCandidateEvidenceConfidence::Low
            }
        }
    }
}

fn build_spatial_correlation(
    primary: &[u8],
    height_candidate_lane: usize,
) -> MapSpatialCorrelationAnalysis {
    build_spatial_correlation_for_sections(std::iter::once(primary), height_candidate_lane)
}

fn build_spatial_correlation_for_sections<'a, I>(
    sections: I,
    height_candidate_lane: usize,
) -> MapSpatialCorrelationAnalysis
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut accumulators: [LaneSpatialAccumulator; MAP_CELL_BYTES] = Default::default();

    for section in sections {
        if section.len() < MAP_PRIMARY_SECTION_LEN {
            continue;
        }
        let primary = &section[..MAP_PRIMARY_SECTION_LEN];
        for (lane, accumulator) in accumulators.iter_mut().enumerate() {
            accumulator.accumulate_primary(primary, lane);
        }
    }

    MapSpatialCorrelationAnalysis {
        byte_lanes: std::array::from_fn(|lane| accumulators[lane].to_stats(lane)),
        height_candidate_lane,
    }
}

#[derive(Debug, Clone, Default)]
struct LaneSpatialAccumulator {
    right_pairs: usize,
    down_pairs: usize,
    right_same_pairs: usize,
    down_same_pairs: usize,
    total_2x2_blocks: usize,
    uniform_2x2_blocks: usize,
    gradient_sum: usize,
    gentle_gradient_pairs: usize,
    moderate_gradient_pairs: usize,
    max_abs_gradient: u8,
    transition_frequencies: BTreeMap<(u8, u8), usize>,
    block_frequencies: BTreeMap<[u8; 4], usize>,
}

impl LaneSpatialAccumulator {
    fn accumulate_primary(&mut self, primary: &[u8], lane: usize) {
        for y in 0..MAP_HEIGHT_CANDIDATE {
            for x in 0..MAP_WIDTH_CANDIDATE {
                let value = lane_value(primary, x, y, lane);

                if x + 1 < MAP_WIDTH_CANDIDATE {
                    let right = lane_value(primary, x + 1, y, lane);
                    self.accumulate_pair(value, right, true);
                }

                if y + 1 < MAP_HEIGHT_CANDIDATE {
                    let down = lane_value(primary, x, y + 1, lane);
                    self.accumulate_pair(value, down, false);
                }

                if x + 1 < MAP_WIDTH_CANDIDATE && y + 1 < MAP_HEIGHT_CANDIDATE {
                    let pattern = [
                        value,
                        lane_value(primary, x + 1, y, lane),
                        lane_value(primary, x, y + 1, lane),
                        lane_value(primary, x + 1, y + 1, lane),
                    ];
                    self.total_2x2_blocks += 1;
                    if pattern.iter().all(|&candidate| candidate == value) {
                        self.uniform_2x2_blocks += 1;
                    }
                    *self.block_frequencies.entry(pattern).or_insert(0) += 1;
                }
            }
        }
    }

    fn accumulate_pair(&mut self, from: u8, to: u8, right_pair: bool) {
        if right_pair {
            self.right_pairs += 1;
            if from == to {
                self.right_same_pairs += 1;
            }
        } else {
            self.down_pairs += 1;
            if from == to {
                self.down_same_pairs += 1;
            }
        }

        if from != to {
            *self.transition_frequencies.entry((from, to)).or_insert(0) += 1;
        }

        let gradient = from.abs_diff(to);
        self.gradient_sum += gradient as usize;
        if gradient <= 1 {
            self.gentle_gradient_pairs += 1;
        }
        if gradient <= 4 {
            self.moderate_gradient_pairs += 1;
        }
        self.max_abs_gradient = self.max_abs_gradient.max(gradient);
    }

    fn to_stats(&self, lane: usize) -> ByteLaneSpatialStats {
        let repeated_2x2_patterns = self
            .block_frequencies
            .values()
            .filter(|&&count| count > 1)
            .count();
        let repeated_2x2_blocks = self
            .block_frequencies
            .values()
            .filter(|&&count| count > 1)
            .sum();
        let neighbour_pairs = self.right_pairs + self.down_pairs;
        let mean_abs_gradient_milli = if neighbour_pairs == 0 {
            0
        } else {
            ((self.gradient_sum * 1000 + neighbour_pairs / 2) / neighbour_pairs) as u32
        };

        ByteLaneSpatialStats {
            lane,
            right_pairs: self.right_pairs,
            down_pairs: self.down_pairs,
            right_same_pairs: self.right_same_pairs,
            down_same_pairs: self.down_same_pairs,
            total_2x2_blocks: self.total_2x2_blocks,
            uniform_2x2_blocks: self.uniform_2x2_blocks,
            repeated_2x2_patterns,
            repeated_2x2_blocks,
            top_transitions: top_transitions(
                &self.transition_frequencies,
                MAP_SPATIAL_TOP_TRANSITIONS,
            ),
            top_2x2_patterns: top_2x2_patterns(&self.block_frequencies, MAP_SPATIAL_TOP_BLOCKS),
            gentle_gradient_pairs: self.gentle_gradient_pairs,
            moderate_gradient_pairs: self.moderate_gradient_pairs,
            max_abs_gradient: self.max_abs_gradient,
            mean_abs_gradient_milli,
        }
    }
}

fn lane_value(primary: &[u8], x: usize, y: usize, lane: usize) -> u8 {
    primary[(y * MAP_WIDTH_CANDIDATE + x) * MAP_CELL_BYTES + lane]
}

fn top_transitions(
    frequencies: &BTreeMap<(u8, u8), usize>,
    limit: usize,
) -> Vec<ByteTransitionFrequency> {
    let mut ranked = frequencies
        .iter()
        .map(|(&(from, to), &count)| ByteTransitionFrequency { from, to, count })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.from.cmp(&right.from))
            .then_with(|| left.to.cmp(&right.to))
    });
    ranked.truncate(limit);
    ranked
}

fn top_2x2_patterns(
    frequencies: &BTreeMap<[u8; 4], usize>,
    limit: usize,
) -> Vec<ByteBlockPatternFrequency> {
    let mut ranked = frequencies
        .iter()
        .map(|(&values, &count)| ByteBlockPatternFrequency { values, count })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.values.cmp(&right.values))
    });
    ranked.truncate(limit);
    ranked
}

fn percent(part: usize, total: usize) -> u8 {
    if total == 0 {
        0
    } else {
        ((part * 100 + total / 2) / total).min(100) as u8
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
        MAP_CELL_BYTES, MAP_CELL_COUNT, MAP_INFERRED_CLASS_BASELINE, MAP_INFERRED_CLASS_DETAIL,
        MAP_INFERRED_CLASS_HEIGHT, MAP_INFERRED_CLASS_MIXED, MAP_INFERRED_CLASS_REFERENCE,
        MAP_INFERRED_CLASS_SURFACE, MAP_PRIMARY_SECTION_LEN, MAP_SIGNATURE_PREVIEW_CLASSES,
        MAP_WIDTH_CANDIDATE, MapCandidateEvidenceConfidence, MapCandidateField, MapCellRecord,
        analyze_payload, analyze_primary_sections,
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
        assert_eq!(grid.byte_stats[0].unique_values, 2);
        assert_eq!(grid.byte_stats[0].zero_values, MAP_CELL_COUNT - 1);
        assert_eq!(grid.byte_stats[0].top_values[0].value, 0);
        assert_eq!(grid.byte_stats[0].top_values[0].count, MAP_CELL_COUNT - 1);
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

        let inferred = analysis.inferred_layer_preview.unwrap();
        assert_eq!(inferred.width, 64);
        assert_eq!(inferred.height, 64);
        assert_eq!(inferred.surface_baseline, 0);
        assert_eq!(
            inferred.class_counts[MAP_INFERRED_CLASS_BASELINE as usize],
            MAP_CELL_COUNT - 1
        );
        assert_eq!(
            inferred.cell(1, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_BASELINE
        );
        assert_eq!(
            inferred.cell(0, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_MIXED
        );
        assert!(inferred.cell(64, 0).is_none());
    }

    #[test]
    fn flags_payloads_below_primary_section_candidate() {
        let analysis = analyze_payload(&[0; 128]);
        assert!(analysis.primary_grid.is_none());
        assert_eq!(analysis.tail.len, 0);
        assert!(analysis.signature_preview.is_none());
        assert!(analysis.inferred_layer_preview.is_none());
        assert!(analysis.spatial_correlation.is_none());
        assert!(analysis.substrate_candidate.is_none());
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

    #[test]
    fn builds_conservative_inferred_layer_from_byte_lanes() {
        let mut payload = vec![0; MAP_PRIMARY_SECTION_LEN];
        set_record(&mut payload, 1, &[9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        set_record(&mut payload, 2, &[0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0]);
        set_record(&mut payload, 3, &[0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0]);
        set_record(&mut payload, 4, &[0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        set_record(&mut payload, 5, &[9, 3, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0]);

        let analysis = analyze_payload(&payload);
        let inferred = analysis.inferred_layer_preview.unwrap();

        assert_eq!(inferred.height_lane, 1);
        assert_eq!(inferred.height_baseline, 0);
        assert_eq!(inferred.surface_unique, 2);
        assert_eq!(inferred.reference_unique, 2);
        assert_eq!(
            inferred.cell(0, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_BASELINE
        );
        assert_eq!(
            inferred.cell(1, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_SURFACE
        );
        assert_eq!(
            inferred.cell(2, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_DETAIL
        );
        assert_eq!(
            inferred.cell(3, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_REFERENCE
        );
        assert_eq!(
            inferred.cell(4, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_HEIGHT
        );
        assert_eq!(inferred.cell(4, 0).unwrap().height_class, 3);
        assert_eq!(
            inferred.cell(5, 0).unwrap().visual_class,
            MAP_INFERRED_CLASS_MIXED
        );
        assert_eq!(inferred.cell(5, 0).unwrap().height_class, 3);
    }

    #[test]
    fn builds_spatial_correlation_diagnostics_for_candidate_lanes() {
        let mut payload = vec![0; MAP_PRIMARY_SECTION_LEN];
        for y in 0..MAP_WIDTH_CANDIDATE {
            for x in 0..MAP_WIDTH_CANDIDATE {
                let start = (y * MAP_WIDTH_CANDIDATE + x) * MAP_CELL_BYTES;
                payload[start] = if x < 32 { 0 } else { 9 };
                payload[start + 1] = y as u8;
            }
        }

        let analysis = analyze_payload(&payload);
        let inferred = analysis.inferred_layer_preview.unwrap();
        assert_eq!(inferred.height_lane, 1);
        assert_eq!(
            inferred.field_value(MapCandidateField::SurfaceIndex, 40, 0),
            Some(9)
        );
        assert_eq!(
            inferred.field_value(MapCandidateField::Height, 0, 7),
            Some(7)
        );
        assert_eq!(
            inferred.field_value(MapCandidateField::Reference, 64, 0),
            None
        );

        let spatial = analysis.spatial_correlation.unwrap();
        assert_eq!(spatial.height_candidate_lane, 1);
        let surface = &spatial.byte_lanes[0];
        assert_eq!(surface.right_pairs, 64 * 63);
        assert_eq!(surface.down_pairs, 64 * 63);
        assert_eq!(surface.right_same_pairs, 64 * 62);
        assert_eq!(surface.down_same_pairs, 64 * 63);
        assert_eq!(surface.continuity_percent(), 99);
        assert_eq!(surface.uniform_2x2_blocks, 63 * 62);
        assert_eq!(surface.top_transitions[0].from, 0);
        assert_eq!(surface.top_transitions[0].to, 9);
        assert_eq!(surface.top_transitions[0].count, 64);

        let height = &spatial.byte_lanes[1];
        assert_eq!(height.right_same_pairs, 64 * 63);
        assert_eq!(height.down_same_pairs, 0);
        assert_eq!(height.continuity_percent(), 50);
        assert_eq!(height.gentle_gradient_percent(), 100);
        assert_eq!(height.moderate_gradient_percent(), 100);
        assert_eq!(height.max_abs_gradient, 1);
        assert_eq!(height.mean_abs_gradient_milli, 500);

        let substrate = analysis.substrate_candidate.unwrap();
        let surface_evidence = substrate
            .evidence_for(MapCandidateField::SurfaceIndex)
            .unwrap();
        let height_evidence = substrate.evidence_for(MapCandidateField::Height).unwrap();
        assert_eq!(
            substrate.field_value(MapCandidateField::SurfaceIndex, 40, 0),
            Some(9)
        );
        assert_eq!(
            substrate.field_value(MapCandidateField::Height, 0, 7),
            Some(7)
        );
        assert_eq!(surface_evidence.lane, 0);
        assert_eq!(surface_evidence.baseline, 0);
        assert_eq!(
            surface_evidence.confidence,
            MapCandidateEvidenceConfidence::High
        );
        assert_eq!(height_evidence.lane, 1);
        assert_eq!(height_evidence.baseline, 0);
        assert_eq!(
            height_evidence.confidence,
            MapCandidateEvidenceConfidence::Medium
        );
    }

    #[test]
    fn aggregates_primary_sections_without_claiming_semantics() {
        let first = vec![0; MAP_PRIMARY_SECTION_LEN];
        let mut second = vec![0; MAP_PRIMARY_SECTION_LEN];
        for chunk in second.chunks_exact_mut(MAP_CELL_BYTES) {
            chunk[0] = 7;
            chunk[4] = 3;
            chunk[8] = 7;
        }

        assert!(analyze_primary_sections([].into_iter()).is_none());
        let aggregate = analyze_primary_sections([first.as_slice(), second.as_slice()]).unwrap();

        assert_eq!(aggregate.map_count, 2);
        assert_eq!(aggregate.total_cells, MAP_CELL_COUNT * 2);
        assert_eq!(aggregate.unique_cells, 2);
        assert_eq!(aggregate.byte_stats[0].unique_values, 2);
        assert_eq!(aggregate.byte_stats[0].top_values[0].value, 0);
        assert_eq!(aggregate.byte_stats[0].top_values[0].count, MAP_CELL_COUNT);
        assert_eq!(aggregate.spatial_correlation.height_candidate_lane, 1);
        assert_eq!(
            aggregate.spatial_correlation.byte_lanes[0].continuity_percent(),
            100
        );
        assert_eq!(
            aggregate.spatial_correlation.byte_lanes[0].repeated_2x2_percent(),
            100
        );
        assert_eq!(
            aggregate.substrate_evidence[0].field,
            MapCandidateField::SurfaceIndex
        );
        assert_eq!(aggregate.substrate_evidence[0].lane, 0);
        assert_eq!(
            aggregate.substrate_evidence[0].confidence,
            MapCandidateEvidenceConfidence::High
        );
    }

    #[test]
    fn classifies_substrate_evidence_without_claiming_semantics() {
        let mut payload = vec![0; MAP_PRIMARY_SECTION_LEN];
        for y in 0..MAP_WIDTH_CANDIDATE {
            for x in 0..MAP_WIDTH_CANDIDATE {
                let start = (y * MAP_WIDTH_CANDIDATE + x) * MAP_CELL_BYTES;
                payload[start] = if x < 32 { 2 } else { 4 };
                payload[start + 4] = if (x + y) % 2 == 0 { 6 } else { 8 };
                payload[start + 8] = if y < 32 { 10 } else { 12 };
                payload[start + 1] = (y / 16) as u8;
            }
        }

        let analysis = analyze_payload(&payload);
        let substrate = analysis.substrate_candidate.unwrap();
        let surface = substrate
            .evidence_for(MapCandidateField::SurfaceIndex)
            .unwrap();
        let detail = substrate
            .evidence_for(MapCandidateField::DetailIndex)
            .unwrap();
        let reference = substrate
            .evidence_for(MapCandidateField::Reference)
            .unwrap();
        let height = substrate.evidence_for(MapCandidateField::Height).unwrap();

        assert_eq!(surface.confidence, MapCandidateEvidenceConfidence::High);
        assert_eq!(detail.confidence, MapCandidateEvidenceConfidence::Low);
        assert_eq!(reference.confidence, MapCandidateEvidenceConfidence::High);
        assert_eq!(height.confidence, MapCandidateEvidenceConfidence::High);
        assert!(
            surface
                .evidence_label()
                .contains("surface_index_candidate evidence")
        );
    }

    fn set_record(payload: &mut [u8], cell_index: usize, record: &[u8; MAP_CELL_BYTES]) {
        let start = cell_index * MAP_CELL_BYTES;
        payload[start..start + MAP_CELL_BYTES].copy_from_slice(record);
    }
}
