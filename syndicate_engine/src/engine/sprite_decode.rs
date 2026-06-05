//! Conservative sprite-bank chunk analysis.
//!
//! The original Bullfrog asset formats vary between banks, so this module does
//! not claim to fully decode sprites yet. It classifies byte chunks and extracts
//! plausible metadata that can guide reverse engineering and future renderers.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpriteChunkKind {
    Empty,
    LikelyRawIndexed,
    LikelyRleOrCommandStream,
    Unknown,
}

impl SpriteChunkKind {
    pub fn conservative_label(self) -> &'static str {
        match self {
            Self::Empty => "empty chunk candidate",
            Self::LikelyRawIndexed => "likely raw indexed chunk candidate",
            Self::LikelyRleOrCommandStream => "likely RLE/command-stream chunk candidate",
            Self::Unknown => "unknown chunk candidate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteChunkInfo {
    pub len: usize,
    pub kind: SpriteChunkKind,
    pub zeroes: usize,
    pub high_bytes: usize,
    pub first_bytes: [u8; 8],
}

impl SpriteChunkInfo {
    pub fn inspect(chunk: &[u8]) -> Self {
        let mut first_bytes = [0; 8];
        for (dst, src) in first_bytes.iter_mut().zip(chunk.iter().copied()) {
            *dst = src;
        }

        let zeroes = chunk.iter().filter(|&&byte| byte == 0).count();
        let high_bytes = chunk.iter().filter(|&&byte| byte >= 0xf0).count();
        let kind = classify(chunk, zeroes, high_bytes);

        Self {
            len: chunk.len(),
            kind,
            zeroes,
            high_bytes,
            first_bytes,
        }
    }

    pub fn short_label(&self) -> String {
        format!(
            "{:?}, len {}, z {}, hi {}, head {:02x} {:02x} {:02x} {:02x}",
            self.kind,
            self.len,
            self.zeroes,
            self.high_bytes,
            self.first_bytes[0],
            self.first_bytes[1],
            self.first_bytes[2],
            self.first_bytes[3]
        )
    }
}

fn classify(chunk: &[u8], zeroes: usize, high_bytes: usize) -> SpriteChunkKind {
    if chunk.is_empty() {
        return SpriteChunkKind::Empty;
    }

    let len = chunk.len();
    let zero_ratio = zeroes as f32 / len as f32;
    let high_ratio = high_bytes as f32 / len as f32;

    if len >= 64 && zero_ratio < 0.08 && high_ratio < 0.15 {
        SpriteChunkKind::LikelyRawIndexed
    } else if len >= 8 && (high_ratio >= 0.15 || zero_ratio >= 0.2) {
        SpriteChunkKind::LikelyRleOrCommandStream
    } else {
        SpriteChunkKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{SpriteChunkInfo, SpriteChunkKind};

    #[test]
    fn classifies_empty_chunk() {
        let info = SpriteChunkInfo::inspect(&[]);
        assert_eq!(info.kind, SpriteChunkKind::Empty);
        assert_eq!(info.len, 0);
    }

    #[test]
    fn classifies_raw_indexed_like_chunk() {
        let chunk = (1..=80).collect::<Vec<u8>>();
        let info = SpriteChunkInfo::inspect(&chunk);
        assert_eq!(info.kind, SpriteChunkKind::LikelyRawIndexed);
        assert_eq!(info.first_bytes[..4], [1, 2, 3, 4]);
    }

    #[test]
    fn classifies_command_stream_like_chunk() {
        let chunk = [0xff, 0x00, 0xfe, 0x00, 0x10, 0x00, 0xf8, 0x01];
        let info = SpriteChunkInfo::inspect(&chunk);
        assert_eq!(info.kind, SpriteChunkKind::LikelyRleOrCommandStream);
    }
}
