use std::{collections::BTreeMap, path::PathBuf};
use walkdir::WalkDir;

use crate::engine::{
    formats::DecodeDiagnostics, map_block_correlation::MapBlockCorrelationScene,
    map_scene::MapDiagnosticScene, runtime_probe::TabRuntimeProbeManifest,
};

#[derive(Debug, Clone, Default)]
pub struct AssetIndex {
    root: PathBuf,
    counts: BTreeMap<String, usize>,
    maps: Vec<PathBuf>,
    missions: Vec<PathBuf>,
    palettes: Vec<PathBuf>,
    sprites: Vec<PathBuf>,
    sounds: Vec<PathBuf>,
    diagnostics: DecodeDiagnostics,
    map_scene: Option<MapDiagnosticScene>,
    map_block_correlation: Option<MapBlockCorrelationScene>,
    tab_probe_manifest: TabRuntimeProbeManifest,
    total_files: usize,
}

impl AssetIndex {
    pub fn discover(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let mut index = Self {
            root: root.clone(),
            ..Self::default()
        };

        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            index.total_files += 1;
            let path = entry.path().to_path_buf();
            let file = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_uppercase();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("NOEXT")
                .to_ascii_uppercase();
            *index.counts.entry(ext).or_insert(0) += 1;

            if file.starts_with("MAP") && file.ends_with(".DAT") {
                index.maps.push(path);
            } else if file.starts_with("MISS") && file.ends_with(".DAT") {
                index.missions.push(path);
            } else if file.contains("PAL") || file.starts_with("COL") {
                index.palettes.push(path);
            } else if file.contains("SPR") || file.ends_with(".ANI") {
                index.sprites.push(path);
            } else if file.contains("SOUND") || file.ends_with(".XMI") {
                index.sounds.push(path);
            }
        }

        index.diagnostics = DecodeDiagnostics::inspect(&root);
        index.map_scene = build_map_scene(&index.diagnostics);
        index.map_block_correlation = index
            .map_scene
            .as_ref()
            .and_then(|scene| MapBlockCorrelationScene::from_root(&root, scene));
        index.tab_probe_manifest = TabRuntimeProbeManifest::from_root(&root);
        index.maps.sort();
        index.missions.sort();
        index.palettes.sort();
        index.sprites.sort();
        index.sounds.sort();
        index
    }

    pub fn root_display(&self) -> String {
        self.root.display().to_string()
    }
    pub fn total_files(&self) -> usize {
        self.total_files
    }
    pub fn maps(&self) -> usize {
        self.maps.len()
    }
    pub fn missions(&self) -> usize {
        self.missions.len()
    }
    pub fn palettes(&self) -> usize {
        self.palettes.len()
    }
    pub fn sprites(&self) -> usize {
        self.sprites.len()
    }
    pub fn sounds(&self) -> usize {
        self.sounds.len()
    }
    pub fn diagnostics(&self) -> &DecodeDiagnostics {
        &self.diagnostics
    }
    pub fn map_scene(&self) -> Option<&MapDiagnosticScene> {
        self.map_scene.as_ref()
    }
    pub fn map_block_correlation(&self) -> Option<&MapBlockCorrelationScene> {
        self.map_block_correlation.as_ref()
    }
    pub fn tab_probe_manifest(&self) -> &TabRuntimeProbeManifest {
        &self.tab_probe_manifest
    }

    pub fn sample_map_name(&self) -> &str {
        self.maps
            .first()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("no MAP*.DAT found")
    }
}

fn build_map_scene(diagnostics: &DecodeDiagnostics) -> Option<MapDiagnosticScene> {
    MapDiagnosticScene::from_candidates(
        diagnostics.map_preview.as_ref(),
        diagnostics.map_inferred_preview.as_ref()?,
        diagnostics.map_substrate_candidate.as_ref()?,
    )
}
