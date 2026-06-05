//! Lightweight diagnostics over original asset files.

use std::{fs, path::Path};

use crate::engine::{
    block_decode::BlockGraphicsAnalysis,
    map_decode::{
        MapDatAnalysis, MapInferredLayerPreview, MapPrimarySubstrateCandidate, MapSignaturePreview,
    },
    palette_decode::{Palette, Rgb8},
    rnc::RncBlock,
    sprite_decode::SpriteChunkInfo,
    tab_bank::{TabArchive, TabVariantAnalysis},
};

#[derive(Debug, Clone, Default)]
pub struct DecodeDiagnostics {
    pub map_status: String,
    pub map_preview: Option<MapSignaturePreview>,
    pub map_inferred_preview: Option<MapInferredLayerPreview>,
    pub map_substrate_candidate: Option<MapPrimarySubstrateCandidate>,
    pub palette_status: String,
    pub tab_status: String,
    pub tab_variant_status: String,
    pub sprite_status: String,
    pub block_graphics_status: String,
    pub palette_preview: Vec<Rgb8>,
}

impl DecodeDiagnostics {
    pub fn inspect(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut palette_preview = Vec::new();
        let (map_status, map_preview, map_inferred_preview, map_substrate_candidate) =
            inspect_map(root);
        Self {
            map_status,
            map_preview,
            map_inferred_preview,
            map_substrate_candidate,
            palette_status: inspect_palette(root, &mut palette_preview),
            tab_status: inspect_tab_bank(root),
            tab_variant_status: inspect_tab_variants(root),
            sprite_status: inspect_sprite_chunks(root),
            block_graphics_status: inspect_block_graphics(root),
            palette_preview,
        }
    }
}

fn inspect_block_graphics(root: &Path) -> String {
    let candidates = [
        root.join("SYNDICAT/DATA/MMAPBLK.DAT"),
        root.join("DATADISK/DATA/MMAPBLK.DAT"),
        root.join("SYNDICAT/DATA/HBLK01.DAT"),
        root.join("DATADISK/DATA/HBLK01.DAT"),
    ];

    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("BLK");
            let analysis = BlockGraphicsAnalysis::analyze_file_bytes(&data);
            let best = analysis
                .best_aligned_record_candidate()
                .map(|candidate| candidate.label())
                .unwrap_or_else(|| "no aligned fixed-size candidate".to_string());
            let layout = analysis
                .best_layout_probe()
                .map(|probe| probe.label())
                .unwrap_or_else(|| "no aggregate layout probe".to_string());
            return format!(
                "{name}: {}; decoded {} bytes; {best}; layout {layout}; entropy {:.3} bits/byte",
                analysis.container_label(),
                analysis.decoded_len,
                analysis.byte_summary.entropy_milli_bits as f32 / 1000.0
            );
        }
    }

    "block graphics: no BLK candidates".to_string()
}

fn inspect_map(
    root: &Path,
) -> (
    String,
    Option<MapSignaturePreview>,
    Option<MapInferredLayerPreview>,
    Option<MapPrimarySubstrateCandidate>,
) {
    let candidates = [
        root.join("SYNDICAT/DATA/MAP01.DAT"),
        root.join("DATADISK/DATA/MAP01.DAT"),
    ];

    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("MAP");
            return match MapDatAnalysis::analyze_file_bytes(&data) {
                Ok(analysis) => {
                    let preview = analysis.payload.signature_preview.clone();
                    let inferred = analysis.payload.inferred_layer_preview.clone();
                    let substrate = analysis.payload.substrate_candidate.clone();
                    if let Some(grid) = &analysis.payload.primary_grid {
                        let substrate_summary = substrate
                            .as_ref()
                            .map(|candidate| {
                                candidate
                                    .field_evidence
                                    .iter()
                                    .map(|evidence| {
                                        format!(
                                            "{}:b{}/{}",
                                            evidence.field.provisional_label(),
                                            evidence.lane,
                                            evidence.confidence.label()
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_else(|| "unavailable".to_string());
                        (
                            format!(
                                "{name}: {}x{}x{} cells, {} unique, tail {} records, inferred {}, substrate evidence {}",
                                grid.width,
                                grid.height,
                                grid.bytes_per_cell,
                                grid.unique_cells,
                                analysis.payload.tail.record_count_12,
                                inferred
                                    .as_ref()
                                    .map(|preview| preview.summary_label())
                                    .unwrap_or_else(|| "unavailable".to_string()),
                                substrate_summary
                            ),
                            preview,
                            inferred,
                            substrate,
                        )
                    } else {
                        (
                            format!("{name}: {}", analysis.payload.short_label()),
                            preview,
                            inferred,
                            substrate,
                        )
                    }
                }
                Err(err) => (
                    format!("{name}: map decode error {err:?}"),
                    None,
                    None,
                    None,
                ),
            };
        }
    }

    (
        "map decode: no MAP*.DAT candidates".to_string(),
        None,
        None,
        None,
    )
}

fn inspect_palette(root: &Path, palette_preview: &mut Vec<Rgb8>) -> String {
    let candidates = [
        root.join("SYNDICAT/DATA/COL01.DAT"),
        root.join("DATADISK/DATA/COL01.DAT"),
        root.join("SYNDICAT/DATA/HPALETTE.DAT"),
        root.join("DATADISK/DATA/HPALETTE.DAT"),
    ];
    let mut fallback = None;

    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("palette");
            if let Some(palette) = Palette::decode_vga_6bit(&data) {
                *palette_preview = palette.preview_ramp(32);
                return format!("{name}: {} VGA colours", palette.colors.len());
            }
            if let Some(block) = RncBlock::parse(&data) {
                match block.decompress() {
                    Ok(decoded) => {
                        if let Some(palette) = Palette::decode_vga_6bit(&decoded) {
                            *palette_preview = palette.preview_ramp(32);
                            return format!(
                                "{name}: RNC method {} verified -> {} VGA colours",
                                block.header.method,
                                palette.colors.len()
                            );
                        }
                        fallback = Some(format!(
                            "{name}: RNC method {} verified -> {} unpacked bytes",
                            block.header.method,
                            decoded.len()
                        ));
                    }
                    Err(err) => {
                        fallback = Some(format!(
                            "{name}: {}, decompress error {:?}",
                            block.diagnostic_summary(),
                            err
                        ));
                    }
                }
                continue;
            }
            fallback = Some(format!(
                "{}: unsupported palette size {}",
                path.display(),
                data.len()
            ));
        }
    }

    fallback.unwrap_or_else(|| "palette: not found".to_string())
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

fn inspect_tab_variants(root: &Path) -> String {
    let candidates = [
        root.join("SYNDICAT/DATA/HSPR-0.TAB"),
        root.join("SYNDICAT/DATA/HSPR-1.TAB"),
        root.join("DATADISK/DATA/MSPR-0-D.TAB"),
    ];

    for tab_path in candidates {
        let dat_path = tab_path.with_extension("DAT");
        if let (Ok(tab), Ok(dat_meta)) = (fs::read(&tab_path), fs::metadata(&dat_path)) {
            let analysis = TabVariantAnalysis::analyze(&tab, dat_meta.len() as usize);
            let name = tab_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("TAB");
            return format!("{name}: {}", analysis.summary());
        }
    }

    "TAB variants: no paired files".to_string()
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
