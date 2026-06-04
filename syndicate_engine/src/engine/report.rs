//! Headless asset inspection reports for reverse-engineering original data.

use std::{collections::BTreeMap, fs, path::Path};

use walkdir::WalkDir;

use crate::engine::{
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
                map_rows.push(format!(
                    "| `{}` | {} | {} |",
                    display_relative(root, path),
                    size,
                    compressed_status(path)
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
                        block.diagnostic_summary()
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
                        compressed_palette_rows.push(format!(
                            "| `{}` | {} | {} |",
                            display_relative(root, path),
                            data.len(),
                            block.diagnostic_summary()
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
        .and_then(|data| RncBlock::parse(&data).map(|block| block.diagnostic_summary()))
        .unwrap_or_else(|| "plain/unknown".to_string())
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
