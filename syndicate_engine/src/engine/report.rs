//! Headless asset inspection reports for reverse-engineering original data.

use std::{collections::BTreeMap, fs, path::Path};

use walkdir::WalkDir;

use crate::engine::{
    block_decode::{BlockGraphicsAnalysis, BlockIndexPlausibility, correlate_map_value_range},
    map_decode::{
        ByteLaneSpatialStats, ByteLaneStats, MAP_CANDIDATE_DETAIL_LANE,
        MAP_CANDIDATE_REFERENCE_LANE, MAP_CANDIDATE_SURFACE_LANE, MAP_PRIMARY_SECTION_LEN,
        MapCandidateField, MapCandidateFieldEvidence, MapDatAnalysis, MapGlobalCorrelationAnalysis,
        MapInferredLayerPreview, MapPrimaryGridAnalysis, MapSpatialCorrelationAnalysis,
        analyze_payload, analyze_primary_sections, decode_map_payload_bytes,
    },
    palette_decode::Palette,
    rnc::RncBlock,
    sprite_decode::SpriteChunkInfo,
    tab_bank::{TabArchive, TabVariantAnalysis},
};

#[derive(Debug, Clone)]
pub struct AssetReport {
    root: String,
    total_files: usize,
    extension_counts: BTreeMap<String, usize>,
    compressed_rows: Vec<String>,
    map_rows: Vec<String>,
    map_diagnostic_rows: Vec<String>,
    map_global_summary: String,
    map_global_candidate_rows: Vec<String>,
    map_global_substrate_rows: Vec<String>,
    mission_rows: Vec<String>,
    palette_rows: Vec<String>,
    compressed_palette_rows: Vec<String>,
    tab_rows: Vec<String>,
    block_graphics_rows: Vec<String>,
    block_map_correlation_rows: Vec<String>,
}

