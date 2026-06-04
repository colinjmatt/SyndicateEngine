//! RNC/ProPack container detection.
//!
//! Many original Bullfrog data files are RNC-compressed. This module only
//! parses the header for now; decompression is a later milestone.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RncHeader {
    pub method: u8,
    pub unpacked_len: u32,
    pub packed_len: u32,
    pub unpacked_crc: u16,
    pub packed_crc: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RncBlockStatus {
    Complete,
    Truncated,
    HasTrailingBytes(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RncBlock<'a> {
    pub header: RncHeader,
    data: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RncError {
    NotRnc,
    UnsupportedMethod(u8),
    DecompressorNotImplemented,
}

impl RncHeader {
    pub const LEN: usize = 18;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::LEN || &data[..3] != b"RNC" {
            return None;
        }

        Some(Self {
            method: data[3],
            unpacked_len: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            packed_len: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            unpacked_crc: u16::from_be_bytes([data[12], data[13]]),
            packed_crc: u16::from_be_bytes([data[14], data[15]]),
        })
    }

    pub fn summary(&self) -> String {
        format!(
            "RNC method {}, packed {} bytes, unpacked {} bytes",
            self.method, self.packed_len, self.unpacked_len
        )
    }
}

impl<'a> RncBlock<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let header = RncHeader::parse(data)?;
        Some(Self { header, data })
    }

    pub fn packed_payload(&self) -> &'a [u8] {
        let start = RncHeader::LEN;
        let end = start
            .saturating_add(self.header.packed_len as usize)
            .min(self.data.len());
        &self.data[start..end]
    }

    pub fn status(&self) -> RncBlockStatus {
        let expected = RncHeader::LEN.saturating_add(self.header.packed_len as usize);
        match self.data.len().cmp(&expected) {
            std::cmp::Ordering::Less => RncBlockStatus::Truncated,
            std::cmp::Ordering::Equal => RncBlockStatus::Complete,
            std::cmp::Ordering::Greater => {
                RncBlockStatus::HasTrailingBytes(self.data.len() - expected)
            }
        }
    }

    pub fn output_capacity(&self) -> usize {
        self.header.unpacked_len as usize
    }

    pub fn decompress(&self) -> Result<Vec<u8>, RncError> {
        match self.header.method {
            1 | 2 => Err(RncError::DecompressorNotImplemented),
            method => Err(RncError::UnsupportedMethod(method)),
        }
    }

    pub fn diagnostic_summary(&self) -> String {
        format!("{}, status {:?}", self.header.summary(), self.status())
    }
}

#[cfg(test)]
mod tests {
    use super::{RncBlock, RncBlockStatus, RncError, RncHeader};

    #[test]
    fn parses_rnc_header() {
        let data = [
            b'R', b'N', b'C', 1, 0, 0, 3, 0, 0, 0, 1, 0, 0x12, 0x34, 0x56, 0x78, 0, 0,
        ];
        let header = RncHeader::parse(&data).unwrap();
        assert_eq!(header.method, 1);
        assert_eq!(header.unpacked_len, 768);
        assert_eq!(header.packed_len, 256);
        assert_eq!(header.unpacked_crc, 0x1234);
        assert_eq!(header.packed_crc, 0x5678);
    }

    #[test]
    fn rejects_non_rnc_data() {
        assert!(RncHeader::parse(b"not rnc").is_none());
    }

    #[test]
    fn exposes_payload_and_status() {
        let data = [
            b'R', b'N', b'C', 1, 0, 0, 3, 0, 0, 0, 0, 4, 0x12, 0x34, 0x56, 0x78, 0, 0, 1, 2, 3, 4,
        ];
        let block = RncBlock::parse(&data).unwrap();
        assert_eq!(block.status(), RncBlockStatus::Complete);
        assert_eq!(block.packed_payload(), &[1, 2, 3, 4]);
        assert_eq!(block.output_capacity(), 768);
        assert_eq!(
            block.decompress(),
            Err(RncError::DecompressorNotImplemented)
        );
    }

    #[test]
    fn detects_trailing_bytes() {
        let data = [
            b'R', b'N', b'C', 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 7, 8,
        ];
        let block = RncBlock::parse(&data).unwrap();
        assert_eq!(block.status(), RncBlockStatus::HasTrailingBytes(1));
    }
}
