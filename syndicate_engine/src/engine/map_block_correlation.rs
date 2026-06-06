//! Runtime-local MAP-to-block/tile candidate addressability diagnostics.
//!
//! This module does not decode or render source graphics. It compares
//! diagnostic MAP candidate byte fields against aggregate block/tile container
//! record-count candidates so the engine can visualize which MAP values are
//! addressable by local user-supplied containers.

use std::{fs, path::Path};

use walkdir::WalkDir;

use crate::engine::{
    block_decode::{BlockGraphicsAnalysis, BlockIndexPlausibility, BlockRecordCandidate},
    map_decode::{MapCandidateEvidenceConfidence, MapCandidateField, MapCandidateFieldEvidence},
    map_scene::MapDiagnosticScene,
};

pub const MAP_BLOCK_CORRELATION_CONTAINER_CAP: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapBlockCorrelationScene {
    pub containers_seen: usize,
    pub containers_used: usize,
    pub candidates: Vec<MapBlockFieldCorrelation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapBlockFieldCorrelation {
    pub field: MapCandidateField,
    pub container: String,
    pub container_status: String,
    pub record_candidate: Option<BlockRecordCandidate>,
    pub plausibility: BlockIndexPlausibility,
    pub lane: usize,
    pub min_value: u8,
    pub max_value: u8,
    pub unique_values: usize,
    pub baseline: u8,
    pub confidence: MapCandidateEvidenceConfidence,
    pub continuity_percent: u8,
    pub repeated_2x2_percent: u8,
    pub gentle_gradient_percent: u8,
    pub addressable_cells: usize,
    pub out_of_range_cells: usize,
    pub baseline_cells: usize,
    pub total_cells: usize,
    pub score: i32,
}

impl MapBlockCorrelationScene {
    pub fn from_root(root: impl AsRef<Path>, scene: &MapDiagnosticScene) -> Option<Self> {
        let root = root.as_ref();
        let mut analyses = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_uppercase();
            if !is_block_graphics_candidate(&name) {
                continue;
            }
            if let Ok(data) = fs::read(path) {
                analyses.push((
                    display_relative(root, path),
                    BlockGraphicsAnalysis::analyze_file_bytes(&data),
                ));
            }
        }

        Self::from_block_analyses(scene, analyses)
    }

    pub fn from_block_analyses(
        scene: &MapDiagnosticScene,
        mut analyses: Vec<(String, BlockGraphicsAnalysis)>,
    ) -> Option<Self> {
        if analyses.is_empty() {
            return None;
        }

        let containers_seen = analyses.len();
        analyses.sort_by(|left, right| {
            block_analysis_priority(&right.1, &right.0)
                .cmp(&block_analysis_priority(&left.1, &left.0))
        });
        analyses.truncate(MAP_BLOCK_CORRELATION_CONTAINER_CAP);
        analyses.sort_by(|left, right| left.0.cmp(&right.0));

        let mut candidates = Vec::new();
        for (container, analysis) in analyses {
            for evidence in scene.field_evidence {
                candidates.push(MapBlockFieldCorrelation::from_scene_and_analysis(
                    scene,
                    evidence,
                    container.clone(),
                    &analysis,
                ));
            }
        }

        candidates.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| {
                    left.field
                        .provisional_label()
                        .cmp(right.field.provisional_label())
                })
                .then_with(|| left.container.cmp(&right.container))
        });

        Some(Self {
            containers_seen,
            containers_used: candidates
                .iter()
                .map(|candidate| candidate.container.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            candidates,
        })
    }

    pub fn selected_candidate(&self) -> Option<&MapBlockFieldCorrelation> {
        self.candidates.first()
    }

    pub fn selected_field(&self) -> Option<MapCandidateField> {
        self.selected_candidate().map(|candidate| candidate.field)
    }

    pub fn container_cap_label(&self) -> String {
        if self.containers_seen > self.containers_used {
            format!(
                "{} of {} block/tile containers used; capped aggregate runtime overlay",
                self.containers_used, self.containers_seen
            )
        } else {
            format!(
                "{} block/tile containers used for aggregate runtime overlay",
                self.containers_used
            )
        }
    }

    pub fn status_label(&self) -> String {
        match self.selected_candidate() {
            Some(candidate) => format!(
                "aggregate block-addressability candidate: {}; {}; runtime-only, not proof of decoded layout or semantics",
                candidate.focus_label(),
                self.container_cap_label()
            ),
            None => "aggregate block-addressability candidate unavailable; runtime-only, not proof of decoded layout or semantics".to_string(),
        }
    }
}