impl AssetReport {
    pub fn generate(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut total_files = 0;
        let mut extension_counts = BTreeMap::new();
        let mut compressed_rows = Vec::new();
        let mut map_rows = Vec::new();
        let mut map_diagnostic_rows = Vec::new();
        let mut map_primary_sections = Vec::new();
        let mut mission_rows = Vec::new();
        let mut palette_rows = Vec::new();
        let mut compressed_palette_rows = Vec::new();
        let mut tab_rows = Vec::new();
        let mut block_graphics_rows = Vec::new();
        let mut block_analyses = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            total_files += 1;
            let path = entry.path();
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_uppercase();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("NOEXT")
                .to_ascii_uppercase();
            *extension_counts.entry(ext).or_insert(0) += 1;

            if name.starts_with("MAP") && name.ends_with(".DAT") {
                let (container, diagnostics, primary_section) = map_decode_report_fields(path);
                if let Some(primary_section) = primary_section {
                    map_primary_sections.push(primary_section);
                }
                map_rows.push(format!(
                    "| `{}` | {} | {} |",
                    display_relative(root, path),
                    size,
                    container
                ));
                map_diagnostic_rows.push(format!(
                    "| `{}` | {} | {} | {} | {} |",
                    display_relative(root, path),
                    diagnostics.word_summary,
                    diagnostics.byte_summary,
                    diagnostics.inferred_summary,
                    diagnostics.spatial_summary
                ));
            }

            if is_block_graphics_candidate(&name) {
                if let Ok(data) = fs::read(path) {
                    let analysis = BlockGraphicsAnalysis::analyze_file_bytes(&data);
                    block_graphics_rows.push(format!(
                        "| `{}` | {} | {} | {} | {} | {} |",
                        display_relative(root, path),
                        size,
                        analysis.container_label(),
                        analysis.decoded_len,
                        format_block_byte_summary(&analysis),
                        format_block_record_candidates(&analysis)
                    ));
                    block_analyses.push((display_relative(root, path), analysis));
                }
            }

            if name.starts_with("MISS") && name.ends_with(".DAT") {
                mission_rows.push(format!(
                    "| `{}` | {} | {} |",
                    display_relative(root, path),
                    size,
                    compressed_status(path)
                ));
            }

            if let Ok(data) = fs::read(path) {
                if let Some(block) = RncBlock::parse(&data) {
                    compressed_rows.push(format!(
                        "| `{}` | {} | {} |",
                        display_relative(root, path),
                        size,
                        rnc_decode_status(&block)
                    ));
                }
            }

            if name.contains("PAL") || name.starts_with("COL") {
                if let Ok(data) = fs::read(path) {
                    if let Some(palette) = Palette::decode_vga_6bit(&data) {
                        palette_rows.push(format!(
                            "| `{}` | {} | {} colours |",
                            display_relative(root, path),
                            data.len(),
                            palette.colors.len()
                        ));
                    } else if let Some(block) = RncBlock::parse(&data) {
                        let status = match block.decompress() {
                            Ok(decoded) => {
                                if let Some(palette) = Palette::decode_vga_6bit(&decoded) {
                                    palette_rows.push(format!(
                                        "| `{}` | {} | RNC method {} -> {} colours |",
                                        display_relative(root, path),
                                        data.len(),
                                        block.header.method,
                                        palette.colors.len()
                                    ));
                                    format!(
                                        "decoded to {}-byte VGA palette, unpacked CRC ok",
                                        decoded.len()
                                    )
                                } else {
                                    format!(
                                        "verified RNC, unpacked {} bytes; not a 768-byte VGA palette",
                                        decoded.len()
                                    )
                                }
                            }
                            Err(err) => format!("decompress error {err:?}"),
                        };
                        compressed_palette_rows.push(format!(
                            "| `{}` | {} | {} |",
                            display_relative(root, path),
                            data.len(),
                            format!("{}; {status}", block.diagnostic_summary())
                        ));
                    }
                }
            }

            if path
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tab"))
            {
                let dat_path = path.with_extension("DAT");
                if let (Ok(tab), Ok(dat)) = (fs::read(path), fs::read(&dat_path)) {
                    let analysis = TabVariantAnalysis::analyze(&tab, dat.len());
                    let best = analysis.summary();
                    let archive_summary = TabArchive::parse(&tab, dat.clone())
                        .map(|archive| {
                            let sprite = archive
                                .chunk(0)
                                .map(SpriteChunkInfo::inspect)
                                .map(|info| info.short_label())
                                .unwrap_or_else(|| "no first chunk".to_string());
                            format!(
                                "{} chunks; {}-{} bytes; {}",
                                archive.bank.entry_count(),
                                archive.bank.min_chunk_len().unwrap_or(0),
                                archive.bank.max_chunk_len().unwrap_or(0),
                                sprite
                            )
                        })
                        .unwrap_or_else(|| "not parsed as 32-bit archive".to_string());

                    tab_rows.push(format!(
                        "| `{}` | {} | {} | {} |",
                        display_relative(root, path),
                        tab.len(),
                        best,
                        archive_summary
                    ));
                }
            }
        }

        compressed_rows.sort();
        map_rows.sort();
        map_diagnostic_rows.sort();
        mission_rows.sort();
        palette_rows.sort();
        compressed_palette_rows.sort();
        tab_rows.sort();
        block_graphics_rows.sort();
        block_analyses.sort_by(|left, right| left.0.cmp(&right.0));

