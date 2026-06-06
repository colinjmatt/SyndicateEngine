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
    record_visible: Vec<bool>,
    rgba: Vec<u8>,
}

const HBLK_TILE_COUNT: usize = 256;
const HBLK_SUBTILES_PER_TILE: usize = 6;
const HBLK_OFFSET_TABLE_BYTES: usize = HBLK_TILE_COUNT * HBLK_SUBTILES_PER_TILE * 4;
const HBLK_SUBTILE_WIDTH: usize = 32;
const HBLK_SUBTILE_HEIGHT: usize = 16;
const HBLK_SUBTILE_BYTES_PER_LINE: usize = 20;
const HBLK_SUBTILE_BYTES: usize = HBLK_SUBTILE_HEIGHT * HBLK_SUBTILE_BYTES_PER_LINE;
const HBLK_TILE_WIDTH: usize = 64;
const HBLK_TILE_HEIGHT: usize = 48;

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
        if let Ok((candidate, decoded)) = load_hblk_tile_candidate(root) {
            return Self::from_hblk_map_tiles(
                candidate.relative_path.to_string(),
                palette_label,
                &decoded,
                &palette,
            );
        }

        let (candidate, decoded) = load_fixed_block_candidate(root)?;
        Self::from_fixed_records(
            candidate.relative_path.to_string(),
            palette_label,
            candidate.record_width,
            candidate.record_height,
            candidate.data_offset,
            &decoded,
            &palette,
        )
    }

    pub fn from_fixed_records(
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
        let mut record_visible = vec![false; record_count];
        for record_index in 0..record_count {
            let record_start = record_index * bytes_per_record;
            let record = &payload[record_start..record_start + bytes_per_record];
            record_visible[record_index] = record.iter().any(|&index| index != 0);
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
            record_visible,
            rgba,
        })
    }

    pub fn from_hblk_map_tiles(
        source_label: String,
        palette_label: String,
        decoded: &[u8],
        palette: &Palette,
    ) -> Result<Self, IndexedBlockGraphicsError> {
        if decoded.len() < HBLK_OFFSET_TABLE_BYTES {
            return Err(IndexedBlockGraphicsError::InvalidRecordLayout);
        }

        let record_count = HBLK_TILE_COUNT;
        let atlas_columns = 16;
        let atlas_rows = HBLK_TILE_COUNT / atlas_columns;
        let atlas_width = atlas_columns * HBLK_TILE_WIDTH;
        let atlas_height = atlas_rows * HBLK_TILE_HEIGHT;
        let mut rgba = vec![0u8; atlas_width * atlas_height * 4];
        let mut record_visible = vec![false; record_count];

        for tile_index in 0..record_count {
            let tile_x = (tile_index % atlas_columns) * HBLK_TILE_WIDTH;
            let tile_y = (tile_index / atlas_columns) * HBLK_TILE_HEIGHT;
            record_visible[tile_index] = draw_hblk_tile(
                tile_index,
                decoded,
                palette,
                tile_x,
                tile_y,
                atlas_width,
                &mut rgba,
            );
        }

        Ok(Self {
            source_label,
            palette_label,
            record_width: HBLK_TILE_WIDTH,
            record_height: HBLK_TILE_HEIGHT,
            record_count,
            data_offset: HBLK_OFFSET_TABLE_BYTES,
            atlas_columns,
            atlas_rows,
            atlas_width,
            atlas_height,
            record_visible,
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

    pub fn record_has_visible_pixels(&self, record_index: usize) -> bool {
        self.record_visible
            .get(record_index)
            .copied()
            .unwrap_or(false)
    }

    pub fn status_label(&self) -> String {
        format!(
            "runtime original graphics candidate: `{}` via `{}`; {} {}x{} records; runtime-only, not proof of MAP placement or gameplay semantics",
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

fn draw_hblk_tile(
    tile_index: usize,
    decoded: &[u8],
    palette: &Palette,
    tile_x: usize,
    tile_y: usize,
    atlas_width: usize,
    rgba: &mut [u8],
) -> bool {
    let mut tile_rgba = vec![0u8; HBLK_TILE_WIDTH * HBLK_TILE_HEIGHT * 4];
    for subtile_index in 0..HBLK_SUBTILES_PER_TILE {
        let Some(offset) = hblk_subtile_offset(decoded, tile_index, subtile_index) else {
            continue;
        };
        if offset < HBLK_OFFSET_TABLE_BYTES || offset + HBLK_SUBTILE_BYTES > decoded.len() {
            continue;
        }

        let (subtile_x, subtile_y) = hblk_subtile_position(subtile_index);
        draw_hblk_subtile(
            &decoded[offset..offset + HBLK_SUBTILE_BYTES],
            palette,
            subtile_x,
            subtile_y,
            HBLK_TILE_WIDTH,
            &mut tile_rgba,
        );
    }

    copy_hblk_tile_to_atlas(&tile_rgba, tile_x, tile_y, atlas_width, rgba)
}

fn copy_hblk_tile_to_atlas(
    tile_rgba: &[u8],
    tile_x: usize,
    tile_y: usize,
    atlas_width: usize,
    rgba: &mut [u8],
) -> bool {
    let mut visible = false;
    for atlas_row in 0..HBLK_TILE_HEIGHT {
        let source_row = HBLK_TILE_HEIGHT - 1 - atlas_row;
        for x in 0..HBLK_TILE_WIDTH {
            let source_index = (source_row * HBLK_TILE_WIDTH + x) * 4;
            let output_index = ((tile_y + atlas_row) * atlas_width + tile_x + x) * 4;
            rgba[output_index..output_index + 4]
                .copy_from_slice(&tile_rgba[source_index..source_index + 4]);
            visible |= tile_rgba[source_index + 3] != 0;
        }
    }
    visible
}

fn hblk_subtile_offset(decoded: &[u8], tile_index: usize, subtile_index: usize) -> Option<usize> {
    let table_index = tile_index
        .checked_mul(HBLK_SUBTILES_PER_TILE)?
        .checked_add(subtile_index)?
        .checked_mul(4)?;
    let bytes = decoded.get(table_index..table_index + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?) as usize)
}

fn hblk_subtile_position(subtile_index: usize) -> (usize, usize) {
    let column = subtile_index / 3;
    let row_from_bottom = subtile_index % 3;
    (
        column * HBLK_SUBTILE_WIDTH,
        (2 - row_from_bottom) * HBLK_SUBTILE_HEIGHT,
    )
}

fn draw_hblk_subtile(
    subtile: &[u8],
    palette: &Palette,
    atlas_x: usize,
    atlas_y: usize,
    atlas_width: usize,
    rgba: &mut [u8],
) {
    for source_line in 0..HBLK_SUBTILE_HEIGHT {
        let line_start = source_line * HBLK_SUBTILE_BYTES_PER_LINE;
        let line = &subtile[line_start..line_start + HBLK_SUBTILE_BYTES_PER_LINE];
        let transparency = be_u32_at(line, 0);
        let bit0 = be_u32_at(line, 4);
        let bit1 = be_u32_at(line, 8);
        let bit2 = be_u32_at(line, 12);
        let bit3 = be_u32_at(line, 16);
        let y = atlas_y + (HBLK_SUBTILE_HEIGHT - 1 - source_line);

        for x in 0..HBLK_SUBTILE_WIDTH {
            let mask = 1u32 << (31 - x);
            let transparent = transparency & mask != 0;
            let palette_index = ((bit0 & mask != 0) as u8)
                | (((bit1 & mask != 0) as u8) << 1)
                | (((bit2 & mask != 0) as u8) << 2)
                | (((bit3 & mask != 0) as u8) << 3);
            let color = palette_color(palette, palette_index);
            let output_index = (y * atlas_width + atlas_x + x) * 4;
            rgba[output_index] = color.r;
            rgba[output_index + 1] = color.g;
            rgba[output_index + 2] = color.b;
            rgba[output_index + 3] = if transparent { 0 } else { 255 };
        }
    }
}

fn be_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("u32 slice"))
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

fn load_hblk_tile_candidate(
    root: &Path,
) -> Result<(BlockGraphicsCandidate, Vec<u8>), IndexedBlockGraphicsError> {
    for candidate in HBLK_TILE_CANDIDATES {
        let path = root.join(candidate.relative_path);
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        let decoded = decode_maybe_rnc(&data)
            .map_err(|err| IndexedBlockGraphicsError::Decode(format!("{err:?}")))?;
        if decoded.len() >= HBLK_OFFSET_TABLE_BYTES + HBLK_SUBTILE_BYTES {
            return Ok((*candidate, decoded));
        }
    }
    Err(IndexedBlockGraphicsError::NoBlockCandidate)
}

fn load_fixed_block_candidate(
    root: &Path,
) -> Result<(BlockGraphicsCandidate, Vec<u8>), IndexedBlockGraphicsError> {
    for candidate in FIXED_BLOCK_GRAPHICS_CANDIDATES {
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
    "SYNDICAT/DATA/HPAL02.DAT",
    "DATADISK/DATA/HPAL02.DAT",
    "SYNDICAT/DATA/HPAL01.DAT",
    "DATADISK/DATA/HPAL01.DAT",
    "SYNDICAT/DATA/HPALETTE.DAT",
    "DATADISK/DATA/HPALETTE.DAT",
    "SYNDICAT/DATA/MSELECT.PAL",
    "DATADISK/DATA/MSELECT.PAL",
];

const HBLK_TILE_CANDIDATES: &[BlockGraphicsCandidate] = &[
    BlockGraphicsCandidate {
        relative_path: "SYNDICAT/DATA/HBLK01.DAT",
        record_width: HBLK_TILE_WIDTH,
        record_height: HBLK_TILE_HEIGHT,
        data_offset: 0,
    },
    BlockGraphicsCandidate {
        relative_path: "DATADISK/DATA/HBLK01.DAT",
        record_width: HBLK_TILE_WIDTH,
        record_height: HBLK_TILE_HEIGHT,
        data_offset: 0,
    },
];

const FIXED_BLOCK_GRAPHICS_CANDIDATES: &[BlockGraphicsCandidate] = &[
    BlockGraphicsCandidate {
        relative_path: "SYNDICAT/DATA/MMAPBLK.DAT",
        record_width: 8,
        record_height: 8,
        data_offset: 0,
    },
    BlockGraphicsCandidate {
        relative_path: "DATADISK/DATA/MMAPBLK.DAT",
        record_width: 8,
        record_height: 8,
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
        let atlas = IndexedBlockGraphics::from_fixed_records(
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
        assert!(atlas.status_label().contains("MAP placement"));
        assert!(!atlas.status_label().contains("[0, 1, 2, 3]"));
    }

    #[test]
    fn rejects_invalid_record_layouts() {
        let palette = synthetic_palette();
        assert!(
            IndexedBlockGraphics::from_fixed_records(
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

    #[test]
    fn keeps_exact_aligned_records_without_skipping_leading_data() {
        let palette = synthetic_palette();
        let decoded = vec![1u8; 64 * 3];
        let atlas = IndexedBlockGraphics::from_fixed_records(
            "synthetic/HBLK01.DAT".to_string(),
            "synthetic/HPALETTE.DAT".to_string(),
            8,
            8,
            0,
            &decoded,
            &palette,
        )
        .unwrap();

        assert_eq!(atlas.record_count, 3);
        assert_eq!(atlas.record_width, 8);
        assert_eq!(atlas.record_height, 8);
        assert_eq!(atlas.data_offset, 0);
    }

    #[test]
    fn decodes_synthetic_hblk_map_tile_bitplanes_without_asset_bytes() {
        let palette = synthetic_palette();
        let mut decoded = vec![0u8; 6144 + 320];
        let offset = 6144u32.to_le_bytes();
        decoded[0..4].copy_from_slice(&offset);
        write_visible_hblk_line(&mut decoded[6144..6144 + 20], 0x8000_0000, 0x0000_0008);

        let atlas = IndexedBlockGraphics::from_hblk_map_tiles(
            "synthetic/HBLK01.DAT".to_string(),
            "synthetic/HPALETTE.DAT".to_string(),
            &decoded,
            &palette,
        )
        .unwrap();

        assert_eq!(atlas.record_count, 256);
        assert_eq!(atlas.record_width, 64);
        assert_eq!(atlas.record_height, 48);
        assert_eq!(atlas.data_offset, 6144);

        let top_left_after_tile_flip = 28 * 4;
        let transparent_pixel = 0;
        assert_eq!(atlas.rgba()[top_left_after_tile_flip + 3], 255);
        assert_eq!(atlas.rgba()[transparent_pixel + 3], 0);
        assert!(atlas.record_has_visible_pixels(0));
        assert!(!atlas.record_has_visible_pixels(1));
    }

    fn write_visible_hblk_line(line: &mut [u8], alpha: u32, bit0: u32) {
        line[0..4].copy_from_slice(&alpha.to_be_bytes());
        line[4..8].copy_from_slice(&bit0.to_be_bytes());
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
