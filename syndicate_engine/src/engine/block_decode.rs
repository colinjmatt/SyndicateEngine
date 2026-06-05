//! Conservative diagnostics for block/tile-like graphics containers.
//!
//! These helpers intentionally avoid decoding or previewing copyrighted pixels.
//! They only report container status, decoded lengths, aggregate byte statistics,
//! and fixed-size record-count plausibility useful for correlating MAP candidate
//! fields with possible graphics banks.

use crate::engine::rnc::RncBlock;

pub const BLOCK_TILE_DIMENSIONS: [(usize, usize); 6] =
    [(8, 8), (16, 16), (32, 16), (32, 32), (64, 32), (64, 64)];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockGraphicsAnalysis {
    pub container: BlockGraphicsContainer,
    pub decoded_len: usize,
    pub byte_summary: BlockByteSummary,
    pub record_candidates: Vec<BlockRecordCandidate>,
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
            byte_summary: summarize_bytes(&decoded),
            record_candidates: candidate_records(decoded_len),
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

#[cfg(test)]
mod tests {
    use super::{
        BlockGraphicsAnalysis, BlockGraphicsContainer, BlockIndexPlausibility,
        correlate_map_value_range,
    };

    #[test]
    fn reports_fixed_size_record_candidates_without_pixel_preview() {
        let decoded = vec![0u8; 140_800];
        let analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, decoded);

        assert_eq!(analysis.decoded_len, 140_800);
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
}
