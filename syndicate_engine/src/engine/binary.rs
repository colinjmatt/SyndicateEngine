//! Small binary-reading helpers for DOS-era little-endian asset formats.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinaryReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> BinaryReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    #[allow(dead_code)]
    pub fn read_u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.offset)?;
        self.offset += 1;
        Some(value)
    }

    #[allow(dead_code)]
    pub fn read_u16_le(&mut self) -> Option<u16> {
        let bytes = self.read_exact(2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32_le(&mut self) -> Option<u32> {
        let bytes = self.read_exact(4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_exact(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(len)?;
        let bytes = self.data.get(self.offset..end)?;
        self.offset = end;
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryReader;

    #[test]
    fn reads_little_endian_values() {
        let mut reader = BinaryReader::new(&[0x12, 0x34, 0x56, 0x78, 0x9a]);
        assert_eq!(reader.read_u8(), Some(0x12));
        assert_eq!(reader.read_u16_le(), Some(0x5634));
        assert_eq!(reader.read_u32_le(), None);
        assert_eq!(reader.remaining(), 2);
    }
}
