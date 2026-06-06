//! Runtime map-tile stack parser for original `MAP*.DAT` files.
//!
//! This module decodes local user-supplied map data into tile-index stacks that
//! can be rendered with the runtime HBLK tile atlas. It must not write decoded
//! map bytes, tile previews, or reconstructable asset-derived output.

use std::{fs, path::Path};

use crate::engine::map_decode::{MapDecodeError, decode_map_payload_bytes};

const HEADER_BYTES: usize = 12;
const MAX_REASONABLE_DIMENSION: usize = 256;
const MAX_REASONABLE_HEIGHT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalMapTiles {
    pub source_label: String,
    pub width: usize,
    pub depth: usize,
    pub height: usize,
    pub unique_stack_offsets: usize,
    pub unique_stacks: usize,
    pub non_empty_columns: usize,
    pub non_zero_tiles: usize,
    pub max_tile_index: u8,
    stacks: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalTileTypes {
    pub source_label: String,
    tile_types: [u8; 256],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalMapTilesError {
    NoMapCandidate,
    NoTileTypeCandidate,
    Decode(String),
    TruncatedHeader,
    InvalidDimensions,
    TruncatedOffsetTable,
    InvalidStackOffset,
    InvalidTileTypeTable,
}

impl OriginalMapTiles {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, OriginalMapTilesError> {
        let root = root.as_ref();
        for relative in MAP_TILE_STACK_CANDIDATES {
            let path = root.join(relative);
            let Ok(data) = fs::read(&path) else {
                continue;
            };
            return Self::from_file_bytes(relative.to_string(), &data);
        }

        Err(OriginalMapTilesError::NoMapCandidate)
    }

    pub fn from_file_bytes(
        source_label: String,
        data: &[u8],
    ) -> Result<Self, OriginalMapTilesError> {
        let (_, decoded) = decode_map_payload_bytes(data).map_err(map_decode_error_label)?;
        Self::from_decoded_bytes(source_label, &decoded)
    }

    pub fn from_decoded_bytes(
        source_label: String,
        decoded: &[u8],
    ) -> Result<Self, OriginalMapTilesError> {
        if decoded.len() < HEADER_BYTES {
            return Err(OriginalMapTilesError::TruncatedHeader);
        }

        let width = read_le_u32(decoded, 0)? as usize;
        let depth = read_le_u32(decoded, 4)? as usize;
        let height = read_le_u32(decoded, 8)? as usize;
        if width == 0
            || depth == 0
            || height == 0
            || width > MAX_REASONABLE_DIMENSION
            || depth > MAX_REASONABLE_DIMENSION
            || height > MAX_REASONABLE_HEIGHT
        {
            return Err(OriginalMapTilesError::InvalidDimensions);
        }

        let column_count = width
            .checked_mul(depth)
            .ok_or(OriginalMapTilesError::InvalidDimensions)?;
        let offset_table_bytes = column_count
            .checked_mul(4)
            .ok_or(OriginalMapTilesError::InvalidDimensions)?;
        let stack_base = HEADER_BYTES
            .checked_add(offset_table_bytes)
            .ok_or(OriginalMapTilesError::InvalidDimensions)?;
        if decoded.len() < stack_base {
            return Err(OriginalMapTilesError::TruncatedOffsetTable);
        }

        let mut stacks = Vec::with_capacity(column_count * height);
        let mut offsets = Vec::with_capacity(column_count);
        for column_index in 0..column_count {
            let table_offset = HEADER_BYTES + column_index * 4;
            let stack_offset = read_le_u32(decoded, table_offset)? as usize;
            let absolute_stack_offset = HEADER_BYTES
                .checked_add(stack_offset)
                .ok_or(OriginalMapTilesError::InvalidStackOffset)?;
            if absolute_stack_offset < stack_base || absolute_stack_offset + height > decoded.len()
            {
                return Err(OriginalMapTilesError::InvalidStackOffset);
            }
            offsets.push(stack_offset);
            stacks
                .extend_from_slice(&decoded[absolute_stack_offset..absolute_stack_offset + height]);
        }

        let unique_stack_offsets = count_unique(offsets.iter().copied());
        let unique_stacks = stacks
            .chunks_exact(height)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let non_empty_columns = stacks
            .chunks_exact(height)
            .filter(|stack| stack.iter().any(|&tile| tile != 0))
            .count();
        let non_zero_tiles = stacks.iter().filter(|&&tile| tile != 0).count();
        let max_tile_index = stacks.iter().copied().max().unwrap_or_default();

        Ok(Self {
            source_label,
            width,
            depth,
            height,
            unique_stack_offsets,
            unique_stacks,
            non_empty_columns,
            non_zero_tiles,
            max_tile_index,
            stacks,
        })
    }

    pub fn stack_at(&self, x: usize, y: usize) -> Option<&[u8]> {
        if x >= self.width || y >= self.depth {
            return None;
        }
        let start = (y * self.width + x) * self.height;
        self.stacks.get(start..start + self.height)
    }

    pub fn tile_at(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        self.stack_at(x, y).and_then(|stack| stack.get(z)).copied()
    }

    pub fn status_label(&self) -> String {
        format!(
            "{}: {}x{}x{} tile stacks, {} unique stacks, max tile {}, runtime-only",
            self.source_label,
            self.width,
            self.depth,
            self.height,
            self.unique_stacks,
            self.max_tile_index
        )
    }
}

impl OriginalTileTypes {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, OriginalMapTilesError> {
        let root = root.as_ref();
        for relative in TILE_TYPE_CANDIDATES {
            let path = root.join(relative);
            let Ok(data) = fs::read(&path) else {
                continue;
            };
            return Self::from_file_bytes(relative.to_string(), &data);
        }

        Err(OriginalMapTilesError::NoTileTypeCandidate)
    }

    pub fn from_file_bytes(
        source_label: String,
        data: &[u8],
    ) -> Result<Self, OriginalMapTilesError> {
        let (_, decoded) = decode_map_payload_bytes(data).map_err(map_decode_error_label)?;
        Self::from_decoded_bytes(source_label, &decoded)
    }

    pub fn from_decoded_bytes(
        source_label: String,
        decoded: &[u8],
    ) -> Result<Self, OriginalMapTilesError> {
        let bytes = decoded
            .get(..256)
            .ok_or(OriginalMapTilesError::InvalidTileTypeTable)?;
        let mut tile_types = [0u8; 256];
        tile_types.copy_from_slice(bytes);
        Ok(Self {
            source_label,
            tile_types,
        })
    }

    pub fn tile_type(&self, tile_index: u8) -> u8 {
        self.tile_types[tile_index as usize]
    }

    pub fn is_renderable_tile(&self, tile_index: u8) -> bool {
        self.tile_type(tile_index) != 0
    }
}

fn read_le_u32(data: &[u8], offset: usize) -> Result<u32, OriginalMapTilesError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(OriginalMapTilesError::TruncatedHeader)?;
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| OriginalMapTilesError::TruncatedHeader)?,
    ))
}

