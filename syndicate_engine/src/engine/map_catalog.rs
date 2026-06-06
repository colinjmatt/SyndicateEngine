//! Runtime catalog of safely renderable decoded MAP diagnostic scenes.
//!
//! Catalog entries keep only non-reconstructable scene/correlation diagnostics
//! derived from local user-supplied assets. They do not retain source payloads,
//! raw cell records, decoded object records, or original graphics.

use std::{fs, path::Path};

use crate::engine::{
    block_decode::BlockGraphicsAnalysis, map_block_correlation::MapBlockCorrelationScene,
    map_decode::MapDatAnalysis, map_scene::MapDiagnosticScene,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapDiagnosticSceneCatalog {
    entries: Vec<MapDiagnosticSceneEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDiagnosticSceneEntry {
    pub label: String,
    pub status: String,
    pub scene: MapDiagnosticScene,
    pub block_correlation: Option<MapBlockCorrelationScene>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDiagnosticSceneInput {
    pub label: String,
    pub status: String,
    pub scene: MapDiagnosticScene,
}

impl MapDiagnosticSceneCatalog {
    pub fn from_map_paths(
        root: impl AsRef<Path>,
        map_paths: &[std::path::PathBuf],
        block_analyses: Vec<(String, BlockGraphicsAnalysis)>,
    ) -> Self {
        let root = root.as_ref();
        let inputs = map_paths
            .iter()
            .filter_map(|path| {
                let data = fs::read(path).ok()?;
                let analysis = MapDatAnalysis::analyze_file_bytes(&data).ok()?;
                let scene = scene_from_analysis(&analysis)?;
                Some(MapDiagnosticSceneInput {
                    label: display_relative(root, path),
                    status: analysis.short_label(),
                    scene,
                })
            })
            .collect::<Vec<_>>();

        Self::from_scene_inputs(inputs, block_analyses)
    }

    pub fn from_scene_inputs(
        mut inputs: Vec<MapDiagnosticSceneInput>,
        block_analyses: Vec<(String, BlockGraphicsAnalysis)>,
    ) -> Self {
        inputs.sort_by(|left, right| left.label.cmp(&right.label));
        let entries = inputs
            .into_iter()
            .map(|input| {
                let block_correlation = MapBlockCorrelationScene::from_block_analyses(
                    &input.scene,
                    block_analyses.clone(),
                );
                MapDiagnosticSceneEntry {
                    label: input.label,
                    status: input.status,
                    scene: input.scene,
                    block_correlation,
                }
            })
            .collect();

        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entry(&self, index: usize) -> Option<&MapDiagnosticSceneEntry> {
        self.entries.get(index)
    }

    pub fn next_index(&self, current: usize) -> usize {
        if self.entries.is_empty() {
            0
        } else {
            (current + 1) % self.entries.len()
        }
    }

    pub fn previous_index(&self, current: usize) -> usize {
        if self.entries.is_empty() {
            0
        } else {
            (current + self.entries.len() - 1) % self.entries.len()
        }
    }

    pub fn status_label(&self) -> String {
        if self.entries.is_empty() {
            "decoded MAP scene catalog unavailable".to_string()
        } else {
            format!(
                "{} decoded MAP diagnostic scenes available; runtime-only, not proof of decoded layout or semantics",
                self.entries.len()
            )
        }
    }
}

impl MapDiagnosticSceneEntry {
    pub fn panel_label(&self, index: usize, total: usize) -> String {
        format!("{} ({}/{})", self.label, index + 1, total.max(1))
    }
}

fn scene_from_analysis(analysis: &MapDatAnalysis) -> Option<MapDiagnosticScene> {
    MapDiagnosticScene::from_candidates(
        analysis.payload.signature_preview.as_ref(),
        analysis.payload.inferred_layer_preview.as_ref()?,
        analysis.payload.substrate_candidate.as_ref()?,
    )
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{MapDiagnosticSceneCatalog, MapDiagnosticSceneInput};
    use crate::engine::{
        block_decode::{BlockGraphicsAnalysis, BlockGraphicsContainer},
        map_decode::{
            MapCandidateEvidenceConfidence, MapCandidateField, MapCandidateFieldEvidence,
            MapInferredLayerPreview, MapPrimarySubstrateCandidate, MapSignaturePreview,
        },
        map_scene::MapDiagnosticScene,
    };

    #[test]
    fn sorts_scene_entries_and_builds_per_map_block_correlations() {
        let block_analysis =
            BlockGraphicsAnalysis::analyze_decoded(BlockGraphicsContainer::Plain, vec![0u8; 512]);
        let catalog = MapDiagnosticSceneCatalog::from_scene_inputs(
            vec![
                input("SYNDICAT/DATA/MAP02.DAT"),
                input("SYNDICAT/DATA/MAP01.DAT"),
            ],
            vec![("SYNDICAT/DATA/MMAPBLK.DAT".to_string(), block_analysis)],
        );

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog.entry(0).unwrap().label, "SYNDICAT/DATA/MAP01.DAT");
        assert_eq!(catalog.entry(1).unwrap().label, "SYNDICAT/DATA/MAP02.DAT");
        assert!(
            catalog
                .entry(0)
                .unwrap()
                .block_correlation
                .as_ref()
                .unwrap()
                .selected_candidate()
                .is_some()
        );
        assert!(catalog.status_label().contains("runtime-only"));
        assert!(catalog.status_label().contains("not proof"));
    }

    #[test]
    fn catalog_navigation_wraps_without_panicking_on_empty_catalogs() {
        let empty = MapDiagnosticSceneCatalog::default();
        assert_eq!(empty.next_index(4), 0);
        assert_eq!(empty.previous_index(4), 0);
        assert_eq!(
            empty.status_label(),
            "decoded MAP scene catalog unavailable"
        );

        let catalog = MapDiagnosticSceneCatalog::from_scene_inputs(
            vec![input("MAP01.DAT"), input("MAP02.DAT")],
            Vec::new(),
        );
        assert_eq!(catalog.next_index(0), 1);
        assert_eq!(catalog.next_index(1), 0);
        assert_eq!(catalog.previous_index(0), 1);
    }

    #[test]
    fn panel_labels_remain_aggregate_and_concise() {
        let catalog =
            MapDiagnosticSceneCatalog::from_scene_inputs(vec![input("MAP01.DAT")], Vec::new());
        let entry = catalog.entry(0).unwrap();

        assert_eq!(entry.panel_label(0, catalog.len()), "MAP01.DAT (1/1)");
        assert!(!entry.panel_label(0, catalog.len()).contains("0, 1, 2, 3"));
    }

    fn input(label: &str) -> MapDiagnosticSceneInput {
        MapDiagnosticSceneInput {
            label: label.to_string(),
            status: "synthetic decoded MAP diagnostic scene; runtime-only".to_string(),
            scene: make_scene(),
        }
    }

    fn make_scene() -> MapDiagnosticScene {
        let inferred = MapInferredLayerPreview {
            width: 2,
            height: 2,
            cells: vec![1, 1, 2, 2],
            height_classes: vec![0, 1, 0, 1],
            surface_values: vec![0, 1, 2, 3],
            detail_values: vec![10, 10, 11, 11],
            reference_values: vec![3, 3, 3, 3],
            height_values: vec![1, 2, 1, 2],
            visual_classes: 3,
            class_counts: [0, 2, 2, 0, 0, 0],
            surface_baseline: 0,
            detail_baseline: 10,
            reference_baseline: 3,
            height_lane: 1,
            height_baseline: 1,
            surface_unique: 4,
            detail_unique: 2,
            reference_unique: 1,
            height_unique: 2,
        };
        let substrate = MapPrimarySubstrateCandidate {
            width: 2,
            height: 2,
            surface_index_candidate: vec![0, 1, 2, 3],
            detail_index_candidate: vec![10, 10, 11, 11],
            reference_candidate: vec![3, 3, 3, 3],
            height_candidate: vec![1, 2, 1, 2],
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
                    10,
                    2,
                    MapCandidateEvidenceConfidence::Medium,
                ),
                evidence(
                    MapCandidateField::Reference,
                    8,
                    3,
                    1,
                    MapCandidateEvidenceConfidence::Low,
                ),
                evidence(
                    MapCandidateField::Height,
                    1,
                    1,
                    2,
                    MapCandidateEvidenceConfidence::High,
                ),
            ],
        };
        let signature = MapSignaturePreview {
            width: 2,
            height: 2,
            cells: vec![0, 1, 1, 2],
            visual_classes: 3,
            unique_signatures: 3,
            dominant_signature_cells: 2,
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
            continuity_percent: 75,
            repeated_2x2_percent: 50,
            gentle_gradient_percent: 80,
            confidence,
        }
    }
}
