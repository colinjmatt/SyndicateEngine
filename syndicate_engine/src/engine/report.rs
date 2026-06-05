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
    runtime_probe::{TabRuntimeProbeArchiveInput, TabRuntimeProbeManifest},
    sprite_decode::{SpriteBankAggregateSummary, SpriteChunkKind, SpriteDistributionSummary},
    tab_bank::{TabArchive, TabArchiveSummary, TabBank, TabBankSummary, TabVariantAnalysis},
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
    tab_family_ranking_rows: Vec<String>,
    tab_family_hint_rows: Vec<String>,
    tab_family_archive_evidence_rows: Vec<String>,
    tab_family_dashboard_rows: Vec<String>,
    tab_family_runtime_probe_rows: Vec<String>,
    tab_family_runtime_probe_queue_rows: Vec<String>,
    tab_family_runtime_probe_selector_rows: Vec<String>,
    tab_family_runtime_probe_dry_run_rows: Vec<String>,
    tab_runtime_probe_manifest_rows: Vec<String>,
    tab_runtime_probe_manifest_phase_rows: Vec<String>,
    tab_family_comparison_rows: Vec<String>,
    block_graphics_rows: Vec<String>,
    block_map_correlation_rows: Vec<String>,
    block_cross_container_rows: Vec<String>,
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
        let mut tab_analyses = Vec::new();

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
                        "| `{}` | {} | {} | {} | {} | {} | {} |",
                        display_relative(root, path),
                        size,
                        analysis.container_label(),
                        analysis.decoded_len,
                        format_block_byte_summary(&analysis),
                        format_block_record_candidates(&analysis),
                        format_block_layout_probes(&analysis)
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
                    let tab_relative = display_relative(root, path);
                    let archive = TabArchive::parse(&tab, dat.clone());
                    let archive_summary = archive
                        .as_ref()
                        .map(|archive| format_tab_archive_summary(&archive.aggregate_summary()))
                        .unwrap_or_else(|| "not parsed as 32-bit archive".to_string());
                    let archive_summary_data = archive.as_ref().map(TabArchive::aggregate_summary);
                    tab_analyses.push(TabBankReportAnalysis {
                        path: tab_relative.clone(),
                        dat_len: dat.len(),
                        best_variant: analysis.best(),
                        archive_bank: archive.map(|archive| archive.bank),
                        archive_summary: archive_summary_data,
                    });

                    tab_rows.push(format!(
                        "| `{}` | {} | {} | {} |",
                        tab_relative,
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
        tab_analyses.sort_by(|left, right| left.path.cmp(&right.path));
        let tab_runtime_probe_manifest =
            tab_runtime_probe_manifest_from_report_analyses(&tab_analyses);

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
            tab_family_ranking_rows: format_tab_family_ranking_rows(&tab_analyses),
            tab_family_hint_rows: format_tab_family_hint_rows(&tab_analyses),
            tab_family_archive_evidence_rows: format_tab_family_archive_evidence_rows(
                &tab_analyses,
            ),
            tab_family_dashboard_rows: format_tab_family_investigation_dashboard_rows(
                &tab_analyses,
            ),
            tab_family_runtime_probe_rows: format_tab_family_runtime_probe_plan_rows(&tab_analyses),
            tab_family_runtime_probe_queue_rows: format_tab_family_runtime_probe_queue_rows(
                &tab_analyses,
            ),
            tab_family_runtime_probe_selector_rows:
                format_tab_family_runtime_probe_selector_catalog_rows(&tab_analyses),
            tab_family_runtime_probe_dry_run_rows: format_tab_family_runtime_probe_dry_run_rows(
                &tab_analyses,
            ),
            tab_runtime_probe_manifest_rows: format_tab_runtime_probe_manifest_rows(
                &tab_runtime_probe_manifest,
            ),
            tab_runtime_probe_manifest_phase_rows: format_tab_runtime_probe_manifest_phase_rows(
                &tab_runtime_probe_manifest,
            ),
            tab_family_comparison_rows: format_tab_family_comparison_rows(&tab_analyses),
            block_graphics_rows,
            block_map_correlation_rows,
            block_cross_container_rows: format_block_cross_container_rows(
                &block_analyses,
                &tab_analyses,
            ),
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
        markdown.push_str("These diagnostics inspect BLK-like containers using RNC status, decoded length, aggregate byte statistics, plausible fixed-size indexed-pixel record counts, and aggregate layout probes only. They do not include pixel previews or byte dumps. Candidate dimensions, table/header hints, and region labels are provisional and not yet proven render layouts.\n\n");
        markdown.push_str("| File | Bytes | Container | Decoded bytes | Aggregate byte summary | Fixed-size record candidates | Aggregate layout probes |\n|---|---:|---|---:|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.block_graphics_rows,
            "no BLK-like graphics candidates found",
            7,
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

        markdown.push_str("\n### Cross-container aggregate relation probes\n\n");
        markdown.push_str("These rows compare BLK-like and TAB/DAT containers using decoded/plain lengths, non-reconstructable content hashes, aggregate layout alignment support, duplicate-file status, chunk-size distributions, exact candidate byte-size matches, sprite classifier counts, and chunk-count compatibility only. Matching values are evidence for candidate relationships, not proof of a render format or semantic role.\n\n");
        markdown.push_str("| Probe group | Containers | Aggregate relation | Candidate evidence | Conservative note |\n|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.block_cross_container_rows,
            "no cross-container aggregate relation probes available",
            5,
        );

        markdown.push_str("\n### TAB/sprite family aggregate ranking candidates\n\n");
        markdown.push_str("These rows rank safely parsed TAB/DAT filename families using non-reconstructable aggregate evidence only: parsed archive counts, classifier totals, candidate metadata-shape support ratios, chunk-length progression support, entropy ranges, and overlapping common chunk-size buckets. The ranking is a prioritization aid for future clean-room decoding, not proof of sprite metadata, rendering, font, sound, or UI semantics.\n\n");
        markdown.push_str("| Family candidate | Parsed archives | Classifier totals | Candidate metadata-shape support | Progression support | Entropy range | Common bucket overlap | Conservative note |\n|---|---:|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_ranking_rows,
            "no safely parsed TAB/DAT family rankings available",
            7,
        );

        markdown.push_str("\n### TAB/sprite family next-investigation hints\n\n");
        markdown.push_str("These rows turn the aggregate family rankings into conservative, non-reconstructable inspection priorities. Hints identify which aggregate evidence to inspect next; they do not decode metadata fields, dimensions, anchors, commands, pixels, audio, UI, or game semantics.\n\n");
        markdown.push_str("| Family candidate | Inspection priority | Candidate metadata-shape to inspect next | Classifier/size-band pattern to prioritize | Chunk-length progression hint | Entropy/common-bucket hint | Conservative limitation |\n|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_hint_rows,
            "no aggregate TAB/sprite investigation hints available",
            7,
        );

        markdown.push_str("\n### TAB/sprite top-priority archive evidence rows\n\n");
        markdown.push_str("These rows expand the highest-priority aggregate family hints into capped per-archive evidence summaries. They include only archive paths, counts, ranges, classifier totals, progression clues, entropy, and common-size buckets. They do not expose chunk bytes, raw headers, decoded dimensions, anchors, commands, pixels, audio, UI, or gameplay semantics.\n\n");
        markdown.push_str("| Family candidate | Archive | Chunk and size-band evidence | Candidate metadata-shape evidence | Classifier totals by size band | Chunk-length progression clue | Entropy/common-bucket summary | Conservative limitation |\n|---|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_archive_evidence_rows,
            "no capped per-archive TAB/sprite evidence rows available",
            8,
        );

        markdown.push_str("\n### TAB/sprite investigation dashboard\n\n");
        markdown.push_str("This compact dashboard combines capped within-family aggregate consistency, selection rationale, and conservative runtime-only next probes for the top selected sprite-like filename families. It uses only counts, ranges, ratios, entropy summaries, progression labels, and common-size bucket overlap/distinction. Hypotheses are clean-room investigation prompts, not decoded layouts or semantics.\n\n");
        markdown.push_str("| Family candidate | Archive inclusion and cap | Selection rationale | Within-family aggregate consistency | Runtime-only next probes | Conservative limitation |\n|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_dashboard_rows,
            "no aggregate TAB/sprite investigation dashboard rows available",
            6,
        );

        markdown.push_str("\n### TAB/sprite runtime-probe planning diagnostics\n\n");
        markdown.push_str("These rows convert the top selected TAB/sprite dashboard families into capped aggregate-only runtime probe plans. They identify safe local-only probe groups by chunk-length buckets, candidate leading metadata-shape support, classifier grouping, and sibling common-bucket comparison. They do not decode or render assets and do not expose bytes, raw headers/chunks, decoded dimensions, anchors, commands, audio, UI, or gameplay semantics.\n\n");
        markdown.push_str("| Family candidate | Probe category | Archive inclusion and cap | Aggregate probe group | Support summary | Runtime-only probe note | Conservative limitation |\n|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_runtime_probe_rows,
            "no aggregate TAB/sprite runtime-probe plans available",
            7,
        );

        markdown.push_str("\n### TAB/sprite cross-family runtime-probe queue diagnostics\n\n");
        markdown.push_str("These rows rank a capped cross-family aggregate probe queue from the selected TAB/sprite dashboard families. Queue order uses counts, ratios, ranges, archive inclusion/capping status, candidate metadata-shape support, classifier ratios, repeated chunk-length bucket support, sibling common-bucket overlap, and entropy/progression consistency only. It is a local clean-room runtime priority list, not decoded layout or semantics.\n\n");
        markdown.push_str("| Queue rank | Family candidate | Recommended aggregate probe focus | Evidence summary | Probe ordering rationale | Conservative limitation |\n|---:|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_runtime_probe_queue_rows,
            "no aggregate TAB/sprite runtime-probe queue entries available",
            6,
        );

        markdown.push_str("\n### TAB/sprite runtime probe selector catalog\n\n");
        markdown.push_str("These rows assign stable aggregate selector IDs to the capped TAB/sprite runtime-probe workbench tasks. Selector IDs are derived only from family candidate, probe category, support tier, and capped aggregate rank. They are local runtime dry-run handles, not decoded asset identifiers.\n\n");
        markdown.push_str("| Selector ID | Family candidate | Support tier | Aggregate selector focus | Aggregate evidence | Runtime preconditions | Stop conditions | Conservative limitation |\n|---|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_runtime_probe_selector_rows,
            "no aggregate TAB/sprite runtime probe selectors available",
            8,
        );

        markdown.push_str("\n### TAB/sprite local runtime dry-run ordering\n\n");
        markdown.push_str("These rows group the selector catalog into capped local-only dry-run phases. Phase ordering keeps metadata grouping, classifier separation, repeated-length buckets, sibling/common-bucket checks, and mixed/unknown audits separate before any decode or render attempt.\n\n");
        markdown.push_str("| Dry-run phase | Selector IDs | Aggregate phase evidence | Phase ordering rationale | Runtime stop condition | Conservative limitation |\n|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_runtime_probe_dry_run_rows,
            "no aggregate TAB/sprite runtime dry-run phases available",
            6,
        );

        markdown.push_str("\n### TAB/sprite runtime probe manifest summary\n\n");
        markdown.push_str("These rows summarize the runtime-facing local probe manifest that the engine and debug tools can build from user-supplied original TAB/DAT assets. The manifest shares the same capped aggregate selector model as the report workbench and contains only counts, family labels, selector IDs, support tiers, phase names, grouping rules, and guardrails.\n\n");
        markdown.push_str("| Manifest scope | Aggregate counts | Families and support | Dry-run phase summary | Runtime guardrails | Conservative limitation |\n|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_runtime_probe_manifest_rows,
            "no aggregate TAB/sprite runtime probe manifest available",
            6,
        );

        markdown.push_str("\n### TAB/sprite runtime probe manifest phases\n\n");
        markdown.push_str("These rows expose the capped dry-run manifest phases that a local runtime probe tool would attempt before any decoder or renderer experiment. Phase rows are aggregate-only and do not include source bytes, chunk data, decoded dimensions, anchors, commands, previews, audio, UI, or gameplay semantics.\n\n");
        markdown.push_str("| Manifest phase | Selector IDs | Families | Support tiers | Aggregate grouping rule | Runtime stop condition | Conservative limitation |\n|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_runtime_probe_manifest_phase_rows,
            "no aggregate TAB/sprite runtime probe manifest phases available",
            7,
        );

        markdown.push_str("\n### TAB/sprite family aggregate comparison candidates\n\n");
        markdown.push_str("These rows compare the top-ranked sprite-like filename-family candidates using aggregate ratios and bucket sets only. Output is capped to the top three selected families to avoid noisy all-pairs listings. Ratio differences, progression differences, entropy ranges, and common-size bucket overlap are compatibility clues for prioritizing clean-room decoding; they do not decode metadata, commands, dimensions, anchors, pixels, audio, or UI semantics.\n\n");
        markdown.push_str("| Family pair | Candidate metadata-shape ratio differences | Classifier ratio differences | Progression support difference | Entropy comparison | Common bucket comparison | Strongest shared compatibility clues | Strongest distinguishing clues | Conservative note |\n|---|---|---|---|---|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.tab_family_comparison_rows,
            "no aggregate TAB/sprite family comparisons available",
            9,
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
        markdown.push_str("These diagnostics summarize paired TAB/DAT banks using offset-width scoring, bounded 32-bit archive chunks when safely parsed, chunk-size distributions, offset sanity counts, fixed tile-byte candidate matches, and aggregate sprite chunk classifier counts only. They do not render sprites, expose chunk bytes, or prove final sprite/tile semantics.\n\n");
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

#[derive(Debug, Clone)]
struct TabBankReportAnalysis {
    path: String,
    dat_len: usize,
    best_variant: Option<crate::engine::tab_bank::TabVariantScore>,
    archive_bank: Option<TabBank>,
    archive_summary: Option<TabArchiveSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabFamilyRankingSummary {
    family: &'static str,
    parsed_archives: usize,
    total_chunks: usize,
    command_stream_chunks: usize,
    raw_chunks: usize,
    unknown_chunks: usize,
    metadata_shape_supports: Vec<TabFamilyMetadataShapeSupport>,
    equal_run_archives: usize,
    repeated_pattern_archives: usize,
    min_entropy_milli_bits: u32,
    max_entropy_milli_bits: u32,
    common_bucket_overlap: Vec<TabFamilyCommonBucketOverlap>,
    small_chunks: usize,
    medium_chunks: usize,
    large_chunks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabFamilyMetadataShapeSupport {
    label: &'static str,
    support_count: usize,
    per_mille: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabFamilyCommonBucketOverlap {
    len: u32,
    archive_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabFamilyComparisonSummary {
    left_family: &'static str,
    right_family: &'static str,
    metadata_differences: Vec<TabFamilyRatioDifference>,
    classifier_differences: Vec<TabFamilyRatioDifference>,
    equal_run_archive_ratio_delta: i32,
    repeated_pattern_archive_ratio_delta: i32,
    left_entropy_range: (u32, u32),
    right_entropy_range: (u32, u32),
    overlapping_common_buckets: Vec<u32>,
    left_distinct_common_buckets: Vec<u32>,
    right_distinct_common_buckets: Vec<u32>,
    shared_compatibility_clues: Vec<String>,
    distinguishing_clues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRuntimeProbeWorkbench {
    selectors: Vec<TabRuntimeProbeSelector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRuntimeProbeSelector {
    id: String,
    rank: usize,
    family: &'static str,
    category: TabRuntimeProbeCategory,
    support_tier: TabRuntimeProbeSupportTier,
    focus: String,
    evidence: String,
    rationale: String,
    priority: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TabRuntimeProbeCategory {
    MetadataShape,
    ClassifierGrouping,
    FixedLengthBuckets,
    SiblingCommonBuckets,
    MixedUnknownAudit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TabRuntimeProbeSupportTier {
    Strong,
    Medium,
    Limited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabFamilyRatioDifference {
    label: String,
    left_per_mille: usize,
    right_per_mille: usize,
    delta_per_mille: i32,
}

fn tab_runtime_probe_manifest_from_report_analyses(
    tab_analyses: &[TabBankReportAnalysis],
) -> TabRuntimeProbeManifest {
    let inputs =
        tab_analyses
            .iter()
            .filter_map(|analysis| {
                analysis.archive_summary.as_ref().cloned().map(|summary| {
                    TabRuntimeProbeArchiveInput {
                        path: analysis.path.clone(),
                        summary,
                    }
                })
            })
            .collect::<Vec<_>>();
    TabRuntimeProbeManifest::from_archive_inputs(inputs)
}

fn format_tab_runtime_probe_manifest_rows(manifest: &TabRuntimeProbeManifest) -> Vec<String> {
    if manifest.selectors.is_empty() {
        return Vec::new();
    }

    vec![format!(
        "| local runtime TAB/sprite aggregate probe manifest | {} parsed TAB/DAT pairs; {} selected families from {} candidate families; {} capped selectors; {} dry-run phases | families [{}]; support tiers [{}] | {} | preconditions: {}; stop: {} | runtime-only aggregate manifest; not proof of decoded layout or semantics; does not expose bytes, raw headers/chunks, previews, decoded dimensions, anchors, commands, audio, UI, or gameplay semantics |",
        manifest.parsed_archives,
        manifest.selected_families,
        manifest.total_candidate_families,
        manifest.selectors.len(),
        manifest.phases.len(),
        manifest.family_summary(),
        manifest.selector_tier_summary(),
        manifest.phase_summary(),
        manifest.preconditions_summary(),
        manifest.stop_conditions_summary()
    )]
}

fn format_tab_runtime_probe_manifest_phase_rows(manifest: &TabRuntimeProbeManifest) -> Vec<String> {
    manifest
        .phases
        .iter()
        .map(|phase| {
            format!(
                "| {}. {} | {} | {} | {} | {} | {} | runtime-only aggregate manifest phase; selector IDs are dry-run handles, not asset identifiers, decoded layouts, or semantics |",
                phase.phase.order(),
                phase.phase.label(),
                phase.selector_ids_summary(),
                phase.families_summary(),
                phase.support_tiers_summary(),
                phase.grouping_rule,
                phase.stop_condition
            )
        })
        .collect()
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

fn format_block_layout_probes(analysis: &BlockGraphicsAnalysis) -> String {
    if analysis.layout_probes.is_empty() {
        return "no complete fixed-size probe records".to_string();
    }

    let best = analysis
        .best_layout_probe()
        .map(|probe| format!("best {}", probe.label()))
        .unwrap_or_else(|| "best unavailable".to_string());
    let candidates = analysis
        .layout_probes
        .iter()
        .take(3)
        .map(|probe| {
            format!(
                "{}x{} {} records, {}, dup {}, zero% min/med/max {}/{}/{}, unique min/med/max {}/{}/{}, entropy min/med/max {:.3}/{:.3}/{:.3}{}{}",
                probe.width,
                probe.height,
                probe.complete_records,
                probe.alignment.label(),
                probe.duplicate_records,
                probe.record_zero_percent.min,
                probe.record_zero_percent.median,
                probe.record_zero_percent.max,
                probe.record_unique_values.min,
                probe.record_unique_values.median,
                probe.record_unique_values.max,
                probe.record_entropy_milli_bits.min as f32 / 1000.0,
                probe.record_entropy_milli_bits.median as f32 / 1000.0,
                probe.record_entropy_milli_bits.max as f32 / 1000.0,
                format_region_hint("lead", probe.leading_region_hint),
                format_region_hint("trail", probe.trailing_region_hint)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    let low_entropy_note = if analysis.byte_summary.unique_values <= 4
        && analysis.byte_summary.entropy_milli_bits <= 1000
    {
        "; observed very low aggregate unique/entropy values, which may indicate masks, tables, or minimap/block metadata rather than final terrain graphics"
    } else {
        ""
    };

    format!("{best}; probes [{candidates}]{low_entropy_note}")
}

fn format_region_hint(
    label: &str,
    hint: Option<crate::engine::block_decode::BlockRegionHint>,
) -> String {
    hint.map(|hint| {
        format!(
            ", {label} {} bytes entropy {:.3}, zero {}%, unique {}",
            hint.bytes,
            hint.entropy_milli_bits as f32 / 1000.0,
            hint.zero_percent,
            hint.unique_values
        )
    })
    .unwrap_or_default()
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

fn format_block_cross_container_rows(
    block_analyses: &[(String, BlockGraphicsAnalysis)],
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    let mut rows = Vec::new();
    rows.extend(format_duplicate_block_rows(block_analyses));
    rows.extend(format_mmap_relation_rows(block_analyses));
    rows.extend(format_layout_support_rows(block_analyses));
    rows.extend(format_tab_family_relation_rows(tab_analyses));
    rows.extend(format_tab_block_relation_rows(block_analyses, tab_analyses));
    rows
}

fn format_duplicate_block_rows(block_analyses: &[(String, BlockGraphicsAnalysis)]) -> Vec<String> {
    let mut by_name: BTreeMap<String, Vec<(&String, &BlockGraphicsAnalysis)>> = BTreeMap::new();
    for (path, analysis) in block_analyses {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        by_name.entry(name).or_default().push((path, analysis));
    }

    by_name
        .into_iter()
        .filter_map(|(name, entries)| {
            (entries.len() > 1).then(|| {
                let same_decoded_hash = entries
                    .windows(2)
                    .all(|pair| pair[0].1.decoded_hash == pair[1].1.decoded_hash);
                let same_decoded_len = entries
                    .windows(2)
                    .all(|pair| pair[0].1.decoded_len == pair[1].1.decoded_len);
                let containers = entries
                    .iter()
                    .map(|(path, _)| format!("`{path}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let lengths = entries
                    .iter()
                    .map(|(_, analysis)| analysis.decoded_len.to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                let status = if same_decoded_hash {
                    "matching decoded checksum/hash status"
                } else if same_decoded_len {
                    "matching decoded lengths, differing aggregate hash status"
                } else {
                    "differing decoded lengths"
                };
                format!(
                    "| duplicate-name candidate `{name}` | {containers} | decoded lengths {lengths}; {status} | non-reconstructable hash comparison only | duplicate status does not prove semantic equivalence across releases |"
                )
            })
        })
        .collect()
}

fn format_mmap_relation_rows(block_analyses: &[(String, BlockGraphicsAnalysis)]) -> Vec<String> {
    let mut rows = Vec::new();
    for prefix in ["SYNDICAT/DATA", "DATADISK/DATA"] {
        let mmap = find_block_analysis(block_analyses, &format!("{prefix}/MMAP.DAT"));
        let mmapout = find_block_analysis(block_analyses, &format!("{prefix}/MMAPOUT.DAT"));
        let mmapblk = find_block_analysis(block_analyses, &format!("{prefix}/MMAPBLK.DAT"));

        if let (Some(mmap), Some(mmapout)) = (mmap, mmapout) {
            rows.push(format!(
                "| MMAP/MMAPOUT length candidate | `{prefix}/MMAP.DAT`, `{prefix}/MMAPOUT.DAT` | decoded/plain length ratio {}:{} ({} permille) | entropy {:.3} vs {:.3} bits/byte; unique {} vs {} | ratio and entropy are aggregate clues only, not a decoded relationship |",
                mmap.decoded_len,
                mmapout.decoded_len,
                ratio_per_mille(mmap.decoded_len, mmapout.decoded_len),
                mmap.byte_summary.entropy_milli_bits as f32 / 1000.0,
                mmapout.byte_summary.entropy_milli_bits as f32 / 1000.0,
                mmap.byte_summary.unique_values,
                mmapout.byte_summary.unique_values,
            ));
        }

        if let (Some(mmap), Some(mmapblk)) = (mmap, mmapblk) {
            rows.push(format!(
                "| MMAP/MMAPBLK length candidate | `{prefix}/MMAP.DAT`, `{prefix}/MMAPBLK.DAT` | decoded/plain length ratio {}:{} ({} permille) | MMAPBLK best layout {}; MMAP entropy {:.3}, MMAPBLK entropy {:.3} | low-entropy block data may be masks/metadata; no terrain semantics inferred |",
                mmap.decoded_len,
                mmapblk.decoded_len,
                ratio_per_mille(mmap.decoded_len, mmapblk.decoded_len),
                mmapblk.best_layout_probe().map(|probe| probe.label()).unwrap_or_else(|| "unavailable".to_string()),
                mmap.byte_summary.entropy_milli_bits as f32 / 1000.0,
                mmapblk.byte_summary.entropy_milli_bits as f32 / 1000.0,
            ));
        }
    }
    rows
}

fn format_layout_support_rows(block_analyses: &[(String, BlockGraphicsAnalysis)]) -> Vec<String> {
    block_analyses
        .iter()
        .filter_map(|(path, analysis)| {
            let supports = analysis.layout_alignment_supports();
            let best = supports.first()?;
            let dimensions = best
                .dimensions
                .iter()
                .map(|(width, height)| format!("{width}x{height}"))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!(
                "| layout-alignment support candidate | `{path}` | {} supported by {} dimensions ({dimensions}) | max complete records {}; decoded length {} | table/header and remainder labels are provisional alignment hints only |",
                best.label.label(),
                best.support_count,
                best.max_complete_records,
                analysis.decoded_len
            ))
        })
        .collect()
}

fn format_tab_archive_summary(summary: &TabArchiveSummary) -> String {
    format!(
        "{}; {}; sprite aggregate [{}]",
        format_tab_bank_summary(&summary.bank),
        format_tab_candidate_size_matches(&summary.bank),
        format_sprite_bank_aggregate(&summary.sprite_bank)
    )
}

fn format_tab_bank_summary(summary: &TabBankSummary) -> String {
    format!(
        "{} chunks; len min/med/max {}/{}/{} bytes; size entropy {:.3} bits; offsets first/last {}; duplicate offsets {}, zero-length candidates {}; common sizes [{}]; chunk-length progression [{}]",
        summary.chunk_count,
        summary.min_chunk_len,
        summary.median_chunk_len,
        summary.max_chunk_len,
        summary.chunk_len_entropy_milli_bits as f32 / 1000.0,
        format_tab_offset_sanity(summary),
        summary.duplicate_offset_count,
        summary.zero_len_chunks,
        format_tab_common_size_buckets(summary),
        format_tab_chunk_length_progression(summary)
    )
}

fn format_tab_offset_sanity(summary: &TabBankSummary) -> String {
    match (summary.first_offset, summary.last_offset) {
        (Some(first), Some(last)) => {
            format!("{}..{} of {} DAT bytes", first, last, summary.dat_len)
        }
        _ => format!("unavailable of {} DAT bytes", summary.dat_len),
    }
}

fn format_tab_common_size_buckets(summary: &TabBankSummary) -> String {
    if summary.common_chunk_len_buckets.is_empty() {
        return "none".to_string();
    }

    summary
        .common_chunk_len_buckets
        .iter()
        .map(|bucket| format!("{}:{}", bucket.len, bucket.count))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_tab_candidate_size_matches(summary: &TabBankSummary) -> String {
    if summary.exact_candidate_size_matches.is_empty() {
        return "no exact matches to fixed tile-byte candidates".to_string();
    }

    let matches = summary
        .exact_candidate_size_matches
        .iter()
        .map(|candidate| {
            format!(
                "{} bytes:{} chunks",
                candidate.bytes_per_chunk, candidate.count
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("exact fixed tile-byte candidate matches [{matches}]")
}

fn format_tab_chunk_length_progression(summary: &TabBankSummary) -> String {
    let longest_run = if summary.longest_equal_len_run.run_chunks > 0 {
        format!(
            "longest equal-size run {} chunks of {} bytes",
            summary.longest_equal_len_run.run_chunks, summary.longest_equal_len_run.len
        )
    } else {
        "no equal-size run candidates".to_string()
    };
    let deltas = if summary.common_adjacent_len_deltas.is_empty() {
        "adjacent deltas none".to_string()
    } else {
        format!(
            "common adjacent deltas {}",
            summary
                .common_adjacent_len_deltas
                .iter()
                .map(|delta| format!("{:+}:{}", delta.delta, delta.count))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let patterns = if summary.repeated_len_patterns.is_empty() {
        "repeated size-pattern candidates none".to_string()
    } else {
        format!(
            "repeated size-pattern candidates {}",
            summary
                .repeated_len_patterns
                .iter()
                .map(|pattern| format!(
                    "len{} x{} range {}..{}",
                    pattern.pattern_len,
                    pattern.repeats,
                    pattern.min_chunk_len,
                    pattern.max_chunk_len
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!("{longest_run}; {deltas}; {patterns}")
}

fn format_sprite_bank_aggregate(summary: &SpriteBankAggregateSummary) -> String {
    format!(
        "classifier counts [{}]; size bands small/medium/large {}/{}/{}; classifier-by-size [{}]; zero/high-byte ratio min/med/max by classifier [{}]; candidate header-shapes [{}]; candidate metadata-shapes [{}]",
        format_tab_sprite_kind_counts_from_aggregate(summary),
        summary.size_band_counts.small,
        summary.size_band_counts.medium,
        summary.size_band_counts.large,
        format_sprite_kind_by_size_buckets(summary),
        format_sprite_kind_ratio_summaries(summary),
        format_sprite_header_shapes(summary),
        format_sprite_metadata_shapes(summary)
    )
}

fn format_tab_sprite_kind_counts_from_aggregate(summary: &SpriteBankAggregateSummary) -> String {
    if summary.kind_aggregates.is_empty() {
        return "none".to_string();
    }

    summary
        .kind_aggregates
        .iter()
        .map(|entry| format!("{}:{}", entry.kind.conservative_label(), entry.count))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_sprite_kind_by_size_buckets(summary: &SpriteBankAggregateSummary) -> String {
    if summary.kind_by_size_bucket.is_empty() {
        return "none".to_string();
    }

    summary
        .kind_by_size_bucket
        .iter()
        .map(|bucket| {
            let counts = bucket
                .kind_counts
                .iter()
                .map(|entry| format!("{}:{}", entry.kind.conservative_label(), entry.count))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} => {counts}", bucket.bucket.label())
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_sprite_kind_ratio_summaries(summary: &SpriteBankAggregateSummary) -> String {
    if summary.kind_aggregates.is_empty() {
        return "none".to_string();
    }

    summary
        .kind_aggregates
        .iter()
        .map(|aggregate| {
            format!(
                "{} zero {}/ high {}",
                aggregate.kind.conservative_label(),
                format_per_mille_distribution(aggregate.zero_ratio_per_mille),
                format_per_mille_distribution(aggregate.high_byte_ratio_per_mille)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_sprite_header_shapes(summary: &SpriteBankAggregateSummary) -> String {
    if summary.header_shape_counts.is_empty() {
        return "none".to_string();
    }

    summary
        .header_shape_counts
        .iter()
        .map(|entry| format!("{}:{}", entry.shape.label(), entry.count))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_sprite_metadata_shapes(summary: &SpriteBankAggregateSummary) -> String {
    if summary.metadata_shape_probes.is_empty() {
        return "none".to_string();
    }

    summary
        .metadata_shape_probes
        .iter()
        .map(|probe| {
            format!(
                "{}:{} chunks, first min/med/max {}, second min/med/max {}",
                probe.kind.label(),
                probe.support_count,
                format_u32_distribution(probe.first_value),
                format_u32_distribution(probe.second_value)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_per_mille_distribution(summary: SpriteDistributionSummary) -> String {
    format!(
        "{:.3}/{:.3}/{:.3}",
        summary.min as f32 / 1000.0,
        summary.median as f32 / 1000.0,
        summary.max as f32 / 1000.0
    )
}

fn format_u32_distribution(summary: SpriteDistributionSummary) -> String {
    format!("{}/{}/{}", summary.min, summary.median, summary.max)
}

fn format_tab_family_relation_rows(tab_analyses: &[TabBankReportAnalysis]) -> Vec<String> {
    let mut grouped: BTreeMap<&'static str, Vec<&TabBankReportAnalysis>> = BTreeMap::new();
    for tab in tab_analyses {
        grouped
            .entry(tab_file_family(&tab.path))
            .or_default()
            .push(tab);
    }

    grouped
        .into_iter()
        .filter_map(|(family, entries)| {
            let parsed = entries
                .iter()
                .filter_map(|entry| entry.archive_summary.as_ref())
                .collect::<Vec<_>>();
            if parsed.is_empty() {
                return None;
            }

            let total_chunks = parsed.iter().map(|summary| summary.bank.chunk_count).sum::<usize>();
            let rle_chunks = parsed
                .iter()
                .flat_map(|summary| summary.sprite_bank.kind_aggregates.iter())
                .filter(|aggregate| {
                    aggregate.kind
                        == crate::engine::sprite_decode::SpriteChunkKind::LikelyRleOrCommandStream
                })
                .map(|aggregate| aggregate.count)
                .sum::<usize>();
            let raw_chunks = parsed
                .iter()
                .flat_map(|summary| summary.sprite_bank.kind_aggregates.iter())
                .filter(|aggregate| {
                    aggregate.kind == crate::engine::sprite_decode::SpriteChunkKind::LikelyRawIndexed
                })
                .map(|aggregate| aggregate.count)
                .sum::<usize>();
            let header_shapes = parsed
                .iter()
                .flat_map(|summary| summary.sprite_bank.header_shape_counts.iter())
                .fold(BTreeMap::new(), |mut counts, entry| {
                    *counts.entry(entry.shape.label()).or_insert(0usize) += entry.count;
                    counts
                })
                .into_iter()
                .map(|(shape, count)| format!("{shape}:{count}"))
                .collect::<Vec<_>>()
                .join(", ");
            let paths = entries
                .iter()
                .map(|entry| format!("`{}`", entry.path))
                .collect::<Vec<_>>()
                .join(", ");

            Some(format!(
                "| TAB/DAT filename-family aggregate candidate `{family}` | {paths} | {} parsed archives, {} total chunks | likely command-stream chunks {}, likely raw chunks {}, header-shapes [{}] | filename family and classifier distribution are aggregate clues only; they do not prove sprite, font, sound, or UI semantics |",
                parsed.len(),
                total_chunks,
                rle_chunks,
                raw_chunks,
                header_shapes
            ))
        })
        .collect()
}

fn format_tab_family_ranking_rows(tab_analyses: &[TabBankReportAnalysis]) -> Vec<String> {
    tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .map(|summary| {
            format!(
                "| `{}` | {} of {} chunks | {} | {} | {} | {} | {} | {} |",
                summary.family,
                summary.parsed_archives,
                summary.total_chunks,
                format_tab_family_classifier_totals(&summary),
                format_tab_family_metadata_support(&summary),
                format_tab_family_progression_support(&summary),
                format!(
                    "{:.3}..{:.3} bits",
                    summary.min_entropy_milli_bits as f32 / 1000.0,
                    summary.max_entropy_milli_bits as f32 / 1000.0
                ),
                format_tab_family_common_bucket_overlap(&summary),
                "candidate ranking only; aggregate support does not decode sprite metadata or prove family semantics"
            )
        })
        .collect()
}

fn tab_family_ranking_summaries(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<TabFamilyRankingSummary> {
    let mut grouped: BTreeMap<&'static str, Vec<&TabArchiveSummary>> = BTreeMap::new();
    for tab in tab_analyses {
        if let Some(summary) = tab.archive_summary.as_ref() {
            grouped
                .entry(tab_file_family(&tab.path))
                .or_default()
                .push(summary);
        }
    }

    let mut summaries = grouped
        .into_iter()
        .filter_map(|(family, entries)| summarize_tab_family_ranking(family, &entries))
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .metadata_support_score()
            .cmp(&left.metadata_support_score())
            .then_with(|| right.progression_score().cmp(&left.progression_score()))
            .then_with(|| right.total_chunks.cmp(&left.total_chunks))
            .then_with(|| left.family.cmp(right.family))
    });
    summaries
}

fn summarize_tab_family_ranking(
    family: &'static str,
    entries: &[&TabArchiveSummary],
) -> Option<TabFamilyRankingSummary> {
    if entries.is_empty() {
        return None;
    }

    let parsed_archives = entries.len();
    let total_chunks = entries
        .iter()
        .map(|summary| summary.bank.chunk_count)
        .sum::<usize>();
    let command_stream_chunks = sum_family_kind(
        entries,
        crate::engine::sprite_decode::SpriteChunkKind::LikelyRleOrCommandStream,
    );
    let raw_chunks = sum_family_kind(
        entries,
        crate::engine::sprite_decode::SpriteChunkKind::LikelyRawIndexed,
    );
    let unknown_chunks = sum_family_kind(
        entries,
        crate::engine::sprite_decode::SpriteChunkKind::Unknown,
    );
    let mut metadata_shape_supports = entries
        .iter()
        .flat_map(|summary| summary.sprite_bank.metadata_shape_probes.iter())
        .fold(BTreeMap::new(), |mut counts, probe| {
            *counts.entry(probe.kind.label()).or_insert(0usize) += probe.support_count;
            counts
        })
        .into_iter()
        .map(|(label, support_count)| TabFamilyMetadataShapeSupport {
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

    let equal_run_archives = entries
        .iter()
        .filter(|summary| summary.bank.longest_equal_len_run.run_chunks >= 2)
        .count();
    let repeated_pattern_archives = entries
        .iter()
        .filter(|summary| !summary.bank.repeated_len_patterns.is_empty())
        .count();
    let min_entropy_milli_bits = entries
        .iter()
        .map(|summary| summary.bank.chunk_len_entropy_milli_bits)
        .min()
        .unwrap_or(0);
    let max_entropy_milli_bits = entries
        .iter()
        .map(|summary| summary.bank.chunk_len_entropy_milli_bits)
        .max()
        .unwrap_or(0);
    let common_bucket_overlap = family_common_bucket_overlap(entries);

    Some(TabFamilyRankingSummary {
        family,
        parsed_archives,
        total_chunks,
        command_stream_chunks,
        raw_chunks,
        unknown_chunks,
        metadata_shape_supports,
        equal_run_archives,
        repeated_pattern_archives,
        min_entropy_milli_bits,
        max_entropy_milli_bits,
        common_bucket_overlap,
        small_chunks: entries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.small)
            .sum(),
        medium_chunks: entries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.medium)
            .sum(),
        large_chunks: entries
            .iter()
            .map(|summary| summary.sprite_bank.size_band_counts.large)
            .sum(),
    })
}

impl TabFamilyRankingSummary {
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

fn sum_family_kind(
    entries: &[&TabArchiveSummary],
    kind: crate::engine::sprite_decode::SpriteChunkKind,
) -> usize {
    entries
        .iter()
        .flat_map(|summary| summary.sprite_bank.kind_aggregates.iter())
        .filter(|aggregate| aggregate.kind == kind)
        .map(|aggregate| aggregate.count)
        .sum()
}

fn family_common_bucket_overlap(
    entries: &[&TabArchiveSummary],
) -> Vec<TabFamilyCommonBucketOverlap> {
    let mut counts = BTreeMap::new();
    for summary in entries {
        for bucket in &summary.bank.common_chunk_len_buckets {
            *counts.entry(bucket.len).or_insert(0usize) += 1;
        }
    }
    let mut overlaps = counts
        .into_iter()
        .filter(|(_, archive_count)| *archive_count >= 2 || entries.len() == 1)
        .map(|(len, archive_count)| TabFamilyCommonBucketOverlap { len, archive_count })
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

fn format_tab_family_classifier_totals(summary: &TabFamilyRankingSummary) -> String {
    format!(
        "command-stream candidates {}, raw candidates {}, unknown candidates {}",
        summary.command_stream_chunks, summary.raw_chunks, summary.unknown_chunks
    )
}

fn format_tab_family_metadata_support(summary: &TabFamilyRankingSummary) -> String {
    if summary.metadata_shape_supports.is_empty() {
        return "no bounded candidate metadata-shape support".to_string();
    }

    summary
        .metadata_shape_supports
        .iter()
        .take(3)
        .map(|support| {
            format!(
                "{} {} chunks ({} per mille)",
                support.label, support.support_count, support.per_mille
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_tab_family_progression_support(summary: &TabFamilyRankingSummary) -> String {
    format!(
        "equal-size runs in {}/{} archives; repeated size-pattern candidates in {}/{} archives",
        summary.equal_run_archives,
        summary.parsed_archives,
        summary.repeated_pattern_archives,
        summary.parsed_archives
    )
}

fn format_tab_family_common_bucket_overlap(summary: &TabFamilyRankingSummary) -> String {
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

fn format_tab_family_hint_rows(tab_analyses: &[TabBankReportAnalysis]) -> Vec<String> {
    tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .filter(is_sprite_like_family_candidate)
        .map(|summary| {
            format!(
                "| `{}` | {} | {} | {} | {} | {} | {} |",
                summary.family,
                tab_family_investigation_priority(&summary),
                tab_family_metadata_hint(&summary),
                tab_family_classifier_size_hint(&summary),
                tab_family_progression_hint(&summary),
                tab_family_entropy_bucket_hint(&summary),
                "aggregate hint only; inspect local user-supplied assets at runtime without committing bytes, headers, chunks, or previews"
            )
        })
        .collect()
}

fn tab_family_investigation_priority(summary: &TabFamilyRankingSummary) -> String {
    let metadata_score = summary.metadata_support_score();
    let progression_score = summary.progression_score();
    let command_ratio = ratio_per_mille(summary.command_stream_chunks, summary.total_chunks);
    if metadata_score >= summary.total_chunks / 2 || (command_ratio >= 700 && progression_score > 0)
    {
        "high aggregate inspection priority: strong candidate metadata/progression evidence"
            .to_string()
    } else if metadata_score > 0 || progression_score > 0 || command_ratio >= 500 {
        "medium aggregate inspection priority: useful classifier or progression evidence"
            .to_string()
    } else {
        "low aggregate inspection priority: limited sprite-like aggregate support".to_string()
    }
}

fn tab_family_metadata_hint(summary: &TabFamilyRankingSummary) -> String {
    summary
        .metadata_shape_supports
        .first()
        .map(|support| {
            format!(
                "inspect `{}` candidate first ({} chunks, {} per mille support)",
                support.label, support.support_count, support.per_mille
            )
        })
        .unwrap_or_else(|| {
            "no bounded candidate metadata-shape selected; prioritize classifier and size-band aggregates first".to_string()
        })
}

fn tab_family_classifier_size_hint(summary: &TabFamilyRankingSummary) -> String {
    let dominant = dominant_classifier_hint(summary);
    let size_band = dominant_size_band_hint(summary);
    format!("{dominant}; prioritize {size_band}")
}

fn dominant_classifier_hint(summary: &TabFamilyRankingSummary) -> String {
    let classifiers = [
        ("command-stream candidate", summary.command_stream_chunks),
        ("raw-indexed candidate", summary.raw_chunks),
        ("unknown candidate", summary.unknown_chunks),
    ];
    let (label, count) = classifiers
        .into_iter()
        .max_by_key(|(label, count)| (*count, std::cmp::Reverse(*label)))
        .unwrap_or(("unknown candidate", 0));
    format!(
        "strongest classifier pattern: {label} {} chunks ({} per mille)",
        count,
        ratio_per_mille(count, summary.total_chunks)
    )
}

fn dominant_size_band_hint(summary: &TabFamilyRankingSummary) -> String {
    let bands = [
        ("small chunks (0..31 bytes)", summary.small_chunks),
        ("medium chunks (32..511 bytes)", summary.medium_chunks),
        ("large chunks (>=512 bytes)", summary.large_chunks),
    ];
    let (label, count) = bands
        .into_iter()
        .max_by_key(|(label, count)| (*count, std::cmp::Reverse(*label)))
        .unwrap_or(("unknown size band", 0));
    format!(
        "{label} with {} chunks ({} per mille)",
        count,
        ratio_per_mille(count, summary.total_chunks)
    )
}

fn tab_family_progression_hint(summary: &TabFamilyRankingSummary) -> String {
    match (
        summary.equal_run_archives == summary.parsed_archives,
        summary.repeated_pattern_archives == summary.parsed_archives,
        summary.equal_run_archives > 0,
        summary.repeated_pattern_archives > 0,
    ) {
        (true, true, _, _) => "chunk-length progression suggests repeated-record candidates across all parsed archives".to_string(),
        (_, true, _, _) => "repeated size-pattern candidates appear in all parsed archives; inspect recurring record groups first".to_string(),
        (_, _, true, true) => "mixed equal-run and repeated-pattern evidence suggests partial repeated records".to_string(),
        (_, _, true, false) => "equal-size runs suggest localized repeated records, but varied records remain likely".to_string(),
        _ => "limited repeated-length evidence; expect varied records until stronger aggregate support appears".to_string(),
    }
}

fn tab_family_entropy_bucket_hint(summary: &TabFamilyRankingSummary) -> String {
    let command_ratio = ratio_per_mille(summary.command_stream_chunks, summary.total_chunks);
    let raw_or_unknown_ratio = ratio_per_mille(
        summary.raw_chunks.saturating_add(summary.unknown_chunks),
        summary.total_chunks,
    );
    let smallest_common_bucket = summary
        .common_bucket_overlap
        .iter()
        .map(|bucket| bucket.len)
        .min();
    let behaviour = if command_ratio >= 800 && summary.max_entropy_milli_bits <= 5500 {
        "closer to command-stream-heavy banks in aggregate"
    } else if smallest_common_bucket.is_some_and(|len| len <= 32)
        && summary.max_entropy_milli_bits <= 5500
    {
        "closer to compact metadata or control-record banks in aggregate"
    } else if raw_or_unknown_ratio >= 300 || summary.max_entropy_milli_bits >= 6500 {
        "closer to mixed banks with varied records in aggregate"
    } else {
        "no single entropy/common-bucket behaviour dominates"
    };
    format!(
        "{}; entropy {:.3}..{:.3} bits; common buckets [{}]",
        behaviour,
        summary.min_entropy_milli_bits as f32 / 1000.0,
        summary.max_entropy_milli_bits as f32 / 1000.0,
        format_tab_family_common_bucket_overlap(summary)
    )
}

fn format_tab_family_archive_evidence_rows(tab_analyses: &[TabBankReportAnalysis]) -> Vec<String> {
    const MAX_FAMILIES: usize = 3;
    const MAX_ARCHIVES_PER_FAMILY: usize = 4;

    let selected_families = tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .filter(is_sprite_like_family_candidate)
        .take(MAX_FAMILIES)
        .map(|summary| summary.family)
        .collect::<Vec<_>>();
    if selected_families.is_empty() {
        return Vec::new();
    }

    selected_families
        .into_iter()
        .flat_map(|family| {
            tab_analyses
                .iter()
                .filter(move |analysis| tab_file_family(&analysis.path) == family)
                .filter_map(move |analysis| {
                    analysis
                        .archive_summary
                        .as_ref()
                        .map(|summary| (family, analysis, summary))
                })
                .take(MAX_ARCHIVES_PER_FAMILY)
                .map(|(family, analysis, summary)| {
                    format!(
                        "| `{}` | `{}` | {} | {} | {} | {} | {} | {} |",
                        family,
                        analysis.path,
                        format_archive_chunk_size_evidence(summary),
                        format_archive_metadata_evidence(summary),
                        format_sprite_kind_by_size_buckets(&summary.sprite_bank),
                        format_tab_chunk_length_progression(&summary.bank),
                        format_archive_entropy_bucket_summary(summary),
                        "capped aggregate archive evidence only; no bytes, raw headers, decoded dimensions, anchors, commands, pixels, audio, UI, or gameplay semantics"
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn format_archive_chunk_size_evidence(summary: &TabArchiveSummary) -> String {
    format!(
        "{} chunks; len min/med/max {}/{}/{} bytes; size bands small/medium/large {}/{}/{}; duplicate offsets {}, zero-length candidates {}",
        summary.bank.chunk_count,
        summary.bank.min_chunk_len,
        summary.bank.median_chunk_len,
        summary.bank.max_chunk_len,
        summary.sprite_bank.size_band_counts.small,
        summary.sprite_bank.size_band_counts.medium,
        summary.sprite_bank.size_band_counts.large,
        summary.bank.duplicate_offset_count,
        summary.bank.zero_len_chunks
    )
}

fn format_archive_metadata_evidence(summary: &TabArchiveSummary) -> String {
    if summary.sprite_bank.metadata_shape_probes.is_empty() {
        return "no bounded candidate metadata-shape support in this archive".to_string();
    }

    summary
        .sprite_bank
        .metadata_shape_probes
        .iter()
        .take(3)
        .map(|probe| {
            format!(
                "{}:{} chunks, first range {}, second range {}",
                probe.kind.label(),
                probe.support_count,
                format_u32_distribution(probe.first_value),
                format_u32_distribution(probe.second_value)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_archive_entropy_bucket_summary(summary: &TabArchiveSummary) -> String {
    format!(
        "chunk-size entropy {:.3} bits; common-size buckets [{}]; {}",
        summary.bank.chunk_len_entropy_milli_bits as f32 / 1000.0,
        format_tab_common_size_buckets(&summary.bank),
        format_tab_candidate_size_matches(&summary.bank)
    )
}

fn format_tab_family_investigation_dashboard_rows(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    const MAX_FAMILIES: usize = 3;
    const MAX_ARCHIVES_PER_FAMILY: usize = 4;

    tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .filter(is_sprite_like_family_candidate)
        .take(MAX_FAMILIES)
        .filter_map(|summary| {
            let family_archives = selected_family_archives(
                tab_analyses,
                summary.family,
                MAX_ARCHIVES_PER_FAMILY,
            );
            (!family_archives.is_empty()).then(|| {
                format!(
                    "| `{}` | {} | {} | {} | {} | {} |",
                    summary.family,
                    format_dashboard_archive_inclusion(
                        summary.family,
                        family_archives.len(),
                        count_parsed_family_archives(tab_analyses, summary.family),
                        MAX_ARCHIVES_PER_FAMILY
                    ),
                    format_dashboard_selection_rationale(&summary),
                    format_dashboard_family_consistency(&family_archives, &summary),
                    format_dashboard_next_probes(&summary),
                    "aggregate consistency and runtime-only next-probe hypotheses only; no bytes, raw headers, chunks, decoded dimensions, anchors, commands, pixels, audio, UI, or gameplay semantics"
                )
            })
        })
        .collect()
}

fn selected_family_archives<'a>(
    tab_analyses: &'a [TabBankReportAnalysis],
    family: &'static str,
    limit: usize,
) -> Vec<&'a TabArchiveSummary> {
    tab_analyses
        .iter()
        .filter(|analysis| tab_file_family(&analysis.path) == family)
        .filter_map(|analysis| analysis.archive_summary.as_ref())
        .take(limit)
        .collect()
}

fn count_parsed_family_archives(
    tab_analyses: &[TabBankReportAnalysis],
    family: &'static str,
) -> usize {
    tab_analyses
        .iter()
        .filter(|analysis| tab_file_family(&analysis.path) == family)
        .filter(|analysis| analysis.archive_summary.is_some())
        .count()
}

fn format_dashboard_archive_inclusion(
    family: &'static str,
    included_archives: usize,
    total_archives: usize,
    cap: usize,
) -> String {
    let cap_status = if total_archives > included_archives {
        format!("capped at {included_archives}/{total_archives} parsed archives")
    } else {
        format!("included {included_archives}/{total_archives} parsed archives")
    };
    format!(
        "top selected `{family}` sprite-like family candidate; {cap_status}; per-family dashboard cap {cap} archives"
    )
}

fn format_dashboard_selection_rationale(summary: &TabFamilyRankingSummary) -> String {
    format!(
        "selected by aggregate ranking: {}; {}; {}; {}",
        tab_family_investigation_priority(summary),
        tab_family_metadata_hint(summary),
        dominant_classifier_hint(summary),
        tab_family_entropy_bucket_hint(summary)
    )
}

fn format_dashboard_family_consistency(
    archives: &[&TabArchiveSummary],
    summary: &TabFamilyRankingSummary,
) -> String {
    format!(
        "archive count {}; chunk-count/range {}; shared metadata-shape candidates {}; classifier-by-size-band {}; common bucket overlap/distinction {}; entropy spread {:.3}..{:.3} bits; progression agreement {}",
        archives.len(),
        format_dashboard_chunk_range_consistency(archives),
        format_dashboard_metadata_consistency(archives),
        format_dashboard_classifier_band_consistency(archives),
        format_tab_family_common_bucket_overlap(summary),
        summary.min_entropy_milli_bits as f32 / 1000.0,
        summary.max_entropy_milli_bits as f32 / 1000.0,
        format_dashboard_progression_agreement(archives)
    )
}

fn format_dashboard_chunk_range_consistency(archives: &[&TabArchiveSummary]) -> String {
    if archives.is_empty() {
        return "no parsed archives".to_string();
    }

    let min_chunks = archives
        .iter()
        .map(|archive| archive.bank.chunk_count)
        .min()
        .unwrap_or(0);
    let max_chunks = archives
        .iter()
        .map(|archive| archive.bank.chunk_count)
        .max()
        .unwrap_or(0);
    let min_len = archives
        .iter()
        .map(|archive| archive.bank.min_chunk_len)
        .min()
        .unwrap_or(0);
    let max_len = archives
        .iter()
        .map(|archive| archive.bank.max_chunk_len)
        .max()
        .unwrap_or(0);
    let label = if min_chunks == max_chunks {
        "consistent chunk counts"
    } else {
        "varied chunk counts"
    };
    format!("{label} {min_chunks}..{max_chunks}; chunk len range {min_len}..{max_len} bytes")
}

fn format_dashboard_metadata_consistency(archives: &[&TabArchiveSummary]) -> String {
    let mut counts = BTreeMap::new();
    for archive in archives {
        for probe in &archive.sprite_bank.metadata_shape_probes {
            *counts.entry(probe.kind.label()).or_insert(0usize) += 1;
        }
    }
    if counts.is_empty() {
        return "no shared bounded candidate metadata-shapes".to_string();
    }

    counts
        .into_iter()
        .map(|(label, archive_count)| {
            let ratio = ratio_per_mille(archive_count, archives.len());
            format!(
                "{label} in {archive_count}/{} archives ({ratio} per mille)",
                archives.len()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_dashboard_classifier_band_consistency(archives: &[&TabArchiveSummary]) -> String {
    let mut dominant_labels = archives
        .iter()
        .map(|archive| dominant_archive_classifier_band_label(archive))
        .collect::<Vec<_>>();
    dominant_labels.sort();
    dominant_labels.dedup();
    if dominant_labels.len() == 1 {
        format!(
            "consistent dominant aggregate band `{}`",
            dominant_labels[0]
        )
    } else {
        format!(
            "mixed dominant aggregate bands [{}]",
            dominant_labels.join(", ")
        )
    }
}

fn dominant_archive_classifier_band_label(archive: &TabArchiveSummary) -> String {
    archive
        .sprite_bank
        .kind_by_size_bucket
        .iter()
        .flat_map(|bucket| {
            bucket.kind_counts.iter().map(move |count| {
                (
                    count.count,
                    format!(
                        "{} {}",
                        bucket.bucket.label(),
                        count.kind.conservative_label()
                    ),
                )
            })
        })
        .max_by_key(|(count, label)| (*count, std::cmp::Reverse(label.clone())))
        .map(|(_, label)| label)
        .unwrap_or_else(|| "no classifier-by-size-band aggregate".to_string())
}

fn format_dashboard_progression_agreement(archives: &[&TabArchiveSummary]) -> String {
    let equal_run_archives = archives
        .iter()
        .filter(|archive| archive.bank.longest_equal_len_run.run_chunks >= 2)
        .count();
    let repeated_pattern_archives = archives
        .iter()
        .filter(|archive| !archive.bank.repeated_len_patterns.is_empty())
        .count();
    if equal_run_archives == archives.len() && repeated_pattern_archives == archives.len() {
        "agreement: equal-size runs and repeated size-pattern candidates in all included archives"
            .to_string()
    } else if equal_run_archives > 0 || repeated_pattern_archives > 0 {
        format!(
            "partial agreement: equal-size runs {}/{}, repeated size-pattern candidates {}/{}",
            equal_run_archives,
            archives.len(),
            repeated_pattern_archives,
            archives.len()
        )
    } else {
        "disagreement/limited support: no repeated progression pattern selected in included archives".to_string()
    }
}

fn format_dashboard_next_probes(summary: &TabFamilyRankingSummary) -> String {
    let mut probes = Vec::new();
    if summary.equal_run_archives > 0 || summary.repeated_pattern_archives > 0 {
        probes.push(
            "runtime-only next probe: inspect repeated fixed-length command/control-record candidates"
                .to_string(),
        );
    }
    if summary
        .metadata_shape_supports
        .iter()
        .any(|support| support.label.contains("offset-pair"))
    {
        probes.push(
            "runtime-only next probe: inspect candidate leading offset-pair metadata shape"
                .to_string(),
        );
    } else if summary.metadata_support_score() > 0 {
        probes.push(
            "runtime-only next probe: compare bounded candidate leading metadata-shape groups"
                .to_string(),
        );
    }

    let command_ratio = ratio_per_mille(summary.command_stream_chunks, summary.total_chunks);
    if command_ratio >= 500 {
        probes.push(
            "runtime-only next probe: separate command-stream-heavy chunks from mixed raw/unknown chunks before rendering attempts"
                .to_string(),
        );
    }
    if !summary.common_bucket_overlap.is_empty() {
        probes.push(
            "runtime-only next probe: compare common-size bucket groups across sibling archives"
                .to_string(),
        );
    }
    if probes.is_empty() {
        probes.push(
            "runtime-only next probe: gather more aggregate consistency before attempting layout hypotheses"
                .to_string(),
        );
    }
    probes.truncate(4);
    probes.join("; ")
}

fn format_tab_family_runtime_probe_plan_rows(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    const MAX_FAMILIES: usize = 3;
    const MAX_ARCHIVES_PER_FAMILY: usize = 4;
    const MAX_ROWS_PER_FAMILY: usize = 4;

    tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .filter(is_sprite_like_family_candidate)
        .take(MAX_FAMILIES)
        .flat_map(|summary| {
            let archives =
                selected_family_archives(tab_analyses, summary.family, MAX_ARCHIVES_PER_FAMILY);
            let inclusion = format_dashboard_archive_inclusion(
                summary.family,
                archives.len(),
                count_parsed_family_archives(tab_analyses, summary.family),
                MAX_ARCHIVES_PER_FAMILY,
            );
            runtime_probe_plan_rows_for_family(&summary, &archives, &inclusion)
                .into_iter()
                .take(MAX_ROWS_PER_FAMILY)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn runtime_probe_plan_rows_for_family(
    summary: &TabFamilyRankingSummary,
    archives: &[&TabArchiveSummary],
    inclusion: &str,
) -> Vec<String> {
    if archives.is_empty() {
        return Vec::new();
    }

    vec![
        format_runtime_probe_plan_row(
            summary.family,
            "fixed-length repeated-record candidate groups",
            inclusion,
            format_runtime_length_bucket_probe_group(archives),
            format_runtime_progression_probe_support(summary, archives),
            "runtime-only aggregate probe plan: group local chunks by repeated chunk-length buckets before any decode/render attempt",
        ),
        format_runtime_probe_plan_row(
            summary.family,
            "candidate leading metadata-shape groups",
            inclusion,
            format_runtime_metadata_probe_group(summary),
            format_tab_family_metadata_support(summary),
            "runtime-only aggregate probe plan: compare bounded leading metadata-shape support groups without treating ranges as decoded dimensions or anchors",
        ),
        format_runtime_probe_plan_row(
            summary.family,
            "classifier grouping",
            inclusion,
            format_runtime_classifier_probe_group(summary),
            format!(
                "command-stream {} per mille, raw {} per mille, unknown {} per mille",
                ratio_per_mille(summary.command_stream_chunks, summary.total_chunks),
                ratio_per_mille(summary.raw_chunks, summary.total_chunks),
                ratio_per_mille(summary.unknown_chunks, summary.total_chunks)
            ),
            "runtime-only aggregate probe plan: separate command-stream-heavy candidates from mixed/raw/unknown groups before rendering experiments",
        ),
        format_runtime_probe_plan_row(
            summary.family,
            "sibling common-bucket comparison",
            inclusion,
            format_runtime_sibling_bucket_probe_group(summary),
            format!(
                "{} parsed archives; common bucket support [{}]",
                summary.parsed_archives,
                format_tab_family_common_bucket_overlap(summary)
            ),
            "runtime-only aggregate probe plan: compare sibling archive common-size buckets locally; matching buckets are not proof of shared layout or semantics",
        ),
    ]
}

fn format_runtime_probe_plan_row(
    family: &'static str,
    category: &str,
    inclusion: &str,
    group: String,
    support: String,
    note: &str,
) -> String {
    format!(
        "| `{family}` | {category} | {inclusion} | {group} | {support} | {note} | aggregate probe plan only; local runtime inspection may decode user-supplied assets but report rows do not expose bytes, raw headers/chunks, previews, decoded dimensions, anchors, commands, audio, UI, or gameplay semantics |"
    )
}

fn format_runtime_length_bucket_probe_group(archives: &[&TabArchiveSummary]) -> String {
    let mut buckets = BTreeMap::new();
    for archive in archives {
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

fn format_runtime_progression_probe_support(
    summary: &TabFamilyRankingSummary,
    archives: &[&TabArchiveSummary],
) -> String {
    let chunk_min = archives
        .iter()
        .map(|archive| archive.bank.chunk_count)
        .min()
        .unwrap_or(0);
    let chunk_max = archives
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

fn format_runtime_metadata_probe_group(summary: &TabFamilyRankingSummary) -> String {
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

fn format_runtime_classifier_probe_group(summary: &TabFamilyRankingSummary) -> String {
    format!(
        "command-stream-heavy candidates {}; mixed/raw candidates {}; unknown candidates {}; size bands small/medium/large {}/{}/{}",
        summary.command_stream_chunks,
        summary.raw_chunks,
        summary.unknown_chunks,
        summary.small_chunks,
        summary.medium_chunks,
        summary.large_chunks
    )
}

fn format_runtime_sibling_bucket_probe_group(summary: &TabFamilyRankingSummary) -> String {
    if summary.common_bucket_overlap.is_empty() {
        return "no sibling common-size bucket group selected".to_string();
    }
    format!(
        "sibling archive common-size bucket groups [{}]",
        format_tab_family_common_bucket_overlap(summary)
    )
}

fn format_tab_family_runtime_probe_queue_rows(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    const MAX_QUEUE_ROWS: usize = 6;

    tab_runtime_probe_workbench(tab_analyses)
        .selectors
        .into_iter()
        .take(MAX_QUEUE_ROWS)
        .map(|selector| format_runtime_probe_queue_row(&selector))
        .collect()
}

fn format_tab_family_runtime_probe_selector_catalog_rows(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    const MAX_SELECTOR_ROWS: usize = 10;

    tab_runtime_probe_workbench(tab_analyses)
        .selectors
        .into_iter()
        .take(MAX_SELECTOR_ROWS)
        .map(|selector| format_runtime_probe_selector_catalog_row(&selector))
        .collect()
}

fn format_tab_family_runtime_probe_dry_run_rows(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    const MAX_DRY_RUN_PHASES: usize = 5;

    let workbench = tab_runtime_probe_workbench(tab_analyses);
    format_runtime_probe_dry_run_rows(&workbench.selectors)
        .into_iter()
        .take(MAX_DRY_RUN_PHASES)
        .collect()
}

fn tab_runtime_probe_workbench(tab_analyses: &[TabBankReportAnalysis]) -> TabRuntimeProbeWorkbench {
    const MAX_FAMILIES: usize = 3;
    const MAX_ARCHIVES_PER_FAMILY: usize = 4;
    const MAX_WORKBENCH_SELECTORS: usize = 15;

    let mut selectors = Vec::new();
    for summary in tab_family_ranking_summaries(tab_analyses)
        .into_iter()
        .filter(is_sprite_like_family_candidate)
        .take(MAX_FAMILIES)
    {
        let archives =
            selected_family_archives(tab_analyses, summary.family, MAX_ARCHIVES_PER_FAMILY);
        if archives.is_empty() {
            continue;
        }

        let inclusion = format_dashboard_archive_inclusion(
            summary.family,
            archives.len(),
            count_parsed_family_archives(tab_analyses, summary.family),
            MAX_ARCHIVES_PER_FAMILY,
        );
        selectors.extend(runtime_probe_selectors_for_family(
            &summary, &archives, &inclusion,
        ));
    }

    selectors.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.family.cmp(right.family))
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.focus.cmp(&right.focus))
    });
    selectors.truncate(MAX_WORKBENCH_SELECTORS);

    for (index, selector) in selectors.iter_mut().enumerate() {
        selector.rank = index + 1;
        selector.id = format_runtime_selector_id(
            selector.family,
            selector.category,
            selector.support_tier,
            selector.rank,
        );
    }

    TabRuntimeProbeWorkbench { selectors }
}

fn runtime_probe_selectors_for_family(
    summary: &TabFamilyRankingSummary,
    archives: &[&TabArchiveSummary],
    inclusion: &str,
) -> Vec<TabRuntimeProbeSelector> {
    let mut selectors = Vec::new();

    if let Some(top_metadata) = summary.metadata_shape_supports.first() {
        selectors.push(make_runtime_probe_selector(
            summary.family,
            TabRuntimeProbeCategory::MetadataShape,
            support_tier_for_per_mille(top_metadata.per_mille),
            format!("candidate metadata-shape support grouping for `{}`", top_metadata.label),
            format!(
                "{inclusion}; support [{}]; {}",
                format_runtime_metadata_probe_group(summary),
                format_runtime_queue_entropy_progression(summary)
            ),
            format!(
                "probe before lower metadata-support tasks because strongest candidate support is {} per mille; run after archive cap review using aggregate groups only",
                top_metadata.per_mille
            ),
            50_000
                + top_metadata.per_mille * 6
                + summary.metadata_support_score().min(1_000)
                + runtime_queue_archive_score(summary, archives)
                + runtime_queue_entropy_consistency_score(summary),
        ));
    }

    if summary.equal_run_archives > 0 || summary.repeated_pattern_archives > 0 {
        let progression_ratio = runtime_queue_progression_per_mille(summary);
        selectors.push(make_runtime_probe_selector(
            summary.family,
            TabRuntimeProbeCategory::FixedLengthBuckets,
            support_tier_for_per_mille(progression_ratio),
            "repeated fixed-length bucket selector grouping".to_string(),
            format!(
                "{inclusion}; {}; {}; {}",
                format_runtime_progression_probe_support(summary, archives),
                format_runtime_length_bucket_probe_group(archives),
                format_runtime_queue_entropy_progression(summary)
            ),
            "probe before sibling bucket comparison when repeated-record support is present; run after metadata grouping if bounded metadata support is stronger".to_string(),
            30_000
                + progression_ratio * 5
                + runtime_queue_length_bucket_support(archives).min(1_000)
                + runtime_queue_archive_score(summary, archives)
                + runtime_queue_entropy_consistency_score(summary),
        ));
    }

    if summary.command_stream_chunks > 0 || summary.raw_chunks > 0 || summary.unknown_chunks > 0 {
        let command_ratio = ratio_per_mille(summary.command_stream_chunks, summary.total_chunks);
        selectors.push(make_runtime_probe_selector(
            summary.family,
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
                format_runtime_queue_entropy_progression(summary)
            ),
            "probe before rendering experiments that mix classifier groups; run after stronger metadata or fixed-length grouping when those aggregate signals dominate".to_string(),
            40_000
                + command_ratio * 4
                + summary.total_chunks.min(1_000)
                + runtime_queue_archive_score(summary, archives)
                + runtime_queue_entropy_consistency_score(summary),
        ));

        let mixed_unknown_chunks = summary.raw_chunks.saturating_add(summary.unknown_chunks);
        if mixed_unknown_chunks > 0 {
            let mixed_unknown_ratio = ratio_per_mille(mixed_unknown_chunks, summary.total_chunks);
            selectors.push(make_runtime_probe_selector(
                summary.family,
                TabRuntimeProbeCategory::MixedUnknownAudit,
                support_tier_for_per_mille(mixed_unknown_ratio),
                "fallback mixed/raw/unknown aggregate selector audit".to_string(),
                format!(
                    "{inclusion}; raw+unknown {} chunks ({} per mille); command-stream {} chunks ({} per mille); {}",
                    mixed_unknown_chunks,
                    mixed_unknown_ratio,
                    summary.command_stream_chunks,
                    command_ratio,
                    format_runtime_queue_entropy_progression(summary)
                ),
                "probe after metadata, classifier, repeated-length, and sibling bucket phases; use only to audit unresolved aggregate groups before any render attempt".to_string(),
                10_000
                    + mixed_unknown_ratio * 4
                    + runtime_queue_archive_score(summary, archives)
                    + runtime_queue_entropy_consistency_score(summary),
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
            summary.family,
            TabRuntimeProbeCategory::SiblingCommonBuckets,
            sibling_bucket_support_tier(summary),
            focus.to_string(),
            format!(
                "{inclusion}; common bucket support [{}]; {}",
                format_tab_family_common_bucket_overlap(summary),
                format_runtime_queue_entropy_progression(summary)
            ),
            rationale.to_string(),
            20_000
                + runtime_queue_common_bucket_score(summary)
                + runtime_queue_archive_score(summary, archives)
                + runtime_queue_entropy_consistency_score(summary),
        ));
    }

    selectors
}

fn make_runtime_probe_selector(
    family: &'static str,
    category: TabRuntimeProbeCategory,
    support_tier: TabRuntimeProbeSupportTier,
    focus: String,
    evidence: String,
    rationale: String,
    priority: usize,
) -> TabRuntimeProbeSelector {
    TabRuntimeProbeSelector {
        id: String::new(),
        rank: 0,
        family,
        category,
        support_tier,
        focus,
        evidence,
        rationale,
        priority,
    }
}

fn format_runtime_probe_queue_row(selector: &TabRuntimeProbeSelector) -> String {
    format!(
        "| {} | `{}` | {} | {} | {} | runtime-only aggregate probe queue; selector `{}` is a local dry-run handle only; local runtime inspection may decode user-supplied assets but report rows do not expose bytes, raw headers/chunks, previews, decoded dimensions, anchors, commands, audio, UI, or gameplay semantics; not proof of decoded layout or semantics |",
        selector.rank,
        selector.family,
        selector.focus,
        selector.evidence,
        selector.rationale,
        selector.id
    )
}

fn format_runtime_probe_selector_catalog_row(selector: &TabRuntimeProbeSelector) -> String {
    format!(
        "| `{}` | `{}` | {} | {} | {} | {} | {} | runtime-only aggregate selector; not proof of decoded layout or semantics and not an asset identifier |",
        selector.id,
        selector.family,
        selector.support_tier.label(),
        selector.focus,
        selector.evidence,
        format_runtime_selector_preconditions(selector),
        format_runtime_selector_stop_conditions(selector)
    )
}

fn format_runtime_probe_dry_run_rows(selectors: &[TabRuntimeProbeSelector]) -> Vec<String> {
    let mut grouped: BTreeMap<TabRuntimeProbeCategory, Vec<&TabRuntimeProbeSelector>> =
        BTreeMap::new();
    for selector in selectors {
        grouped.entry(selector.category).or_default().push(selector);
    }

    grouped
        .into_iter()
        .map(|(category, mut selectors)| {
            selectors.sort_by(|left, right| {
                left.rank
                    .cmp(&right.rank)
                    .then_with(|| left.family.cmp(right.family))
            });
            format!(
                "| {}. {} | {} | {} | {} | {} | runtime-only dry-run phase; aggregate ordering does not decode, render, or prove asset layout or semantics |",
                category.phase_order(),
                category.phase_label(),
                format_runtime_selector_id_list(&selectors),
                format_runtime_phase_evidence(category, &selectors),
                category.phase_rationale(),
                category.phase_stop_condition()
            )
        })
        .collect()
}

fn format_runtime_selector_id(
    family: &'static str,
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

fn format_runtime_selector_preconditions(selector: &TabRuntimeProbeSelector) -> String {
    format!(
        "local user-supplied assets available at runtime; group by aggregate selector `{}` before any decode or render attempt",
        selector.id
    )
}

fn format_runtime_selector_stop_conditions(selector: &TabRuntimeProbeSelector) -> String {
    match selector.category {
        TabRuntimeProbeCategory::MetadataShape => "do not infer dimensions, anchors, or commands from metadata-shape ranges alone; do not commit generated previews or decoded asset-derived bytes".to_string(),
        TabRuntimeProbeCategory::ClassifierGrouping => "do not treat classifier labels as decoded commands, pixels, audio, UI, or gameplay semantics; do not commit generated previews or decoded asset-derived bytes".to_string(),
        TabRuntimeProbeCategory::FixedLengthBuckets => "do not treat repeated lengths as decoded records or frame dimensions; do not commit generated previews or decoded asset-derived bytes".to_string(),
        TabRuntimeProbeCategory::SiblingCommonBuckets => "do not treat common buckets as shared layout proof; do not commit generated previews or decoded asset-derived bytes".to_string(),
        TabRuntimeProbeCategory::MixedUnknownAudit => "do not promote mixed/raw/unknown groups to render semantics without stronger evidence; do not commit generated previews or decoded asset-derived bytes".to_string(),
    }
}

fn format_runtime_selector_id_list(selectors: &[&TabRuntimeProbeSelector]) -> String {
    const MAX_IDS: usize = 4;
    let mut ids = selectors
        .iter()
        .take(MAX_IDS)
        .map(|selector| format!("`{}`", selector.id))
        .collect::<Vec<_>>();
    if selectors.len() > MAX_IDS {
        ids.push(format!(
            "{} more capped selectors",
            selectors.len() - MAX_IDS
        ));
    }
    ids.join("; ")
}

fn format_runtime_phase_evidence(
    category: TabRuntimeProbeCategory,
    selectors: &[&TabRuntimeProbeSelector],
) -> String {
    let mut families = selectors
        .iter()
        .map(|selector| selector.family)
        .collect::<Vec<_>>();
    families.sort_unstable();
    families.dedup();
    let mut tiers = selectors
        .iter()
        .map(|selector| selector.support_tier.label())
        .collect::<Vec<_>>();
    tiers.sort_unstable();
    tiers.dedup();
    let ranks = selectors
        .iter()
        .map(|selector| selector.rank.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{} aggregate selectors {}; families [{}]; support tiers [{}]; capped ranks [{}]",
        category.phase_label(),
        selectors.len(),
        families.join(", "),
        tiers.join(", "),
        ranks
    )
}

fn format_runtime_queue_entropy_progression(summary: &TabFamilyRankingSummary) -> String {
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

fn runtime_queue_progression_per_mille(summary: &TabFamilyRankingSummary) -> usize {
    ratio_per_mille(
        summary
            .equal_run_archives
            .saturating_add(summary.repeated_pattern_archives),
        summary.parsed_archives.saturating_mul(2),
    )
}

fn runtime_queue_length_bucket_support(archives: &[&TabArchiveSummary]) -> usize {
    archives
        .iter()
        .flat_map(|archive| archive.bank.common_chunk_len_buckets.iter())
        .map(|bucket| bucket.count)
        .sum()
}

fn runtime_queue_common_bucket_score(summary: &TabFamilyRankingSummary) -> usize {
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

fn runtime_queue_archive_score(
    summary: &TabFamilyRankingSummary,
    archives: &[&TabArchiveSummary],
) -> usize {
    ratio_per_mille(archives.len(), summary.parsed_archives)
}

fn runtime_queue_entropy_consistency_score(summary: &TabFamilyRankingSummary) -> usize {
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

fn sibling_bucket_support_tier(summary: &TabFamilyRankingSummary) -> TabRuntimeProbeSupportTier {
    if summary.parsed_archives > 1 && summary.common_bucket_overlap.len() >= 3 {
        TabRuntimeProbeSupportTier::Strong
    } else if !summary.common_bucket_overlap.is_empty() {
        TabRuntimeProbeSupportTier::Medium
    } else {
        TabRuntimeProbeSupportTier::Limited
    }
}

impl TabRuntimeProbeCategory {
    fn slug(self) -> &'static str {
        match self {
            Self::MetadataShape => "metadata-shape",
            Self::ClassifierGrouping => "classifier-grouping",
            Self::FixedLengthBuckets => "fixed-length-buckets",
            Self::SiblingCommonBuckets => "sibling-common-buckets",
            Self::MixedUnknownAudit => "mixed-unknown-audit",
        }
    }

    fn phase_label(self) -> &'static str {
        match self {
            Self::MetadataShape => "metadata-shape grouping",
            Self::ClassifierGrouping => "command-stream-heavy separation",
            Self::FixedLengthBuckets => "repeated fixed-length bucket grouping",
            Self::SiblingCommonBuckets => "sibling/common bucket comparison",
            Self::MixedUnknownAudit => "fallback mixed/unknown audit",
        }
    }

    fn phase_order(self) -> usize {
        match self {
            Self::MetadataShape => 1,
            Self::ClassifierGrouping => 2,
            Self::FixedLengthBuckets => 3,
            Self::SiblingCommonBuckets => 4,
            Self::MixedUnknownAudit => 5,
        }
    }

    fn phase_rationale(self) -> &'static str {
        match self {
            Self::MetadataShape => {
                "start with bounded aggregate metadata-shape groups before classifier or length probes"
            }
            Self::ClassifierGrouping => {
                "separate command-stream-heavy aggregate groups before any render-oriented experiment"
            }
            Self::FixedLengthBuckets => {
                "group repeated fixed-length candidates after metadata/classifier separation"
            }
            Self::SiblingCommonBuckets => {
                "compare sibling/common buckets only after per-family groups are established"
            }
            Self::MixedUnknownAudit => {
                "audit unresolved mixed/raw/unknown selectors last and keep them local-only"
            }
        }
    }

    fn phase_stop_condition(self) -> &'static str {
        match self {
            Self::MetadataShape => {
                "stop before treating ranges as decoded dimensions, anchors, or commands"
            }
            Self::ClassifierGrouping => {
                "stop before treating classifier labels as decoded commands, pixels, audio, UI, or gameplay semantics"
            }
            Self::FixedLengthBuckets => {
                "stop before treating repeated lengths as decoded records or frame dimensions"
            }
            Self::SiblingCommonBuckets => {
                "stop before treating common buckets as proof of shared layout"
            }
            Self::MixedUnknownAudit => {
                "stop before promoting mixed/raw/unknown aggregates to render semantics"
            }
        }
    }
}

impl TabRuntimeProbeSupportTier {
    fn label(self) -> &'static str {
        match self {
            Self::Strong => "strong aggregate support",
            Self::Medium => "medium aggregate support",
            Self::Limited => "limited aggregate support",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Self::Strong => "strong",
            Self::Medium => "medium",
            Self::Limited => "limited",
        }
    }
}

fn format_tab_family_comparison_rows(tab_analyses: &[TabBankReportAnalysis]) -> Vec<String> {
    tab_family_comparison_summaries(tab_analyses)
        .into_iter()
        .map(|summary| {
            format!(
                "| `{}` vs `{}` | {} | {} | {} | {} | {} | {} | {} | {} |",
                summary.left_family,
                summary.right_family,
                format_tab_family_ratio_differences(&summary.metadata_differences),
                format_tab_family_ratio_differences(&summary.classifier_differences),
                format_tab_family_progression_difference(&summary),
                format_tab_family_entropy_comparison(&summary),
                format_tab_family_bucket_comparison(&summary),
                format_clue_list(&summary.shared_compatibility_clues),
                format_clue_list(&summary.distinguishing_clues),
                "aggregate comparison only; compatibility clues do not decode or prove family semantics"
            )
        })
        .collect()
}

fn tab_family_comparison_summaries(
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<TabFamilyComparisonSummary> {
    let rankings = tab_family_ranking_summaries(tab_analyses);
    let selected = rankings
        .iter()
        .filter(|summary| is_sprite_like_family_candidate(summary))
        .take(3)
        .collect::<Vec<_>>();

    let mut comparisons = Vec::new();
    for left_index in 0..selected.len() {
        for right_index in left_index + 1..selected.len() {
            comparisons.push(compare_tab_families(
                selected[left_index],
                selected[right_index],
            ));
        }
    }
    comparisons
}

fn is_sprite_like_family_candidate(summary: &TabFamilyRankingSummary) -> bool {
    summary.family != "SOUND"
        && summary.family != "OTHER"
        && (summary.metadata_support_score() > 0
            || summary.command_stream_chunks > 0
            || summary.raw_chunks > 0)
}

fn compare_tab_families(
    left: &TabFamilyRankingSummary,
    right: &TabFamilyRankingSummary,
) -> TabFamilyComparisonSummary {
    let metadata_differences = compare_metadata_ratios(left, right);
    let classifier_differences = compare_classifier_ratios(left, right);
    let equal_run_archive_ratio_delta = signed_delta_per_mille(
        ratio_per_mille(left.equal_run_archives, left.parsed_archives),
        ratio_per_mille(right.equal_run_archives, right.parsed_archives),
    );
    let repeated_pattern_archive_ratio_delta = signed_delta_per_mille(
        ratio_per_mille(left.repeated_pattern_archives, left.parsed_archives),
        ratio_per_mille(right.repeated_pattern_archives, right.parsed_archives),
    );
    let left_buckets = left
        .common_bucket_overlap
        .iter()
        .map(|bucket| bucket.len)
        .collect::<Vec<_>>();
    let right_buckets = right
        .common_bucket_overlap
        .iter()
        .map(|bucket| bucket.len)
        .collect::<Vec<_>>();
    let overlapping_common_buckets = sorted_intersection(&left_buckets, &right_buckets);
    let left_distinct_common_buckets = sorted_difference(&left_buckets, &right_buckets);
    let right_distinct_common_buckets = sorted_difference(&right_buckets, &left_buckets);
    let shared_compatibility_clues = shared_tab_family_clues(
        left,
        right,
        &metadata_differences,
        &classifier_differences,
        &overlapping_common_buckets,
        equal_run_archive_ratio_delta,
        repeated_pattern_archive_ratio_delta,
    );
    let distinguishing_clues = distinguishing_tab_family_clues(
        left,
        right,
        &metadata_differences,
        &classifier_differences,
    );

    TabFamilyComparisonSummary {
        left_family: left.family,
        right_family: right.family,
        metadata_differences,
        classifier_differences,
        equal_run_archive_ratio_delta,
        repeated_pattern_archive_ratio_delta,
        left_entropy_range: (left.min_entropy_milli_bits, left.max_entropy_milli_bits),
        right_entropy_range: (right.min_entropy_milli_bits, right.max_entropy_milli_bits),
        overlapping_common_buckets,
        left_distinct_common_buckets,
        right_distinct_common_buckets,
        shared_compatibility_clues,
        distinguishing_clues,
    }
}

fn shared_tab_family_clues(
    left: &TabFamilyRankingSummary,
    right: &TabFamilyRankingSummary,
    metadata_differences: &[TabFamilyRatioDifference],
    classifier_differences: &[TabFamilyRatioDifference],
    overlapping_common_buckets: &[u32],
    equal_run_archive_ratio_delta: i32,
    repeated_pattern_archive_ratio_delta: i32,
) -> Vec<String> {
    let mut clues = Vec::new();
    let shared_metadata = metadata_differences
        .iter()
        .filter(|difference| difference.left_per_mille > 0 && difference.right_per_mille > 0)
        .min_by_key(|difference| difference.delta_per_mille.abs());
    if let Some(difference) = shared_metadata {
        clues.push(format!(
            "shared candidate metadata-shape `{}` at {} vs {} per mille",
            difference.label, difference.left_per_mille, difference.right_per_mille
        ));
    }

    if !overlapping_common_buckets.is_empty() {
        clues.push(format!(
            "overlapping common-size buckets [{}]",
            format_bucket_lens(overlapping_common_buckets)
        ));
    }

    if equal_run_archive_ratio_delta.abs() <= 100
        && repeated_pattern_archive_ratio_delta.abs() <= 100
    {
        clues.push("similar aggregate chunk-length progression support".to_string());
    }

    if let Some(difference) = classifier_differences
        .iter()
        .min_by_key(|difference| difference.delta_per_mille.abs())
        .filter(|difference| difference.delta_per_mille.abs() <= 100)
    {
        clues.push(format!(
            "similar `{}` classifier ratio at {} vs {} per mille",
            difference.label, difference.left_per_mille, difference.right_per_mille
        ));
    }

    if clues.is_empty() {
        clues.push(format!(
            "both families are parsed sprite-like aggregate candidates with {} and {} chunks",
            left.total_chunks, right.total_chunks
        ));
    }
    clues.truncate(3);
    clues
}

fn distinguishing_tab_family_clues(
    left: &TabFamilyRankingSummary,
    right: &TabFamilyRankingSummary,
    metadata_differences: &[TabFamilyRatioDifference],
    classifier_differences: &[TabFamilyRatioDifference],
) -> Vec<String> {
    let mut clues = Vec::new();
    if let Some(difference) = metadata_differences.first() {
        clues.push(format!(
            "metadata clue `{}` differs by {:+} per mille",
            difference.label, difference.delta_per_mille
        ));
    }
    if let Some(difference) = classifier_differences.first() {
        clues.push(format!(
            "classifier clue `{}` differs by {:+} per mille",
            difference.label, difference.delta_per_mille
        ));
    }

    let entropy_mid_delta = signed_delta_per_mille(
        ((left.min_entropy_milli_bits + left.max_entropy_milli_bits) / 2) as usize,
        ((right.min_entropy_milli_bits + right.max_entropy_milli_bits) / 2) as usize,
    );
    if entropy_mid_delta.abs() >= 500 {
        clues.push(format!(
            "chunk-size entropy midpoint differs by {:+.3} bits",
            entropy_mid_delta as f32 / 1000.0
        ));
    }

    if clues.is_empty() {
        clues.push("no single strong distinguishing aggregate clue selected".to_string());
    }
    clues.truncate(3);
    clues
}

fn compare_metadata_ratios(
    left: &TabFamilyRankingSummary,
    right: &TabFamilyRankingSummary,
) -> Vec<TabFamilyRatioDifference> {
    let mut labels = left
        .metadata_shape_supports
        .iter()
        .map(|support| support.label)
        .chain(
            right
                .metadata_shape_supports
                .iter()
                .map(|support| support.label),
        )
        .collect::<Vec<_>>();
    labels.sort_unstable();
    labels.dedup();

    let mut differences = labels
        .into_iter()
        .map(|label| {
            let left_per_mille = left
                .metadata_shape_supports
                .iter()
                .find(|support| support.label == label)
                .map(|support| support.per_mille)
                .unwrap_or(0);
            let right_per_mille = right
                .metadata_shape_supports
                .iter()
                .find(|support| support.label == label)
                .map(|support| support.per_mille)
                .unwrap_or(0);
            TabFamilyRatioDifference {
                label: label.to_string(),
                left_per_mille,
                right_per_mille,
                delta_per_mille: signed_delta_per_mille(left_per_mille, right_per_mille),
            }
        })
        .collect::<Vec<_>>();
    sort_ratio_differences(&mut differences);
    differences.truncate(4);
    differences
}

fn compare_classifier_ratios(
    left: &TabFamilyRankingSummary,
    right: &TabFamilyRankingSummary,
) -> Vec<TabFamilyRatioDifference> {
    let classifier_counts = [
        (
            SpriteChunkKind::LikelyRleOrCommandStream.conservative_label(),
            left.command_stream_chunks,
            right.command_stream_chunks,
        ),
        (
            SpriteChunkKind::LikelyRawIndexed.conservative_label(),
            left.raw_chunks,
            right.raw_chunks,
        ),
        (
            SpriteChunkKind::Unknown.conservative_label(),
            left.unknown_chunks,
            right.unknown_chunks,
        ),
    ];
    let mut differences = classifier_counts
        .into_iter()
        .map(|(label, left_count, right_count)| {
            let left_per_mille = ratio_per_mille(left_count, left.total_chunks);
            let right_per_mille = ratio_per_mille(right_count, right.total_chunks);
            TabFamilyRatioDifference {
                label: label.to_string(),
                left_per_mille,
                right_per_mille,
                delta_per_mille: signed_delta_per_mille(left_per_mille, right_per_mille),
            }
        })
        .collect::<Vec<_>>();
    sort_ratio_differences(&mut differences);
    differences
}

fn sort_ratio_differences(differences: &mut [TabFamilyRatioDifference]) {
    differences.sort_by(|left, right| {
        right
            .delta_per_mille
            .abs()
            .cmp(&left.delta_per_mille.abs())
            .then_with(|| left.label.cmp(&right.label))
    });
}

fn format_tab_family_ratio_differences(differences: &[TabFamilyRatioDifference]) -> String {
    if differences.is_empty() {
        return "no aggregate ratio differences available".to_string();
    }

    differences
        .iter()
        .map(|difference| {
            format!(
                "{}: left {} per mille, right {} per mille, delta {:+} per mille",
                difference.label,
                difference.left_per_mille,
                difference.right_per_mille,
                difference.delta_per_mille
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn format_tab_family_progression_difference(summary: &TabFamilyComparisonSummary) -> String {
    format!(
        "equal-size run archive-ratio delta {:+} per mille; repeated size-pattern archive-ratio delta {:+} per mille",
        summary.equal_run_archive_ratio_delta, summary.repeated_pattern_archive_ratio_delta
    )
}

fn format_tab_family_entropy_comparison(summary: &TabFamilyComparisonSummary) -> String {
    format!(
        "{} {:.3}..{:.3} bits vs {} {:.3}..{:.3} bits",
        summary.left_family,
        summary.left_entropy_range.0 as f32 / 1000.0,
        summary.left_entropy_range.1 as f32 / 1000.0,
        summary.right_family,
        summary.right_entropy_range.0 as f32 / 1000.0,
        summary.right_entropy_range.1 as f32 / 1000.0
    )
}

fn format_tab_family_bucket_comparison(summary: &TabFamilyComparisonSummary) -> String {
    format!(
        "overlap [{}]; {} distinct [{}]; {} distinct [{}]",
        format_bucket_lens(&summary.overlapping_common_buckets),
        summary.left_family,
        format_bucket_lens(&summary.left_distinct_common_buckets),
        summary.right_family,
        format_bucket_lens(&summary.right_distinct_common_buckets)
    )
}

fn format_clue_list(clues: &[String]) -> String {
    if clues.is_empty() {
        return "no aggregate clues selected".to_string();
    }

    clues.join("; ")
}

fn format_bucket_lens(lengths: &[u32]) -> String {
    if lengths.is_empty() {
        return "none".to_string();
    }

    lengths
        .iter()
        .map(|len| format!("{len} bytes"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sorted_intersection(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut values = left
        .iter()
        .copied()
        .filter(|len| right.contains(len))
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

fn sorted_difference(left: &[u32], right: &[u32]) -> Vec<u32> {
    let mut values = left
        .iter()
        .copied()
        .filter(|len| !right.contains(len))
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

fn signed_delta_per_mille(left: usize, right: usize) -> i32 {
    left as i32 - right as i32
}

fn tab_file_family(path: &str) -> &'static str {
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

fn format_tab_block_relation_rows(
    block_analyses: &[(String, BlockGraphicsAnalysis)],
    tab_analyses: &[TabBankReportAnalysis],
) -> Vec<String> {
    let mut rows = Vec::new();
    for tab in tab_analyses {
        let Some(bank) = &tab.archive_bank else {
            continue;
        };
        let chunk_count = bank.entry_count();
        for (path, block) in block_analyses {
            let candidates = block
                .record_candidates
                .iter()
                .filter(|candidate| candidate.record_count > 0)
                .filter(|candidate| {
                    candidate.record_count == chunk_count
                        || candidate.record_count.abs_diff(chunk_count) <= 2
                })
                .map(|candidate| {
                    format!(
                        "{}x{}:{}",
                        candidate.width, candidate.height, candidate.record_count
                    )
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                continue;
            }

            rows.push(format!(
                "| TAB/DAT chunk-count compatibility candidate | `{}`, `{}` | {} chunks vs block record candidates [{}] | TAB DAT bytes {}; {}; best variant {} | chunk count compatibility is aggregate-only and does not imply sprite/tile format equivalence |",
                tab.path,
                path,
                chunk_count,
                candidates.join(", "),
                tab.dat_len,
                tab.archive_summary
                    .as_ref()
                    .map(|summary| format_tab_relation_evidence(&summary.bank))
                    .unwrap_or_else(|| format!(
                        "chunk-size range {}..{}",
                        bank.min_chunk_len().unwrap_or(0),
                        bank.max_chunk_len().unwrap_or(0)
                    )),
                tab.best_variant
                    .map(|score| format!("TAB{}", score.offset_width * 8))
                    .unwrap_or_else(|| "unknown".to_string())
            ));
        }
    }
    rows
}

fn format_tab_relation_evidence(summary: &TabBankSummary) -> String {
    format!(
        "chunk len min/med/max {}/{}/{}; entropy {:.3}; {}; common [{}]",
        summary.min_chunk_len,
        summary.median_chunk_len,
        summary.max_chunk_len,
        summary.chunk_len_entropy_milli_bits as f32 / 1000.0,
        format_tab_candidate_size_matches(summary),
        format_tab_common_size_buckets(summary)
    )
}

fn find_block_analysis<'a>(
    block_analyses: &'a [(String, BlockGraphicsAnalysis)],
    path: &str,
) -> Option<&'a BlockGraphicsAnalysis> {
    block_analyses
        .iter()
        .find_map(|(candidate_path, analysis)| (candidate_path == path).then_some(analysis))
}

fn ratio_per_mille(numerator: usize, denominator: usize) -> usize {
    if denominator == 0 {
        return 0;
    }
    numerator.saturating_mul(1000) / denominator
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
    use super::{AssetReport, TabBankReportAnalysis};
    use crate::engine::sprite_decode::SpriteBankAggregateSummary;
    use crate::engine::tab_bank::{TabArchive, TabVariantAnalysis};

    #[test]
    fn empty_report_still_renders_markdown() {
        let report = AssetReport::generate("definitely-not-a-real-asset-dir");
        let markdown = report.to_markdown();
        assert!(markdown.contains("Generated asset inspection report"));
        assert!(markdown.contains("TAB/DAT bank analysis"));
        assert!(markdown.contains("File extension inventory"));
        assert!(markdown.contains("Map inventory"));
        assert!(markdown.contains("Block/tile graphics container candidates"));
        assert!(markdown.contains("Aggregate layout probes"));
        assert!(markdown.contains("MAP substrate to block/tile candidate correlations"));
        assert!(markdown.contains("Cross-container aggregate relation probes"));
    }

    #[test]
    fn formats_sprite_bank_aggregate_without_bytes() {
        let raw = (1..=80).collect::<Vec<u8>>();
        let command = [0xff, 0x00, 0xfe, 0x00, 0x10, 0x00, 0xf8, 0x01];
        let chunks = [raw.as_slice(), command.as_slice()];
        let summary = SpriteBankAggregateSummary::from_chunks(chunks);
        let formatted = super::format_sprite_bank_aggregate(&summary);

        assert!(formatted.contains("classifier-by-size"));
        assert!(formatted.contains("zero/high-byte ratio min/med/max"));
        assert!(formatted.contains("candidate header-shapes"));
        assert!(formatted.contains("candidate metadata-shapes"));
        assert!(!formatted.contains("ff 00 fe 00"));
    }

    #[test]
    fn ranks_tab_families_with_aggregate_only_evidence() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-0.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([32, 24, 0xf0, 0], 128),
                chunk_with_prefix([32, 24, 0xf0, 0], 128),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([8, 8, 0xf0, 0], 128),
                chunk_with_prefix([8, 8, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
            ],
        );
        let mspr = make_tab_report_analysis(
            "SYNDICAT/DATA/MSPR-0.TAB",
            vec![(1..=80).collect::<Vec<u8>>(), (2..=81).collect::<Vec<u8>>()],
        );
        let rows = super::format_tab_family_ranking_rows(&[hspr_a, hspr_b, mspr]);

        assert!(rows[0].starts_with("| `HSPR` | 2 of 8 chunks"));
        assert!(rows[0].contains("candidate leading u8 width/height range"));
        assert!(rows[0].contains("repeated size-pattern candidates in 2/2 archives"));
        assert!(rows[0].contains("128 bytes in 2 archives"));
        assert!(rows.iter().any(|row| row.starts_with("| `MSPR` |")));
        assert!(!rows.join("\n").contains("f0 00"));
    }

    #[test]
    fn compares_hspr_and_mspr_with_aggregate_only_evidence() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-0.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );

        let rows = super::format_tab_family_comparison_rows(&[hspr_a, hspr_b, mspr]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 1);
        assert!(joined.contains("`HSPR` vs `MSPR`"));
        assert!(
            joined.contains("Candidate metadata-shape ratio differences")
                || joined.contains("candidate leading")
        );
        assert!(joined.contains("likely RLE/command-stream chunk candidate"));
        assert!(joined.contains("equal-size run archive-ratio delta"));
        assert!(joined.contains("overlap"));
        assert!(joined.contains("shared") || joined.contains("similar"));
        assert!(joined.contains("distinguishing") || joined.contains("differs by"));
        assert!(joined.contains("aggregate comparison only"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn selects_top_sprite_like_family_comparisons_without_all_pairs_noise() {
        let hspr = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([12, 16, 1, 1], 96),
                chunk_with_prefix([12, 16, 1, 1], 96),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let font = make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![
                chunk_with_prefix([8, 12, 0, 0], 64),
                chunk_with_prefix([8, 12, 0, 0], 64),
                chunk_with_prefix([8, 12, 0, 0], 64),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![
                chunk_with_prefix([97, 116, 0, 0], 64),
                chunk_with_prefix([97, 116, 0, 0], 64),
                chunk_with_prefix([97, 116, 0, 0], 64),
                chunk_with_prefix([97, 116, 0, 0], 64),
            ],
        );

        let rows = super::format_tab_family_comparison_rows(&[hspr, mspr, font, sound]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 3);
        assert!(joined.contains("`HSPR` vs `MSPR`") || joined.contains("`MSPR` vs `HSPR`"));
        assert!(joined.contains("`HSPR` vs `FONT`") || joined.contains("`FONT` vs `HSPR`"));
        assert!(joined.contains("`MSPR` vs `FONT`") || joined.contains("`FONT` vs `MSPR`"));
        assert!(!joined.contains("SOUND` vs"));
        assert!(joined.contains("shared candidate") || joined.contains("similar aggregate"));
        assert!(joined.contains("metadata clue") || joined.contains("classifier clue"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_tab_family_next_investigation_hints_without_bytes() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows = super::format_tab_family_hint_rows(&[hspr_a, hspr_b, mspr, sound]);
        let joined = rows.join("\n");

        assert!(joined.contains("`HSPR`"));
        assert!(joined.contains("`MSPR`"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("aggregate inspection priority"));
        assert!(
            joined.contains("candidate leading") || joined.contains("candidate metadata-shape")
        );
        assert!(joined.contains("strongest classifier pattern"));
        assert!(joined.contains("prioritize"));
        assert!(joined.contains("records") || joined.contains("repeated-length evidence"));
        assert!(joined.contains("entropy"));
        assert!(joined.contains("aggregate hint only"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_top_priority_archive_evidence_rows_with_caps_without_bytes() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let font = make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![
                chunk_with_prefix([8, 12, 0, 0], 64),
                chunk_with_prefix([8, 12, 0, 0], 64),
            ],
        );
        let other = make_tab_report_analysis(
            "DATA/OTHER-0.TAB",
            vec![
                chunk_with_prefix([8, 12, 0, 0], 64),
                chunk_with_prefix([8, 12, 0, 0], 64),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows = super::format_tab_family_archive_evidence_rows(&[
            hspr_a, hspr_b, mspr, font, other, sound,
        ]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 4);
        assert!(joined.contains("`HSPR`"));
        assert!(joined.contains("`MSPR`"));
        assert!(joined.contains("`FONT`"));
        assert!(!joined.contains("`SOUND`"));
        assert!(!joined.contains("`OTHER`"));
        assert!(joined.contains("len min/med/max"));
        assert!(joined.contains("size bands small/medium/large"));
        assert!(joined.contains("candidate leading"));
        assert!(joined.contains("classifier") || joined.contains("chunk candidate"));
        assert!(joined.contains("chunk-size entropy"));
        assert!(joined.contains("capped aggregate archive evidence only"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_archive_evidence_rows_per_family() {
        let analyses = (0..6)
            .map(|index| {
                make_tab_report_analysis(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();

        let rows = super::format_tab_family_archive_evidence_rows(&analyses);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 4);
        assert!(joined.contains("HSPR-0.TAB"));
        assert!(joined.contains("HSPR-3.TAB"));
        assert!(!joined.contains("HSPR-4.TAB"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_investigation_dashboard_with_selection_consistency_and_hypotheses() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows =
            super::format_tab_family_investigation_dashboard_rows(&[hspr_a, hspr_b, mspr, sound]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 2);
        assert!(joined.contains("TAB") || joined.contains("HSPR"));
        assert!(joined.contains("`HSPR`"));
        assert!(joined.contains("`MSPR`"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("top selected"));
        assert!(joined.contains("selected by aggregate ranking"));
        assert!(joined.contains("Within-family") || joined.contains("archive count"));
        assert!(joined.contains("shared metadata-shape candidates"));
        assert!(joined.contains("classifier-by-size-band"));
        assert!(joined.contains("common bucket overlap/distinction"));
        assert!(joined.contains("progression agreement"));
        assert!(joined.contains("runtime-only next probe"));
        assert!(
            joined.contains("fixed-length command/control-record")
                || joined.contains("common-size bucket groups")
        );
        assert!(
            joined.contains("aggregate consistency and runtime-only next-probe hypotheses only")
        );
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_investigation_dashboard_families_and_archives_without_bytes() {
        let mut analyses = (0..6)
            .map(|index| {
                make_tab_report_analysis(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        analyses.push(make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let rows = super::format_tab_family_investigation_dashboard_rows(&analyses);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 3);
        assert!(joined.contains("capped at 4/6 parsed archives"));
        assert!(joined.contains("per-family dashboard cap 4 archives"));
        assert!(!joined.contains("`SOUND`"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_runtime_probe_plans_with_aggregate_groups_and_conservative_language() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows = super::format_tab_family_runtime_probe_plan_rows(&[hspr_a, hspr_b, mspr, sound]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 8);
        assert!(joined.contains("`HSPR`"));
        assert!(joined.contains("`MSPR`"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("fixed-length repeated-record candidate groups"));
        assert!(joined.contains("candidate leading metadata-shape groups"));
        assert!(joined.contains("classifier grouping"));
        assert!(joined.contains("sibling common-bucket comparison"));
        assert!(joined.contains("candidate chunk-length buckets"));
        assert!(joined.contains("per mille"));
        assert!(joined.contains("runtime-only aggregate probe plan"));
        assert!(joined.contains("aggregate probe plan only"));
        assert!(joined.contains("not proof") || joined.contains("without treating ranges"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_runtime_probe_plan_families_and_archives_without_bytes() {
        let mut analyses = (0..6)
            .map(|index| {
                make_tab_report_analysis(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        analyses.push(make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let rows = super::format_tab_family_runtime_probe_plan_rows(&analyses);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 12);
        assert!(joined.contains("capped at 4/6 parsed archives"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("do not expose bytes, raw headers/chunks"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_runtime_probe_queue_with_cross_family_ranking_and_conservative_language() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let font = make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![
                chunk_with_prefix([8, 12, 0, 0], 64),
                chunk_with_prefix([8, 12, 0, 0], 64),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows =
            super::format_tab_family_runtime_probe_queue_rows(&[hspr_a, hspr_b, mspr, font, sound]);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 6);
        assert!(
            rows[0].starts_with("| 1 | `")
                && rows[0].contains("candidate metadata-shape support grouping")
                && rows[0].contains("1000 per mille"),
            "{joined}"
        );
        assert!(joined.contains("`HSPR`"));
        assert!(joined.contains("`MSPR`"));
        assert!(joined.contains("aggregate probe queue"));
        assert!(joined.contains("runtime-only"));
        assert!(joined.contains("before"));
        assert!(joined.contains("after"));
        assert!(joined.contains("per mille"));
        assert!(joined.contains("entropy spread"));
        assert!(joined.contains("not proof of decoded layout or semantics"));
        assert!(!joined.contains("`SOUND`"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_runtime_probe_queue_families_archives_and_rows_without_bytes() {
        let mut analyses = (0..6)
            .map(|index| {
                make_tab_report_analysis(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        analyses.push(make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let rows = super::format_tab_family_runtime_probe_queue_rows(&analyses);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 6);
        assert!(joined.contains("capped at 4/6 parsed archives"));
        assert!(joined.contains("top selected `HSPR` sprite-like family candidate"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("do not expose bytes, raw headers/chunks"));
        assert!(joined.contains("not proof of decoded layout or semantics"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_runtime_probe_selector_catalog_with_stable_ids_and_stop_conditions() {
        let id = super::format_runtime_selector_id(
            "HSPR",
            super::TabRuntimeProbeCategory::MetadataShape,
            super::TabRuntimeProbeSupportTier::Strong,
            1,
        );
        assert_eq!(id, "tab-sprite-hspr-metadata-shape-strong-r01");

        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );
        let sound = make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        );

        let rows = super::format_tab_family_runtime_probe_selector_catalog_rows(&[
            hspr_a, hspr_b, mspr, sound,
        ]);
        let joined = rows.join("\n");

        assert!(!rows.is_empty());
        assert!(rows.len() <= 10);
        assert!(joined.contains("tab-sprite-"));
        assert!(joined.contains("-r01"));
        assert!(
            joined.contains("strong aggregate support")
                || joined.contains("medium aggregate support")
        );
        assert!(joined.contains("local user-supplied assets available at runtime"));
        assert!(joined.contains("group by aggregate selector"));
        assert!(joined.contains("do not infer dimensions, anchors, or commands"));
        assert!(joined.contains("do not commit generated previews or decoded asset-derived bytes"));
        assert!(joined.contains("not proof of decoded layout or semantics"));
        assert!(!joined.contains("`SOUND`"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn orders_runtime_probe_dry_run_phases_from_selector_workbench() {
        let hspr_a = make_tab_report_analysis(
            "SYNDICAT/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([16, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let hspr_b = make_tab_report_analysis(
            "DATADISK/DATA/HSPR-1.TAB",
            vec![
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([24, 16, 0xf0, 0], 128),
                chunk_with_prefix([0, 0, 12, 0], 20),
                chunk_with_prefix([0, 0, 12, 0], 20),
            ],
        );
        let mspr = make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![
                chunk_with_prefix([8, 12, 1, 1], 64),
                chunk_with_prefix([10, 12, 1, 1], 64),
                (1..=80).collect::<Vec<u8>>(),
                (2..=81).collect::<Vec<u8>>(),
            ],
        );

        let rows = super::format_tab_family_runtime_probe_dry_run_rows(&[hspr_a, hspr_b, mspr]);
        let joined = rows.join("\n");

        assert!(!rows.is_empty());
        assert!(rows[0].starts_with("| 1. metadata-shape grouping |"));
        assert!(joined.contains("2. command-stream-heavy separation"));
        assert!(joined.contains("3. repeated fixed-length bucket grouping"));
        assert!(joined.contains("4. sibling/common bucket comparison"));
        assert!(joined.contains("5. fallback mixed/unknown audit"));
        assert!(joined.contains("tab-sprite-"));
        assert!(joined.contains("support tiers"));
        assert!(joined.contains("capped ranks"));
        assert!(joined.contains("stop before"));
        assert!(joined.contains("runtime-only dry-run phase"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn formats_shared_runtime_probe_manifest_summary_without_bytes() {
        let analyses = vec![
            make_tab_report_analysis(
                "SYNDICAT/DATA/HSPR-1.TAB",
                vec![
                    chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                ],
            ),
            make_tab_report_analysis(
                "DATADISK/DATA/HSPR-1.TAB",
                vec![
                    chunk_with_prefix([24, 16, 0xf0, 0], 128),
                    chunk_with_prefix([24, 16, 0xf0, 0], 128),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                    chunk_with_prefix([0, 0, 12, 0], 20),
                ],
            ),
            make_tab_report_analysis(
                "DATADISK/DATA/MSPR-0-D.TAB",
                vec![
                    chunk_with_prefix([8, 12, 1, 1], 64),
                    chunk_with_prefix([10, 12, 1, 1], 64),
                    (1..=80).collect::<Vec<u8>>(),
                    (2..=81).collect::<Vec<u8>>(),
                ],
            ),
            make_tab_report_analysis(
                "SYNDICAT/DATA/SOUND-0.TAB",
                vec![chunk_with_prefix([97, 116, 0, 0], 64)],
            ),
        ];

        let manifest = super::tab_runtime_probe_manifest_from_report_analyses(&analyses);
        let summary_rows = super::format_tab_runtime_probe_manifest_rows(&manifest);
        let phase_rows = super::format_tab_runtime_probe_manifest_phase_rows(&manifest);
        let joined = [summary_rows.join("\n"), phase_rows.join("\n")].join("\n");

        assert_eq!(summary_rows.len(), 1);
        assert!(phase_rows.len() >= 4);
        assert!(joined.contains("local runtime TAB/sprite aggregate probe manifest"));
        assert!(joined.contains("parsed TAB/DAT pairs"));
        assert!(joined.contains("selected families"));
        assert!(joined.contains("tab-sprite-"));
        assert!(joined.contains("metadata-shape grouping"));
        assert!(joined.contains("selector IDs are dry-run handles"));
        assert!(joined.contains("does not expose bytes"));
        assert!(joined.contains("not proof of decoded layout or semantics"));
        assert!(!joined.contains("`SOUND`"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn caps_runtime_probe_selector_catalog_families_archives_and_rows_without_bytes() {
        let mut analyses = (0..6)
            .map(|index| {
                make_tab_report_analysis(
                    &format!("SYNDICAT/DATA/HSPR-{index}.TAB"),
                    vec![
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                        chunk_with_prefix([16, 16, 0xf0, 0], 128),
                    ],
                )
            })
            .collect::<Vec<_>>();
        analyses.push(make_tab_report_analysis(
            "DATA/FONT.TAB",
            vec![chunk_with_prefix([8, 12, 0, 0], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "DATADISK/DATA/MSPR-0-D.TAB",
            vec![chunk_with_prefix([8, 12, 1, 1], 64)],
        ));
        analyses.push(make_tab_report_analysis(
            "SYNDICAT/DATA/SOUND-0.TAB",
            vec![chunk_with_prefix([97, 116, 0, 0], 64)],
        ));

        let rows = super::format_tab_family_runtime_probe_selector_catalog_rows(&analyses);
        let joined = rows.join("\n");

        assert_eq!(rows.len(), 10);
        assert!(joined.contains("capped at 4/6 parsed archives"));
        assert!(joined.contains("tab-sprite-hspr"));
        assert!(!joined.contains("`SOUND`"));
        assert!(joined.contains("aggregate selector"));
        assert!(joined.contains("do not commit generated previews"));
        assert!(!joined.contains("f0 00"));
    }

    #[test]
    fn renders_empty_tab_family_ranking_section_conservatively() {
        let report = AssetReport::generate("definitely-not-a-real-asset-dir");
        let markdown = report.to_markdown();

        assert!(markdown.contains("TAB/sprite family aggregate ranking candidates"));
        assert!(markdown.contains("TAB/sprite family next-investigation hints"));
        assert!(markdown.contains("TAB/sprite top-priority archive evidence rows"));
        assert!(markdown.contains("TAB/sprite investigation dashboard"));
        assert!(markdown.contains("TAB/sprite runtime-probe planning diagnostics"));
        assert!(markdown.contains("TAB/sprite cross-family runtime-probe queue diagnostics"));
        assert!(markdown.contains("TAB/sprite runtime probe selector catalog"));
        assert!(markdown.contains("TAB/sprite local runtime dry-run ordering"));
        assert!(markdown.contains("TAB/sprite runtime probe manifest summary"));
        assert!(markdown.contains("TAB/sprite runtime probe manifest phases"));
        assert!(markdown.contains("TAB/sprite family aggregate comparison candidates"));
        assert!(markdown.contains("no safely parsed TAB/DAT family rankings available"));
        assert!(markdown.contains("no aggregate TAB/sprite investigation hints available"));
        assert!(markdown.contains("no capped per-archive TAB/sprite evidence rows available"));
        assert!(
            markdown.contains("no aggregate TAB/sprite investigation dashboard rows available")
        );
        assert!(markdown.contains("no aggregate TAB/sprite runtime-probe plans available"));
        assert!(markdown.contains("no aggregate TAB/sprite runtime-probe queue entries available"));
        assert!(markdown.contains("no aggregate TAB/sprite runtime probe selectors available"));
        assert!(markdown.contains("no aggregate TAB/sprite runtime dry-run phases available"));
        assert!(markdown.contains("no aggregate TAB/sprite runtime probe manifest available"));
        assert!(
            markdown.contains("no aggregate TAB/sprite runtime probe manifest phases available")
        );
        assert!(markdown.contains("no aggregate TAB/sprite family comparisons available"));
    }

    fn make_tab_report_analysis(path: &str, chunks: Vec<Vec<u8>>) -> TabBankReportAnalysis {
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

        TabBankReportAnalysis {
            path: path.to_string(),
            dat_len: dat.len(),
            best_variant: analysis.best(),
            archive_bank: Some(archive.bank.clone()),
            archive_summary: Some(archive.aggregate_summary()),
        }
    }

    fn chunk_with_prefix(prefix: [u8; 4], len: usize) -> Vec<u8> {
        let mut chunk = vec![1; len];
        chunk[..prefix.len()].copy_from_slice(&prefix);
        chunk
    }
}
