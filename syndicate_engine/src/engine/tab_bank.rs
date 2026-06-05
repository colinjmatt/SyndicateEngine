//! Index parser for Bullfrog-style `.TAB` files paired with `.DAT` banks.

use crate::engine::binary::BinaryReader;
use crate::engine::sprite_decode::{SpriteBankAggregateSummary, SpriteChunkInfo, SpriteChunkKind};

use std::collections::BTreeMap;

pub const CANDIDATE_CHUNK_BYTE_SIZES: [usize; 6] = [64, 256, 512, 1024, 2048, 4096];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankEntry {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBank {
    pub entries: Vec<BankEntry>,
    pub dat_len: usize,
    pub duplicate_offset_count: usize,
    pub zero_len_chunk_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabArchive {
    pub bank: TabBank,
    dat: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBankSummary {
    pub chunk_count: usize,
    pub dat_len: usize,
    pub min_chunk_len: u32,
    pub median_chunk_len: u32,
    pub max_chunk_len: u32,
    pub zero_len_chunks: usize,
    pub duplicate_offset_count: usize,
    pub first_offset: Option<u32>,
    pub last_offset: Option<u32>,
    pub chunk_len_entropy_milli_bits: u32,
    pub common_chunk_len_buckets: Vec<TabChunkLenBucket>,
    pub exact_candidate_size_matches: Vec<TabCandidateSizeMatch>,
    pub longest_equal_len_run: TabEqualLenRun,
    pub common_adjacent_len_deltas: Vec<TabAdjacentLenDelta>,
    pub repeated_len_patterns: Vec<TabRepeatedLenPattern>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabChunkLenBucket {
    pub len: u32,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabCandidateSizeMatch {
    pub bytes_per_chunk: usize,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabEqualLenRun {
    pub len: u32,
    pub run_chunks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabAdjacentLenDelta {
    pub delta: i32,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRepeatedLenPattern {
    pub pattern_len: usize,
    pub repeats: usize,
    pub min_chunk_len: u32,
    pub max_chunk_len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabArchiveSummary {
    pub bank: TabBankSummary,
    pub sprite_kind_counts: Vec<TabSpriteKindCount>,
    pub sprite_bank: SpriteBankAggregateSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSpriteKindCount {
    pub kind: SpriteChunkKind,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabVariantScore {
    pub offset_width: usize,
    pub records: usize,
    pub valid_offsets: usize,
    pub unique_offsets: usize,
    pub monotonic_pairs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabVariantAnalysis {
    pub scores: Vec<TabVariantScore>,
}

impl TabBank {
    pub fn parse(tab: &[u8], dat_len: usize) -> Option<Self> {
        if tab.len() < 8 || tab.len() % 4 != 0 {
            return None;
        }

        let mut reader = BinaryReader::new(tab);
        let mut offsets = Vec::with_capacity(tab.len() / 4);
        while reader.remaining() >= 4 {
            offsets.push(reader.read_u32_le()?);
        }
        offsets.retain(|offset| (*offset as usize) <= dat_len);
        offsets.sort_unstable();
        let duplicate_offset_count = offsets.windows(2).filter(|pair| pair[0] == pair[1]).count();
        let zero_len_chunk_count = duplicate_offset_count;
        offsets.dedup();

        let entries = offsets
            .windows(2)
            .filter_map(|pair| {
                let len = pair[1].checked_sub(pair[0])?;
                (len > 0).then_some(BankEntry {
                    offset: pair[0],
                    len,
                })
            })
            .collect::<Vec<_>>();

        (!entries.is_empty()).then_some(Self {
            entries,
            dat_len,
            duplicate_offset_count,
            zero_len_chunk_count,
        })
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn entry(&self, index: usize) -> Option<BankEntry> {
        self.entries.get(index).copied()
    }

    pub fn chunk_bounds(&self, index: usize) -> Option<std::ops::Range<usize>> {
        let entry = self.entry(index)?;
        let start = entry.offset as usize;
        let end = start.checked_add(entry.len as usize)?;
        (end <= self.dat_len).then_some(start..end)
    }

    pub fn min_chunk_len(&self) -> Option<u32> {
        self.entries.iter().map(|entry| entry.len).min()
    }

    pub fn max_chunk_len(&self) -> Option<u32> {
        self.entries.iter().map(|entry| entry.len).max()
    }

    pub fn aggregate_summary(&self) -> TabBankSummary {
        summarize_bank(self, None)
    }
}

impl TabArchive {
    pub fn parse(tab: &[u8], dat: Vec<u8>) -> Option<Self> {
        let bank = TabBank::parse(tab, dat.len())?;
        Some(Self { bank, dat })
    }

    pub fn chunk(&self, index: usize) -> Option<&[u8]> {
        let bounds = self.bank.chunk_bounds(index)?;
        self.dat.get(bounds)
    }

    pub fn aggregate_summary(&self) -> TabArchiveSummary {
        let mut kind_counts: BTreeMap<SpriteChunkKind, usize> = BTreeMap::new();
        let chunks = (0..self.bank.entry_count())
            .filter_map(|index| self.chunk(index))
            .collect::<Vec<_>>();
        for chunk in &chunks {
            *kind_counts
                .entry(SpriteChunkInfo::inspect(chunk).kind)
                .or_default() += 1;
        }

        TabArchiveSummary {
            bank: summarize_bank(&self.bank, Some(&self.dat)),
            sprite_bank: SpriteBankAggregateSummary::from_chunks(chunks),
            sprite_kind_counts: kind_counts
                .into_iter()
                .map(|(kind, count)| TabSpriteKindCount { kind, count })
                .collect(),
        }
    }
}

impl TabVariantAnalysis {
    pub fn analyze(tab: &[u8], dat_len: usize) -> Self {
        let scores = [2, 3, 4]
            .into_iter()
            .map(|offset_width| score_variant(tab, dat_len, offset_width))
            .collect();
        Self { scores }
    }

    pub fn best(&self) -> Option<TabVariantScore> {
        self.scores.iter().copied().max_by_key(|score| {
            let monotonic_ratio =
                ratio_per_mille(score.monotonic_pairs, score.records.saturating_sub(1));
            let unique_ratio = ratio_per_mille(score.unique_offsets, score.valid_offsets);
            let valid_ratio = ratio_per_mille(score.valid_offsets, score.records);
            (
                monotonic_ratio,
                unique_ratio,
                valid_ratio,
                score.offset_width,
            )
        })
    }

    pub fn summary(&self) -> String {
        let Some(best) = self.best() else {
            return "TAB variants: no candidates".to_string();
        };

        format!(
            "TAB{} best: {}/{} valid, {} unique, {} monotonic",
            best.offset_width * 8,
            best.valid_offsets,
            best.records,
            best.unique_offsets,
            best.monotonic_pairs
        )
    }
}

fn score_variant(tab: &[u8], dat_len: usize, offset_width: usize) -> TabVariantScore {
    let records = tab.len() / offset_width;
    let offsets = (0..records)
        .map(|i| read_le_width(&tab[i * offset_width..][..offset_width]))
        .collect::<Vec<_>>();

    let valid_offsets = offsets
        .iter()
        .filter(|&&offset| offset <= dat_len as u32)
        .count();
    let mut unique = offsets
        .iter()
        .copied()
        .filter(|&offset| offset <= dat_len as u32)
        .collect::<Vec<_>>();
    unique.sort_unstable();
    unique.dedup();

    let monotonic_pairs = offsets
        .windows(2)
        .filter(|pair| pair[0] <= dat_len as u32 && pair[1] <= dat_len as u32 && pair[1] >= pair[0])
        .count();

    TabVariantScore {
        offset_width,
        records,
        valid_offsets,
        unique_offsets: unique.len(),
        monotonic_pairs,
    }
}

fn read_le_width(bytes: &[u8]) -> u32 {
    bytes.iter().enumerate().fold(0u32, |value, (shift, byte)| {
        value | ((*byte as u32) << (shift * 8))
    })
}

fn ratio_per_mille(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_mul(1000) / denominator
}

fn summarize_bank(bank: &TabBank, _dat: Option<&[u8]>) -> TabBankSummary {
    let ordered_lengths = bank
        .entries
        .iter()
        .map(|entry| entry.len)
        .collect::<Vec<_>>();
    let longest_equal_len_run = longest_equal_len_run(&ordered_lengths);
    let common_adjacent_len_deltas = common_adjacent_len_deltas(&ordered_lengths);
    let repeated_len_patterns = repeated_len_patterns(&ordered_lengths);

    let mut lengths = ordered_lengths.clone();
    lengths.sort_unstable();

    let mut offsets = bank
        .entries
        .iter()
        .flat_map(|entry| [entry.offset, entry.offset.saturating_add(entry.len)])
        .filter(|offset| (*offset as usize) <= bank.dat_len)
        .collect::<Vec<_>>();
    offsets.sort_unstable();
    offsets.dedup();

    let mut frequency = BTreeMap::new();
    for len in &lengths {
        *frequency.entry(*len).or_insert(0usize) += 1;
    }
    let mut common_chunk_len_buckets = frequency
        .iter()
        .map(|(&len, &count)| TabChunkLenBucket { len, count })
        .collect::<Vec<_>>();
    common_chunk_len_buckets.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.len.cmp(&right.len))
    });
    common_chunk_len_buckets.truncate(5);

    let exact_candidate_size_matches = CANDIDATE_CHUNK_BYTE_SIZES
        .into_iter()
        .filter_map(|bytes_per_chunk| {
            let count = frequency
                .get(&(bytes_per_chunk as u32))
                .copied()
                .unwrap_or(0);
            (count > 0).then_some(TabCandidateSizeMatch {
                bytes_per_chunk,
                count,
            })
        })
        .collect();

    TabBankSummary {
        chunk_count: bank.entry_count(),
        dat_len: bank.dat_len,
        min_chunk_len: lengths.first().copied().unwrap_or(0),
        median_chunk_len: lengths.get(lengths.len() / 2).copied().unwrap_or(0),
        max_chunk_len: lengths.last().copied().unwrap_or(0),
        zero_len_chunks: bank.zero_len_chunk_count,
        duplicate_offset_count: bank.duplicate_offset_count,
        first_offset: offsets.first().copied(),
        last_offset: offsets.last().copied(),
        chunk_len_entropy_milli_bits: chunk_len_entropy_milli_bits(&frequency, lengths.len()),
        common_chunk_len_buckets,
        exact_candidate_size_matches,
        longest_equal_len_run,
        common_adjacent_len_deltas,
        repeated_len_patterns,
    }
}

fn longest_equal_len_run(lengths: &[u32]) -> TabEqualLenRun {
    let Some(&first) = lengths.first() else {
        return TabEqualLenRun {
            len: 0,
            run_chunks: 0,
        };
    };

    let mut best = TabEqualLenRun {
        len: first,
        run_chunks: 1,
    };
    let mut current = best;
    for &len in &lengths[1..] {
        if len == current.len {
            current.run_chunks += 1;
        } else {
            if current.run_chunks > best.run_chunks {
                best = current;
            }
            current = TabEqualLenRun { len, run_chunks: 1 };
        }
    }
    if current.run_chunks > best.run_chunks {
        best = current;
    }
    best
}

fn common_adjacent_len_deltas(lengths: &[u32]) -> Vec<TabAdjacentLenDelta> {
    let mut counts = BTreeMap::new();
    for pair in lengths.windows(2) {
        let delta = pair[1] as i64 - pair[0] as i64;
        if let Ok(delta) = i32::try_from(delta) {
            *counts.entry(delta).or_insert(0usize) += 1;
        }
    }
    let mut deltas = counts
        .into_iter()
        .map(|(delta, count)| TabAdjacentLenDelta { delta, count })
        .collect::<Vec<_>>();
    deltas.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.delta.abs().cmp(&right.delta.abs()))
            .then_with(|| left.delta.cmp(&right.delta))
    });
    deltas.truncate(5);
    deltas
}

fn repeated_len_patterns(lengths: &[u32]) -> Vec<TabRepeatedLenPattern> {
    let mut candidates = Vec::new();
    for pattern_len in 2..=4 {
        if lengths.len() < pattern_len * 2 {
            continue;
        }
        let mut index = 0;
        while index + pattern_len * 2 <= lengths.len() {
            let pattern = &lengths[index..index + pattern_len];
            let mut repeats = 1;
            while index + (repeats + 1) * pattern_len <= lengths.len()
                && &lengths[index + repeats * pattern_len..index + (repeats + 1) * pattern_len]
                    == pattern
            {
                repeats += 1;
            }
            if repeats >= 2 {
                let min_chunk_len = pattern.iter().copied().min().unwrap_or(0);
                let max_chunk_len = pattern.iter().copied().max().unwrap_or(0);
                candidates.push(TabRepeatedLenPattern {
                    pattern_len,
                    repeats,
                    min_chunk_len,
                    max_chunk_len,
                });
                index += repeats * pattern_len;
            } else {
                index += 1;
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .repeats
            .cmp(&left.repeats)
            .then_with(|| left.pattern_len.cmp(&right.pattern_len))
            .then_with(|| left.min_chunk_len.cmp(&right.min_chunk_len))
            .then_with(|| left.max_chunk_len.cmp(&right.max_chunk_len))
    });
    candidates.truncate(5);
    candidates
}

fn chunk_len_entropy_milli_bits(frequency: &BTreeMap<u32, usize>, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    let entropy = frequency
        .values()
        .copied()
        .filter(|&count| count > 0)
        .map(|count| {
            let probability = count as f64 / total as f64;
            -probability * probability.log2()
        })
        .sum::<f64>();
    (entropy * 1000.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::{TabArchive, TabBank, TabVariantAnalysis};

    #[test]
    fn parses_monotonic_offsets_into_lengths() {
        let tab = [0u32, 10, 25, 25, 40]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let bank = TabBank::parse(&tab, 40).unwrap();
        assert_eq!(bank.entry_count(), 3);
        assert_eq!(bank.entries[1].offset, 10);
        assert_eq!(bank.entries[1].len, 15);
    }

    #[test]
    fn archive_exposes_safe_chunks() {
        let tab = [0u32, 2, 5]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let archive = TabArchive::parse(&tab, vec![10, 11, 12, 13, 14]).unwrap();
        assert_eq!(archive.chunk(0), Some([10, 11].as_slice()));
        assert_eq!(archive.chunk(1), Some([12, 13, 14].as_slice()));
        assert_eq!(archive.chunk(2), None);
        assert_eq!(archive.bank.min_chunk_len(), Some(2));
        assert_eq!(archive.bank.max_chunk_len(), Some(3));
    }

    #[test]
    fn scores_offset_width_variants() {
        let tab = [0u32, 4, 8, 12]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let analysis = TabVariantAnalysis::analyze(&tab, 12);
        let best = analysis.best().unwrap();
        assert_eq!(best.offset_width, 4);
        assert_eq!(best.valid_offsets, 4);
        assert_eq!(best.monotonic_pairs, 3);
    }

    #[test]
    fn summarizes_chunk_size_distribution_without_bytes() {
        let tab = [0u32, 64, 128, 384, 640]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let archive = TabArchive::parse(&tab, vec![1; 640]).unwrap();
        let summary = archive.aggregate_summary();

        assert_eq!(summary.bank.chunk_count, 4);
        assert_eq!(summary.bank.min_chunk_len, 64);
        assert_eq!(summary.bank.median_chunk_len, 256);
        assert_eq!(summary.bank.max_chunk_len, 256);
        assert!(summary.bank.chunk_len_entropy_milli_bits > 0);
        assert!(
            summary
                .bank
                .exact_candidate_size_matches
                .iter()
                .any(|candidate| candidate.bytes_per_chunk == 64 && candidate.count == 2)
        );
        assert_eq!(
            summary
                .sprite_kind_counts
                .iter()
                .map(|entry| entry.count)
                .sum::<usize>(),
            4
        );
    }

    #[test]
    fn summarizes_chunk_length_progression_without_bytes() {
        let offsets = [0u32, 10, 20, 25, 35, 45, 50, 60, 70]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let archive = TabArchive::parse(&offsets, vec![1; 70]).unwrap();
        let summary = archive.aggregate_summary();

        assert_eq!(summary.bank.longest_equal_len_run.len, 10);
        assert_eq!(summary.bank.longest_equal_len_run.run_chunks, 2);
        assert!(
            summary
                .bank
                .common_adjacent_len_deltas
                .iter()
                .any(|delta| delta.delta == 0 && delta.count >= 1)
        );
        assert!(
            summary
                .bank
                .repeated_len_patterns
                .iter()
                .any(|pattern| pattern.pattern_len == 3 && pattern.repeats == 2)
        );
    }
}