impl MapBlockFieldCorrelation {
    fn from_scene_and_analysis(
        scene: &MapDiagnosticScene,
        evidence: MapCandidateFieldEvidence,
        container: String,
        analysis: &BlockGraphicsAnalysis,
    ) -> Self {
        let record_candidate = analysis.best_aligned_record_candidate();
        let record_count = record_candidate.map(|candidate| candidate.record_count);
        let (min_value, max_value, unique_values, baseline_cells) =
            field_distribution(scene, evidence.field, evidence.baseline);
        let plausibility = correlate_range(min_value, max_value, record_count);
        let addressable_cells = count_addressable_cells(scene, evidence.field, record_count);
        let total_cells = scene.cell_count();
        let out_of_range_cells = record_count
            .map(|_| total_cells.saturating_sub(addressable_cells))
            .unwrap_or(0);
        let score = score_correlation(
            plausibility,
            evidence.confidence,
            unique_values,
            evidence.continuity_percent,
            evidence.repeated_2x2_percent,
            evidence.gentle_gradient_percent,
            addressable_cells,
            total_cells,
            record_count,
        );

        Self {
            field: evidence.field,
            container,
            container_status: analysis.container_label(),
            record_candidate,
            plausibility,
            lane: evidence.lane,
            min_value,
            max_value,
            unique_values,
            baseline: evidence.baseline,
            confidence: evidence.confidence,
            continuity_percent: evidence.continuity_percent,
            repeated_2x2_percent: evidence.repeated_2x2_percent,
            gentle_gradient_percent: evidence.gentle_gradient_percent,
            addressable_cells,
            out_of_range_cells,
            baseline_cells,
            total_cells,
            score,
        }
    }

    pub fn record_count(&self) -> Option<usize> {
        self.record_candidate
            .map(|candidate| candidate.record_count)
    }

    pub fn is_value_addressable(&self, value: u8) -> Option<bool> {
        self.record_count()
            .map(|record_count| (value as usize) < record_count)
    }

    pub fn addressable_percent(&self) -> u8 {
        percent(self.addressable_cells, self.total_cells)
    }

    pub fn focus_label(&self) -> String {
        format!(
            "{} against `{}` {}",
            self.field.provisional_label(),
            self.container,
            self.record_label()
        )
    }

    pub fn value_range_label(&self) -> String {
        format!(
            "b{} 0x{:02x}..0x{:02x}, {} unique, baseline 0x{:02x}",
            self.lane, self.min_value, self.max_value, self.unique_values, self.baseline
        )
    }

    pub fn addressability_label(&self) -> String {
        match self.record_count() {
            Some(record_count) => format!(
                "{} of {} cells addressable ({}%, {} out of range) by {} records",
                self.addressable_cells,
                self.total_cells,
                self.addressable_percent(),
                self.out_of_range_cells,
                record_count
            ),
            None => "addressability unknown; no aligned fixed-size record candidate".to_string(),
        }
    }

    pub fn evidence_summary(&self) -> String {
        format!(
            "{}; {}; confidence {}; continuity {}%, repeated 2x2 {}%, gentle gradient {}%; {}",
            self.value_range_label(),
            self.plausibility.label(),
            self.confidence.label(),
            self.continuity_percent,
            self.repeated_2x2_percent,
            self.gentle_gradient_percent,
            self.addressability_label()
        )
    }

    pub fn runtime_limit_label(&self) -> &'static str {
        "runtime-only aggregate addressability; not proof of decoded layout or semantics"
    }

    pub fn record_label(&self) -> String {
        self.record_candidate
            .map(|candidate| candidate.label())
            .unwrap_or_else(|| "with no aligned fixed-size record candidate".to_string())
    }
}

fn block_analysis_priority<'a>(
    analysis: &BlockGraphicsAnalysis,
    label: &'a str,
) -> (u8, usize, usize, &'a str) {
    let best = analysis.best_aligned_record_candidate();
    (
        u8::from(best.is_some()),
        best.map(|candidate| candidate.record_count)
            .unwrap_or_default(),
        analysis.decoded_len,
        label,
    )
}

