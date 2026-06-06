//! Runtime-only original game-sprite and animation assembly support.
//!
//! This module decodes local user-supplied HSPR/ANI assets into memory for the
//! running engine. It must not write asset-derived bytes, pixels, previews, or
//! dimensions into source, docs, reports, or tests.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::engine::{
    mission_source,
    palette_decode::{Palette, Rgb8},
    rnc::{RncBlock, RncError},
};

const SPRITE_TAB_ENTRY_BYTES: usize = 6;
const SPRITE_PIXELS_PER_BLOCK: usize = 8;
const SPRITE_BLOCK_BYTES: usize = 5;
const TRANSPARENT_INDEX: u8 = 255;
const GAME_SPRITE_ATLAS_WIDTH: usize = 1024;
const HELE_RECORD_BYTES: usize = 10;
const HFRA_RECORD_BYTES: usize = 8;
const HSTA_RECORD_BYTES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalObjectSpriteRenderAssets {
    pub sprite_atlas: OriginalGameSpriteAtlas,
    pub animation_bank: OriginalAnimationBank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalGameSpriteAtlas {
    pub source_label: String,
    pub palette_label: String,
    pub sprite_count: usize,
    pub decoded_sprite_count: usize,
    pub atlas_width: usize,
    pub atlas_height: usize,
    rects: Vec<Option<OriginalSpriteAtlasRect>>,
    rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalSpriteAtlasRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalAnimationBank {
    pub source_labels: Vec<String>,
    pub element_records: usize,
    pub frame_records: usize,
    pub animation_records: usize,
    elements: Vec<OriginalFrameElement>,
    frames: Vec<OriginalFrame>,
    animations: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalFrameAssembly {
    pub strategy: OriginalFrameAssemblyStrategy,
    pub elements: Vec<OriginalFrameElementDraw>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginalFrameAssemblyStrategy {
    AnimationInitial,
    AnimationOffset,
    DirectFrame,
    PedDirectional,
    VehicleDirectional,
    WeaponGroundCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalFrameElementDraw {
    pub sprite_id: u16,
    pub offset_x: i16,
    pub offset_y: i16,
    pub flipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalStaticFrameRefs {
    pub base_anim: Option<u16>,
    pub current_anim: Option<u16>,
    pub current_frame: Option<u16>,
    pub subtype: Option<u8>,
    pub orientation: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalObjectFrameRefs {
    pub kind: OriginalRenderObjectKind,
    pub base_anim: Option<u16>,
    pub current_anim: Option<u16>,
    pub current_frame: Option<u16>,
    pub subtype: Option<u8>,
    pub orientation: Option<u8>,
    pub state: Option<u8>,
    pub animation_frame: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OriginalRenderObjectKind {
    Static,
    Ped,
    Weapon,
    Vehicle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalStaticFrameSupport {
    pub assembled: bool,
    pub sprites_supported: bool,
    pub element_count: usize,
    pub strategy: Option<OriginalFrameAssemblyStrategy>,
}

pub type OriginalObjectFrameSupport = OriginalStaticFrameSupport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalSpriteRenderError {
    NoPaletteCandidate,
    NoSpriteCandidate,
    NoAnimationCandidate,
    Decode(String),
    InvalidSpriteTab,
    InvalidSpriteBounds { sprite_index: usize },
    InvalidSpriteAtlas,
    InvalidAnimationBank,
    InvalidAnimationLink,
    UnsupportedStaticFrame,
    UnsupportedSpriteRef { sprite_id: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalSpriteTabEntry {
    offset: usize,
    width: usize,
    height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalFrameElement {
    sprite_id: u16,
    offset_x: i16,
    offset_y: i16,
    flipped: bool,
    next_element: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OriginalFrame {
    first_element: u16,
    flags: u16,
    next_frame: u16,
}

impl OriginalObjectSpriteRenderAssets {
    pub fn from_root_with_palette_id(
        root: impl AsRef<Path>,
        palette_id: Option<u8>,
    ) -> Result<Self, OriginalSpriteRenderError> {
        let root = root.as_ref();
        let (palette_label, palette) = load_palette(root, palette_id)?;
        let sprite_atlas = OriginalGameSpriteAtlas::from_root(root, palette_label, &palette)?;
        let animation_bank = OriginalAnimationBank::from_root(root)?;
        Ok(Self {
            sprite_atlas,
            animation_bank,
        })
    }

    pub fn static_frame_support(
        &self,
        refs: OriginalStaticFrameRefs,
    ) -> OriginalStaticFrameSupport {
        self.object_frame_support(OriginalObjectFrameRefs::from(refs))
    }

    pub fn object_frame_support(
        &self,
        refs: OriginalObjectFrameRefs,
    ) -> OriginalObjectFrameSupport {
        match self.assemble_object_frame(refs) {
            Ok(assembly) => {
                let sprites_supported = assembly.elements.iter().all(|element| {
                    self.sprite_atlas
                        .source_rect(element.sprite_id as usize)
                        .is_some()
                });
                OriginalStaticFrameSupport {
                    assembled: true,
                    sprites_supported,
                    element_count: assembly.elements.len(),
                    strategy: Some(assembly.strategy),
                }
            }
            Err(_) => OriginalStaticFrameSupport {
                assembled: false,
                sprites_supported: false,
                element_count: 0,
                strategy: None,
            },
        }
    }

    pub fn assemble_static_frame(
        &self,
        refs: OriginalStaticFrameRefs,
    ) -> Result<OriginalFrameAssembly, OriginalSpriteRenderError> {
        self.assemble_object_frame(OriginalObjectFrameRefs::from(refs))
    }

    pub fn assemble_object_frame(
        &self,
        refs: OriginalObjectFrameRefs,
    ) -> Result<OriginalFrameAssembly, OriginalSpriteRenderError> {
        for candidate in object_frame_candidates(refs) {
            let assembly = match candidate.strategy {
                OriginalFrameAssemblyStrategy::AnimationInitial => self
                    .animation_bank
                    .assemble_animation_frame(candidate.animation_id, 0, candidate.strategy),
                OriginalFrameAssemblyStrategy::AnimationOffset => {
                    self.animation_bank.assemble_animation_frame(
                        candidate.animation_id,
                        candidate.frame_offset.unwrap_or_default(),
                        candidate.strategy,
                    )
                }
                OriginalFrameAssemblyStrategy::DirectFrame => self
                    .animation_bank
                    .assemble_direct_frame(candidate.animation_id, candidate.strategy),
                OriginalFrameAssemblyStrategy::PedDirectional
                | OriginalFrameAssemblyStrategy::VehicleDirectional
                | OriginalFrameAssemblyStrategy::WeaponGroundCandidate => {
                    let frame_id = animation_frame_for_candidate(
                        &self.animation_bank,
                        candidate.animation_id,
                        candidate.frame_offset.unwrap_or_default(),
                    );
                    self.animation_bank.assemble_animation_frame(
                        candidate.animation_id,
                        frame_id,
                        candidate.strategy,
                    )
                }
            };
            let Ok(assembly) = assembly else {
                continue;
            };
            if assembly.elements.iter().all(|element| {
                self.sprite_atlas
                    .source_rect(element.sprite_id as usize)
                    .is_some()
            }) {
                return Ok(assembly);
            }
        }

        Err(OriginalSpriteRenderError::UnsupportedStaticFrame)
    }

    pub fn render_support_label(&self) -> String {
        format!(
            "runtime HSPR/ANI support sprites {}/{}; frames {}; anims {}; no previews",
            self.sprite_atlas.decoded_sprite_count,
            self.sprite_atlas.sprite_count,
            self.animation_bank.frame_records,
            self.animation_bank.animation_records
        )
    }
}

fn animation_frame_for_candidate(
    animation_bank: &OriginalAnimationBank,
    animation_id: u16,
    requested_frame: u16,
) -> u16 {
    animation_bank
        .animation_frame_count(animation_id)
        .filter(|count| *count > 0)
        .map(|count| requested_frame % count)
        .unwrap_or(requested_frame)
}

impl From<OriginalStaticFrameRefs> for OriginalObjectFrameRefs {
    fn from(value: OriginalStaticFrameRefs) -> Self {
        Self {
            kind: OriginalRenderObjectKind::Static,
            base_anim: value.base_anim,
            current_anim: value.current_anim,
            current_frame: value.current_frame,
            subtype: value.subtype,
            orientation: value.orientation,
            state: None,
            animation_frame: 0,
        }
    }
}

impl OriginalGameSpriteAtlas {
    pub fn from_root(
        root: &Path,
        palette_label: String,
        palette: &Palette,
    ) -> Result<Self, OriginalSpriteRenderError> {
        for prefix in ["SYNDICAT/DATA", "DATADISK/DATA"] {
            let tab_label = format!("{prefix}/HSPR-0.TAB");
            let dat_label = format!("{prefix}/HSPR-0.DAT");
            let tab_path = root.join(&tab_label);
            let dat_path = root.join(&dat_label);
            let (Some(tab), Some(dat)) = (
                read_original_asset_bytes(&tab_path),
                read_original_asset_bytes(&dat_path),
            ) else {
                continue;
            };
            return Self::from_bytes(
                format!("{tab_label} + {dat_label}"),
                palette_label,
                &tab,
                &dat,
                palette,
            );
        }

        Err(OriginalSpriteRenderError::NoSpriteCandidate)
    }

    pub fn from_bytes(
        source_label: String,
        palette_label: String,
        tab: &[u8],
        dat: &[u8],
        palette: &Palette,
    ) -> Result<Self, OriginalSpriteRenderError> {
        let entries = parse_sprite_tab(tab)?;
        let mut decoded = Vec::new();
        for (sprite_index, entry) in entries.iter().enumerate() {
            if entry.width == 0 || entry.height == 0 {
                decoded.push(None);
                continue;
            }
            let bytes_per_sprite = sprite_bytes_required(*entry)
                .ok_or(OriginalSpriteRenderError::InvalidSpriteBounds { sprite_index })?;
            let end = entry
                .offset
                .checked_add(bytes_per_sprite)
                .ok_or(OriginalSpriteRenderError::InvalidSpriteBounds { sprite_index })?;
            let Some(payload) = dat.get(entry.offset..end) else {
                return Err(OriginalSpriteRenderError::InvalidSpriteBounds { sprite_index });
            };
            let rgba = decode_game_sprite(entry.width, entry.height, payload, palette)?;
            decoded.push(Some(DecodedGameSprite {
                width: entry.width,
                height: entry.height,
                rgba,
            }));
        }

        Self::pack_sprites(source_label, palette_label, decoded)
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn texture_size_u16(&self) -> Option<(u16, u16)> {
        if self.atlas_width > u16::MAX as usize || self.atlas_height > u16::MAX as usize {
            None
        } else {
            Some((self.atlas_width as u16, self.atlas_height as u16))
        }
    }

    pub fn source_rect(&self, sprite_index: usize) -> Option<OriginalSpriteAtlasRect> {
        self.rects.get(sprite_index).copied().flatten()
    }

    fn pack_sprites(
        source_label: String,
        palette_label: String,
        decoded: Vec<Option<DecodedGameSprite>>,
    ) -> Result<Self, OriginalSpriteRenderError> {
        let mut placements = vec![None; decoded.len()];
        let mut cursor_x = 0usize;
        let mut cursor_y = 0usize;
        let mut row_height = 0usize;
        for (sprite_index, sprite) in decoded.iter().enumerate() {
            let Some(sprite) = sprite else {
                continue;
            };
            if sprite.width > GAME_SPRITE_ATLAS_WIDTH {
                return Err(OriginalSpriteRenderError::InvalidSpriteAtlas);
            }
            if cursor_x + sprite.width > GAME_SPRITE_ATLAS_WIDTH {
                cursor_x = 0;
                cursor_y = cursor_y
                    .checked_add(row_height)
                    .ok_or(OriginalSpriteRenderError::InvalidSpriteAtlas)?;
                row_height = 0;
            }
            placements[sprite_index] = Some(OriginalSpriteAtlasRect {
                x: cursor_x,
                y: cursor_y,
                width: sprite.width,
                height: sprite.height,
            });
            cursor_x += sprite.width;
            row_height = row_height.max(sprite.height);
        }

        let atlas_height = cursor_y
            .checked_add(row_height)
            .ok_or(OriginalSpriteRenderError::InvalidSpriteAtlas)?
            .max(1);
        if atlas_height > u16::MAX as usize {
            return Err(OriginalSpriteRenderError::InvalidSpriteAtlas);
        }
        let mut rgba = vec![0u8; GAME_SPRITE_ATLAS_WIDTH * atlas_height * 4];
        for (sprite_index, sprite) in decoded.iter().enumerate() {
            let (Some(sprite), Some(rect)) = (sprite, placements[sprite_index]) else {
                continue;
            };
            for y in 0..sprite.height {
                let source_start = y * sprite.width * 4;
                let source_end = source_start + sprite.width * 4;
                let target_start = ((rect.y + y) * GAME_SPRITE_ATLAS_WIDTH + rect.x) * 4;
                let target_end = target_start + sprite.width * 4;
                rgba[target_start..target_end]
                    .copy_from_slice(&sprite.rgba[source_start..source_end]);
            }
        }

        Ok(Self {
            source_label,
            palette_label,
            sprite_count: decoded.len(),
            decoded_sprite_count: decoded.iter().filter(|sprite| sprite.is_some()).count(),
            atlas_width: GAME_SPRITE_ATLAS_WIDTH,
            atlas_height,
            rects: placements,
            rgba,
        })
    }
}

impl OriginalAnimationBank {
    pub fn from_root(root: &Path) -> Result<Self, OriginalSpriteRenderError> {
        for prefix in ["SYNDICAT/DATA", "DATADISK/DATA"] {
            let labels = [
                format!("{prefix}/HELE-0.ANI"),
                format!("{prefix}/HFRA-0.ANI"),
                format!("{prefix}/HSTA-0.ANI"),
            ];
            let hele = read_original_asset_bytes(&root.join(&labels[0]));
            let hfra = read_original_asset_bytes(&root.join(&labels[1]));
            let hsta = read_original_asset_bytes(&root.join(&labels[2]));
            let (Some(hele), Some(hfra), Some(hsta)) = (hele, hfra, hsta) else {
                continue;
            };
            return Self::from_bytes(labels.to_vec(), &hele, &hfra, &hsta);
        }

        Err(OriginalSpriteRenderError::NoAnimationCandidate)
    }

    pub fn from_bytes(
        source_labels: Vec<String>,
        hele: &[u8],
        hfra: &[u8],
        hsta: &[u8],
    ) -> Result<Self, OriginalSpriteRenderError> {
        if hele.len() % HELE_RECORD_BYTES != 0
            || hfra.len() % HFRA_RECORD_BYTES != 0
            || hsta.len() % HSTA_RECORD_BYTES != 0
        {
            return Err(OriginalSpriteRenderError::InvalidAnimationBank);
        }

        let elements = hele
            .chunks_exact(HELE_RECORD_BYTES)
            .map(|record| {
                let sprite_units = le_u16(record, 0);
                if sprite_units % SPRITE_TAB_ENTRY_BYTES as u16 != 0 {
                    return Err(OriginalSpriteRenderError::InvalidAnimationBank);
                }
                Ok(OriginalFrameElement {
                    sprite_id: sprite_units / SPRITE_TAB_ENTRY_BYTES as u16,
                    offset_x: le_i16(record, 2),
                    offset_y: le_i16(record, 4),
                    flipped: le_u16(record, 6) != 0,
                    next_element: le_u16(record, 8),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let frames = hfra
            .chunks_exact(HFRA_RECORD_BYTES)
            .map(|record| OriginalFrame {
                first_element: le_u16(record, 0),
                flags: le_u16(record, 4),
                next_frame: le_u16(record, 6),
            })
            .collect::<Vec<_>>();
        let animations = hsta
            .chunks_exact(HSTA_RECORD_BYTES)
            .map(|record| le_u16(record, 0))
            .collect::<Vec<_>>();

        if elements.iter().any(|element| {
            element.next_element != 0 && element.next_element as usize >= elements.len()
        }) || frames.iter().any(|frame| {
            frame.first_element as usize >= elements.len()
                || frame.next_frame as usize >= frames.len()
        }) || animations
            .iter()
            .any(|frame| *frame as usize >= frames.len())
        {
            return Err(OriginalSpriteRenderError::InvalidAnimationLink);
        }

        Ok(Self {
            source_labels,
            element_records: elements.len(),
            frame_records: frames.len(),
            animation_records: animations.len(),
            elements,
            frames,
            animations,
        })
    }

    pub fn assemble_animation_frame(
        &self,
        anim_id: u16,
        frame_id: u16,
        strategy: OriginalFrameAssemblyStrategy,
    ) -> Result<OriginalFrameAssembly, OriginalSpriteRenderError> {
        let frame_index = self.animation_frame_index(anim_id, frame_id)?;
        self.assemble_frame_index(frame_index, strategy)
    }

    pub fn assemble_direct_frame(
        &self,
        frame_index: u16,
        strategy: OriginalFrameAssemblyStrategy,
    ) -> Result<OriginalFrameAssembly, OriginalSpriteRenderError> {
        self.assemble_frame_index(frame_index, strategy)
    }

    fn animation_frame_index(
        &self,
        anim_id: u16,
        frame_id: u16,
    ) -> Result<u16, OriginalSpriteRenderError> {
        let mut frame_index = *self
            .animations
            .get(anim_id as usize)
            .ok_or(OriginalSpriteRenderError::UnsupportedStaticFrame)?;
        let mut remaining = frame_id as usize;
        let mut visited = BTreeSet::new();
        while remaining > 0 {
            let frame = self
                .frames
                .get(frame_index as usize)
                .ok_or(OriginalSpriteRenderError::UnsupportedStaticFrame)?;
            if !visited.insert(frame_index) {
                return Err(OriginalSpriteRenderError::InvalidAnimationLink);
            }
            frame_index = frame.next_frame;
            remaining -= 1;
        }
        Ok(frame_index)
    }

    fn assemble_frame_index(
        &self,
        frame_index: u16,
        strategy: OriginalFrameAssemblyStrategy,
    ) -> Result<OriginalFrameAssembly, OriginalSpriteRenderError> {
        let frame = self
            .frames
            .get(frame_index as usize)
            .ok_or(OriginalSpriteRenderError::UnsupportedStaticFrame)?;
        let mut element_index = frame.first_element;
        let mut elements = Vec::new();
        let mut visited = BTreeSet::new();
        loop {
            let element = self
                .elements
                .get(element_index as usize)
                .ok_or(OriginalSpriteRenderError::UnsupportedStaticFrame)?;
            elements.push(OriginalFrameElementDraw {
                sprite_id: element.sprite_id,
                offset_x: element.offset_x,
                offset_y: element.offset_y,
                flipped: element.flipped,
            });
            if element.next_element == 0 {
                break;
            }
            if !visited.insert(element_index) {
                return Err(OriginalSpriteRenderError::InvalidAnimationLink);
            }
            element_index = element.next_element;
        }

        Ok(OriginalFrameAssembly { strategy, elements })
    }

    pub fn frame_index_to_animation_offset(&self, frame_index: u16) -> Option<u16> {
        let mut frame_id = 0u16;
        let mut frame = self.frames.get(frame_index as usize)?;
        let mut visited = BTreeSet::new();
        loop {
            frame = self.frames.get(frame.next_frame as usize)?;
            if frame.flags == 0x0100 {
                break;
            }
            if !visited.insert(frame.next_frame) {
                return None;
            }
        }

        visited.clear();
        loop {
            frame = self.frames.get(frame.next_frame as usize)?;
            if frame.next_frame == frame_index {
                return Some(frame_id);
            }
            if !visited.insert(frame.next_frame) {
                return None;
            }
            frame_id = frame_id.checked_add(1)?;
        }
    }

    pub fn animation_frame_count(&self, anim_id: u16) -> Option<u16> {
        let start = *self.animations.get(anim_id as usize)?;
        let mut frame_index = start;
        let mut count = 1u16;
        let mut visited = BTreeSet::new();
        loop {
            let frame = self.frames.get(frame_index as usize)?;
            if frame.next_frame == start {
                return Some(count);
            }
            if !visited.insert(frame_index) {
                return None;
            }
            frame_index = frame.next_frame;
            count = count.checked_add(1)?;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedGameSprite {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ObjectFrameCandidate {
    animation_id: u16,
    frame_offset: Option<u16>,
    strategy: OriginalFrameAssemblyStrategy,
}

fn object_frame_candidates(refs: OriginalObjectFrameRefs) -> Vec<ObjectFrameCandidate> {
    match refs.kind {
        OriginalRenderObjectKind::Static => static_frame_candidates(refs),
        OriginalRenderObjectKind::Ped => ped_frame_candidates(refs),
        OriginalRenderObjectKind::Weapon => weapon_frame_candidates(refs),
        OriginalRenderObjectKind::Vehicle => vehicle_frame_candidates(refs),
    }
}

fn static_frame_candidates(refs: OriginalObjectFrameRefs) -> Vec<ObjectFrameCandidate> {
    let mut candidates = Vec::new();
    match refs.subtype {
        Some(0x05..=0x08) => {
            candidates.push(ObjectFrameCandidate {
                animation_id: 1040,
                frame_offset: Some(refs.subtype.unwrap_or(0x05).saturating_sub(0x05) as u16),
                strategy: OriginalFrameAssemblyStrategy::AnimationOffset,
            });
        }
        Some(0x0c..=0x0f) => {
            if let Some(base) = refs.base_anim {
                let orientation_adjust = match refs.orientation {
                    Some(0x00 | 0x80 | 0x7e | 0xfe) => 0,
                    Some(_) => 1,
                    None => 0,
                };
                let state_adjust = match refs.subtype {
                    Some(0x0e | 0x0f) => 2,
                    _ => 0,
                };
                candidates.push(ObjectFrameCandidate {
                    animation_id: base
                        .saturating_add(orientation_adjust)
                        .saturating_add(state_adjust),
                    frame_offset: None,
                    strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
                });
            }
        }
        Some(0x12) => {
            if let Some(anim) = refs.current_anim.and_then(|anim| anim.checked_sub(2)) {
                candidates.push(ObjectFrameCandidate {
                    animation_id: anim.saturating_add(2),
                    frame_offset: None,
                    strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
                });
            }
        }
        Some(0x15) => {
            if let Some(anim) = refs.current_anim.and_then(|anim| anim.checked_sub(6)) {
                candidates.push(ObjectFrameCandidate {
                    animation_id: anim.saturating_add(6),
                    frame_offset: None,
                    strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
                });
            }
        }
        Some(0x1f) => {
            if let Some(current_anim) = refs.current_anim {
                candidates.push(ObjectFrameCandidate {
                    animation_id: current_anim,
                    frame_offset: None,
                    strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
                });
            }
        }
        _ => {
            if let Some(current_anim) = refs.current_anim {
                candidates.push(ObjectFrameCandidate {
                    animation_id: current_anim,
                    frame_offset: None,
                    strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
                });
            }
        }
    }

    if let Some(frame) = refs.current_frame {
        candidates.push(ObjectFrameCandidate {
            animation_id: frame,
            frame_offset: None,
            strategy: OriginalFrameAssemblyStrategy::DirectFrame,
        });
    }
    if let Some(base_anim) = refs.base_anim {
        candidates.push(ObjectFrameCandidate {
            animation_id: base_anim,
            frame_offset: None,
            strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
        });
    }

    dedup_frame_candidates(candidates)
}

fn ped_frame_candidates(refs: OriginalObjectFrameRefs) -> Vec<ObjectFrameCandidate> {
    let mut candidates = Vec::new();
    let direction = refs.orientation.map_or(0, |dir| discrete_direction(dir, 8));
    if let Some(base) = refs.base_anim {
        match refs.state {
            Some(0x11) => candidates.push(ObjectFrameCandidate {
                animation_id: base.saturating_add(206),
                frame_offset: Some(0),
                strategy: OriginalFrameAssemblyStrategy::PedDirectional,
            }),
            Some(0x10) => candidates.push(ObjectFrameCandidate {
                animation_id: base.saturating_add(8).saturating_add(direction as u16),
                frame_offset: Some(refs.animation_frame),
                strategy: OriginalFrameAssemblyStrategy::PedDirectional,
            }),
            _ => candidates.push(ObjectFrameCandidate {
                animation_id: base.saturating_add(direction as u16),
                frame_offset: Some(0),
                strategy: OriginalFrameAssemblyStrategy::PedDirectional,
            }),
        }
    }

    add_current_animation_fallbacks(&mut candidates, refs);
    dedup_frame_candidates(candidates)
}

fn weapon_frame_candidates(refs: OriginalObjectFrameRefs) -> Vec<ObjectFrameCandidate> {
    let mut candidates = Vec::new();
    if let Some(current_anim) = refs.current_anim {
        candidates.push(ObjectFrameCandidate {
            animation_id: current_anim,
            frame_offset: Some(refs.animation_frame),
            strategy: OriginalFrameAssemblyStrategy::WeaponGroundCandidate,
        });
    }
    add_current_animation_fallbacks(&mut candidates, refs);
    dedup_frame_candidates(candidates)
}

fn vehicle_frame_candidates(refs: OriginalObjectFrameRefs) -> Vec<ObjectFrameCandidate> {
    let mut candidates = Vec::new();
    let direction_8 = refs.orientation.unwrap_or_default() >> 5;
    let direction_4 = discrete_direction(refs.orientation.unwrap_or_default(), 4);
    if let Some(current_anim) = refs.current_anim {
        let base = current_anim.saturating_sub(direction_8 as u16);
        let anim_id = if refs.subtype == Some(0x04) {
            base.saturating_sub(12)
                .saturating_add((direction_8 >> 1) as u16)
                .saturating_add(12)
                .saturating_add(direction_4 as u16)
        } else {
            base.saturating_add((direction_4 as u16) * 2)
        };
        candidates.push(ObjectFrameCandidate {
            animation_id: anim_id,
            frame_offset: Some(refs.animation_frame),
            strategy: OriginalFrameAssemblyStrategy::VehicleDirectional,
        });
    }
    add_current_animation_fallbacks(&mut candidates, refs);
    dedup_frame_candidates(candidates)
}

fn add_current_animation_fallbacks(
    candidates: &mut Vec<ObjectFrameCandidate>,
    refs: OriginalObjectFrameRefs,
) {
    if let Some(current_anim) = refs.current_anim {
        candidates.push(ObjectFrameCandidate {
            animation_id: current_anim,
            frame_offset: Some(refs.animation_frame),
            strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
        });
    }
    if let Some(frame) = refs.current_frame {
        candidates.push(ObjectFrameCandidate {
            animation_id: frame,
            frame_offset: None,
            strategy: OriginalFrameAssemblyStrategy::DirectFrame,
        });
    }
    if let Some(base_anim) = refs.base_anim {
        candidates.push(ObjectFrameCandidate {
            animation_id: base_anim,
            frame_offset: Some(0),
            strategy: OriginalFrameAssemblyStrategy::AnimationInitial,
        });
    }
}

fn discrete_direction(direction: u8, directions: u8) -> u8 {
    let sector = 256 / directions as u16;
    let half = sector / 2;
    let direction = direction as u16;
    for index in 0..directions {
        let center = index as u16 * sector;
        if index == 0 {
            if direction >= 256 - half || direction < center + half {
                return index;
            }
        } else if direction >= center - half && direction < center + half {
            return index;
        }
    }
    0
}

fn dedup_frame_candidates(candidates: Vec<ObjectFrameCandidate>) -> Vec<ObjectFrameCandidate> {
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            seen.insert((
                candidate.animation_id,
                candidate.frame_offset.unwrap_or_default(),
                candidate.strategy,
            ))
        })
        .collect()
}

fn parse_sprite_tab(tab: &[u8]) -> Result<Vec<OriginalSpriteTabEntry>, OriginalSpriteRenderError> {
    if tab.is_empty() || tab.len() % SPRITE_TAB_ENTRY_BYTES != 0 {
        return Err(OriginalSpriteRenderError::InvalidSpriteTab);
    }
    Ok(tab
        .chunks_exact(SPRITE_TAB_ENTRY_BYTES)
        .map(|entry| OriginalSpriteTabEntry {
            offset: u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize,
            width: entry[4] as usize,
            height: entry[5] as usize,
        })
        .collect())
}

fn sprite_bytes_required(entry: OriginalSpriteTabEntry) -> Option<usize> {
    let stride = entry.width.next_multiple_of(SPRITE_PIXELS_PER_BLOCK);
    let blocks_per_row = stride / SPRITE_PIXELS_PER_BLOCK;
    blocks_per_row
        .checked_mul(SPRITE_BLOCK_BYTES)?
        .checked_mul(entry.height)
}

fn decode_game_sprite(
    width: usize,
    height: usize,
    payload: &[u8],
    palette: &Palette,
) -> Result<Vec<u8>, OriginalSpriteRenderError> {
    let stride = width.next_multiple_of(SPRITE_PIXELS_PER_BLOCK);
    let blocks_per_row = stride / SPRITE_PIXELS_PER_BLOCK;
    let bytes_per_row = blocks_per_row * SPRITE_BLOCK_BYTES;
    if payload.len() < bytes_per_row * height {
        return Err(OriginalSpriteRenderError::InvalidSpriteBounds { sprite_index: 0 });
    }

    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let row = &payload[y * bytes_per_row..(y + 1) * bytes_per_row];
        for block in 0..blocks_per_row {
            let block_bytes = &row[block * SPRITE_BLOCK_BYTES..(block + 1) * SPRITE_BLOCK_BYTES];
            for bit in 0..SPRITE_PIXELS_PER_BLOCK {
                let x = block * SPRITE_PIXELS_PER_BLOCK + bit;
                if x >= width {
                    continue;
                }
                let mask = 1u8 << (7 - bit);
                let palette_index = if block_bytes[0] & mask != 0 {
                    TRANSPARENT_INDEX
                } else {
                    ((block_bytes[1] & mask != 0) as u8)
                        | (((block_bytes[2] & mask != 0) as u8) << 1)
                        | (((block_bytes[3] & mask != 0) as u8) << 2)
                        | (((block_bytes[4] & mask != 0) as u8) << 3)
                };
                let target = (y * width + x) * 4;
                if palette_index == TRANSPARENT_INDEX {
                    rgba[target + 3] = 0;
                } else {
                    let color = palette_color(palette, palette_index);
                    rgba[target] = color.r;
                    rgba[target + 1] = color.g;
                    rgba[target + 2] = color.b;
                    rgba[target + 3] = 255;
                }
            }
        }
    }

    Ok(rgba)
}

fn load_palette(
    root: &Path,
    preferred_palette_id: Option<u8>,
) -> Result<(String, Palette), OriginalSpriteRenderError> {
    for relative in palette_candidates(preferred_palette_id) {
        let path = root.join(&relative);
        let Some(decoded) = read_original_asset_bytes(&path) else {
            continue;
        };
        if let Some(palette) = Palette::decode_vga_6bit(&decoded) {
            return Ok((relative, palette));
        }
    }

    Err(OriginalSpriteRenderError::NoPaletteCandidate)
}

fn palette_candidates(preferred_palette_id: Option<u8>) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(palette_id) = preferred_palette_id {
        candidates.extend(mission_source::palette_candidates(palette_id));
    }
    for fallback in [
        "SYNDICAT/DATA/HPAL02.DAT",
        "DATADISK/DATA/HPAL02.DAT",
        "SYNDICAT/DATA/HPAL01.DAT",
        "DATADISK/DATA/HPAL01.DAT",
        "SYNDICAT/DATA/HPALETTE.DAT",
        "DATADISK/DATA/HPALETTE.DAT",
    ] {
        let fallback = fallback.to_string();
        if !candidates.contains(&fallback) {
            candidates.push(fallback);
        }
    }
    candidates
}

fn read_original_asset_bytes(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(PathBuf::from(path)).ok()?;
    if RncBlock::parse(&data).is_some() {
        decode_maybe_rnc(&data).ok()
    } else {
        Some(data)
    }
}

fn decode_maybe_rnc(data: &[u8]) -> Result<Vec<u8>, RncError> {
    if let Some(block) = RncBlock::parse(data) {
        block.decompress()
    } else {
        Ok(data.to_vec())
    }
}

fn palette_color(palette: &Palette, index: u8) -> Rgb8 {
    palette.colors.get(index as usize).copied().unwrap_or(Rgb8 {
        r: index,
        g: index,
        b: index,
    })
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn le_i16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::{
        HELE_RECORD_BYTES, HFRA_RECORD_BYTES, HSTA_RECORD_BYTES, OriginalAnimationBank,
        OriginalFrameAssemblyStrategy, OriginalGameSpriteAtlas, OriginalObjectFrameRefs,
        OriginalObjectSpriteRenderAssets, OriginalRenderObjectKind, OriginalStaticFrameRefs,
        SPRITE_TAB_ENTRY_BYTES, decode_game_sprite,
    };
    use crate::engine::palette_decode::Palette;

    #[test]
    fn parses_six_byte_sprite_tab_and_decodes_transparency_mask() {
        let palette = synthetic_palette();
        let mut tab = vec![0u8; SPRITE_TAB_ENTRY_BYTES];
        tab[4] = 8;
        tab[5] = 1;
        let dat = [
            0b1000_0000,
            0b0101_0101,
            0b0011_0011,
            0b0000_1111,
            0b1111_0000,
        ];

        let atlas = OriginalGameSpriteAtlas::from_bytes(
            "synthetic/HSPR-0".to_string(),
            "synthetic/HPAL".to_string(),
            &tab,
            &dat,
            &palette,
        )
        .unwrap();

        let rect = atlas.source_rect(0).unwrap();
        assert_eq!(rect.width, 8);
        assert_eq!(rect.height, 1);
        assert_eq!(atlas.rgba()[3], 0);
        assert_eq!(atlas.rgba()[7], 255);
    }

    #[test]
    fn rejects_sprite_payloads_that_exceed_bounds() {
        let palette = synthetic_palette();
        let mut tab = vec![0u8; SPRITE_TAB_ENTRY_BYTES];
        tab[4] = 8;
        tab[5] = 2;
        let dat = [0u8; 5];

        assert!(
            OriginalGameSpriteAtlas::from_bytes(
                "synthetic/HSPR-0".to_string(),
                "synthetic/HPAL".to_string(),
                &tab,
                &dat,
                &palette,
            )
            .is_err()
        );
    }

    #[test]
    fn assembles_animation_frame_elements_with_offsets_and_flips() {
        let mut hele = vec![0u8; HELE_RECORD_BYTES * 2];
        write_element(&mut hele[0..HELE_RECORD_BYTES], 0, -3, 4, true, 1);
        write_element(
            &mut hele[HELE_RECORD_BYTES..HELE_RECORD_BYTES * 2],
            6,
            5,
            -6,
            false,
            0,
        );
        let mut hfra = vec![0u8; HFRA_RECORD_BYTES];
        write_frame(&mut hfra, 0, 0x0100, 0);
        let hsta = [0u8; HSTA_RECORD_BYTES];

        let bank =
            OriginalAnimationBank::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
                .unwrap();
        let assembly = bank
            .assemble_animation_frame(0, 0, OriginalFrameAssemblyStrategy::AnimationInitial)
            .unwrap();

        assert_eq!(assembly.elements.len(), 2);
        assert_eq!(assembly.elements[0].sprite_id, 0);
        assert_eq!(assembly.elements[0].offset_x, -3);
        assert!(assembly.elements[0].flipped);
        assert_eq!(assembly.elements[1].sprite_id, 1);
    }

    #[test]
    fn static_support_uses_animation_then_direct_frame_without_bytes() {
        let palette = synthetic_palette();
        let mut tab = vec![0u8; SPRITE_TAB_ENTRY_BYTES];
        tab[4] = 8;
        tab[5] = 1;
        let dat = [0u8; 5];
        let sprite_atlas = OriginalGameSpriteAtlas::from_bytes(
            "synthetic/HSPR-0".to_string(),
            "synthetic/HPAL".to_string(),
            &tab,
            &dat,
            &palette,
        )
        .unwrap();
        let mut hele = vec![0u8; HELE_RECORD_BYTES];
        write_element(&mut hele, 0, 0, 0, false, 0);
        let mut hfra = vec![0u8; HFRA_RECORD_BYTES];
        write_frame(&mut hfra, 0, 0x0100, 0);
        let hsta = [0u8; HSTA_RECORD_BYTES];
        let animation_bank =
            OriginalAnimationBank::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
                .unwrap();
        let assets = OriginalObjectSpriteRenderAssets {
            sprite_atlas,
            animation_bank,
        };

        let support = assets.static_frame_support(OriginalStaticFrameRefs {
            base_anim: None,
            current_anim: Some(0),
            current_frame: Some(0),
            subtype: Some(0x16),
            orientation: Some(0),
        });

        assert!(support.assembled);
        assert!(support.sprites_supported);
        assert_eq!(
            support.strategy,
            Some(OriginalFrameAssemblyStrategy::AnimationInitial)
        );
        assert!(!assets.render_support_label().contains("00 00"));
    }

    #[test]
    fn object_support_assembles_ped_weapon_and_vehicle_frames_conservatively() {
        let palette = synthetic_palette();
        let mut tab = vec![0u8; SPRITE_TAB_ENTRY_BYTES];
        tab[4] = 8;
        tab[5] = 1;
        let dat = [0u8; 5];
        let sprite_atlas = OriginalGameSpriteAtlas::from_bytes(
            "synthetic/HSPR-0".to_string(),
            "synthetic/HPAL".to_string(),
            &tab,
            &dat,
            &palette,
        )
        .unwrap();
        let mut hele = vec![0u8; HELE_RECORD_BYTES];
        write_element(&mut hele, 0, 0, 0, false, 0);
        let mut hfra = vec![0u8; HFRA_RECORD_BYTES];
        write_frame(&mut hfra, 0, 0x0100, 0);
        let mut hsta = Vec::new();
        for _ in 0..16 {
            hsta.extend_from_slice(&0u16.to_le_bytes());
        }
        let animation_bank =
            OriginalAnimationBank::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
                .unwrap();
        let assets = OriginalObjectSpriteRenderAssets {
            sprite_atlas,
            animation_bank,
        };

        let ped = assets.object_frame_support(OriginalObjectFrameRefs {
            kind: OriginalRenderObjectKind::Ped,
            base_anim: Some(0),
            current_anim: Some(0),
            current_frame: Some(0),
            subtype: Some(0x02),
            orientation: Some(0),
            state: Some(0x10),
            animation_frame: 9,
        });
        let weapon = assets.object_frame_support(OriginalObjectFrameRefs {
            kind: OriginalRenderObjectKind::Weapon,
            base_anim: None,
            current_anim: Some(0),
            current_frame: Some(0),
            subtype: None,
            orientation: None,
            state: None,
            animation_frame: 3,
        });
        let vehicle = assets.object_frame_support(OriginalObjectFrameRefs {
            kind: OriginalRenderObjectKind::Vehicle,
            base_anim: None,
            current_anim: Some(0),
            current_frame: Some(0),
            subtype: Some(0),
            orientation: Some(0),
            state: None,
            animation_frame: 4,
        });

        assert_eq!(
            ped.strategy,
            Some(OriginalFrameAssemblyStrategy::PedDirectional)
        );
        assert_eq!(
            weapon.strategy,
            Some(OriginalFrameAssemblyStrategy::WeaponGroundCandidate)
        );
        assert_eq!(
            vehicle.strategy,
            Some(OriginalFrameAssemblyStrategy::VehicleDirectional)
        );
        assert!(ped.sprites_supported);
        assert!(weapon.sprites_supported);
        assert!(vehicle.sprites_supported);
    }

    #[test]
    fn animation_frame_selector_wraps_timed_object_frames() {
        let mut hele = vec![0u8; HELE_RECORD_BYTES];
        write_element(&mut hele, 0, 0, 0, false, 0);
        let mut hfra = vec![0u8; HFRA_RECORD_BYTES * 3];
        write_frame(&mut hfra[0..HFRA_RECORD_BYTES], 0, 0x0100, 1);
        write_frame(
            &mut hfra[HFRA_RECORD_BYTES..HFRA_RECORD_BYTES * 2],
            0,
            0x0100,
            2,
        );
        write_frame(
            &mut hfra[HFRA_RECORD_BYTES * 2..HFRA_RECORD_BYTES * 3],
            0,
            0x0100,
            0,
        );
        let hsta = 0u16.to_le_bytes();
        let bank =
            OriginalAnimationBank::from_bytes(vec!["synthetic".to_string()], &hele, &hfra, &hsta)
                .unwrap();

        assert_eq!(bank.animation_frame_count(0), Some(3));
        assert_eq!(super::animation_frame_for_candidate(&bank, 0, 0), 0);
        assert_eq!(super::animation_frame_for_candidate(&bank, 0, 4), 1);
        assert_eq!(super::animation_frame_for_candidate(&bank, 0, 8), 2);
    }

    #[test]
    fn sprite_decoder_exposes_no_reconstructable_text() {
        let palette = synthetic_palette();
        let decoded = decode_game_sprite(8, 1, &[0, 0, 0, 0, 0], &palette).unwrap();
        assert_eq!(decoded.len(), 32);
        assert!(!format!("{decoded:?}").contains("HSPR"));
    }

    fn synthetic_palette() -> Palette {
        let mut data = vec![0u8; 768];
        for i in 0..256 {
            data[i * 3] = (i % 64) as u8;
            data[i * 3 + 1] = ((i * 3) % 64) as u8;
            data[i * 3 + 2] = ((i * 5) % 64) as u8;
        }
        Palette::decode_vga_6bit(&data).unwrap()
    }

    fn write_element(record: &mut [u8], sprite_units: u16, x: i16, y: i16, flip: bool, next: u16) {
        record[0..2].copy_from_slice(&sprite_units.to_le_bytes());
        record[2..4].copy_from_slice(&x.to_le_bytes());
        record[4..6].copy_from_slice(&y.to_le_bytes());
        record[6..8].copy_from_slice(&(flip as u16).to_le_bytes());
        record[8..10].copy_from_slice(&next.to_le_bytes());
    }

    fn write_frame(record: &mut [u8], first_element: u16, flags: u16, next_frame: u16) {
        record[0..2].copy_from_slice(&first_element.to_le_bytes());
        record[4..6].copy_from_slice(&flags.to_le_bytes());
        record[6..8].copy_from_slice(&next_frame.to_le_bytes());
    }
}
