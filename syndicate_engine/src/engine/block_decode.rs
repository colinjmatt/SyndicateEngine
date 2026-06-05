//! Conservative diagnostics for block/tile-like graphics containers.
//!
//! These helpers intentionally avoid decoding or previewing copyrighted pixels.
//! They only report container status, decoded lengths, aggregate byte statistics,
//! and fixed-size record-count plausibility useful for correlating MAP candidate
//! fields with possible graphics banks.

use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use crate::engine::rnc::RncBlock;

pub const BLOCK_TILE_DIMENSIONS: [(usize, usize); 6] =
    [(8, 8), (16, 16), (32, 16), (32, 32), (64, 32), (64, 64)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockGraphicsAnalysis {
    pub container: BlockGraphicsContainer,
    pub decoded_len: usize,
    pub decoded_hash: u64,
    pub byte_summary: BlockByteSummary,
    pub record_candidates: Vec<BlockRecordCandidate>,
    pub layout_probes: Vec<BlockLayoutProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockGraphicsContainer {
    Plain,
    RncVerified {
        method: u8,
        packed_len: u32,
        unpacked_len: u32,
        block_count: u8,
    },
    RncDecodeError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockByteSummary {
    pub zero_values: usize,
    pub unique_values: usize,
    pub entropy_milli_bits: u32,
    pub dominant_value_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRecordCandidate {
    pub width: usize,
    pub height: usize,
    pub bytes_per_record: usize,
    pub record_count: usize,
    pub remainder: usize,
    pub palette_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLayoutProbe {
    pub width: usize,
    pub height: usize,
    pub bytes_per_record: usize,
    pub alignment: BlockRecordAlignment,
    pub complete_records: usize,
    pub duplicate_records: usize,
    pub all_zero_records: usize,
    pub record_zero_percent: BlockDistributionSummary,
    pub record_unique_values: BlockDistributionSummary,
    pub record_entropy_milli_bits: BlockDistributionSummary,
    pub leading_region_hint: Option<BlockRegionHint>,
    pub trailing_region_hint: Option<BlockRegionHint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockRecordAlignment {
    Exact,
    LeadingOffset { bytes: usize },
    TrailingRemainder { bytes: usize },
    Unaligned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDistributionSummary {
    pub min: u32,
    pub median: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRegionHint {
    pub bytes: usize,
    pub entropy_milli_bits: u32,
    pub zero_percent: u8,
    pub unique_values: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLayoutAlignmentSupport {
    pub label: BlockLayoutAlignmentLabel,
    pub support_count: usize,
    pub dimensions: Vec<(usize, usize)>,
    pub max_complete_records: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlockLayoutAlignmentLabel {
    Exact,
    LeadingOffset { bytes: usize },
    TrailingRemainder { bytes: usize },
    Unaligned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockIndexPlausibility {
    FitsRecordCount,
    FitsByteRangeOnly,
    OutOfRange,
    Unknown,
}

impl BlockGraphicsAnalysis {
    pub fn analyze_file_bytes(data: &[u8]) -> Self {
        let (container, decoded) = if let Some(block) = RncBlock::parse(data) {
            match block.decompress() {
                Ok(decoded) => (
                    BlockGraphicsContainer::RncVerified {
                        method: block.header.method,
                        packed_len: block.header.packed_len,
                        unpacked_len: block.header.unpacked_len,
                        block_count: block.header.block_count,
                    },
                    decoded,
                ),
                Err(err) => (
                    BlockGraphicsContainer::RncDecodeError(format!("{err:?}")),
                    Vec::new(),
                ),
            }
        } else {
            (BlockGraphicsContainer::Plain, data.to_vec())
        };

        Self::analyze_decoded(container, decoded)
    }

    pub fn analyze_decoded(container: BlockGraphicsContainer, decoded: Vec<u8>) -> Self {
        let decoded_len = decoded.len();
        Self {
            container,
            decoded_len,
            decoded_hash: hash_bytes(&decoded),
            byte_summary: summarize_bytes(&decoded),
            record_candidates: candidate_records(decoded_len),
            layout_probes: probe_layouts(&decoded),
        }
    }

    pub fn container_label(&self) -> String {
        match &self.container {
            BlockGraphicsContainer::Plain => "plain/unknown".to_string(),
            BlockGraphicsContainer::RncVerified {
                method,
                packed_len,
                unpacked_len,
                block_count,
            } => format!(
                "RNC method {method} verified, packed {packed_len}, unpacked {unpacked_len}, {block_count} blocks"
            ),
            BlockGraphicsContainer::RncDecodeError(err) => format!("RNC decode error {err}"),
        }
    }

    pub fn best_aligned_record_candidate(&self) -> Option<BlockRecordCandidate> {
        self.record_candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.remainder == 0)
            .max_by_key(|candidate| (candidate.record_count, candidate.bytes_per_record))
    }

    pub fn best_layout_probe(&self) -> Option<&BlockLayoutProbe> {
        self.layout_probes.iter().max_by_key(|probe| {
            let alignment_score = match probe.alignment {
                BlockRecordAlignment::Exact => 3,
                BlockRecordAlignment::LeadingOffset { .. } => 2,
                BlockRecordAlignment::TrailingRemainder { .. } => 1,
                BlockRecordAlignment::Unaligned => 0,
            };
            (
                alignment_score,
                probe.complete_records,
                probe.bytes_per_record,
            )
        })
    }

    pub fn layout_alignment_supports(&self) -> Vec<BlockLayoutAlignmentSupport> {
        summarize_layout_alignment_supports(&self.layout_probes)
    }
}

impl BlockRecordCandidate {
    pub fn label(self) -> String {
        let alignment = if self.remainder == 0 {
            "aligned".to_string()
        } else {
            format!("remainder {}", self.remainder)
        };
        format!(
            "{}x{} indexed-pixel candidate: {} records ({alignment})",
            self.width, self.height, self.record_count
        )
    }
}

impl BlockLayoutProbe {
    pub fn label(&self) -> String {
        format!(
            "{}x{}: {}; {} duplicate records, zero-records {}, entropy min/med/max {:.3}/{:.3}/{:.3}",
            self.width,
            self.height,
            self.alignment.label(),
            self.duplicate_records,
            self.all_zero_records,
            self.record_entropy_milli_bits.min as f32 / 1000.0,
            self.record_entropy_milli_bits.median as f32 / 1000.0,
            self.record_entropy_milli_bits.max as f32 / 1000.0
        )
    }
}

impl BlockRecordAlignment {
    pub fn label(self) -> String {
        match self {
            Self::Exact => "exact alignment".to_string(),
            Self::LeadingOffset { bytes } => {
                format!("candidate leading table/header {bytes} bytes")
            }
            Self::TrailingRemainder { bytes } => {
                format!("candidate trailing remainder {bytes} bytes")
            }
            Self::Unaligned => "unaligned".to_string(),
        }
    }
}

impl BlockLayoutAlignmentLabel {
    pub fn label(self) -> String {
        match self {
            Self::Exact => "exact alignment".to_string(),
            Self::LeadingOffset { bytes } => {
                format!("candidate leading table/header {bytes} bytes")
            }
            Self::TrailingRemainder { bytes } => {
                format!("candidate trailing remainder {bytes} bytes")
            }
            Self::Unaligned => "unaligned".to_string(),
        }
    }
}

impl From<BlockRecordAlignment> for BlockLayoutAlignmentLabel {
    fn from(alignment: BlockRecordAlignment) -> Self {
        match alignment {
            BlockRecordAlignment::Exact => Self::Exact,
            BlockRecordAlignment::LeadingOffset { bytes } => Self::LeadingOffset { bytes },
            BlockRecordAlignment::TrailingRemainder { bytes } => Self::TrailingRemainder { bytes },
            BlockRecordAlignment::Unaligned => Self::Unaligned,
        }
    }
}

impl BlockIndexPlausibility {
    pub fn label(self) -> &'static str {
        match self {
            Self::FitsRecordCount => "fits candidate record count",
            Self::FitsByteRangeOnly => "fits byte-sized MAP value range only",
            Self::OutOfRange => "out of candidate range",
            Self::Unknown => "unknown",
        }
    }
}

pub fn correlate_map_value_range(
    min_value: u8,
    max_value: u8,
    record_count: Option<usize>,
) -> BlockIndexPlausibility {
    if max_value < min_value {
        return BlockIndexPlausibility::Unknown;
    }
    if let Some(record_count) = record_count {
        if record_count > max_value as usize {
            BlockIndexPlausibility::FitsRecordCount
        } else {
            BlockIndexPlausibility::OutOfRange
        }
    } else if max_value <= u8::MAX {
        BlockIndexPlausibility::FitsByteRangeOnly
    } else {
        BlockIndexPlausibility::Unknown
    }
}

pub fn summarize_layout_alignment_supports(
    probes: &[BlockLayoutProbe],
) -> Vec<BlockLayoutAlignmentSupport> {
    let mut grouped: BTreeMap<BlockLayoutAlignmentLabel, BlockLayoutAlignmentSupport> =
        BTreeMap::new();

    for probe in probes {
        let label = BlockLayoutAlignmentLabel::from(probe.alignment);
        let entry = grouped
            .entry(label)
            .or_insert_with(|| BlockLayoutAlignmentSupport {
                label,
                support_count: 0,
                dimensions: Vec::new(),
                max_complete_records: 0,
            });
        entry.support_count += 1;
        entry.dimensions.push((probe.width, probe.height));
        entry.max_complete_records = entry.max_complete_records.max(probe.complete_records);
    }

    let mut supports = grouped.into_values().collect::<Vec<_>>();
    supports.sort_by(|left, right| {
        right
            .support_count
            .cmp(&left.support_count)
            .then_with(|| right.max_complete_records.cmp(&left.max_complete_records))
            .then_with(|| left.label.cmp(&right.label))
    });
    supports
}

pub fn hash_bytes(data: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

fn summarize_bytes(data: &[u8]) -> BlockByteSummary {
    let mut frequencies = [0usize; 256];
    for &byte in data {
        frequencies[byte as usize] += 1;
    }
    let unique_values = frequencies.iter().filter(|&&count| count > 0).count();
    let dominant_value_count = frequencies.iter().copied().max().unwrap_or(0);
    let entropy_milli_bits = entropy_milli_bits(&frequencies, data.len());

    BlockByteSummary {
        zero_values: frequencies[0],
        unique_values,
        entropy_milli_bits,
        dominant_value_count,
    }
}

fn entropy_milli_bits(frequencies: &[usize; 256], total: usize) -> u32 {
    if total == 0 {
        return 0;
    }

    let entropy = frequencies
        .iter()
        .copied()
        .filter(|&count| count > 0)
        .map(|count| {
            let probability = count as f64 / total as f64;
            -probability * probability.log2()
        })
        .sum::<f64>();
    (entropy * 1000.0).round() as u32
}

fn candidate_records(decoded_len: usize) -> Vec<BlockRecordCandidate> {
    BLOCK_TILE_DIMENSIONS
        .into_iter()
        .map(|(width, height)| {
            let bytes_per_record = width * height;
            BlockRecordCandidate {
                width,
                height,
                bytes_per_record,
                record_count: decoded_len / bytes_per_record,
                remainder: decoded_len % bytes_per_record,
                palette_compatible: true,
            }
        })
        .collect()
}

fn probe_layouts(data: &[u8]) -> Vec<BlockLayoutProbe> {
    BLOCK_TILE_DIMENSIONS
        .into_iter()
        .filter_map(|(width, height)| probe_layout(data, width, height))
        .collect()
}

fn probe_layout(data: &[u8], width: usize, height: usize) -> Option<BlockLayoutProbe> {
    let bytes_per_record = width * height;
    if bytes_per_record == 0 || data.len() < bytes_per_record {
        return None;
    }

    let alignment = classify_alignment(data.len(), bytes_per_record);
    let start = match alignment {
        BlockRecordAlignment::LeadingOffset { bytes } => bytes,
        _ => 0,
    };
    let analyzable_len = data.len().saturating_sub(start);
    let complete_records = analyzable_len / bytes_per_record;
    if complete_records == 0 {
        return None;
    }
    let record_bytes = &data[start..start + complete_records * bytes_per_record];

    let mut hashes = BTreeMap::new();
    let mut all_zero_records = 0;
    let mut zero_percents = Vec::with_capacity(complete_records);
    let mut unique_values = Vec::with_capacity(complete_records);
    let mut entropy_values = Vec::with_capacity(complete_records);

    for record in record_bytes.chunks_exact(bytes_per_record) {
        let mut hasher = DefaultHasher::new();
        record.hash(&mut hasher);
        *hashes.entry(hasher.finish()).or_insert(0usize) += 1;

        let summary = summarize_bytes(record);
        if summary.zero_values == record.len() {
            all_zero_records += 1;
        }
        zero_percents.push(percent_u32(summary.zero_values, record.len()));
        unique_values.push(summary.unique_values as u32);
        entropy_values.push(summary.entropy_milli_bits);
    }

    let duplicate_records = hashes.values().map(|&count| count.saturating_sub(1)).sum();
    let leading_region_hint = match alignment {
        BlockRecordAlignment::LeadingOffset { bytes } => summarize_region_hint(&data[..bytes]),
        _ => None,
    };
    let trailing_bytes = data.len() - (start + complete_records * bytes_per_record);
    let trailing_region_hint = if trailing_bytes > 0 {
        summarize_region_hint(&data[data.len() - trailing_bytes..])
    } else {
        None
    };

    Some(BlockLayoutProbe {
        width,
        height,
        bytes_per_record,
        alignment,
        complete_records,
        duplicate_records,
        all_zero_records,
        record_zero_percent: distribution_summary(zero_percents),
        record_unique_values: distribution_summary(unique_values),
        record_entropy_milli_bits: distribution_summary(entropy_values),
        leading_region_hint,
        trailing_region_hint,
    })
}

fn classify_alignment(decoded_len: usize, bytes_per_record: usize) -> BlockRecordAlignment {
    let remainder = decoded_len % bytes_per_record;
    if remainder == 0 {
        return BlockRecordAlignment::Exact;
    }

    const COMMON_TABLE_ALIGNMENTS: [usize; 6] = [2, 4, 16, 32, 38, 128];
    if let Some(bytes) = COMMON_TABLE_ALIGNMENTS
        .into_iter()
        .find(|&offset| decoded_len > offset && (decoded_len - offset) % bytes_per_record == 0)
    {
        return BlockRecordAlignment::LeadingOffset { bytes };
    }

    if remainder <= 512 && decoded_len.saturating_sub(remainder) >= bytes_per_record {
        return BlockRecordAlignment::TrailingRemainder { bytes: remainder };
    }
    BlockRecordAlignment::Unaligned
}

fn summarize_region_hint(data: &[u8]) -> Option<BlockRegionHint> {
    if data.is_empty() {
        return None;
    }
    let summary = summarize_bytes(data);
    Some(BlockRegionHint {
        bytes: data.len(),
        entropy_milli_bits: summary.entropy_milli_bits,
        zero_percent: percent_u32(summary.zero_values, data.len()) as u8,
        unique_values: summary.unique_values,
    })
}

fn distribution_summary(mut values: Vec<u32>) -> BlockDistributionSummary {
    if values.is_empty() {
        return BlockDistributionSummary {
            min: 0,
            median: 0,
            max: 0,
        };
    }
    values.sort_unstable();
    BlockDistributionSummary {
        min: values[0],
        median: values[values.len() / 2],
        max: values[values.len() - 1],
    }
}

fn percent_u32(part: usize, total: usize) -> u32 {
    if total == 0 {
        0
    } else {
        ((part * 100 + total / 2) / total) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BlockGraphicsAnalysis, BlockGraphicsContainer, BlockIndexPlausibility,
        BlockLayoutAlignmentLabel, BlockRecordAlignment, correlate_map_value_range,
    };

    #[test]
    fn reports_fixed_size_record_candidates_without_pixel_preview() {
        let decoded = vec![0u8; 140_800];
        let analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, decoded);

        assert_eq!(analysis.decoded_len, 140_800);
        assert_ne!(analysis.decoded_hash, 0);
        assert_eq!(analysis.byte_summary.zero_values, 140_800);
        assert_eq!(analysis.byte_summary.unique_values, 1);
        assert!(
            analysis
                .record_candidates
                .iter()
                .any(|candidate| candidate.width == 16
                    && candidate.height == 16
                    && candidate.record_count == 550
                    && candidate.remainder == 0)
        );
    }

    #[test]
    fn correlates_map_candidate_ranges_to_record_counts_conservatively() {
        assert_eq!(
            correlate_map_value_range(0, 252, Some(550)),
            BlockIndexPlausibility::FitsRecordCount
        );
        assert_eq!(
            correlate_map_value_range(0, 252, Some(128)),
            BlockIndexPlausibility::OutOfRange
        );
        assert_eq!(
            correlate_map_value_range(0, 252, None),
            BlockIndexPlausibility::FitsByteRangeOnly
        );
    }

    #[test]
    fn probes_record_entropy_duplicates_and_alignment_without_previews() {
        let mut decoded = Vec::new();
        decoded.extend_from_slice(&[7, 7, 7, 7]);
        decoded.extend_from_slice(&[0u8; 64]);
        decoded.extend_from_slice(&[0u8; 64]);
        decoded.extend(0u8..64u8);

        let analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, decoded);
        let probe = analysis
            .layout_probes
            .iter()
            .find(|probe| probe.width == 8 && probe.height == 8)
            .unwrap();

        assert_eq!(
            probe.alignment,
            BlockRecordAlignment::LeadingOffset { bytes: 4 }
        );
        assert_eq!(probe.complete_records, 3);
        assert_eq!(probe.duplicate_records, 1);
        assert_eq!(probe.all_zero_records, 2);
        assert_eq!(probe.leading_region_hint.unwrap().bytes, 4);
        assert!(probe.record_entropy_milli_bits.max > probe.record_entropy_milli_bits.min);
    }

    #[test]
    fn ranks_layout_alignment_supports_by_aggregate_probe_count() {
        let mut decoded = Vec::new();
        decoded.extend_from_slice(&[1u8; 38]);
        decoded.extend_from_slice(&[2u8; 256]);
        decoded.extend_from_slice(&[3u8; 256]);

        let analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, decoded);
        let supports = analysis.layout_alignment_supports();

        let leading = supports
            .iter()
            .find(|support| support.label == BlockLayoutAlignmentLabel::LeadingOffset { bytes: 38 })
            .unwrap();
        assert!(leading.support_count >= 2);
        assert!(leading.dimensions.contains(&(8, 8)));
        assert!(leading.dimensions.contains(&(16, 16)));
    }
}
