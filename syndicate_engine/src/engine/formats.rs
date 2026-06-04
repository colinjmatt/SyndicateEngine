//! Lightweight diagnostics over original asset files.

use std::{fs, path::Path};

use crate::engine::{
    palette_decode::{Palette, Rgb8},
    sprite_decode::SpriteChunkInfo,
    tab_bank::TabArchive,
};

#[derive(Debug, Clone, Default)]
pub struct DecodeDiagnostics {
    pub palette_status: String,
    pub tab_status: String,
    pub sprite_status: String,
    pub palette_preview: Vec<Rgb8>,
}

impl DecodeDiagnostics {
    pub fn inspect(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut palette_preview = Vec::new();
        Self {
            palette_status: inspect_palette(root, &mut palette_preview),
            tab_status: inspect_tab_bank(root),
            sprite_status: inspect_sprite_chunks(root),
            palette_preview,
        }
    }
}

fn inspect_palette(root: &Path, palette_preview: &mut Vec<Rgb8>) -> String {
    let candidates = [
        root.join("SYNDICAT/DATA/COL01.DAT"),
        root.join("DATADISK/DATA/COL01.DAT"),
        root.join("SYNDICAT/DATA/HPALETTE.DAT"),
        root.join("DATADISK/DATA/HPALETTE.DAT"),
    ];

    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            if let Some(palette) = Palette::decode_vga_6bit(&data) {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("palette");
                *palette_preview = palette.preview_ramp(32);
                return format!("{name}: {} VGA colours", palette.colors.len());
            }
            return format!(
                "{}: unsupported palette size {}",
                path.display(),
                data.len()
            );
        }
    }

    "palette: not found".to_string()
}

fn inspect_tab_bank(root: &Path) -> String {
    let pairs = [
        (
            root.join("SYNDICAT/DATA/HSPR-0.TAB"),
            root.join("SYNDICAT/DATA/HSPR-0.DAT"),
        ),
        (
            root.join("SYNDICAT/DATA/HSPR-1.TAB"),
            root.join("SYNDICAT/DATA/HSPR-1.DAT"),
        ),
    ];

    for (tab_path, dat_path) in pairs {
        if let (Ok(tab), Ok(dat)) = (fs::read(&tab_path), fs::read(&dat_path)) {
            if let Some(archive) = TabArchive::parse(&tab, dat) {
                let name = tab_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("TAB");
                return format!(
                    "{name}: {} chunks, {}-{} bytes, first {} bytes",
                    archive.bank.entry_count(),
                    archive.bank.min_chunk_len().unwrap_or(0),
                    archive.bank.max_chunk_len().unwrap_or(0),
                    archive.chunk(0).map(|chunk| chunk.len()).unwrap_or(0)
                );
            }
            return format!("{}: unsupported TAB/DAT pair", tab_path.display());
        }
    }

    "TAB bank: not found".to_string()
}

fn inspect_sprite_chunks(root: &Path) -> String {
    let pairs = [
        (
            root.join("SYNDICAT/DATA/HSPR-1.TAB"),
            root.join("SYNDICAT/DATA/HSPR-1.DAT"),
        ),
        (
            root.join("DATADISK/DATA/MSPR-0-D.TAB"),
            root.join("DATADISK/DATA/MSPR-0-D.DAT"),
        ),
    ];

    for (tab_path, dat_path) in pairs {
        if let (Ok(tab), Ok(dat)) = (fs::read(&tab_path), fs::read(&dat_path)) {
            if let Some(archive) = TabArchive::parse(&tab, dat) {
                if let Some(chunk) = archive.chunk(0) {
                    let info = SpriteChunkInfo::inspect(chunk);
                    let name = tab_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("sprite bank");
                    return format!("{name}: first chunk {}", info.short_label());
                }
            }
        }
    }

    "sprite chunks: awaiting compatible bank".to_string()
}
