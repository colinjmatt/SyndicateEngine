//! Runtime-facing local probe manifests for original TAB/sprite assets.
//!
//! This module may read local user-supplied assets at runtime, but its public
//! manifest model stores only aggregate, non-reconstructable probe metadata.

use std::{collections::BTreeMap, fs, path::Path};

use walkdir::WalkDir;

use crate::engine::{
    sprite_decode::SpriteChunkKind,
    tab_bank::{TabArchive, TabArchiveSummary},
};

const MAX_FAMILIES: usize = 3;
const MAX_ARCHIVES_PER_FAMILY: usize = 4;
const MAX_MANIFEST_SELECTORS: usize = 15;
const MAX_PHASE_SELECTOR_IDS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbeArchiveInput {
    pub path: String,
    pub summary: TabArchiveSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TabRuntimeProbeManifest {
    pub selectors: Vec<TabRuntimeProbeSelector>,
    pub phases: Vec<TabRuntimeProbePhaseSummary>,
    pub parsed_archives: usize,
    pub selected_families: usize,
    pub total_candidate_families: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbeSelector {
    pub id: String,
    pub rank: usize,
    pub family: String,
    pub category: TabRuntimeProbeCategory,
    pub support_tier: TabRuntimeProbeSupportTier,
    pub phase: TabRuntimeProbePhase,
    pub focus: String,
    pub aggregate_evidence: String,
    pub grouping_rule: String,
    pub preconditions: String,
    pub stop_conditions: String,
    pub rationale: String,
    pub priority: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbePhaseSummary {
    pub phase: TabRuntimeProbePhase,
    pub selector_ids: Vec<String>,
    pub families: Vec<String>,
    pub support_tiers: Vec<TabRuntimeProbeSupportTier>,
    pub grouping_rule: String,
    pub rationale: String,
    pub stop_condition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabRuntimeProbeCategory {
    MetadataShape,
    ClassifierGrouping,
    FixedLengthBuckets,
    SiblingCommonBuckets,
    MixedUnknownAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabRuntimeProbePhase {
    MetadataShapeGrouping,
    CommandStreamSeparation,
    FixedLengthBucketGrouping,
    SiblingCommonBucketComparison,
    MixedUnknownAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabRuntimeProbeSupportTier {
    Strong,
    Medium,
    Limited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbeExecution {
    pub manifest: TabRuntimeProbeManifest,
    pub selector_results: Vec<TabRuntimeProbeSelectorExecution>,
    pub phase_results: Vec<TabRuntimeProbePhaseExecution>,
    pub parsed_archives: usize,
    pub executed_selectors: usize,
    pub skipped_selectors: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbeSelectorExecution {
    pub selector_id: String,
    pub rank: usize,
    pub family: String,
    pub phase: TabRuntimeProbePhase,
    pub category: TabRuntimeProbeCategory,
    pub support_tier: TabRuntimeProbeSupportTier,
    pub readiness: TabRuntimeProbeExecutionReadiness,
    pub archive_scope: String,
    pub aggregate_group_count: usize,
    pub aggregate_unit_count: usize,
    pub strongest_group: String,
    pub execution_summary: String,
    pub stop_condition: String,
    pub conservative_limitation: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRuntimeProbePhaseExecution {
    pub phase: TabRuntimeProbePhase,
    pub selector_ids: Vec<String>,
    pub families: Vec<String>,
    pub support_tiers: Vec<TabRuntimeProbeSupportTier>,
    pub readiness: Vec<TabRuntimeProbeExecutionReadiness>,
    pub executed_selectors: usize,
    pub aggregate_group_count: usize,
    pub aggregate_unit_count: usize,
    pub grouping_rule: String,
    pub stop_condition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TabRuntimeProbeExecutionReadiness {
    Ready,
    Limited,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRuntimeProbeFamilySummary {
    family: String,
    parsed_archives: usize,
    included_archives: usize,
    total_chunks: usize,
    command_stream_chunks: usize,
    raw_chunks: usize,
    unknown_chunks: usize,
    metadata_shape_supports: Vec<TabRuntimeProbeMetadataSupport>,
    equal_run_archives: usize,
    repeated_pattern_archives: usize,
    min_entropy_milli_bits: u32,
    max_entropy_milli_bits: u32,
    common_bucket_overlap: Vec<TabRuntimeProbeCommonBucketOverlap>,
    small_chunks: usize,
    medium_chunks: usize,
    large_chunks: usize,
    included_archive_summaries: Vec<TabArchiveSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRuntimeProbeMetadataSupport {
    label: &'static str,
    support_count: usize,
    per_mille: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRuntimeProbeCommonBucketOverlap {
    len: u32,
    archive_count: usize,
}

impl TabRuntimeProbeManifest {
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        Self::from_archive_inputs(tab_runtime_probe_archive_inputs(root))
    }

    pub fn from_archive_inputs(
        inputs: impl IntoIterator<Item = TabRuntimeProbeArchiveInput>,
    ) -> Self {
        let inputs = inputs.into_iter().collect::<Vec<_>>();
        let parsed_archives = inputs.len();
        let family_summaries = tab_runtime_probe_family_summaries(&inputs);
        let total_candidate_families = family_summaries
            .iter()
            .filter(|summary| is_runtime_sprite_like_family_candidate(summary))
            .count();
        let selected = family_summaries
            .into_iter()
            .filter(is_runtime_sprite_like_family_candidate)
            .take(MAX_FAMILIES)
            .collect::<Vec<_>>();
        let selected_families = selected.len();

        let mut selectors = selected
            .iter()
            .flat_map(runtime_probe_selectors_for_family)
            .collect::<Vec<_>>();
        sort_runtime_probe_selectors(&mut selectors);
        selectors.truncate(MAX_MANIFEST_SELECTORS);
        assign_runtime_probe_selector_ids(&mut selectors);

        let phases = runtime_probe_phase_summaries(&selectors);

        Self {
            selectors,
            phases,
            parsed_archives,
            selected_families,
            total_candidate_families,
        }
    }

    pub fn compact_status(&self) -> String {
        if self.selectors.is_empty() {
            return "TAB probe manifest: no aggregate runtime selectors available".to_string();
        }
        format!(
            "TAB probe manifest: {} selectors across {} dry-run phases; families [{}]",
            self.selectors.len(),
            self.phases.len(),
            self.family_summary()
        )
    }

    pub fn family_summary(&self) -> String {
        let mut families = self
            .selectors
            .iter()
            .map(|selector| selector.family.clone())
            .collect::<Vec<_>>();
        families.sort();
        families.dedup();
        if families.is_empty() {
            "none".to_string()
        } else {
            families.join(", ")
        }
    }

    pub fn selector_tier_summary(&self) -> String {
        let mut counts: BTreeMap<TabRuntimeProbeSupportTier, usize> = BTreeMap::new();
        for selector in &self.selectors {
            *counts.entry(selector.support_tier).or_default() += 1;
        }
        if counts.is_empty() {
            return "no selector support tiers".to_string();
        }
        counts
            .into_iter()
            .map(|(tier, count)| format!("{} {}", tier.label(), count))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn phase_summary(&self) -> String {
        if self.phases.is_empty() {
            return "no dry-run phases".to_string();
        }
        self.phases
            .iter()
            .map(|phase| {
                format!(
                    "{}:{} selectors",
                    phase.phase.label(),
                    phase.selector_ids.len()
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn grouping_rules_summary(&self) -> String {
        if self.phases.is_empty() {
            return "no aggregate grouping rules selected".to_string();
        }
        self.phases
            .iter()
            .map(|phase| format!("{}: {}", phase.phase.label(), phase.grouping_rule))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn preconditions_summary(&self) -> &'static str {
        "local user-supplied assets available at runtime; build selectors from aggregate TAB/DAT summaries before decode or render attempts"
    }

    pub fn stop_conditions_summary(&self) -> &'static str {
        "do not infer decoded dimensions, anchors, commands, pixels, audio, UI, or gameplay semantics; do not commit generated previews or decoded asset-derived bytes"
    }
}

impl TabRuntimeProbeExecution {
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        Self::from_archive_inputs(tab_runtime_probe_archive_inputs(root))
    }

    pub fn from_archive_inputs(
        inputs: impl IntoIterator<Item = TabRuntimeProbeArchiveInput>,
    ) -> Self {
        let inputs = inputs.into_iter().collect::<Vec<_>>();
        let manifest = TabRuntimeProbeManifest::from_archive_inputs(inputs.clone());
        Self::from_manifest_and_archive_inputs(manifest, inputs)
    }

    pub fn from_manifest_and_archive_inputs(
        manifest: TabRuntimeProbeManifest,
        inputs: impl IntoIterator<Item = TabRuntimeProbeArchiveInput>,
    ) -> Self {
        let inputs = inputs.into_iter().collect::<Vec<_>>();
        let parsed_archives = inputs.len();
        let family_summaries = tab_runtime_probe_family_summaries(&inputs);
        let family_summaries = family_summaries
            .iter()
            .map(|summary| (summary.family.clone(), summary))
            .collect::<BTreeMap<_, _>>();

        let selector_results = manifest
            .selectors
            .iter()
            .map(|selector| {
                execute_runtime_probe_selector(selector, family_summaries.get(&selector.family))
            })
            .collect::<Vec<_>>();
        let phase_results = runtime_probe_phase_execution_summaries(&selector_results);
        let executed_selectors = selector_results
            .iter()
            .filter(|result| result.readiness != TabRuntimeProbeExecutionReadiness::Skipped)
            .count();
        let skipped_selectors = selector_results.len().saturating_sub(executed_selectors);

        Self {
            manifest,
            selector_results,
            phase_results,
            parsed_archives,
            executed_selectors,
            skipped_selectors,
        }
    }

    pub fn compact_status(&self) -> String {
        if self.selector_results.is_empty() {
            return "TAB probe execution: no aggregate selector executions available".to_string();
        }
        format!(
            "TAB probe execution: {} selector dry-runs across {} phases; {} executed, {} skipped",
            self.selector_results.len(),
            self.phase_results.len(),
            self.executed_selectors,
            self.skipped_selectors
        )
    }

    pub fn readiness_summary(&self) -> String {
        let mut counts: BTreeMap<TabRuntimeProbeExecutionReadiness, usize> = BTreeMap::new();
        for result in &self.selector_results {
            *counts.entry(result.readiness).or_default() += 1;
        }
        if counts.is_empty() {
            return "no execution readiness results".to_string();
        }
        counts
            .into_iter()
            .map(|(readiness, count)| format!("{} {}", readiness.label(), count))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn phase_summary(&self) -> String {
        if self.phase_results.is_empty() {
            return "no execution phases".to_string();
        }
        self.phase_results
            .iter()
            .map(|phase| {
                format!(
                    "{}:{} selectors/{} groups",
                    phase.phase.label(),
                    phase.executed_selectors,
                    phase.aggregate_group_count
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl TabRuntimeProbeSelector {
    pub fn conservative_limitation(&self) -> &'static str {
        "runtime-only manifest selector; not proof of decoded layout or semantics and not an asset identifier"
    }
}

impl TabRuntimeProbePhaseSummary {
    pub fn selector_ids_summary(&self) -> String {
        format_capped_id_list(&self.selector_ids)
    }

    pub fn families_summary(&self) -> String {
        if self.families.is_empty() {
            "none".to_string()
        } else {
            self.families.join(", ")
        }
    }

    pub fn support_tiers_summary(&self) -> String {
        if self.support_tiers.is_empty() {
            return "none".to_string();
        }
        self.support_tiers
            .iter()
            .map(|tier| tier.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl TabRuntimeProbePhaseExecution {
    pub fn selector_ids_summary(&self) -> String {
        format_capped_id_list(&self.selector_ids)
    }

    pub fn families_summary(&self) -> String {
        if self.families.is_empty() {
            "none".to_string()
        } else {
            self.families.join(", ")
        }
    }

    pub fn support_tiers_summary(&self) -> String {
        if self.support_tiers.is_empty() {
            return "none".to_string();
        }
        self.support_tiers
            .iter()
            .map(|tier| tier.label())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn readiness_summary(&self) -> String {
        if self.readiness.is_empty() {
            return "none".to_string();
        }
        self.readiness
            .iter()
            .map(|readiness| readiness.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl TabRuntimeProbeExecutionReadiness {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready aggregate dry-run",
            Self::Limited => "limited aggregate dry-run",
            Self::Skipped => "skipped aggregate dry-run",
        }
    }
}

impl TabRuntimeProbeCategory {
    pub fn slug(self) -> &'static str {
        match self {
            Self::MetadataShape => "metadata-shape",
            Self::ClassifierGrouping => "classifier-grouping",
            Self::FixedLengthBuckets => "fixed-length-buckets",
            Self::SiblingCommonBuckets => "sibling-common-buckets",
            Self::MixedUnknownAudit => "mixed-unknown-audit",
        }
    }

    pub fn phase(self) -> TabRuntimeProbePhase {
        match self {
            Self::MetadataShape => TabRuntimeProbePhase::MetadataShapeGrouping,
            Self::ClassifierGrouping => TabRuntimeProbePhase::CommandStreamSeparation,
            Self::FixedLengthBuckets => TabRuntimeProbePhase::FixedLengthBucketGrouping,
            Self::SiblingCommonBuckets => TabRuntimeProbePhase::SiblingCommonBucketComparison,
            Self::MixedUnknownAudit => TabRuntimeProbePhase::MixedUnknownAudit,
        }
    }
}

impl TabRuntimeProbePhase {
    pub fn order(self) -> usize {
        match self {
            Self::MetadataShapeGrouping => 1,
            Self::CommandStreamSeparation => 2,
            Self::FixedLengthBucketGrouping => 3,
            Self::SiblingCommonBucketComparison => 4,
            Self::MixedUnknownAudit => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MetadataShapeGrouping => "metadata-shape grouping",
            Self::CommandStreamSeparation => "command-stream-heavy separation",
            Self::FixedLengthBucketGrouping => "repeated fixed-length bucket grouping",
            Self::SiblingCommonBucketComparison => "sibling/common bucket comparison",
            Self::MixedUnknownAudit => "fallback mixed/unknown audit",
        }
    }

    pub fn grouping_rule(self) -> &'static str {
        match self {
            Self::MetadataShapeGrouping => {
                "group local chunks by bounded candidate metadata-shape support tier"
            }
            Self::CommandStreamSeparation => {
                "separate command-stream-heavy candidates from mixed/raw/unknown aggregate groups"
            }
            Self::FixedLengthBucketGrouping => {
                "group local chunks by repeated chunk-length bucket labels"
            }
            Self::SiblingCommonBucketComparison => {
                "compare common-size bucket labels across capped sibling archives"
            }
            Self::MixedUnknownAudit => {
                "audit unresolved mixed/raw/unknown aggregate selector groups last"
            }
        }
    }

    pub fn rationale(self) -> &'static str {
        match self {
            Self::MetadataShapeGrouping => {
                "start with bounded aggregate metadata-shape groups before classifier or length probes"
            }
            Self::CommandStreamSeparation => {
                "separate command-stream-heavy aggregate groups before any render-oriented experiment"
            }
            Self::FixedLengthBucketGrouping => {
                "group repeated fixed-length candidates after metadata/classifier separation"
            }
            Self::SiblingCommonBucketComparison => {
                "compare sibling/common buckets only after per-family groups are established"
            }
            Self::MixedUnknownAudit => {
                "audit unresolved mixed/raw/unknown selectors last and keep them local-only"
            }
        }
    }

    pub fn stop_condition(self) -> &'static str {
        match self {
            Self::MetadataShapeGrouping => {
                "do not treat ranges as decoded dimensions, anchors, or commands; stop before render attempts"
            }
            Self::CommandStreamSeparation => {
                "do not treat classifier labels as decoded commands, pixels, audio, UI, or gameplay semantics"
            }
            Self::FixedLengthBucketGrouping => {
                "do not treat repeated lengths as decoded records or frame dimensions"
            }
            Self::SiblingCommonBucketComparison => {
                "do not treat common buckets as proof of shared layout"
            }
            Self::MixedUnknownAudit => {
                "do not promote mixed/raw/unknown aggregates to render semantics"
            }
        }
    }
}

impl TabRuntimeProbeSupportTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Strong => "strong aggregate support",
            Self::Medium => "medium aggregate support",
            Self::Limited => "limited aggregate support",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Medium => "medium",
            Self::Limited => "limited",
        }
    }
}

pub fn tab_runtime_probe_archive_inputs(
    root: impl AsRef<Path>,
) -> Vec<TabRuntimeProbeArchiveInput> {
    let root = root.as_ref();
    let mut inputs = Vec::new();
    for path in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| has_tab_extension(path))
    {
        let dat_path = path.with_extension("DAT");
        let (Ok(tab), Ok(dat)) = (fs::read(&path), fs::read(&dat_path)) else {
            continue;
        };
        let Some(archive) = TabArchive::parse(&tab, dat) else {
            continue;
        };
        inputs.push(TabRuntimeProbeArchiveInput {
            path: display_relative(root, &path),
            summary: archive.aggregate_summary(),
        });
    }
    inputs.sort_by(|left, right| left.path.cmp(&right.path));
    inputs
}

fn tab_runtime_probe_family_summaries(
    inputs: &[TabRuntimeProbeArchiveInput],
) -> Vec<TabRuntimeProbeFamilySummary> {
    let mut grouped: BTreeMap<String, Vec<&TabRuntimeProbeArchiveInput>> = BTreeMap::new();
    for input in inputs {
        grouped
            .entry(tab_probe_family(&input.path).to_string())
            .or_default()
            .push(input);
    }

    let mut summaries = grouped
        .into_iter()
        .filter_map(|(family, entries)| summarize_runtime_probe_family(family, &entries))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .metadata_support_score()
            .cmp(&left.metadata_support_score())
            .then_with(|| right.progression_score().cmp(&left.progression_score()))
            .then_with(|| right.total_chunks.cmp(&left.total_chunks))
            .then_with(|| left.family.cmp(&right.family))
    });
    summaries
}

fn summarize_runtime_probe_family(
    family: String,
    entries: &[&TabRuntimeProbeArchiveInput],
) -> Option<TabRuntimeProbeFamilySummary> {
    if entries.is_empty() {
        return None;
    }

    let summaries = entries
        .iter()
        .map(|entry| &entry.summary)
        .collect::<Vec<_>>();
    let included_archive_summaries = summaries
        .iter()
        .take(MAX_ARCHIVES_PER_FAMILY)
        .map(|summary| (*summary).clone())
        .collect::<Vec<_>>();
    let parsed_archives = summaries.len();
    let included_archives = included_archive_summaries.len();
    let total_chunks = summaries
        .iter()
        .map(|summary| summary.bank.chunk_count)
        .sum::<usize>();
    let command_stream_chunks =
        sum_runtime_family_kind(&summaries, SpriteChunkKind::LikelyRleOrCommandStream);
    let raw_chunks = sum_runtime_family_kind(&summaries, SpriteChunkKind::LikelyRawIndexed);
    let unknown_chunks = sum_runtime_family_kind(&summaries, SpriteChunkKind::Unknown);
    let mut metadata_shape_supports = summaries
        .iter()
        .flat_map(|summary| summary.sprite_bank.metadata_shape_probes.iter())
        .fold(BTreeMap::new(), |mut counts, probe| {
            *counts.entry(probe.kind.label()).or_insert(0usize) += probe.support_count;
            counts
        })
        .into_iter()
        .map(|(label, support_count)| TabRuntimeProbeMetadataSupport {
            label,
            support_count,
            per_mille: ratio_per_mille(support_count, total_chunks),
        })
        .collect::<Vec<_>>();
    metadata_shape_supports.sort_by(|left, right| {
        right
            .support_count
            .cmp(&left.support_count)
            .then_with(|| left.label.cmp(right.label))
    });

    let equal_run_archives = summaries
        .iter()
        .filter(|summary| summary.bank.longest_equal_len_run.run_chunks >= 2)
        .count();
    let repeated_pattern_archives = summaries
        .iter()
        .filter(|summary| !summary.bank.repeated_len_patterns.is_empty())
        .count();

    Some(TabRuntimeProbeFamilySummary {
        family,
        parsed_archives,
        included_archives,
        total_chunks,
        command_stream_chunks,
        raw_chunks,
        unknown_chunks,
        metadata_shape_supports,
        equal_run_archives,
        repeated_pattern_archives,
        min_entropy_milli_bits: summaries
            .iter()
            .map(|summary| summary.bank.chunk_len_entropy_milli_bits)
            .min()
            .unwrap_or(0),
        max_entropy_milli_bits: summaries
            .iter()
            .map(|summary| summary.bank.chunk_len_entropy_milli_bits)
            .max()
            .unwrap_or(0),
        common_bucket_overlap: runtime_family_common_bucket_overlap(&summaries),
        small_chunks: summaries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.small)
            .sum(),
        medium_chunks: summaries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.medium)
            .sum(),
        large_chunks: summaries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.large)
            .sum(),
        included_archive_summaries,
    })
}

fn runtime_probe_selectors_for_family(
    summary: &TabRuntimeProbeFamilySummary,
) -> Vec<TabRuntimeProbeSelector> {
    let mut selectors = Vec::new();
    let inclusion = format_runtime_probe_archive_inclusion(summary);

    if let Some(top_metadata) = summary.metadata_shape_supports.first() {
        selectors.push(make_runtime_probe_selector(
            &summary.family,
            TabRuntimeProbeCategory::MetadataShape,
            support_tier_for_per_mille(top_metadata.per_mille),
            format!(
                "candidate metadata-shape support grouping for `{}`",
                top_metadata.label
            ),
            format!(
                "{inclusion}; support [{}]; {}",
                format_runtime_metadata_probe_group(summary),
                format_runtime_probe_entropy_progression(summary)
            ),
            format!(
                "probe before lower metadata-support tasks because strongest candidate support is {} per mille; run after archive cap review using aggregate groups only",
                top_metadata.per_mille
            ),
            50_000
                + top_metadata.per_mille * 6
                + summary.metadata_support_score().min(1_000)
                + runtime_probe_archive_score(summary)
                + runtime_probe_entropy_consistency_score(summary),
        ));
    }

    if summary.equal_run_archives > 0 || summary.repeated_pattern_archives > 0 {
        let progression_ratio = runtime_probe_progression_per_mille(summary);
        selectors.push(make_runtime_probe_selector(
            &summary.family,
            TabRuntimeProbeCategory::FixedLengthBuckets,
            support_tier_for_per_mille(progression_ratio),
            "repeated fixed-length bucket selector grouping".to_string(),
            format!(
                "{inclusion}; {}; {}; {}",
                format_runtime_progression_probe_support(summary),
                format_runtime_length_bucket_probe_group(summary),
                format_runtime_probe_entropy_progression(summary)
            ),
            "probe before sibling bucket comparison when repeated-record support is present; run after metadata grouping if bounded metadata support is stronger".to_string(),
            30_000
                + progression_ratio * 5
                + runtime_probe_length_bucket_support(summary).min(1_000)
                + runtime_probe_archive_score(summary)
                + runtime_probe_entropy_consistency_score(summary),
        ));
    }

    if summary.command_stream_chunks > 0 || summary.raw_chunks > 0 || summary.unknown_chunks > 0 {
        let command_ratio = ratio_per_mille(summary.command_stream_chunks, summary.total_chunks);
        selectors.push(make_runtime_probe_selector(
            &summary.family,
            TabRuntimeProbeCategory::ClassifierGrouping,
            support_tier_for_per_mille(command_ratio),
            "command-stream-heavy versus mixed/raw/unknown classifier selector grouping"
                .to_string(),
            format!(
                "{inclusion}; command-stream {} per mille, raw {} per mille, unknown {} per mille; size bands small/medium/large {}/{}/{}; {}",
                command_ratio,
                ratio_per_mille(summary.raw_chunks, summary.total_chunks),
                ratio_per_mille(summary.unknown_chunks, summary.total_chunks),
                summary.small_chunks,
                summary.medium_chunks,
                summary.large_chunks,
                format_runtime_probe_entropy_progression(summary)
            ),
            "probe before rendering experiments that mix classifier groups; run after stronger metadata or fixed-length grouping when those aggregate signals dominate".to_string(),
            40_000
                + command_ratio * 4
                + summary.total_chunks.min(1_000)
                + runtime_probe_archive_score(summary)
                + runtime_probe_entropy_consistency_score(summary),
        ));

        let mixed_unknown_chunks = summary.raw_chunks.saturating_add(summary.unknown_chunks);
        if mixed_unknown_chunks > 0 {
            let mixed_unknown_ratio = ratio_per_mille(mixed_unknown_chunks, summary.total_chunks);
            selectors.push(make_runtime_probe_selector(
                &summary.family,
                TabRuntimeProbeCategory::MixedUnknownAudit,
                support_tier_for_per_mille(mixed_unknown_ratio),
                "fallback mixed/raw/unknown aggregate selector audit".to_string(),
                format!(
                    "{inclusion}; raw+unknown {} chunks ({} per mille); command-stream {} chunks ({} per mille); {}",
                    mixed_unknown_chunks,
                    mixed_unknown_ratio,
                    summary.command_stream_chunks,
                    command_ratio,
                    format_runtime_probe_entropy_progression(summary)
                ),
                "probe after metadata, classifier, repeated-length, and sibling bucket phases; use only to audit unresolved aggregate groups before any render attempt".to_string(),
                10_000
                    + mixed_unknown_ratio * 4
                    + runtime_probe_archive_score(summary)
                    + runtime_probe_entropy_consistency_score(summary),
            ));
        }
    }

    if !summary.common_bucket_overlap.is_empty() {
        let focus = if summary.parsed_archives > 1 {
            "sibling common-size bucket selector comparison"
        } else {
            "single-archive common-size bucket selector baseline"
        };
        let rationale = if summary.parsed_archives > 1 {
            "probe after per-family metadata/classifier grouping; use before lower-overlap sibling tasks because common buckets recur across parsed archives"
        } else {
            "probe after multi-archive sibling tasks because only one parsed archive is available; keep this as a runtime-only local baseline"
        };
        selectors.push(make_runtime_probe_selector(
            &summary.family,
            TabRuntimeProbeCategory::SiblingCommonBuckets,
            sibling_bucket_support_tier(summary),
            focus.to_string(),
            format!(
                "{inclusion}; common bucket support [{}]; {}",
                format_runtime_family_common_bucket_overlap(summary),
                format_runtime_probe_entropy_progression(summary)
            ),
            rationale.to_string(),
            20_000
                + runtime_probe_common_bucket_score(summary)
                + runtime_probe_archive_score(summary)
                + runtime_probe_entropy_consistency_score(summary),
        ));
    }

    selectors
}

fn make_runtime_probe_selector(
    family: &str,
    category: TabRuntimeProbeCategory,
    support_tier: TabRuntimeProbeSupportTier,
    focus: String,
    aggregate_evidence: String,
    rationale: String,
    priority: usize,
) -> TabRuntimeProbeSelector {
    let phase = category.phase();
    TabRuntimeProbeSelector {
        id: String::new(),
        rank: 0,
        family: family.to_string(),
        category,
        support_tier,
        phase,
        focus,
        aggregate_evidence,
        grouping_rule: phase.grouping_rule().to_string(),
        preconditions: "local user-supplied assets available at runtime; group by aggregate selector before any decode or render attempt".to_string(),
        stop_conditions: phase.stop_condition().to_string(),
        rationale,
        priority,
    }
}

fn runtime_probe_phase_summaries(
    selectors: &[TabRuntimeProbeSelector],
) -> Vec<TabRuntimeProbePhaseSummary> {
    let mut grouped: BTreeMap<TabRuntimeProbePhase, Vec<&TabRuntimeProbeSelector>> =
        BTreeMap::new();
    for selector in selectors {
        grouped.entry(selector.phase).or_default().push(selector);
    }

    grouped
        .into_iter()
        .map(|(phase, mut selectors)| {
            selectors.sort_by(|left, right| {
                left.rank
                    .cmp(&right.rank)
                    .then_with(|| left.family.cmp(&right.family))
            });
            let mut families = selectors
                .iter()
                .map(|selector| selector.family.clone())
                .collect::<Vec<_>>();
            families.sort();
            families.dedup();
            let mut support_tiers = selectors
                .iter()
                .map(|selector| selector.support_tier)
                .collect::<Vec<_>>();
            support_tiers.sort_unstable();
            support_tiers.dedup();
            TabRuntimeProbePhaseSummary {
                phase,
                selector_ids: selectors
                    .iter()
                    .map(|selector| selector.id.clone())
                    .collect(),
                families,
                support_tiers,
                grouping_rule: phase.grouping_rule().to_string(),
                rationale: phase.rationale().to_string(),
                stop_condition: phase.stop_condition().to_string(),
            }
        })
        .collect()
}

fn execute_runtime_probe_selector(
    selector: &TabRuntimeProbeSelector,
    summary: Option<&&TabRuntimeProbeFamilySummary>,
) -> TabRuntimeProbeSelectorExecution {
    let Some(summary) = summary.copied() else {
        return TabRuntimeProbeSelectorExecution {
            selector_id: selector.id.clone(),
            rank: selector.rank,
            family: selector.family.clone(),
            phase: selector.phase,
            category: selector.category,
            support_tier: selector.support_tier,
            readiness: TabRuntimeProbeExecutionReadiness::Skipped,
            archive_scope: "no aggregate family summary available for selector".to_string(),
            aggregate_group_count: 0,
            aggregate_unit_count: 0,
            strongest_group: "no aggregate group selected".to_string(),
            execution_summary:
                "aggregate execution skipped; selector has no matching family summary".to_string(),
            stop_condition: selector.stop_conditions.clone(),
            conservative_limitation: execution_limitation().to_string(),
        };
    };

    let (group_count, unit_count, strongest_group, execution_summary) = match selector.category {
        TabRuntimeProbeCategory::MetadataShape => execute_metadata_shape_probe(summary),
        TabRuntimeProbeCategory::ClassifierGrouping => execute_classifier_grouping_probe(summary),
        TabRuntimeProbeCategory::FixedLengthBuckets => execute_fixed_length_bucket_probe(summary),
        TabRuntimeProbeCategory::SiblingCommonBuckets => {
            execute_sibling_common_bucket_probe(summary)
        }
        TabRuntimeProbeCategory::MixedUnknownAudit => execute_mixed_unknown_audit_probe(summary),
    };
    let readiness = execution_readiness(selector.support_tier, group_count, unit_count);

    TabRuntimeProbeSelectorExecution {
        selector_id: selector.id.clone(),
        rank: selector.rank,
        family: selector.family.clone(),
        phase: selector.phase,
        category: selector.category,
        support_tier: selector.support_tier,
        readiness,
        archive_scope: format_runtime_probe_archive_inclusion(summary),
        aggregate_group_count: group_count,
        aggregate_unit_count: unit_count,
        strongest_group,
        execution_summary,
        stop_condition: selector.stop_conditions.clone(),
        conservative_limitation: execution_limitation().to_string(),
    }
}

fn runtime_probe_phase_execution_summaries(
    selector_results: &[TabRuntimeProbeSelectorExecution],
) -> Vec<TabRuntimeProbePhaseExecution> {
    let mut grouped: BTreeMap<TabRuntimeProbePhase, Vec<&TabRuntimeProbeSelectorExecution>> =
        BTreeMap::new();
    for result in selector_results {
        grouped.entry(result.phase).or_default().push(result);
    }

    grouped
        .into_iter()
        .map(|(phase, mut results)| {
            results.sort_by(|left, right| {
                left.rank
                    .cmp(&right.rank)
                    .then_with(|| left.family.cmp(&right.family))
            });
            let mut families = results
                .iter()
                .map(|result| result.family.clone())
                .collect::<Vec<_>>();
            families.sort();
            families.dedup();
            let mut support_tiers = results
                .iter()
                .map(|result| result.support_tier)
                .collect::<Vec<_>>();
            support_tiers.sort_unstable();
            support_tiers.dedup();
            let mut readiness = results
                .iter()
                .map(|result| result.readiness)
                .collect::<Vec<_>>();
            readiness.sort_unstable();
            readiness.dedup();
            TabRuntimeProbePhaseExecution {
                phase,
                selector_ids: results
                    .iter()
                    .map(|result| result.selector_id.clone())
                    .collect(),
                families,
                support_tiers,
                readiness,
                executed_selectors: results
                    .iter()
                    .filter(|result| result.readiness != TabRuntimeProbeExecutionReadiness::Skipped)
                    .count(),
                aggregate_group_count: results
                    .iter()
                    .map(|result| result.aggregate_group_count)
                    .sum(),
                aggregate_unit_count: results
                    .iter()
                    .map(|result| result.aggregate_unit_count)
                    .sum(),
                grouping_rule: phase.grouping_rule().to_string(),
                stop_condition: phase.stop_condition().to_string(),
            }
        })
        .collect()
}

fn execute_metadata_shape_probe(
    summary: &TabRuntimeProbeFamilySummary,
) -> (usize, usize, String, String) {
    let group_count = summary.metadata_shape_supports.len();
    let unit_count = summary
        .metadata_shape_supports
        .iter()
        .map(|support| support.support_count)
        .sum::<usize>();
    let strongest = summary
        .metadata_shape_supports
        .first()
        .map(|support| {
            format!(
                "{}:{} support-count observations ({} per mille)",
                support.label, support.support_count, support.per_mille
            )
        })
        .unwrap_or_else(|| "no bounded candidate metadata-shape support group".to_string());
    let summary_text = format!(
        "aggregate execution grouped {group_count} candidate metadata-shape labels covering {unit_count} support-count observations across {} parsed archives; strongest {strongest}; {}",
        summary.parsed_archives,
        format_runtime_probe_entropy_progression(summary)
    );
    (group_count, unit_count, strongest, summary_text)
}

fn execute_classifier_grouping_probe(
    summary: &TabRuntimeProbeFamilySummary,
) -> (usize, usize, String, String) {
    let groups = [
        (
            "command-stream-heavy candidate group",
            summary.command_stream_chunks,
        ),
        ("likely raw indexed candidate group", summary.raw_chunks),
        ("unknown candidate group", summary.unknown_chunks),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .collect::<Vec<_>>();
    let group_count = groups.len();
    let unit_count = groups.iter().map(|(_, count)| *count).sum::<usize>();
    let strongest = groups
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(label, count)| {
            format!(
                "{label}:{count} chunks ({} per mille)",
                ratio_per_mille(*count, summary.total_chunks)
            )
        })
        .unwrap_or_else(|| "no classifier group selected".to_string());
    let summary_text = format!(
        "aggregate execution separated {group_count} classifier groups covering {unit_count} chunks; command/raw/unknown per mille {}/{}/{}; size bands small/medium/large {}/{}/{}; strongest {strongest}",
        ratio_per_mille(summary.command_stream_chunks, summary.total_chunks),
        ratio_per_mille(summary.raw_chunks, summary.total_chunks),
        ratio_per_mille(summary.unknown_chunks, summary.total_chunks),
        summary.small_chunks,
        summary.medium_chunks,
        summary.large_chunks
    );
    (group_count, unit_count, strongest, summary_text)
}

fn execute_fixed_length_bucket_probe(
    summary: &TabRuntimeProbeFamilySummary,
) -> (usize, usize, String, String) {
    let buckets = runtime_length_bucket_counts(summary);
    let group_count = buckets.len();
    let unit_count = buckets.iter().map(|(_, count)| *count).sum::<usize>();
    let strongest = buckets
        .first()
        .map(|(len, count)| format!("{len} byte chunk-length bucket:{count} chunks"))
        .unwrap_or_else(|| "no repeated chunk-length bucket selected".to_string());
    let summary_text = format!(
        "aggregate execution grouped {group_count} repeated chunk-length bucket labels covering {unit_count} chunk observations; {}; strongest {strongest}",
        format_runtime_progression_probe_support(summary)
    );
    (group_count, unit_count, strongest, summary_text)
}

fn execute_sibling_common_bucket_probe(
    summary: &TabRuntimeProbeFamilySummary,
) -> (usize, usize, String, String) {
    let group_count = summary.common_bucket_overlap.len();
    let unit_count = summary
        .common_bucket_overlap
        .iter()
        .map(|overlap| overlap.archive_count)
        .sum::<usize>();
    let strongest = summary
        .common_bucket_overlap
        .first()
        .map(|overlap| {
            format!(
                "{} byte common-size bucket:{} archive-support observations",
                overlap.len, overlap.archive_count
            )
        })
        .unwrap_or_else(|| "no common-size bucket overlap selected".to_string());
    let summary_text = format!(
        "aggregate execution compared {group_count} common-size bucket labels with {unit_count} archive-support observations; common bucket support [{}]; strongest {strongest}",
        format_runtime_family_common_bucket_overlap(summary)
    );
    (group_count, unit_count, strongest, summary_text)
}

fn execute_mixed_unknown_audit_probe(
    summary: &TabRuntimeProbeFamilySummary,
) -> (usize, usize, String, String) {
    let groups = [
        ("likely raw indexed candidate group", summary.raw_chunks),
        ("unknown candidate group", summary.unknown_chunks),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .collect::<Vec<_>>();
    let group_count = groups.len();
    let unit_count = groups.iter().map(|(_, count)| *count).sum::<usize>();
    let strongest = groups
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(label, count)| {
            format!(
                "{label}:{count} chunks ({} per mille of all family chunks)",
                ratio_per_mille(*count, summary.total_chunks)
            )
        })
        .unwrap_or_else(|| "no mixed/raw/unknown group selected".to_string());
    let summary_text = format!(
        "aggregate execution audited {group_count} mixed/raw/unknown groups covering {unit_count} chunks; raw+unknown {} per mille; command-stream baseline {} per mille; strongest {strongest}",
        ratio_per_mille(
            summary.raw_chunks.saturating_add(summary.unknown_chunks),
            summary.total_chunks
        ),
        ratio_per_mille(summary.command_stream_chunks, summary.total_chunks)
    );
    (group_count, unit_count, strongest, summary_text)
}

fn runtime_length_bucket_counts(summary: &TabRuntimeProbeFamilySummary) -> Vec<(u32, usize)> {
    let mut buckets = BTreeMap::new();
    for archive in &summary.included_archive_summaries {
        for bucket in &archive.bank.common_chunk_len_buckets {
            *buckets.entry(bucket.len).or_insert(0usize) += bucket.count;
        }
    }
    let mut buckets = buckets.into_iter().collect::<Vec<_>>();
    buckets.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    buckets.truncate(5);
    buckets
}

fn execution_readiness(
    tier: TabRuntimeProbeSupportTier,
    group_count: usize,
    unit_count: usize,
) -> TabRuntimeProbeExecutionReadiness {
    if group_count == 0 || unit_count == 0 {
        TabRuntimeProbeExecutionReadiness::Skipped
    } else if tier == TabRuntimeProbeSupportTier::Limited {
        TabRuntimeProbeExecutionReadiness::Limited
    } else {
        TabRuntimeProbeExecutionReadiness::Ready
    }
}

fn execution_limitation() -> &'static str {
    "runtime-only aggregate dry-run; not proof of decoded layout or semantics; no bytes, raw headers/chunks, previews, dimensions, anchors, commands, audio, UI, or gameplay semantics emitted"
}

fn format_capped_id_list(ids: &[String]) -> String {
    let mut formatted = ids
        .iter()
        .take(MAX_PHASE_SELECTOR_IDS)
        .map(|id| format!("`{id}`"))
        .collect::<Vec<_>>();
    if ids.len() > MAX_PHASE_SELECTOR_IDS {
        formatted.push(format!(
            "{} more capped selectors",
            ids.len() - MAX_PHASE_SELECTOR_IDS
        ));
    }
    formatted.join("; ")
}

fn sort_runtime_probe_selectors(selectors: &mut [TabRuntimeProbeSelector]) {
    selectors.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.family.cmp(&right.family))
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.focus.cmp(&right.focus))
    });
}

fn assign_runtime_probe_selector_ids(selectors: &mut [TabRuntimeProbeSelector]) {
    for (index, selector) in selectors.iter_mut().enumerate() {
        selector.rank = index + 1;
        selector.id = format_runtime_probe_selector_id(
            &selector.family,
            selector.category,
            selector.support_tier,
            selector.rank,
        );
    }
}

pub fn format_runtime_probe_selector_id(
    family: &str,
    category: TabRuntimeProbeCategory,
    tier: TabRuntimeProbeSupportTier,
    rank: usize,
) -> String {
    format!(
        "tab-sprite-{}-{}-{}-r{rank:02}",
        family.to_ascii_lowercase(),
        category.slug(),
        tier.slug()
    )
}

fn format_runtime_probe_archive_inclusion(summary: &TabRuntimeProbeFamilySummary) -> String {
    let cap_status = if summary.parsed_archives > summary.included_archives {
        format!(
            "capped at {}/{} parsed archives",
            summary.included_archives, summary.parsed_archives
        )
    } else {
        format!(
            "included {}/{} parsed archives",
            summary.included_archives, summary.parsed_archives
        )
    };
    format!(
        "top selected `{}` sprite-like family candidate; {cap_status}; per-family runtime manifest cap {MAX_ARCHIVES_PER_FAMILY} archives",
        summary.family
    )
}

fn format_runtime_metadata_probe_group(summary: &TabRuntimeProbeFamilySummary) -> String {
    if summary.metadata_shape_supports.is_empty() {
        return "no bounded candidate leading metadata-shape groups selected".to_string();
    }
    summary
        .metadata_shape_supports
        .iter()
        .take(3)
        .map(|support| {
            format!(
                "{}:{} chunks ({} per mille)",
                support.label, support.support_count, support.per_mille
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_runtime_length_bucket_probe_group(summary: &TabRuntimeProbeFamilySummary) -> String {
    let mut buckets = BTreeMap::new();
    for archive in &summary.included_archive_summaries {
        for bucket in &archive.bank.common_chunk_len_buckets {
            *buckets.entry(bucket.len).or_insert(0usize) += bucket.count;
        }
    }
    let mut buckets = buckets
        .into_iter()
        .map(|(len, count)| (count, len))
        .collect::<Vec<_>>();
    buckets.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    buckets.truncate(4);
    if buckets.is_empty() {
        return "no repeated chunk-length buckets selected".to_string();
    }
    format!(
        "candidate chunk-length buckets [{}]",
        buckets
            .into_iter()
            .map(|(count, len)| format!("{len} bytes:{count} chunks"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn format_runtime_progression_probe_support(summary: &TabRuntimeProbeFamilySummary) -> String {
    let chunk_min = summary
        .included_archive_summaries
        .iter()
        .map(|archive| archive.bank.chunk_count)
        .min()
        .unwrap_or(0);
    let chunk_max = summary
        .included_archive_summaries
        .iter()
        .map(|archive| archive.bank.chunk_count)
        .max()
        .unwrap_or(0);
    format!(
        "chunk count range {chunk_min}..{chunk_max}; equal-size run archives {}/{}; repeated size-pattern archives {}/{}",
        summary.equal_run_archives,
        summary.parsed_archives,
        summary.repeated_pattern_archives,
        summary.parsed_archives
    )
}

fn format_runtime_probe_entropy_progression(summary: &TabRuntimeProbeFamilySummary) -> String {
    let entropy_spread = summary
        .max_entropy_milli_bits
        .saturating_sub(summary.min_entropy_milli_bits);
    let progression = if summary.equal_run_archives == summary.parsed_archives
        && summary.repeated_pattern_archives == summary.parsed_archives
    {
        "progression consistent across all parsed archives"
    } else if summary.equal_run_archives > 0 || summary.repeated_pattern_archives > 0 {
        "partial progression consistency"
    } else {
        "limited progression consistency"
    };
    format!(
        "{progression}; equal-size runs {}/{} archives; repeated size-pattern candidates {}/{} archives; entropy spread {:.3} bits",
        summary.equal_run_archives,
        summary.parsed_archives,
        summary.repeated_pattern_archives,
        summary.parsed_archives,
        entropy_spread as f32 / 1000.0
    )
}

fn format_runtime_family_common_bucket_overlap(summary: &TabRuntimeProbeFamilySummary) -> String {
    if summary.common_bucket_overlap.is_empty() {
        return "no repeated common-size buckets across parsed archives".to_string();
    }
    summary
        .common_bucket_overlap
        .iter()
        .map(|overlap| {
            format!(
                "{} bytes in {} archives",
                overlap.len, overlap.archive_count
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn sum_runtime_family_kind(summaries: &[&TabArchiveSummary], kind: SpriteChunkKind) -> usize {
    summaries
        .iter()
        .flat_map(|summary| summary.sprite_bank.kind_aggregates.iter())
        .filter(|aggregate| aggregate.kind == kind)
        .map(|aggregate| aggregate.count)
        .sum()
}

fn runtime_family_common_bucket_overlap(
    summaries: &[&TabArchiveSummary],
) -> Vec<TabRuntimeProbeCommonBucketOverlap> {
    let mut counts = BTreeMap::new();
    for summary in summaries {
        for bucket in &summary.bank.common_chunk_len_buckets {
            *counts.entry(bucket.len).or_insert(0usize) += 1;
        }
    }
    let mut overlaps = counts
        .into_iter()
        .filter(|(_, archive_count)| *archive_count >= 2 || summaries.len() == 1)
        .map(|(len, archive_count)| TabRuntimeProbeCommonBucketOverlap { len, archive_count })
        .collect::<Vec<_>>();
    overlaps.sort_by(|left, right| {
        right
            .archive_count
            .cmp(&left.archive_count)
            .then_with(|| left.len.cmp(&right.len))
    });
    overlaps.truncate(5);
    overlaps
}

fn runtime_probe_progression_per_mille(summary: &TabRuntimeProbeFamilySummary) -> usize {
    ratio_per_mille(
        summary
            .equal_run_archives
            .saturating_add(summary.repeated_pattern_archives),
        summary.parsed_archives.saturating_mul(2),
    )
}

fn runtime_probe_length_bucket_support(summary: &TabRuntimeProbeFamilySummary) -> usize {
    summary
        .included_archive_summaries
        .iter()
        .flat_map(|archive| archive.bank.common_chunk_len_buckets.iter())
        .map(|bucket| bucket.count)
        .sum()
}

fn runtime_probe_common_bucket_score(summary: &TabRuntimeProbeFamilySummary) -> usize {
    let archive_support = summary
        .common_bucket_overlap
        .iter()
        .map(|bucket| bucket.archive_count)
        .sum::<usize>();
    let sibling_bonus = if summary.parsed_archives > 1 {
        1_000
    } else {
        0
    };
    sibling_bonus + archive_support * 400 + summary.common_bucket_overlap.len() * 100
}

fn runtime_probe_archive_score(summary: &TabRuntimeProbeFamilySummary) -> usize {
    ratio_per_mille(summary.included_archives, summary.parsed_archives)
}

fn runtime_probe_entropy_consistency_score(summary: &TabRuntimeProbeFamilySummary) -> usize {
    let spread = summary
        .max_entropy_milli_bits
        .saturating_sub(summary.min_entropy_milli_bits);
    if spread <= 500 {
        500
    } else if spread <= 1_500 {
        250
    } else {
        0
    }
}

fn support_tier_for_per_mille(value: usize) -> TabRuntimeProbeSupportTier {
    if value >= 600 {
        TabRuntimeProbeSupportTier::Strong
    } else if value >= 250 {
        TabRuntimeProbeSupportTier::Medium
    } else {
        TabRuntimeProbeSupportTier::Limited
    }
}

fn sibling_bucket_support_tier(
    summary: &TabRuntimeProbeFamilySummary,
) -> TabRuntimeProbeSupportTier {
    if summary.parsed_archives > 1 && summary.common_bucket_overlap.len() >= 3 {
        TabRuntimeProbeSupportTier::Strong
    } else if !summary.common_bucket_overlap.is_empty() {
        TabRuntimeProbeSupportTier::Medium
    } else {
        TabRuntimeProbeSupportTier::Limited
    }
}

impl TabRuntimeProbeFamilySummary {
    fn metadata_support_score(&self) -> usize {
        self.metadata_shape_supports
            .iter()
            .map(|support| support.support_count)
            .sum()
    }

    fn progression_score(&self) -> usize {
        self.equal_run_archives + self.repeated_pattern_archives
    }
}

fn is_runtime_sprite_like_family_candidate(summary: &TabRuntimeProbeFamilySummary) -> bool {
    summary.family != "SOUND"
        && summary.family != "OTHER"
        && (summary.metadata_support_score() > 0
            || summary.command_stream_chunks > 0
            || summary.raw_chunks > 0)
}

fn tab_probe_family(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_uppercase();
    if name.starts_with("HSPR") {
        "HSPR"
    } else if name.starts_with("MSPR") {
        "MSPR"
    } else if name.starts_with("MFNT") {
        "MFNT"
    } else if name.starts_with("FONT") {
        "FONT"
    } else if name.starts_with("SOUND") || name.starts_with("GSOUND") || name.starts_with("ISNDS") {
        "SOUND"
    } else {
        "OTHER"
    }
}

fn has_tab_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tab"))
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn ratio_per_mille(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_mul(1000) / denominator
}

#[cfg(test)]
mod tests {
    use super::{
        TabRuntimeProbeArchiveInput, TabRuntimeProbeCategory, TabRuntimeProbeExecution,
        TabRuntimeProbeExecutionReadiness, TabRuntimeProbeManifest, TabRuntimeProbeSupportTier,
        format_runtime_probe_selector_id,
    };
    use crate::engine::tab_bank::{TabArchive, TabVariantAnalysis};

    #[test]
    fn builds_manifest_from_synthetic_archives_without_bytes() {
        let hspr_a = make_archive_input(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_archive_input(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_archive_input(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let sound = make_archive_input(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let manifest = TabRuntimeProbeManifest::from_archive_inputs([hspr_a, hspr_b, mspr, sound]);
        let joined = manifest
            .selectors
            .iter()
            .map(|selector| {
                format!(
                    "{} {} {} {}",
                    selector.id,
                    selector.family,
                    selector.aggregate_evidence,
                    selector.stop_conditions
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!manifest.selectors.is_empty());
        assert!(manifest.selectors.len() <= 15);
        assert!(manifest.phases.len() >= 4);
        assert!(manifest.compact_status().contains("TAB probe manifest"));
        assert!(manifest.family_summary().contains("HSPR"));
        assert!(!manifest.family_summary().contains("SOUND"));
        assert!(joined.contains("tab-sprite-"));
        assert!(joined.contains("runtime manifest cap 4 archives"));
        assert!(joined.contains("do not"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_stable_selector_ids_from_aggregate_terms() {
        let id = format_runtime_probe_selector_id(
            "HSPR",
            TabRuntimeProbeCategory::MetadataShape,
            TabRuntimeProbeSupportTier::Strong,
            1,
        );
        assert_eq!(id, "tab-sprite-hspr-metadata-shape-strong-r01");
    }

    #[test]
    fn caps_manifest_families_and_preserves_phase_order() {
        let mut inputs = (0..6)
            .map(|index| {
                make_archive_input(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        inputs.push(make_archive_input(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        inputs.push(make_archive_input(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        inputs.push(make_archive_input(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let manifest = TabRuntimeProbeManifest::from_archive_inputs(inputs);
        let phase_labels = manifest
            .phases
            .iter()
            .map(|phase| phase.phase.label())
            .collect::<Vec<_>>();

        assert!(manifest.selectors.len() <= 15);
        assert!(manifest.selected_families <= 3);
        assert!(manifest.total_candidate_families >= manifest.selected_families);
        assert!(
            manifest
                .selectors
                .iter()
                .any(|selector| selector.aggregate_evidence.contains("capped at 4/6"))
        );
        assert_eq!(phase_labels.first(), Some(&"metadata-shape grouping"));
        assert!(phase_labels.contains(&"fallback mixed/unknown audit"));
        assert!(
            manifest
                .selectors
                .iter()
                .all(|selector| !selector.family.contains("SOUND"))
        );
    }

    #[test]
    fn empty_manifest_has_conservative_status() {
        let manifest = TabRuntimeProbeManifest::from_archive_inputs([]);
        assert!(manifest.selectors.is_empty());
        assert!(manifest.phases.is_empty());
        assert!(
            manifest
                .compact_status()
                .contains("no aggregate runtime selectors available")
        );
        assert_eq!(manifest.family_summary(), "none");
        assert_eq!(manifest.phase_summary(), "no dry-run phases");
    }

    #[test]
    fn executes_manifest_selectors_as_aggregate_dry_runs_without_bytes() {
        let inputs = vec![
            make_archive_input(
                "SYNDICAT/DATA/HSPR-1.TAB",
                vec![
                    chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                ],
            ),
            make_archive_input(
                "DATADISK/DATA/HSPR-1.TAB",
                vec![
                    chunk_with_prefix([24, 16, 0xf0, 0], 128),
                    chunk_with_prefix([24, 16, 0xf0, 0], 128),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                ],
            ),
            make_archive_input(
                "DATADISK/DATA/MSPR-0-D.TAB",
                vec![
                    chunk_with_prefix([8, 12, 1, 1], 64),
                    chunk_with_prefix([10, 12, 1, 1], 64),
                    (1..=80).collect::<Vec<u8>>(),
                    (2..=81).collect::<Vec<u8>>(),
                ],
            ),
            make_archive_input(
                "SYNDICAT/DATA/SOUND-0.TAB",
                vec![chunk_with_prefix([97, 116, 0, 0], 64)],
            ),
        ];
        let execution = TabRuntimeProbeExecution::from_archive_inputs(inputs);
        let joined = execution
            .selector_results
            .iter()
            .map(|result| {
                format!(
                    "{} {} {} {} {}",
                    result.selector_id,
                    result.family,
                    result.readiness.label(),
                    result.execution_summary,
                    result.conservative_limitation
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!execution.selector_results.is_empty());
        assert!(execution.selector_results.len() <= 15);
        assert!(execution.phase_results.len() >= 4);
        assert!(execution.compact_status().contains("TAB probe execution"));
        assert!(execution.readiness_summary().contains("aggregate dry-run"));
        assert!(joined.contains("aggregate execution"));
        assert!(joined.contains("tab-sprite-"));
        assert!(joined.contains("not proof of decoded layout or semantics"));
        assert!(!joined.contains("SOUND"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_execution_and_preserves_phase_order() {
        let mut inputs = (0..6)
            .map(|index| {
                make_archive_input(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        inputs.push(make_archive_input(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        inputs.push(make_archive_input(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        inputs.push(make_archive_input(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let execution = TabRuntimeProbeExecution::from_archive_inputs(inputs);
        let phase_labels = execution
            .phase_results
            .iter()
            .map(|phase| phase.phase.label())
            .collect::<Vec<_>>();

        assert!(execution.selector_results.len() <= 15);
        assert_eq!(phase_labels.first(), Some(&"metadata-shape grouping"));
        assert!(phase_labels.contains(&"fallback mixed/unknown audit"));
        assert!(
            execution
                .selector_results
                .iter()
                .any(|result| result.archive_scope.contains("capped at 4/6"))
        );
        assert!(
            execution
                .selector_results
                .iter()
                .all(|result| result.family != "SOUND")
        );
        assert!(execution.phase_results.iter().all(|phase| {
            phase.executed_selectors > 0
                && phase.aggregate_group_count > 0
                && phase.aggregate_unit_count > 0
        }));
    }

    #[test]
    fn empty_execution_has_conservative_status() {
        let execution = TabRuntimeProbeExecution::from_archive_inputs([]);
        assert!(execution.selector_results.is_empty());
        assert!(execution.phase_results.is_empty());
        assert_eq!(execution.executed_selectors, 0);
        assert_eq!(execution.skipped_selectors, 0);
        assert!(
            execution
                .compact_status()
                .contains("no aggregate selector executions available")
        );
        assert_eq!(
            execution.readiness_summary(),
            "no execution readiness results"
        );
        assert_eq!(execution.phase_summary(), "no execution phases");
    }

    #[test]
    fn marks_missing_family_selector_execution_as_skipped() {
        let manifest = TabRuntimeProbeManifest::from_archive_inputs([make_archive_input(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![chunk_with_prefix([16, 16, 0xf0, 0], 128)],
        )]);
        let execution = TabRuntimeProbeExecution::from_manifest_and_archive_inputs(manifest, []);

        assert!(
            execution
                .selector_results
                .iter()
                .any(|result| result.readiness == TabRuntimeProbeExecutionReadiness::Skipped)
        );
        assert!(
            execution
                .selector_results
                .iter()
                .any(|result| result.execution_summary.contains("skipped"))
        );
    }

    fn make_archive_input(path: &str, chunks: Vec<Vec<u8>>) -> TabRuntimeProbeArchiveInput {
        let mut dat = Vec::new();
        let mut offsets = Vec::new();
        for chunk in chunks {
            offsets.push(dat.len() as u32);
            dat.extend(chunk);
        }
        offsets.push(dat.len() as u32);
        let tab = offsets
            .iter()
            .flat_map(|offset| offset.to_le_bytes())
            .collect::<Vec<_>>();
        let archive = TabArchive::parse(&tab, dat.clone()).unwrap();
        let analysis = TabVariantAnalysis::analyze(&tab, dat.len());
        assert!(analysis.best().is_some());

        TabRuntimeProbeArchiveInput {
            path: path.to_string(),
            summary: archive.aggregate_summary(),
        }
    }

    fn chunk_with_prefix(prefix: [u8; 4], len: usize) -> Vec<u8> {
        let mut chunk = vec![1; len];
        chunk[..prefix.len()].copy_from_slice(&prefix);
        chunk
    }
}
