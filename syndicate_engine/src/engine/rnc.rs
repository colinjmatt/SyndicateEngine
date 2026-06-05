//! RNC/ProPack container detection and method-1 decompression.
//!
//! Many original Bullfrog data files are RNC-compressed. This module implements
//! the Huffman/LZ method used by the original Syndicate data files while keeping
//! the API conservative: payload and unpacked CRCs are verified before decoded
//! bytes are returned.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RncHeader {
    pub method: u8,
    pub unpacked_len: u32,
    pub packed_len: u32,
    pub unpacked_crc: u16,
    pub packed_crc: u16,
    /// In-place decompression slack metadata; not required for out-of-place decoding.
    pub leeway: u8,
    /// Number of Huffman/LZ blocks for method 1 streams.
    pub block_count: u8,
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
    Truncated { expected: usize, actual: usize },
    UnexpectedEnd,
    InvalidHuffmanTable,
    InvalidBackReference { offset: usize, output_len: usize },
    OutputLengthMismatch { expected: usize, actual: usize },
    PackedCrcMismatch { expected: u16, actual: u16 },
    UnpackedCrcMismatch { expected: u16, actual: u16 },
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
            leeway: data[16],
            block_count: data[17],
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
            1 => self.decompress_method1(),
            method => Err(RncError::UnsupportedMethod(method)),
        }
    }

    pub fn packed_crc_actual(&self) -> Option<u16> {
        if self.status() == RncBlockStatus::Truncated {
            return None;
        }
        Some(crc16_ibm(self.packed_payload()))
    }

    pub fn packed_crc_matches(&self) -> Option<bool> {
        self.packed_crc_actual()
            .map(|actual| actual == self.header.packed_crc)
    }

    pub fn diagnostic_summary(&self) -> String {
        let crc_status = match self.packed_crc_matches() {
            Some(true) => "packed CRC ok".to_string(),
            Some(false) => format!(
                "packed CRC mismatch expected {:04x}",
                self.header.packed_crc
            ),
            None => "packed CRC unavailable".to_string(),
        };
        format!(
            "{}, {} blocks, status {:?}, {}",
            self.header.summary(),
            self.header.block_count,
            self.status(),
            crc_status
        )
    }

    fn decompress_method1(&self) -> Result<Vec<u8>, RncError> {
        let expected_input = RncHeader::LEN.saturating_add(self.header.packed_len as usize);
        if self.data.len() < expected_input {
            return Err(RncError::Truncated {
                expected: expected_input,
                actual: self.data.len(),
            });
        }

        let payload = self.packed_payload();
        let packed_crc = crc16_ibm(payload);
        if packed_crc != self.header.packed_crc {
            return Err(RncError::PackedCrcMismatch {
                expected: self.header.packed_crc,
                actual: packed_crc,
            });
        }

        let expected_output = self.header.unpacked_len as usize;
        let mut reader = Method1Reader::new(payload);
        let mut output = Vec::with_capacity(expected_output);
        reader.read_bits(2)?;

        for _ in 0..self.header.block_count {
            if output.len() >= expected_output {
                break;
            }

            let raw_table = HuffmanTable::read(&mut reader)?;
            let distance_table = HuffmanTable::read(&mut reader)?;
            let length_table = HuffmanTable::read(&mut reader)?;
            let token_count = reader.read_bits(16)? as usize;

            for token_index in 0..token_count {
                let literal_len = read_huffman_value(&mut reader, &raw_table)?;
                reader.copy_literals_to(&mut output, literal_len)?;

                if output.len() > expected_output {
                    return Err(RncError::OutputLengthMismatch {
                        expected: expected_output,
                        actual: output.len(),
                    });
                }

                if token_index + 1 < token_count {
                    let offset = read_huffman_value(&mut reader, &distance_table)? + 1;
                    let length = read_huffman_value(&mut reader, &length_table)? + 2;
                    copy_from_output(&mut output, offset, length, expected_output)?;
                }
            }
        }

        if output.len() != expected_output {
            return Err(RncError::OutputLengthMismatch {
                expected: expected_output,
                actual: output.len(),
            });
        }

        let unpacked_crc = crc16_ibm(&output);
        if unpacked_crc != self.header.unpacked_crc {
            return Err(RncError::UnpackedCrcMismatch {
                expected: self.header.unpacked_crc,
                actual: unpacked_crc,
            });
        }

        Ok(output)
    }
}