fn count_unique(values: impl Iterator<Item = usize>) -> usize {
    values.collect::<std::collections::BTreeSet<_>>().len()
}

fn map_decode_error_label(err: MapDecodeError) -> OriginalMapTilesError {
    OriginalMapTilesError::Decode(format!("{err:?}"))
}

const MAP_TILE_STACK_CANDIDATES: &[&str] = &["SYNDICAT/DATA/MAP01.DAT", "DATADISK/DATA/MAP01.DAT"];

const TILE_TYPE_CANDIDATES: &[&str] = &["SYNDICAT/DATA/COL01.DAT", "DATADISK/DATA/COL01.DAT"];

#[cfg(test)]
mod tests {
    use super::{HEADER_BYTES, OriginalMapTiles, OriginalMapTilesError, OriginalTileTypes};

    #[test]
    fn parses_synthetic_map_tile_stacks_without_asset_bytes() {
        let decoded = synthetic_map_bytes(2, 2, 3, &[[1, 2, 0], [3, 0, 0], [4, 5, 6], [0, 0, 0]]);
        let map = OriginalMapTiles::from_decoded_bytes("synthetic/MAP01.DAT".to_string(), &decoded)
            .unwrap();

        assert_eq!(map.width, 2);
        assert_eq!(map.depth, 2);
        assert_eq!(map.height, 3);
        assert_eq!(map.tile_at(0, 0, 0), Some(1));
        assert_eq!(map.tile_at(0, 1, 2), Some(6));
        assert_eq!(map.non_empty_columns, 3);
        assert_eq!(map.non_zero_tiles, 6);
        assert_eq!(map.max_tile_index, 6);
        assert!(map.status_label().contains("runtime-only"));
        assert!(!map.status_label().contains("[1, 2, 0]"));
    }

    #[test]
    fn rejects_stack_offsets_that_do_not_point_into_stack_payload() {
        let mut decoded = synthetic_map_bytes(1, 1, 2, &[[1, 2]]);
        decoded[HEADER_BYTES..HEADER_BYTES + 4].copy_from_slice(&0u32.to_le_bytes());

        assert_eq!(
            OriginalMapTiles::from_decoded_bytes("synthetic/MAP01.DAT".to_string(), &decoded),
            Err(OriginalMapTilesError::InvalidStackOffset)
        );
    }

    #[test]
    fn decodes_synthetic_tile_type_visibility_table_without_asset_bytes() {
        let mut decoded = vec![0u8; 256];
        decoded[2] = 5;
        decoded[3] = 0;
        let tile_types =
            OriginalTileTypes::from_decoded_bytes("synthetic/COL01.DAT".to_string(), &decoded)
                .unwrap();

        assert!(tile_types.is_renderable_tile(2));
        assert!(!tile_types.is_renderable_tile(3));
    }

    fn synthetic_map_bytes<const H: usize>(
        width: u32,
        depth: u32,
        height: u32,
        stacks: &[[u8; H]],
    ) -> Vec<u8> {
        assert_eq!(height as usize, H);
        let column_count = (width * depth) as usize;
        assert_eq!(stacks.len(), column_count);

        let mut data = Vec::new();
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(&depth.to_le_bytes());
        data.extend_from_slice(&height.to_le_bytes());
        let offset_table_bytes = column_count * 4;
        let mut stack_payload = Vec::new();
        for stack in stacks {
            let offset_from_byte_12 = (offset_table_bytes + stack_payload.len()) as u32;
            data.extend_from_slice(&offset_from_byte_12.to_le_bytes());
            stack_payload.extend_from_slice(stack);
        }
        data.extend_from_slice(&stack_payload);
        data
    }
}
