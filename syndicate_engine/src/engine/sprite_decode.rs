//! Conservative sprite-bank chunk analysis.
//!
//! The original Bullfrog asset formats vary between banks, so this module does
//! not claim to fully decode sprites yet. It classifies byte chunks and extracts
//! plausible metadata that can guide reverse engineering and future renderers.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteChunkKind {
    Empty,
    LikelyRawIndexed,
    LikelyRleOrCommandStream,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteChunkSizeBucket {
    Empty,
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteChunkHeaderShape {
    Empty,
    StartsWithZeroCandidate,
    StartsWithHighByteCandidate,
    CompactPairCandidate,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteDistributionSummary {
    pub min: u32,
    pub median: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteKindAggregate {
    pub kind: SpriteChunkKind,
    pub count: usize,
    pub zero_ratio_per_mille: SpriteDistributionSummary,
    pub high_byte_ratio_per_mille: SpriteDistributionSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteKindBySizeBucket {
    pub bucket: SpriteChunkSizeBucket,
    pub kind_counts: Vec<SpriteKindCount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteKindCount {
    pub kind: SpriteChunkKind,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteHeaderShapeCount {
    pub shape: SpriteChunkHeaderShape,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteMetadataShapeKind {
    LeadingU8Dimensions,
    LeadingLeU16Dimensions,
    LeadingLeU16Offsets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteMetadataShapeProbe {
    pub kind: SpriteMetadataShapeKind,
    pub support_count: usize,
    pub first_value: SpriteDistributionSummary,
    pub second_value: SpriteDistributionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteSizeBandCounts {
    pub small: usize,
    pub medium: usize,
    pub large: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteBankAggregateSummary {
    pub chunk_count: usize,
    pub size_band_counts: SpriteSizeBandCounts,
    pub kind_aggregates: Vec<SpriteKindAggregate>,
    pub kind_by_size_bucket: Vec<SpriteKindBySizeBucket>,
    pub header_shape_counts: Vec<SpriteHeaderShapeCount>,
    pub metadata_shape_probes: Vec<SpriteMetadataShapeProbe>,
}

impl SpriteChunkKind {
    pub fn conservative_label(self) -> &'static str {
        match self {
            Self::Empty => "empty chunk candidate",
            Self::LikelyRawIndexed => "likely raw indexed chunk candidate",
            Self::LikelyRleOrCommandStream => "likely RLE/command-stream chunk candidate",
            Self::Unknown => "unknown chunk candidate",
        }
    }
}

impl SpriteChunkSizeBucket {
    pub fn for_len(len: usize) -> Self {
        match len {
            0 => Self::Empty,
            1..=31 => Self::Tiny,
            32..=127 => Self::Small,
            128..=511 => Self::Medium,
            512..=2047 => Self::Large,
            _ => Self::Huge,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Tiny => "1..31 bytes",
            Self::Small => "32..127 bytes",
            Self::Medium => "128..511 bytes",
            Self::Large => "512..2047 bytes",
            Self::Huge => ">=2048 bytes",
        }
    }
}

impl SpriteChunkHeaderShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty chunk candidate",
            Self::StartsWithZeroCandidate => "leading-zero header candidate",
            Self::StartsWithHighByteCandidate => "leading-high-byte command candidate",
            Self::CompactPairCandidate => "compact leading-pair metadata candidate",
            Self::Other => "other leading-byte pattern",
        }
    }
}

impl SpriteMetadataShapeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::LeadingU8Dimensions => "candidate leading u8 width/height range",
            Self::LeadingLeU16Dimensions => "candidate leading le-u16 width/height range",
            Self::LeadingLeU16Offsets => "candidate leading le-u16 offset-pair range",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteChunkInfo {
    pub len: usize,
    pub kind: SpriteChunkKind,
    pub zeroes: usize,
    pub high_bytes: usize,
    pub header_shape: SpriteChunkHeaderShape,
    pub first_bytes: [u8; 8],
}

impl SpriteChunkInfo {
    pub fn inspect(chunk: &[u8]) -> Self {
        let mut first_bytes = [0; 8];
        for (dst, src) in first_bytes.iter_mut().zip(chunk.iter().copied()) {
            *dst = src;
        }

        let zeroes = chunk.iter().filter(|&&byte| byte == 0).count();
        let high_bytes = chunk.iter().filter(|&&byte| byte >= 0xf0).count();
        let kind = classify(chunk, zeroes, high_bytes);
        let header_shape = classify_header_shape(chunk);

        Self {
            len: chunk.len(),
            kind,
            zeroes,
            high_bytes,
            header_shape,
            first_bytes,
        }
    }

    pub fn short_label(&self) -> String {
        format!(
            "{:?}, len {}, z {}, hi {}, head {:02x} {:02x} {:02x} {:02x}",
            self.kind,
            self.len,
            self.zeroes,
            self.high_bytes,
            self.first_bytes[0],
            self.first_bytes[1],
            self.first_bytes[2],
            self.first_bytes[3]
        )
    }
}

impl SpriteBankAggregateSummary {
    pub fn from_chunks<'a>(chunks: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let infos = chunks
            .into_iter()
            .map(SpriteChunkInfo::inspect)
            .collect::<Vec<_>>();
        Self::from_chunk_infos(&infos)
    }

    pub fn from_chunk_infos(infos: &[SpriteChunkInfo]) -> Self {
        let mut by_kind: BTreeMap<SpriteChunkKind, Vec<&SpriteChunkInfo>> = BTreeMap::new();
        let mut by_bucket: BTreeMap<SpriteChunkSizeBucket, BTreeMap<SpriteChunkKind, usize>> =
            BTreeMap::new();
        let mut by_shape: BTreeMap<SpriteChunkHeaderShape, usize> = BTreeMap::new();
        let mut size_band_counts = SpriteSizeBandCounts {
            small: 0,
            medium: 0,
            large: 0,
        };

        for info in infos {
            by_kind.entry(info.kind).or_default().push(info);
            *by_bucket
                .entry(SpriteChunkSizeBucket::for_len(info.len))
                .or_default()
                .entry(info.kind)
                .or_default() += 1;
            *by_shape.entry(info.header_shape).or_default() += 1;

            match info.len {
                0..=31 => size_band_counts.small += 1,
                32..=511 => size_band_counts.medium += 1,
                _ => size_band_counts.large += 1,
            }
        }

        let kind_aggregates = by_kind
            .into_iter()
            .map(|(kind, infos)| SpriteKindAggregate {
                kind,
                count: infos.len(),
                zero_ratio_per_mille: distribution_summary(
                    infos
                        .iter()
                        .map(|info| ratio_per_mille(info.zeroes, info.len)),
                ),
                high_byte_ratio_per_mille: distribution_summary(
                    infos
                        .iter()
                        .map(|info| ratio_per_mille(info.high_bytes, info.len)),
                ),
            })
            .collect();

        let kind_by_size_bucket = by_bucket
            .into_iter()
            .map(|(bucket, counts)| SpriteKindBySizeBucket {
                bucket,
                kind_counts: counts
                    .into_iter()
                    .map(|(kind, count)| SpriteKindCount { kind, count })
                    .collect(),
            })
            .collect();

        let header_shape_counts = by_shape
            .into_iter()
            .map(|(shape, count)| SpriteHeaderShapeCount { shape, count })
            .collect();

        Self {
            chunk_count: infos.len(),
            size_band_counts,
            kind_aggregates,
            kind_by_size_bucket,
            header_shape_counts,
            metadata_shape_probes: probe_metadata_shapes(infos),
        }
    }
}

fn probe_metadata_shapes(infos: &[SpriteChunkInfo]) -> Vec<SpriteMetadataShapeProbe> {
    [
        SpriteMetadataShapeKind::LeadingU8Dimensions,
        SpriteMetadataShapeKind::LeadingLeU16Dimensions,
        SpriteMetadataShapeKind::LeadingLeU16Offsets,
    ]
    .into_iter()
    .filter_map(|kind| probe_metadata_shape(kind, infos))
    .collect()
}

fn probe_metadata_shape(
    kind: SpriteMetadataShapeKind,
    infos: &[SpriteChunkInfo],
) -> Option<SpriteMetadataShapeProbe> {
    let values = infos
        .iter()
        .filter_map(|info| metadata_shape_values(kind, info))
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| SpriteMetadataShapeProbe {
        kind,
        support_count: values.len(),
        first_value: distribution_summary(values.iter().map(|(first, _)| *first)),
        second_value: distribution_summary(values.iter().map(|(_, second)| *second)),
    })
}

fn metadata_shape_values(
    kind: SpriteMetadataShapeKind,
    info: &SpriteChunkInfo,
) -> Option<(u32, u32)> {
    match kind {
        SpriteMetadataShapeKind::LeadingU8Dimensions => {
            if info.len < 2 {
                return None;
            }
            let width = info.first_bytes[0] as u32;
            let height = info.first_bytes[1] as u32;
            let area = width.saturating_mul(height);
            ((1..=128).contains(&width)
                && (1..=128).contains(&height)
                && area <= info.len.saturating_mul(4) as u32)
                .then_some((width, height))
        }
        SpriteMetadataShapeKind::LeadingLeU16Dimensions => {
            if info.len < 4 {
                return None;
            }
            let width = u16::from_le_bytes([info.first_bytes[0], info.first_bytes[1]]) as u32;
            let height = u16::from_le_bytes([info.first_bytes[2], info.first_bytes[3]]) as u32;
            let area = width.saturating_mul(height);
            ((1..=512).contains(&width)
                && (1..=512).contains(&height)
                && area <= info.len.saturating_mul(8) as u32)
                .then_some((width, height))
        }
        SpriteMetadataShapeKind::LeadingLeU16Offsets => {
            if info.len < 4 {
                return None;
            }
            let first = u16::from_le_bytes([info.first_bytes[0], info.first_bytes[1]]) as u32;
            let second = u16::from_le_bytes([info.first_bytes[2], info.first_bytes[3]]) as u32;
            (first <= info.len as u32 && second <= info.len as u32).then_some((first, second))
        }
    }
}

fn classify(chunk: &[u8], zeroes: usize, high_bytes: usize) -> SpriteChunkKind {
    if chunk.is_empty() {
        return SpriteChunkKind::Empty;
    }

    let len = chunk.len();
    let zero_ratio = zeroes as f32 / len as f32;
    let high_ratio = high_bytes as f32 / len as f32;

    if len >= 64 && zero_ratio < 0.08 && high_ratio < 0.15 {
        SpriteChunkKind::LikelyRawIndexed
    } else if len >= 8 && (high_ratio >= 0.15 || zero_ratio >= 0.2) {
        SpriteChunkKind::LikelyRleOrCommandStream
    } else {
        SpriteChunkKind::Unknown
    }
}

fn classify_header_shape(chunk: &[u8]) -> SpriteChunkHeaderShape {
    let Some(&first) = chunk.first() else {
        return SpriteChunkHeaderShape::Empty;
    };
    if first == 0 {
        return SpriteChunkHeaderShape::StartsWithZeroCandidate;
    }
    if first >= 0xf0 {
        return SpriteChunkHeaderShape::StartsWithHighByteCandidate;
    }
    if chunk
        .get(1)
        .is_some_and(|&second| (1..=64).contains(&first) && (1..=64).contains(&second))
    {
        return SpriteChunkHeaderShape::CompactPairCandidate;
    }
    SpriteChunkHeaderShape::Other
}

fn ratio_per_mille(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator * 1000 + denominator / 2) / denominator) as u32
}

fn distribution_summary(values: impl IntoIterator<Item = u32>) -> SpriteDistributionSummary {
    let mut values = values.into_iter().collect::<Vec<_>>();
    if values.is_empty() {
        return SpriteDistributionSummary {
            min: 0,
            median: 0,
            max: 0,
        };
    }
    values.sort_unstable();
    SpriteDistributionSummary {
        min: values[0],
        median: values[values.len() / 2],
        max: values[values.len() - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SpriteBankAggregateSummary, SpriteChunkHeaderShape, SpriteChunkInfo, SpriteChunkKind,
    };

    #[test]
    fn classifies_empty_chunk() {
        let info = SpriteChunkInfo::inspect(&[]);
        assert_eq!(info.kind, SpriteChunkKind::Empty);
        assert_eq!(info.len, 0);
    }

    #[test]
    fn classifies_raw_indexed_like_chunk() {
        let chunk = (1..=80).collect::<Vec<u8>>();
        let info = SpriteChunkInfo::inspect(&chunk);
        assert_eq!(info.kind, SpriteChunkKind::LikelyRawIndexed);
        assert_eq!(info.first_bytes[..4], [1, 2, 3, 4]);
    }

    #[test]
    fn classifies_command_stream_like_chunk() {
        let chunk = [0xff, 0x00, 0xfe, 0x00, 0x10, 0x00, 0xf8, 0x01];
        let info = SpriteChunkInfo::inspect(&chunk);
        assert_eq!(info.kind, SpriteChunkKind::LikelyRleOrCommandStream);
        assert_eq!(
            info.header_shape,
            SpriteChunkHeaderShape::StartsWithHighByteCandidate
        );
    }

    #[test]
    fn summarizes_sprite_bank_aggregates_without_bytes() {
        let raw = (1..=80).collect::<Vec<u8>>();
        let command = [0xff, 0x00, 0xfe, 0x00, 0x10, 0x00, 0xf8, 0x01];
        let compact = [8, 16, 1, 2, 3, 4, 5, 6];
        let chunks = [raw.as_slice(), command.as_slice(), compact.as_slice()];
        let summary = SpriteBankAggregateSummary::from_chunks(chunks);

        assert_eq!(summary.chunk_count, 3);
        assert_eq!(summary.size_band_counts.small, 2);
        assert_eq!(summary.size_band_counts.medium, 1);
        assert!(
            summary
                .kind_aggregates
                .iter()
                .any(
                    |aggregate| aggregate.kind == SpriteChunkKind::LikelyRawIndexed
                        && aggregate.zero_ratio_per_mille.max == 0
                )
        );
        assert!(summary.header_shape_counts.iter().any(|entry| {
            entry.shape == SpriteChunkHeaderShape::CompactPairCandidate && entry.count >= 1
        }));
        assert!(summary.kind_by_size_bucket.iter().any(|bucket| {
            bucket
                .kind_counts
                .iter()
                .map(|entry| entry.count)
                .sum::<usize>()
                > 0
        }));
        assert!(!summary.metadata_shape_probes.is_empty());
    }

    #[test]
    fn probes_candidate_metadata_shapes_without_exposing_header_bytes() {
        let mut chunk_a = vec![1; 128];
        chunk_a[..4].copy_from_slice(&[16, 24, 8, 0]);
        let mut chunk_b = vec![2; 256];
        chunk_b[..4].copy_from_slice(&[32, 32, 10, 0]);
        let chunks = [chunk_a.as_slice(), chunk_b.as_slice()];
        let summary = SpriteBankAggregateSummary::from_chunks(chunks);

        let u8_dims = summary
            .metadata_shape_probes
            .iter()
            .find(|probe| probe.kind == super::SpriteMetadataShapeKind::LeadingU8Dimensions)
            .unwrap();
        assert_eq!(u8_dims.support_count, 2);
        assert_eq!(u8_dims.first_value.min, 16);
        assert_eq!(u8_dims.second_value.max, 32);
    }
}