pub fn crc16_ibm(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xa001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HuffmanEntry {
    code: u16,
    bits: u8,
    symbol: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HuffmanTable {
    entries: Vec<HuffmanEntry>,
}

impl HuffmanTable {
    fn read(reader: &mut Method1Reader<'_>) -> Result<Self, RncError> {
        let symbol_count = reader.read_bits(5)? as usize;
        let mut lengths = Vec::with_capacity(symbol_count);
        for _ in 0..symbol_count {
            lengths.push(reader.read_bits(4)? as u8);
        }

        let mut entries = Vec::new();
        let mut code = 0u32;
        for bits in 1..=16 {
            for (symbol, &length) in lengths.iter().enumerate() {
                if length == bits {
                    entries.push(HuffmanEntry {
                        code: reverse_bits(code, bits),
                        bits,
                        symbol,
                    });
                    code = code.checked_add(1).ok_or(RncError::InvalidHuffmanTable)?;
                }
            }
            code = code.checked_shl(1).ok_or(RncError::InvalidHuffmanTable)?;
        }

        if entries.is_empty() {
            return Err(RncError::InvalidHuffmanTable);
        }

        Ok(Self { entries })
    }

    fn read_symbol(&self, reader: &mut Method1Reader<'_>) -> Result<usize, RncError> {
        for entry in &self.entries {
            if reader.peek_bits(entry.bits)? as u16 == entry.code {
                reader.read_bits(entry.bits)?;
                return Ok(entry.symbol);
            }
        }
        Err(RncError::InvalidHuffmanTable)
    }
}

#[derive(Debug, Clone)]
struct Method1Reader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buffer: u64,
    bit_count: u8,
}

impl<'a> Method1Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    fn read_bits(&mut self, bits: u8) -> Result<usize, RncError> {
        let value = self.peek_bits(bits)?;
        self.bit_buffer >>= bits;
        self.bit_count -= bits;
        Ok(value)
    }

    fn peek_bits(&mut self, bits: u8) -> Result<usize, RncError> {
        while self.bit_count < bits {
            self.refill()?;
        }
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        Ok((self.bit_buffer & mask) as usize)
    }

    fn refill(&mut self) -> Result<(), RncError> {
        if self.pos >= self.data.len() {
            return Err(RncError::UnexpectedEnd);
        }

        let low = self.data[self.pos];
        let high = self.data.get(self.pos + 1).copied().unwrap_or(0);
        let consumed = if self.pos + 1 < self.data.len() { 2 } else { 1 };
        let word = u16::from_le_bytes([low, high]) as u64;
        self.pos += consumed;
        self.bit_buffer |= word << self.bit_count;
        self.bit_count += 16;
        Ok(())
    }

    fn copy_literals_to(&mut self, output: &mut Vec<u8>, len: usize) -> Result<(), RncError> {
        let end = self.pos.checked_add(len).ok_or(RncError::UnexpectedEnd)?;
        if end > self.data.len() {
            return Err(RncError::UnexpectedEnd);
        }
        output.extend_from_slice(&self.data[self.pos..end]);
        self.pos = end;
        Ok(())
    }
}

fn read_huffman_value(
    reader: &mut Method1Reader<'_>,
    table: &HuffmanTable,
) -> Result<usize, RncError> {
    let symbol = table.read_symbol(reader)?;
    if symbol < 2 {
        Ok(symbol)
    } else {
        Ok((1usize << (symbol - 1)) + reader.read_bits((symbol - 1) as u8)?)
    }
}

fn copy_from_output(
    output: &mut Vec<u8>,
    offset: usize,
    length: usize,
    expected_output: usize,
) -> Result<(), RncError> {
    if offset == 0 || offset > output.len() {
        return Err(RncError::InvalidBackReference {
            offset,
            output_len: output.len(),
        });
    }

    if output.len().saturating_add(length) > expected_output {
        return Err(RncError::OutputLengthMismatch {
            expected: expected_output,
            actual: output.len() + length,
        });
    }

    for _ in 0..length {
        let value = output[output.len() - offset];
        output.push(value);
    }
    Ok(())
}

fn reverse_bits(mut value: u32, bits: u8) -> u16 {
    let mut reversed = 0;
    for _ in 0..bits {
        reversed = (reversed << 1) | (value as u16 & 1);
        value >>= 1;
    }
    reversed
}

#[cfg(test)]
mod tests {
    use super::{RncBlock, RncBlockStatus, RncError, RncHeader, crc16_ibm};

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
        assert_eq!(header.leeway, 0);
        assert_eq!(header.block_count, 0);
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
            Err(RncError::PackedCrcMismatch {
                expected: 0x5678,
                actual: 0x0fa1
            })
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

    #[test]
    fn crc16_ibm_matches_rnc_payload_crc() {
        assert_eq!(crc16_ibm(b"ABC"), 0x4521);
        assert_eq!(crc16_ibm(&[1, 2, 3, 4]), 0x0fa1);
    }

    #[test]
    fn decompresses_simple_method1_literal_stream() {
        let payload = [
            0x0c, 0x80, 0x08, 0x11, 0x22, 0x00, 0x40, 0x00, b'A', b'B', b'C',
        ];
        let mut data = Vec::new();
        data.extend_from_slice(b"RNC");
        data.push(1);
        data.extend_from_slice(&3u32.to_be_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        data.extend_from_slice(&crc16_ibm(b"ABC").to_be_bytes());
        data.extend_from_slice(&crc16_ibm(&payload).to_be_bytes());
        data.push(0);
        data.push(1);
        data.extend_from_slice(&payload);

        let block = RncBlock::parse(&data).unwrap();
        assert_eq!(block.packed_crc_matches(), Some(true));
        assert_eq!(block.decompress().unwrap(), b"ABC");
    }
}
