//! Runtime-only indexed block graphics atlas construction.
//!
//! This module decodes local user-supplied graphics containers into in-memory
//! RGBA pixels that the engine can upload to textures at runtime. It must never
//! write asset-derived pixels, bytes, or previews into source, docs, or tests.

use std::{fs, path::Path};

use crate::engine::{
    palette_decode::{Palette, Rgb8},
    rnc::{RncBlock, RncError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedBlockGraphics {
    pub source_label: String,
    pub palette_label: String,
    pub record_width: usize,
    pub record_height: usize,
    pub record_count: usize,
    pub data_offset: usize,
    pub atlas_columns: usize,
    pub atlas_rows: usize,
    pub atlas_width: usize,
    pub atlas_height: usize,
    rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexedBlockGraphicsError {
    NoPaletteCandidate,
    NoBlockCandidate,
    Decode(String),
    InvalidRecordLayout,
    TextureTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockGraphicsCandidate {
    relative_path: &'static str,
    record_width: usize,
    record_height: usize,
    data_offset: usize,
}

impl IndexedBlockGraphics {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, IndexedBlockGraphicsError> {
        let root = root.as_ref();
        let (palette_label, palette) = load_palette(root)?;
        let (candidate, decoded) = load_block_candidate(root)?;
        Self::from_decoded_parts(
            candidate.relative_path.to_string(),
            palette_label,
            candidate.record_width,
            candidate.record_height,
            candidate.data_offset,
            &decoded,
            &palette,
        )
    }

    pub fn from_decoded_parts(
        source_label: String,
        palette_label: String,
        record_width: usize,
        record_height: usize,
        data_offset: usize,
        decoded: &[u8],
        palette: &Palette,
    ) -> Result<Self, IndexedBlockGraphicsError> {
        let bytes_per_record = record_width
            .checked_mul(record_height)
            .ok_or(IndexedBlockGraphicsError::InvalidRecordLayout)?;
        if bytes_per_record == 0 || decoded.len() <= data_offset {
            return Err(IndexedBlockGraphicsError::InvalidRecordLayout);
        }

        let payload = &decoded[data_offset..];
        let record_count = payload.len() / bytes_per_record;
        if record_count == 0 {
            return Err(IndexedBlockGraphicsError::InvalidRecordLayout);
        }

        let atlas_columns = choose_atlas_columns(record_count);
        let atlas_rows = record_count.div_ceil(atlas_columns);
        let atlas_width = atlas_columns * record_width;
        let atlas_height = atlas_rows * record_height;
        if atlas_width > u16::MAX as usize || atlas_height > u16::MAX as usize {
            return Err(IndexedBlockGraphicsError::TextureTooLarge);
        }

        let mut rgba = vec![0u8; atlas_width * atlas_height * 4];
        for record_index in 0..record_count {
            let record_start = record_index * bytes_per_record;
            let record = &payload[record_start..record_start + bytes_per_record];
            let atlas_x = (record_index % atlas_columns) * record_width;
            let atlas_y = (record_index / atlas_columns) * record_height;
            copy_record_to_atlas(
                record,
                record_width,
                record_height,
                atlas_x,
                atlas_y,
                atlas_width,
                palette,
                &mut rgba,
            );
        }

        Ok(Self {
            source_label,
            palette_label,
            record_width,
            record_height,
            record_count,
            data_offset,
            atlas_columns,
            atlas_rows,
            atlas_width,
            atlas_height,
            rgba,
        })
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn texture_size_u16(&self) -> (u16, u16) {
        (self.atlas_width as u16, self.atlas_height as u16)
    }

    pub fn source_rect(&self, record_index: usize) -> Option<(f32, f32, f32, f32)> {
        if record_index >= self.record_count {
            return None;
        }
        let x = (record_index % self.atlas_columns) * self.record_width;
        let y = (record_index / self.atlas_columns) * self.record_height;
        Some((
            x as f32,
            y as f32,
            self.record_width as f32,
            self.record_height as f32,
        ))
    }

    pub fn status_label(&self) -> String {
        format!(
            "runtime original graphics candidate: `{}` via `{}`; {} {}x{} records; runtime-only, not proof of tile semantics",
            self.source_label,
            self.palette_label,
            self.record_count,
            self.record_width,
            self.record_height
        )
    }
}

fn copy_record_to_atlas(
    record: &[u8],
    record_width: usize,
    record_height: usize,
    atlas_x: usize,
    atlas_y: usize,
    atlas_width: usize,
    palette: &Palette,
    rgba: &mut [u8],
) {
    for y in 0..record_height {
        for x in 0..record_width {
            let index = record[y * record_width + x];
            let color = palette_color(palette, index);
            let output_index = ((atlas_y + y) * atlas_width + atlas_x + x) * 4;
            rgba[output_index] = color.r;
            rgba[output_index + 1] = color.g;
            rgba[output_index + 2] = color.b;
            rgba[output_index + 3] = if index == 0 { 0 } else { 255 };
        }
    }
}

fn palette_color(palette: &Palette, index: u8) -> Rgb8 {
    palette.colors.get(index as usize).copied().unwrap_or(Rgb8 {
        r: index,
        g: index,
        b: index,
    })
}

fn choose_atlas_columns(record_count: usize) -> usize {
    if record_count <= 16 {
        record_count.max(1)
    } else {
        32
    }
}

fn load_palette(root: &Path) -> Result<(String, Palette), IndexedBlockGraphicsError> {
    for relative in PALETTE_CANDIDATES {
        let path = root.join(relative);
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        let decoded = decode_maybe_rnc(&data)
            .map_err(|err| IndexedBlockGraphicsError::Decode(format!("{err:?}")))?;
        if let Some(palette) = Palette::decode_vga_6bit(&decoded) {
            return Ok((relative.to_string(), palette));
        }
    }
    Err(IndexedBlockGraphicsError::NoPaletteCandidate)
}

fn load_block_candidate(
    root: &Path,
) -> Result<(BlockGraphicsCandidate, Vec<u8>), IndexedBlockGraphicsError> {
    for candidate in BLOCK_GRAPHICS_CANDIDATES {
        let path = root.join(candidate.relative_path);
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        let decoded = decode_maybe_rnc(&data)
            .map_err(|err| IndexedBlockGraphicsError::Decode(format!("{err:?}")))?;
        if decoded.len() > candidate.data_offset
            && (decoded.len() - candidate.data_offset)
                >= candidate
                    .record_width
                    .saturating_mul(candidate.record_height)
        {
            return Ok((*candidate, decoded));
        }
    }
    Err(IndexedBlockGraphicsError::NoBlockCandidate)
}

fn decode_maybe_rnc(data: &[u8]) -> Result<Vec<u8>, RncError> {
    if let Some(block) = RncBlock::parse(data) {
        block.decompress()
    } else {
        Ok(data.to_vec())
    }
}

const PALETTE_CANDIDATES: &[&str] = &[
    "SYNDICAT/DATA/HPALETTE.DAT",
    "DATADISK/DATA/HPALETTE.DAT",
    "SYNDICAT/DATA/HPAL01.DAT",
    "DATADISK/DATA/HPAL01.DAT",
    "SYNDICAT/DATA/MSELECT.PAL",
    "DATADISK/DATA/MSELECT.PAL",
];

const BLOCK_GRAPHICS_CANDIDATES: &[BlockGraphicsCandidate] = &[
    BlockGraphicsCandidate {
        relative_path: "SYNDICAT/DATA/HBLK01.DAT",
        record_width: 16,
        record_height: 16,
        data_offset: 128,
    },
    BlockGraphicsCandidate {
        relative_path: "DATADISK/DATA/HBLK01.DAT",
        record_width: 16,
        record_height: 16,
        data_offset: 128,
    },
    BlockGraphicsCandidate {
        relative_path: "SYNDICAT/DATA/MMAPBLK.DAT",
        record_width: 16,
        record_height: 16,
        data_offset: 0,
    },
    BlockGraphicsCandidate {
        relative_path: "DATADISK/DATA/MMAPBLK.DAT",
        record_width: 16,
        record_height: 16,
        data_offset: 0,
    },
];

#[cfg(test)]
mod tests {
    use super::IndexedBlockGraphics;
    use crate::engine::palette_decode::Palette;

    #[test]
    fn builds_runtime_atlas_from_indexed_records_without_exposing_bytes() {
        let palette = synthetic_palette();
        let decoded = vec![0, 1, 2, 3, 3, 2, 1, 0];
        let atlas = IndexedBlockGraphics::from_decoded_parts(
            "synthetic/BLK.DAT".to_string(),
            "synthetic/PAL.DAT".to_string(),
            2,
            2,
            0,
            &decoded,
            &palette,
        )
        .unwrap();

        assert_eq!(atlas.record_count, 2);
        assert_eq!(atlas.atlas_width, 4);
        assert_eq!(atlas.atlas_height, 2);
        assert_eq!(atlas.source_rect(1), Some((2.0, 0.0, 2.0, 2.0)));
        assert_eq!(atlas.rgba()[3], 0);
        assert_eq!(atlas.rgba()[7], 255);
        assert!(
            atlas
                .status_label()
                .contains("runtime original graphics candidate")
        );
        assert!(atlas.status_label().contains("runtime-only"));
        assert!(atlas.status_label().contains("not proof"));
        assert!(!atlas.status_label().contains("[0, 1, 2, 3]"));
    }

    #[test]
    fn rejects_invalid_record_layouts() {
        let palette = synthetic_palette();
        assert!(
            IndexedBlockGraphics::from_decoded_parts(
                "synthetic/BLK.DAT".to_string(),
                "synthetic/PAL.DAT".to_string(),
                0,
                2,
                0,
                &[1, 2, 3],
                &palette,
            )
            .is_err()
        );
    }

    fn synthetic_palette() -> Palette {
        let mut data = vec![0u8; 768];
        for index in 0..256 {
            data[index * 3] = (index % 64) as u8;
            data[index * 3 + 1] = ((index * 2) % 64) as u8;
            data[index * 3 + 2] = ((index * 3) % 64) as u8;
        }
        Palette::decode_vga_6bit(&data).unwrap()
    }
}