        let (
            map_global_summary,
            map_global_candidate_rows,
            map_global_substrate_rows,
            block_map_correlation_rows,
        ) = analyze_primary_sections(map_primary_sections.iter().map(Vec::as_slice))
            .map(|analysis| {
                (
                    format_global_map_summary(&analysis),
                    format_global_candidate_rows(&analysis),
                    format_global_substrate_rows(&analysis),
                    format_block_map_correlation_rows(&analysis, &block_analyses),
                )
            })
            .unwrap_or_else(|| {
                (
                    "no decoded MAP primary sections available".to_string(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )
            });

        Self {
            root: root.display().to_string(),
            total_files,
            extension_counts,
            compressed_rows,
            map_rows,
            map_diagnostic_rows,
            map_global_summary,
            map_global_candidate_rows,
            map_global_substrate_rows,
            mission_rows,
            palette_rows,
            compressed_palette_rows,
            tab_rows,
            block_graphics_rows,
            block_map_correlation_rows,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut markdown = String::new();
        markdown.push_str("# Generated asset inspection report\n\n");
        markdown.push_str("This report is generated from local original assets and should not include copyrighted asset bytes.\n\n");
        markdown.push_str(&format!("- Asset root: `{}`\n", self.root));
        markdown.push_str(&format!("- Total files scanned: {}\n", self.total_files));
        markdown.push_str(&format!("- Maps found: {}\n", self.map_rows.len()));
        markdown.push_str(&format!("- Missions found: {}\n", self.mission_rows.len()));
        markdown.push_str(&format!(
            "- RNC-compressed files found: {}\n",
            self.compressed_rows.len()
        ));
        markdown.push_str(&format!(
            "- VGA palettes decoded: {}\n",
            self.palette_rows.len()
        ));
        markdown.push_str(&format!(
            "- RNC-compressed palette candidates: {}\n",
            self.compressed_palette_rows.len()
        ));
        markdown.push_str(&format!(
            "- TAB/DAT pairs analyzed: {}\n\n",
            self.tab_rows.len()
        ));

        markdown.push_str("\n## File extension inventory\n\n");
        markdown.push_str("| Extension | Count |\n|---|---:|\n");
        for (ext, count) in &self.extension_counts {
            markdown.push_str(&format!("| `{ext}` | {count} |\n"));
        }

        markdown.push_str("\n## Map inventory\n\n");
        markdown.push_str("| File | Bytes | Container |\n|---|---:|---|\n");
        append_rows_or_empty(&mut markdown, &self.map_rows, "no MAP*.DAT files found", 3);

        markdown.push_str("\n## MAP primary-cell field diagnostics\n\n");
        markdown.push_str("These rows summarize aggregate diagnostics over each file's 64x64x12 primary section only. Word, byte-lane, and spatial-correlation names are provisional candidates, not final terrain semantics.\n\n");
        markdown.push_str("| File | Three u32 word ranges | Candidate byte lanes | Inferred layer preview | Candidate spatial correlation |\n|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.map_diagnostic_rows,
            "no MAP*.DAT diagnostics available",
            5,
        );

        markdown.push_str("\n## MAP global field-correlation diagnostics\n\n");
        markdown.push_str("These rows aggregate all decoded MAP primary sections found in the scanned tree. Candidate labels are evidence-backed by byte-lane frequency and spatial continuity only; they are not final terrain/building semantics.\n\n");
        markdown.push_str(&format!("- {}\n\n", self.map_global_summary));
        markdown.push_str("| Candidate field | Lane | Byte distribution | Spatial continuity | Common transitions | 2x2/block-like patterns | Height-gradient check |\n|---|---:|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.map_global_candidate_rows,
            "no global MAP candidate diagnostics available",
            7,
        );

        markdown.push_str("\n### MAP provisional substrate candidate evidence\n\n");
        markdown.push_str("This substrate view copies selected byte lanes into diagnostic-only channels and summarizes why each lane was selected. Confidence terms are heuristic evidence labels, not semantic proof.\n\n");
        markdown.push_str("| Candidate field | Selected lane | Baseline | Unique values | Evidence confidence | Selection rationale |\n|---|---:|---:|---:|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.map_global_substrate_rows,
            "no global MAP substrate-candidate evidence available",
            6,
        );

        markdown.push_str("\n## Block/tile graphics container candidates\n\n");
        markdown.push_str("These diagnostics inspect BLK-like containers using RNC status, decoded length, aggregate byte statistics, and plausible fixed-size indexed-pixel record counts only. They do not include pixel previews or byte dumps. Candidate dimensions are provisional and not yet proven render layouts.\n\n");
        markdown.push_str("| File | Bytes | Container | Decoded bytes | Aggregate byte summary | Fixed-size record candidates |\n|---|---:|---|---:|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.block_graphics_rows,
            "no BLK-like graphics candidates found",
            6,
        );

        markdown.push_str("\n### MAP substrate to block/tile candidate correlations\n\n");
        markdown.push_str("These rows compare MAP candidate byte-lane value ranges with possible block/tile record counts. A fit only means the observed byte range could address records in a candidate container; it is not proof of terrain, building, or object semantics.\n\n");
        markdown.push_str("| Candidate field | MAP lane/range | Block/tile container | Best aligned record candidate | Plausibility | Notes |\n|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.block_map_correlation_rows,
            "no MAP-to-block candidate correlations available",
            6,
        );

        markdown.push_str("\n## Mission inventory\n\n");
        markdown.push_str("| File | Bytes | Container |\n|---|---:|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.mission_rows,
            "no MISS*.DAT files found",
            3,
        );

        markdown.push_str("\n## RNC-compressed file inventory\n\n");
        markdown.push_str("| File | Bytes | RNC header |\n|---|---:|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.compressed_rows,
            "no RNC files found",
            3,
        );

        markdown.push_str("## Decoded palettes\n\n");
        markdown.push_str("| File | Bytes | Result |\n|---|---:|---|\n");
        if self.palette_rows.is_empty() {
            markdown.push_str("| _none_ | 0 | no compatible palette files found |\n");
        } else {
            markdown.push_str(&self.palette_rows.join("\n"));
            markdown.push('\n');
        }

        markdown.push_str("\n## Compressed palette candidates\n\n");
        markdown.push_str("| File | Bytes | RNC header |\n|---|---:|---|\n");
        if self.compressed_palette_rows.is_empty() {
            markdown.push_str("| _none_ | 0 | no RNC palette candidates found |\n");
        } else {
            markdown.push_str(&self.compressed_palette_rows.join("\n"));
            markdown.push('\n');
        }

        markdown.push_str("\n## TAB/DAT bank analysis\n\n");
        markdown.push_str("| TAB file | TAB bytes | Variant score | 32-bit archive summary |\n|---|---:|---|---|\n");
        if self.tab_rows.is_empty() {
            markdown.push_str("| _none_ | 0 | no paired files found | - |\n");
        } else {
            markdown.push_str(&self.tab_rows.join("\n"));
            markdown.push('\n');
        }

        markdown
    }
}

pub fn write_report(root: impl AsRef<Path>, output: impl AsRef<Path>) -> std::io::Result<()> {
    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }

