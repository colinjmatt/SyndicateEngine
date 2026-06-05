//! Headless asset inspection reports for reverse-engineering original data.

use std::{collections::BTreeMap, fs, path::Path};

use walkdir::WalkDir;

use crate::engine::{
    map_decode::{ByteLaneStats, MapDatAnalysis, MapInferredLayerPreview, MapPrimaryGridAnalysis},
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
    mission_rows: Vec<String>,
    palette_rows: Vec<String>,
    compressed_palette_rows: Vec<String>,
    tab_rows: Vec<String>,
}

impl AssetReport {
    pub fn generate(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut total_files = 0;
        let mut extension_counts = BTreeMap::new();
        let mut compressed_rows = Vec::new();
        let mut map_rows = Vec::new();
        let mut map_diagnostic_rows = Vec::new();
        let mut mission_rows = Vec::new();
        let mut palette_rows = Vec::new();
        let mut compressed_palette_rows = Vec::new();
        let mut tab_rows = Vec::new();

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
                let (container, diagnostics) = map_decode_report_fields(path);
                map_rows.push(format!(
                    "| `{}` | {} | {} |",
                    display_relative(root, path),
                    size,
                    container
                ));
                map_diagnostic_rows.push(format!(
                    "| `{}` | {} | {} | {} |",
                    display_relative(root, path),
                    diagnostics.word_summary,
                    diagnostics.byte_summary,
                    diagnostics.inferred_summary
                ));
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

        Self {
            root: root.display().to_string(),
            total_files,
            extension_counts,
            compressed_rows,
            map_rows,
            map_diagnostic_rows,
            mission_rows,
            palette_rows,
            compressed_palette_rows,
            tab_rows,
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
        markdown.push_str("These rows summarize aggregate diagnostics over the 64x64x12 primary section only. Word and byte-lane names are provisional candidates, not final terrain semantics.\n\n");
        markdown.push_str("| File | Three u32 word ranges | Candidate byte lanes | Inferred layer preview |\n|---|---|---|---|\n");
        append_rows_or_empty(
            &mut markdown,
            &self.map_diagnostic_rows,
            "no MAP*.DAT diagnostics available",
            4,
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
}

fn map_decode_report_fields(path: &Path) -> (String, MapReportDiagnostics) {
    let fallback = MapReportDiagnostics {
        word_summary: "-".to_string(),
        byte_summary: "-".to_string(),
        inferred_summary: "-".to_string(),
    };

    let Ok(data) = fs::read(path) else {
        return ("unreadable".to_string(), fallback);
    };

    match MapDatAnalysis::analyze_file_bytes(&data) {
        Ok(analysis) => {
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
                },
                None => fallback,
            };
            (analysis.short_label(), diagnostics)
        }
        Err(err) => (format!("map decode error {err:?}"), fallback),
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
    }
}
