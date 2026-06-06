//! Runtime diagnostic scene model for decoded MAP primary-cell candidates.
//!
//! This is a local render scaffold, not a semantic city-map decoder. It keeps
//! the stable 64x64 primary-cell evidence in memory so the engine can render a
//! provisional scene without changing gameplay/pathfinding.

use crate::engine::map_decode::{
    MapCandidateField, MapCandidateFieldEvidence, MapInferredLayerPreview,
    MapPrimarySubstrateCandidate, MapSignaturePreview,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDiagnosticScene {
    pub width: usize,
    pub height: usize,
    cells: Vec<MapDiagnosticSceneCell>,
    pub field_evidence: [MapCandidateFieldEvidence; 4],
    pub visual_classes: usize,
    pub signature_classes: usize,
    pub unique_signatures: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapDiagnosticSceneCell {
    pub visual_class: u8,
    pub height_class: u8,
    pub signature_class: Option<u8>,
    pub surface_index_candidate: u8,
    pub detail_index_candidate: u8,
    pub reference_candidate: u8,
    pub height_candidate: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapDiagnosticSceneLayer {
    Inferred,
    Signature,
    CandidateField(MapCandidateField),
}

impl MapDiagnosticScene {
    pub fn from_candidates(
        signature: Option<&MapSignaturePreview>,
        inferred: &MapInferredLayerPreview,
        substrate: &MapPrimarySubstrateCandidate,
    ) -> Option<Self> {
        if inferred.width != substrate.width || inferred.height != substrate.height {
            return None;
        }
        if signature.is_some_and(|signature| {
            signature.width != inferred.width || signature.height != inferred.height
        }) {
            return None;
        }

        let mut cells = Vec::with_capacity(inferred.width * inferred.height);
        for y in 0..inferred.height {
            for x in 0..inferred.width {
                let inferred_cell = inferred.cell(x, y)?;
                cells.push(MapDiagnosticSceneCell {
                    visual_class: inferred_cell.visual_class,
                    height_class: inferred_cell.height_class,
                    signature_class: signature.and_then(|signature| signature.cell(x, y)),
                    surface_index_candidate: substrate.field_value(
                        MapCandidateField::SurfaceIndex,
                        x,
                        y,
                    )?,
                    detail_index_candidate: substrate.field_value(
                        MapCandidateField::DetailIndex,
                        x,
                        y,
                    )?,
                    reference_candidate: substrate.field_value(
                        MapCandidateField::Reference,
                        x,
                        y,
                    )?,
                    height_candidate: substrate.field_value(MapCandidateField::Height, x, y)?,
                });
            }
        }

        Some(Self {
            width: inferred.width,
            height: inferred.height,
            cells,
            field_evidence: substrate.field_evidence,
            visual_classes: inferred.visual_classes,
            signature_classes: signature
                .map(|signature| signature.visual_classes)
                .unwrap_or_default(),
            unique_signatures: signature
                .map(|signature| signature.unique_signatures)
                .unwrap_or_default(),
        })
    }

    pub fn cell(&self, x: usize, y: usize) -> Option<MapDiagnosticSceneCell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y * self.width + x).copied()
    }

    pub fn field_evidence(&self, field: MapCandidateField) -> Option<MapCandidateFieldEvidence> {
        self.field_evidence
            .iter()
            .copied()
            .find(|evidence| evidence.field == field)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn status_label(&self) -> String {
        let evidence = self
            .field_evidence
            .iter()
            .map(|evidence| {
                format!(
                    "{}:{}",
                    evidence.field.provisional_label(),
                    evidence.confidence.label()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}x{} diagnostic MAP scene; {} cells; inferred classes {}; signature classes {}; unique signatures {}; evidence [{}]",
            self.width,
            self.height,
            self.cell_count(),
            self.visual_classes,
            self.signature_classes,
            self.unique_signatures,
            evidence
        )
    }
}

impl MapDiagnosticSceneCell {
    pub fn field_value(self, field: MapCandidateField) -> u8 {
        match field {
            MapCandidateField::SurfaceIndex => self.surface_index_candidate,
            MapCandidateField::DetailIndex => self.detail_index_candidate,
            MapCandidateField::Reference => self.reference_candidate,
            MapCandidateField::Height => self.height_candidate,
        }
    }
}

impl MapDiagnosticSceneLayer {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inferred => "decoded MAP inferred scene",
            Self::Signature => "decoded MAP signature scene",
            Self::CandidateField(field) => field.provisional_label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MapDiagnosticScene, MapDiagnosticSceneLayer};
    use crate::engine::map_decode::{
        MapCandidateEvidenceConfidence, MapCandidateField, MapCandidateFieldEvidence,
        MapInferredLayerPreview, MapPrimarySubstrateCandidate, MapSignaturePreview,
    };

    #[test]
    fn builds_scene_from_candidate_layers_without_bytes() {
        let inferred = make_inferred(2, 2);
        let substrate = make_substrate(2, 2);
        let signature = MapSignaturePreview {
            width: 2,
            height: 2,
            cells: vec![0, 1, 1, 2],
            visual_classes: 3,
            unique_signatures: 4,
            dominant_signature_cells: 2,
        };

        let scene =
            MapDiagnosticScene::from_candidates(Some(&signature), &inferred, &substrate).unwrap();

        assert_eq!(scene.width, 2);
        assert_eq!(scene.height, 2);
        assert_eq!(scene.cell_count(), 4);
        assert_eq!(scene.visual_classes, 3);
        assert_eq!(scene.signature_classes, 3);
        assert_eq!(
            scene
                .cell(1, 0)
                .unwrap()
                .field_value(MapCandidateField::DetailIndex),
            21
        );
        assert_eq!(
            scene
                .field_evidence(MapCandidateField::SurfaceIndex)
                .unwrap()
                .confidence,
            MapCandidateEvidenceConfidence::High
        );
        assert!(scene.status_label().contains("diagnostic MAP scene"));
        assert!(scene.status_label().contains("evidence"));
    }

    #[test]
    fn rejects_mismatched_candidate_dimensions() {
        let inferred = make_inferred(2, 2);
        let substrate = make_substrate(2, 3);

        assert!(MapDiagnosticScene::from_candidates(None, &inferred, &substrate).is_none());
    }

    #[test]
    fn scene_layer_labels_remain_provisional() {
        assert_eq!(
            MapDiagnosticSceneLayer::CandidateField(MapCandidateField::SurfaceIndex).label(),
            "surface_index_candidate"
        );
        assert!(
            MapDiagnosticSceneLayer::Inferred
                .label()
                .contains("inferred")
        );
    }

    fn make_inferred(width: usize, height: usize) -> MapInferredLayerPreview {
        MapInferredLayerPreview {
            width,
            height,
            cells: vec![0, 1, 2, 3],
            height_classes: vec![0, 1, 2, 3],
            surface_values: vec![10, 11, 12, 13],
            detail_values: vec![20, 21, 22, 23],
            reference_values: vec![30, 31, 32, 33],
            height_values: vec![40, 41, 42, 43],
            visual_classes: 3,
            class_counts: [1, 1, 1, 1, 0, 0],
            surface_baseline: 10,
            detail_baseline: 20,
            reference_baseline: 30,
            height_lane: 1,
            height_baseline: 40,
            surface_unique: 4,
            detail_unique: 4,
            reference_unique: 4,
            height_unique: 4,
        }
    }

    fn make_substrate(width: usize, height: usize) -> MapPrimarySubstrateCandidate {
        MapPrimarySubstrateCandidate {
            width,
            height,
            surface_index_candidate: vec![10, 11, 12, 13, 14, 15],
            detail_index_candidate: vec![20, 21, 22, 23, 24, 25],
            reference_candidate: vec![30, 31, 32, 33, 34, 35],
            height_candidate: vec![40, 41, 42, 43, 44, 45],
            field_evidence: [
                evidence(
                    MapCandidateField::SurfaceIndex,
                    MapCandidateEvidenceConfidence::High,
                ),
                evidence(
                    MapCandidateField::DetailIndex,
                    MapCandidateEvidenceConfidence::Medium,
                ),
                evidence(
                    MapCandidateField::Reference,
                    MapCandidateEvidenceConfidence::High,
                ),
                evidence(
                    MapCandidateField::Height,
                    MapCandidateEvidenceConfidence::High,
                ),
            ],
        }
    }

    fn evidence(
        field: MapCandidateField,
        confidence: MapCandidateEvidenceConfidence,
    ) -> MapCandidateFieldEvidence {
        MapCandidateFieldEvidence {
            field,
            lane: 0,
            baseline: 0,
            unique_values: 4,
            continuity_percent: 90,
            repeated_2x2_percent: 80,
            gentle_gradient_percent: 75,
            confidence,
        }
    }
}
