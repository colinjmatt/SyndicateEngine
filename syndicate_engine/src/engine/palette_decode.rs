//! Palette decoding for original 6-bit VGA palette data.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub colors: Vec<Rgb8>,
}

impl Palette {
    pub fn decode_vga_6bit(data: &[u8]) -> Option<Self> {
        if data.len() < 768 {
            return None;
        }

        let colors = data[..768]
            .chunks_exact(3)
            .map(|rgb| Rgb8 {
                r: expand_6bit(rgb[0]),
                g: expand_6bit(rgb[1]),
                b: expand_6bit(rgb[2]),
            })
            .collect();

        Some(Self { colors })
    }

    pub fn preview_ramp(&self, count: usize) -> Vec<Rgb8> {
        if self.colors.is_empty() || count == 0 {
            return Vec::new();
        }

        let max = self.colors.len() - 1;
        (0..count)
            .map(|i| {
                let index = if count == 1 { 0 } else { i * max / (count - 1) };
                self.colors[index]
            })
            .collect()
    }
}

fn expand_6bit(value: u8) -> u8 {
    let value = value.min(63) as u16;
    ((value * 255 + 31) / 63) as u8
}

#[cfg(test)]
mod tests {
    use super::Palette;

    #[test]
    fn decodes_256_color_vga_palette() {
        let mut data = vec![0; 768];
        data[0] = 63;
        data[4] = 32;
        let palette = Palette::decode_vga_6bit(&data).unwrap();
        assert_eq!(palette.colors.len(), 256);
        assert_eq!(palette.colors[0].r, 255);
        assert_eq!(palette.colors[1].g, 130);
    }

    #[test]
    fn creates_even_palette_preview_ramp() {
        let mut data = vec![0; 768];
        for i in 0..256 {
            data[i * 3] = (i % 64) as u8;
        }
        let palette = Palette::decode_vga_6bit(&data).unwrap();
        let preview = palette.preview_ramp(4);
        assert_eq!(preview.len(), 4);
        assert!(preview[3].r >= preview[0].r);
    }
}