fn is_block_graphics_candidate(name: &str) -> bool {
    name.ends_with(".DAT") && (name.contains("BLK") || name == "MMAP.DAT" || name == "MMAPOUT.DAT")
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn field_distribution(
    scene: &MapDiagnosticScene,
    field: MapCandidateField,
    baseline: u8,
) -> (u8, u8, usize, usize) {
    let mut min_value = u8::MAX;
    let mut max_value = u8::MIN;
    let mut seen = [false; 256];
    let mut unique_values = 0usize;
    let mut baseline_cells = 0usize;

    for y in 0..scene.height {
        for x in 0..scene.width {
            let Some(cell) = scene.cell(x, y) else {
                continue;
            };
            let value = cell.field_value(field);
            min_value = min_value.min(value);
            max_value = max_value.max(value);
            if !seen[value as usize] {
                seen[value as usize] = true;
                unique_values += 1;
            }
            if value == baseline {
                baseline_cells += 1;
            }
        }
    }

    if scene.cell_count() == 0 {
        (0, 0, 0, 0)
    } else {
        (min_value, max_value, unique_values, baseline_cells)
    }
}

fn count_addressable_cells(
    scene: &MapDiagnosticScene,
    field: MapCandidateField,
    record_count: Option<usize>,
) -> usize {
    let Some(record_count) = record_count else {
        return 0;
    };
    let mut count = 0usize;
    for y in 0..scene.height {
        for x in 0..scene.width {
            if scene
                .cell(x, y)
                .is_some_and(|cell| (cell.field_value(field) as usize) < record_count)
            {
                count += 1;
            }
        }
    }
    count
}

fn correlate_range(
    min_value: u8,
    max_value: u8,
    record_count: Option<usize>,
) -> BlockIndexPlausibility {
    if max_value < min_value {
        return BlockIndexPlausibility::Unknown;
    }
    match record_count {
        Some(record_count) if record_count > max_value as usize => {
            BlockIndexPlausibility::FitsRecordCount
        }
        Some(_) => BlockIndexPlausibility::OutOfRange,
        None => BlockIndexPlausibility::FitsByteRangeOnly,
    }
}

fn score_correlation(
    plausibility: BlockIndexPlausibility,
    confidence: MapCandidateEvidenceConfidence,
    unique_values: usize,
    continuity_percent: u8,
    repeated_2x2_percent: u8,
    gentle_gradient_percent: u8,
    addressable_cells: usize,
    total_cells: usize,
    record_count: Option<usize>,
) -> i32 {
    let plausibility_score = match plausibility {
        BlockIndexPlausibility::FitsRecordCount => 120,
        BlockIndexPlausibility::FitsByteRangeOnly => 35,
        BlockIndexPlausibility::Unknown => 0,
        BlockIndexPlausibility::OutOfRange => -80,
    };
    let confidence_score = match confidence {
        MapCandidateEvidenceConfidence::High => 35,
        MapCandidateEvidenceConfidence::Medium => 18,
        MapCandidateEvidenceConfidence::Low => 4,
    };
    let addressability_score = if record_count.is_some() {
        percent(addressable_cells, total_cells) as i32
    } else {
        0
    };

    plausibility_score
        + confidence_score
        + unique_values.min(64) as i32
        + continuity_percent as i32 / 4
        + repeated_2x2_percent as i32 / 5
        + gentle_gradient_percent as i32 / 8
        + addressability_score
}

fn percent(part: usize, total: usize) -> u8 {
    if total == 0 {
        0
    } else {
        ((part * 100 + total / 2) / total).min(100) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{MAP_BLOCK_CORRELATION_CONTAINER_CAP, MapBlockCorrelationScene};
    use crate::engine::{
        block_decode::{BlockGraphicsAnalysis, BlockGraphicsContainer},
        map_decode::{
            MapCandidateEvidenceConfidence, MapCandidateField, MapCandidateFieldEvidence,
            MapInferredLayerPreview, MapPrimarySubstrateCandidate, MapSignaturePreview,
        },
        map_scene::MapDiagnosticScene,
    };

    #[test]
    fn ranks_high_confidence_addressable_field_before_out_of_range_fields() {
        let scene = make_scene();
        let fitting =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, vec![0u8; 512]);
        let tiny =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, vec![0u8; 64]);

        let correlation = MapBlockCorrelationScene::from_block_analyses(
            &scene,
            vec![
                ("SYNDICAT/DATA/TINYBLK.DAT".to_string(), tiny),
                ("SYNDICAT/DATA/MMAPBLK.DAT".to_string(), fitting),
            ],
        )
        .unwrap();
        let selected = correlation.selected_candidate().unwrap();

        assert_eq!(selected.field, MapCandidateField::SurfaceIndex);
        assert_eq!(selected.container, "SYNDICAT/DATA/MMAPBLK.DAT");
        assert_eq!(selected.addressable_cells, scene.cell_count());
        assert!(selected.score > 200);
        assert!(
            selected
                .evidence_summary()
                .contains("fits candidate record count")
        );
    }

    #[test]
    fn caps_containers_and_uses_conservative_runtime_language() {
        let scene = make_scene();
        let analyses = (0..(MAP_BLOCK_CORRELATION_CONTAINER_CAP + 3))
            .map(|idx| {
                (
                    format!("SYNDICAT/DATA/CAP{idx:02}BLK.DAT"),
                    BlockGraphicsAnalysis::analyze_decoded(
                        BlockGraphicsContainer::Plain,
                        vec![idx as u8; 512],
                    ),
                )
            })
            .collect();

        let correlation = MapBlockCorrelationScene::from_block_analyses(&scene, analyses).unwrap();

        assert_eq!(
            correlation.containers_used,
            MAP_BLOCK_CORRELATION_CONTAINER_CAP
        );
        assert!(correlation.containers_seen > correlation.containers_used);
        assert!(correlation.status_label().contains("aggregate"));
        assert!(correlation.status_label().contains("runtime-only"));
        assert!(correlation.status_label().contains("not proof"));
    }

    #[test]
    fn formatting_stays_aggregate_and_non_reconstructable() {
        let scene = make_scene();
        let analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, vec![0u8; 512]);
        let correlation = MapBlockCorrelationScene::from_block_analyses(
            &scene,
            vec![("SYNDICAT/DATA/MMAPBLK.DAT".to_string(), analysis)],
        )
        .unwrap();
        let selected = correlation.selected_candidate().unwrap();

        assert!(selected.value_range_label().contains("0x00..0x03"));
        assert!(
            selected
                .addressability_label()
                .contains("cells addressable")
        );
        assert_eq!(
            selected.runtime_limit_label(),
            "runtime-only aggregate addressability; not proof of decoded layout or semantics"
        );
        assert!(!selected.evidence_summary().contains("[0, 1, 2, 3]"));
        assert!(!selected.evidence_summary().contains("pixel"));
    }

    fn make_scene() -> MapDiagnosticScene {
        let inferred = MapInferredLayerPreview {
            width: 4,
            height: 4,
            cells: vec![1; 16],
            height_classes: vec![0; 16],
            surface_values: (0..16).map(|idx| (idx % 4) as u8).collect(),
            detail_values: vec![70; 16],
            reference_values: vec![2; 16],
            height_values: vec![1; 16],
            visual_classes: 2,
            class_counts: [0, 16, 0, 0, 0, 0],
            surface_baseline: 0,
            detail_baseline: 70,
            reference_baseline: 2,
            height_lane: 1,
            height_baseline: 1,
            surface_unique: 4,
            detail_unique: 1,
            reference_unique: 1,
            height_unique: 1,
        };
        let substrate = MapPrimarySubstrateCandidate {
            width: 4,
            height: 4,
            surface_index_candidate: (0..16).map(|idx| (idx % 4) as u8).collect(),
            detail_index_candidate: vec![70; 16],
            reference_candidate: vec![2; 16],
            height_candidate: vec![1; 16],
            field_evidence: [
                evidence(
                    MapCandidateField::SurfaceIndex,
                    0,
                    0,
                    4,
                    MapCandidateEvidenceConfidence::High,
                ),
                evidence(
                    MapCandidateField::DetailIndex,
                    4,
                    70,
                    1,
                    MapCandidateEvidenceConfidence::Low,
                ),
                evidence(
                    MapCandidateField::Reference,
                    8,
                    2,
                    1,
                    MapCandidateEvidenceConfidence::Medium,
                ),
                evidence(
                    MapCandidateField::Height,
                    1,
                    1,
                    1,
                    MapCandidateEvidenceConfidence::High,
                ),
            ],
        };
        let signature = MapSignaturePreview {
            width: 4,
            height: 4,
            cells: vec![0; 16],
            visual_classes: 1,
            unique_signatures: 1,
            dominant_signature_cells: 16,
        };
        MapDiagnosticScene::from_candidates(Some(&signature), &inferred, &substrate).unwrap()
    }

    fn evidence(
        field: MapCandidateField,
        lane: usize,
        baseline: u8,
        unique_values: usize,
        confidence: MapCandidateEvidenceConfidence,
    ) -> MapCandidateFieldEvidence {
        MapCandidateFieldEvidence {
            field,
            lane,
            baseline,
            unique_values,
            continuity_percent: 80,
            repeated_2x2_percent: 65,
            gentle_gradient_percent: 70,
            confidence,
        }
    }
}