    let report = AssetReport::generate(root);
    fs::write(output, report.to_markdown())
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn compressed_status(path: &Path) -> String {
    fs::read(path)
        .ok()
        .and_then(|data| RncBlock::parse(&data).map(|block| rnc_decode_status(&block)))
        .unwrap_or_else(|| "plain/unknown".to_string())
}

#[derive(Debug, Clone)]
struct MapReportDiagnostics {
    word_summary: String,
    byte_summary: String,
    inferred_summary: String,
    spatial_summary: String,
}

fn map_decode_report_fields(path: &Path) -> (String, MapReportDiagnostics, Option<Vec<u8>>) {
    let fallback = MapReportDiagnostics {
        word_summary: "-".to_string(),
        byte_summary: "-".to_string(),
        inferred_summary: "-".to_string(),
        spatial_summary: "-".to_string(),
    };

    let Ok(data) = fs::read(path) else {
        return ("unreadable".to_string(), fallback, None);
    };

    match decode_map_payload_bytes(&data) {
        Ok((container, payload)) => {
            let primary_section = payload
                .get(..MAP_PRIMARY_SECTION_LEN)
                .map(|primary| primary.to_vec());
            let analysis = MapDatAnalysis {
                container,
                payload: analyze_payload(&payload),
            };
            let diagnostics = match &analysis.payload.primary_grid {
                Some(grid) => MapReportDiagnostics {
                    word_summary: format_word_summary(grid),
                    byte_summary: format_candidate_byte_lanes(grid),
                    inferred_summary: analysis
                        .payload
                        .inferred_layer_preview
                        .as_ref()
                        .map(format_inferred_summary)
                        .unwrap_or_else(|| "unavailable".to_string()),
                    spatial_summary: analysis
                        .payload
                        .substrate_candidate
                        .as_ref()
                        .map(format_substrate_summary)
                        .or_else(|| {
                            analysis
                                .payload
                                .spatial_correlation
                                .as_ref()
                                .map(format_candidate_spatial_summary)
                        })
                        .unwrap_or_else(|| "unavailable".to_string()),
                },
                None => fallback.clone(),
            };
            (analysis.short_label(), diagnostics, primary_section)
        }
        Err(err) => (format!("map decode error {err:?}"), fallback, None),
    }
}

fn format_word_summary(grid: &MapPrimaryGridAnalysis) -> String {
    grid.word_stats
        .iter()
        .enumerate()
        .map(|(index, stats)| {
            format!(
                "w{index}=0x{:08x}..0x{:08x}, unique {}, zero {}",
                stats.min, stats.max, stats.unique_values, stats.zero_values
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_candidate_byte_lanes(grid: &MapPrimaryGridAnalysis) -> String {
    [0usize, 4, 8]
        .into_iter()
        .map(|index| format_byte_lane(index, &grid.byte_stats[index]))
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_byte_lane(index: usize, stats: &ByteLaneStats) -> String {
    let top_values = stats
        .top_values
        .iter()
        .map(|entry| format!("0x{:02x}:{}", entry.value, entry.count))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "b{index}=0x{:02x}..0x{:02x}, unique {}, zero {}, top [{}]",
        stats.min, stats.max, stats.unique_values, stats.zero_values, top_values
    )
}

fn format_inferred_summary(preview: &MapInferredLayerPreview) -> String {
    let class_counts = preview
        .class_counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(class, count)| {
            format!(
                "{}:{}",
                MapInferredLayerPreview::class_label(class as u8),
                *count
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}; class counts [{}]; unique low bytes w0/b0 {}, w1/b4 {}, w2/b8 {}, height b{} {}",
        preview.summary_label(),
        class_counts,
        preview.surface_unique,
        preview.detail_unique,
        preview.reference_unique,
        preview.height_lane,
        preview.height_unique
    )
}

fn format_candidate_spatial_summary(spatial: &MapSpatialCorrelationAnalysis) -> String {
    candidate_spatial_lanes(spatial)
        .into_iter()
        .map(|(field, stats)| {
            format!(
                "{} b{}: continuity {}% (right {}%, down {}%), uniform 2x2 {}%, repeated 2x2 {}%, gentle Δ<=1 {}%",
                field.provisional_label(),
                stats.lane,
                stats.continuity_percent(),
                stats.right_continuity_percent(),
                stats.down_continuity_percent(),
                stats.uniform_2x2_percent(),
                stats.repeated_2x2_percent(),
                stats.gentle_gradient_percent()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_global_map_summary(analysis: &MapGlobalCorrelationAnalysis) -> String {
    format!(
        "{} decoded MAP primary sections, {} total cells, {} unique exact 12-byte cells; global height candidate lane b{}; word ranges: {}",
        analysis.map_count,
        analysis.total_cells,
        analysis.unique_cells,
        analysis.spatial_correlation.height_candidate_lane,
        format_global_word_summary(analysis)
    )
}

fn format_global_word_summary(analysis: &MapGlobalCorrelationAnalysis) -> String {
    analysis
        .word_stats
        .iter()
        .enumerate()
        .map(|(index, stats)| {
            format!(
                "w{index}=0x{:08x}..0x{:08x}, unique {}",
                stats.min, stats.max, stats.unique_values
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_global_candidate_rows(analysis: &MapGlobalCorrelationAnalysis) -> Vec<String> {
    candidate_spatial_lanes(&analysis.spatial_correlation)
        .into_iter()
        .map(|(field, stats)| {
            format!(
                "| {} | b{} | {} | {} | {} | {} | {} |",
                field.provisional_label(),
                stats.lane,
                format_byte_lane(stats.lane, &analysis.byte_stats[stats.lane]),
                format_spatial_continuity(stats),
                format_transitions(stats),
                format_block_patterns(stats),
                format_height_gradient(stats)
            )
        })
        .collect()
}

fn format_global_substrate_rows(analysis: &MapGlobalCorrelationAnalysis) -> Vec<String> {
    analysis
        .substrate_evidence
        .iter()
        .map(|evidence| {
            format!(
                "| {} | b{} | 0x{:02x} | {} | {} | {} |",
                evidence.field.provisional_label(),
                evidence.lane,
                evidence.baseline,
                evidence.unique_values,
                evidence.confidence.label(),
                format_selection_rationale(*evidence)
            )
        })
        .collect()
}

fn format_substrate_summary(
    candidate: &crate::engine::map_decode::MapPrimarySubstrateCandidate,
) -> String {
    candidate
        .field_evidence
        .iter()
        .map(|evidence| evidence.evidence_label())
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_selection_rationale(evidence: MapCandidateFieldEvidence) -> String {
    let lane_family = match evidence.field {
        MapCandidateField::SurfaceIndex => {
            "word-0 low byte retained as provisional surface-index channel"
        }
        MapCandidateField::DetailIndex => {
            "word-1 low byte retained as provisional detail-index channel"
        }
        MapCandidateField::Reference => "word-2 low byte retained as provisional reference channel",
        MapCandidateField::Height => {
            "narrow varying non-low byte selected by height-lane heuristic and gradient checks"
        }
    };
    format!(
        "{lane_family}; continuity {}%, repeated 2x2 {}%, gentle Δ<=1 {}%",
        evidence.continuity_percent,
        evidence.repeated_2x2_percent,
        evidence.gentle_gradient_percent
    )
}

fn candidate_spatial_lanes(
    spatial: &MapSpatialCorrelationAnalysis,
) -> Vec<(MapCandidateField, &ByteLaneSpatialStats)> {
    MapCandidateField::ALL
        .into_iter()
        .filter_map(|field| {
            let lane = match field {
                MapCandidateField::SurfaceIndex => MAP_CANDIDATE_SURFACE_LANE,
                MapCandidateField::DetailIndex => MAP_CANDIDATE_DETAIL_LANE,
                MapCandidateField::Reference => MAP_CANDIDATE_REFERENCE_LANE,
                MapCandidateField::Height => spatial.height_candidate_lane,
            };
            spatial.byte_lanes.get(lane).map(|stats| (field, stats))
        })
        .collect()
}

fn format_spatial_continuity(stats: &ByteLaneSpatialStats) -> String {
    format!(
        "{}% same neighbours (right {}%, down {}%; {}/{})",
        stats.continuity_percent(),
        stats.right_continuity_percent(),
        stats.down_continuity_percent(),
        stats.same_neighbour_pairs(),
        stats.neighbour_pairs()
    )
}

fn format_transitions(stats: &ByteLaneSpatialStats) -> String {
    if stats.top_transitions.is_empty() {
        return "no non-identical transitions".to_string();
    }

    stats
        .top_transitions
        .iter()
        .map(|transition| {
            format!(
                "0x{:02x}->0x{:02x}:{}",
                transition.from, transition.to, transition.count
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_block_patterns(stats: &ByteLaneSpatialStats) -> String {
    let top_patterns = stats
        .top_2x2_patterns
        .iter()
        .map(|pattern| {
            format!(
                "[{:02x} {:02x}; {:02x} {:02x}]:{}",
                pattern.values[0],
                pattern.values[1],
                pattern.values[2],
                pattern.values[3],
                pattern.count
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "uniform {}% ({}/{}), repeated {}% across {} repeated patterns; top [{}]",
        stats.uniform_2x2_percent(),
        stats.uniform_2x2_blocks,
        stats.total_2x2_blocks,
        stats.repeated_2x2_percent(),
        stats.repeated_2x2_patterns,
        top_patterns
    )
}

fn format_height_gradient(stats: &ByteLaneSpatialStats) -> String {
    format!(
        "gentle Δ<=1 {}%, moderate Δ<=4 {}%, max Δ {}, mean Δ {:.3}",
        stats.gentle_gradient_percent(),
        stats.moderate_gradient_percent(),
        stats.max_abs_gradient,
        stats.mean_abs_gradient_milli as f32 / 1000.0
    )
}

fn is_block_graphics_candidate(name: &str) -> bool {
    name.ends_with(".DAT") && (name.contains("BLK") || name == "MMAP.DAT" || name == "MMAPOUT.DAT")
}

fn format_block_byte_summary(analysis: &BlockGraphicsAnalysis) -> String {
    let total = analysis.decoded_len.max(1);
    let zero_percent = analysis.byte_summary.zero_values * 100 / total;
    let dominant_percent = analysis.byte_summary.dominant_value_count * 100 / total;
    format!(
        "unique {}, zero {} ({}%), dominant {}%, entropy {:.3} bits/byte",
        analysis.byte_summary.unique_values,
        analysis.byte_summary.zero_values,
        zero_percent,
        dominant_percent,
        analysis.byte_summary.entropy_milli_bits as f32 / 1000.0
    )
}

fn format_block_record_candidates(analysis: &BlockGraphicsAnalysis) -> String {
    analysis
        .record_candidates
        .iter()
        .filter(|candidate| candidate.record_count > 0)
        .map(|candidate| {
            let alignment = if candidate.remainder == 0 {
                "aligned".to_string()
            } else {
                format!("rem {}", candidate.remainder)
            };
            format!(
                "{}x{}:{} records ({alignment})",
                candidate.width, candidate.height, candidate.record_count
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_block_map_correlation_rows(
    map_analysis: &MapGlobalCorrelationAnalysis,
    block_analyses: &[(String, BlockGraphicsAnalysis)],
) -> Vec<String> {
    if block_analyses.is_empty() {
        return Vec::new();
    }

    map_analysis
        .substrate_evidence
        .iter()
        .flat_map(|evidence| {
            let lane_stats = &map_analysis.byte_stats[evidence.lane];
            block_analyses.iter().map(move |(path, block_analysis)| {
                let best = block_analysis.best_aligned_record_candidate();
                let record_count = best.map(|candidate| candidate.record_count);
                let plausibility =
                    correlate_map_value_range(lane_stats.min, lane_stats.max, record_count);
                format!(
                    "| {} | b{} 0x{:02x}..0x{:02x} ({} unique) | `{}` | {} | {} | {} |",
                    evidence.field.provisional_label(),
                    evidence.lane,
                    lane_stats.min,
                    lane_stats.max,
                    lane_stats.unique_values,
                    path,
                    best.map(|candidate| candidate.label())
                        .unwrap_or_else(|| "no aligned fixed-size record candidate".to_string()),
                    plausibility.label(),
                    format_block_correlation_note(plausibility, evidence.field)
                )
            })
        })
        .collect()
}

fn format_block_correlation_note(
    plausibility: BlockIndexPlausibility,
    field: MapCandidateField,
) -> String {
    match plausibility {
        BlockIndexPlausibility::FitsRecordCount => format!(
            "candidate {} values are within the selected container's aligned record count; this is range evidence only",
            field.provisional_label()
        ),
        BlockIndexPlausibility::FitsByteRangeOnly => format!(
            "candidate {} values fit an 8-bit range, but no aligned record count was selected",
            field.provisional_label()
        ),
        BlockIndexPlausibility::OutOfRange => format!(
            "candidate {} values exceed the selected aligned record count",
            field.provisional_label()
        ),
        BlockIndexPlausibility::Unknown => "insufficient aggregate evidence".to_string(),
    }
}

fn rnc_decode_status(block: &RncBlock<'_>) -> String {
    let decode_status = match block.decompress() {
        Ok(decoded) => format!("unpacked CRC ok, decoded {} bytes", decoded.len()),
        Err(err) => format!("decompress error {err:?}"),
    };
    format!("{}; {decode_status}", block.diagnostic_summary())
}

fn append_rows_or_empty(markdown: &mut String, rows: &[String], empty: &str, columns: usize) {
    if rows.is_empty() {
        let blanks = if columns > 1 {
            format!("{} |", " - |".repeat(columns - 1))
        } else {
            String::new()
        };
        markdown.push_str(&format!("| _none_ |{blanks} {empty} |\n"));
    } else {
        markdown.push_str(&rows.join("\n"));
        markdown.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::AssetReport;

    #[test]
    fn empty_report_still_renders_markdown() {
        let report = AssetReport::generate("definitely-not-a-real-asset-dir");
        let markdown = report.to_markdown();
        assert!(markdown.contains("Generated asset inspection report"));
        assert!(markdown.contains("TAB/DAT bank analysis"));
        assert!(markdown.contains("File extension inventory"));
        assert!(markdown.contains("Map inventory"));
        assert!(markdown.contains("Block/tile graphics container candidates"));
        assert!(markdown.contains("MAP substrate to block/tile candidate correlations"));
    }
}
