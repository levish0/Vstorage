use image::{Rgb, RgbImage};

use crate::config::{FrameConfig, HEADER_ROWS};

/// Map a quantization level (0..levels-1) to a pixel channel value (0..255)
pub fn quantize(value: u8, levels: u8) -> u8 {
    if levels <= 1 {
        return 0;
    }
    (value as u16 * 255 / (levels as u16 - 1)) as u8
}

/// Map a pixel channel value (0..255) to the nearest quantization level (0..levels-1)
pub fn dequantize(pixel: u8, levels: u8) -> u8 {
    if levels <= 1 {
        return 0;
    }
    let step = 255.0 / (levels as f64 - 1.0);
    ((pixel as f64 / step).round() as u8).min(levels - 1)
}

// ── Bit stream helpers ──────────────────────────────────────────────────────

pub struct BitWriter {
    bytes: Vec<u8>,
    current: u8,
    count: u8,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            current: 0,
            count: 0,
        }
    }

    /// Write `num_bits` from `value` (MSB first). num_bits must be <= 8.
    pub fn write_bits(&mut self, value: u8, num_bits: u8) {
        for i in (0..num_bits).rev() {
            let bit = (value >> i) & 1;
            self.current = (self.current << 1) | bit;
            self.count += 1;
            if self.count == 8 {
                self.bytes.push(self.current);
                self.current = 0;
                self.count = 0;
            }
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.current <<= 8 - self.count;
            self.bytes.push(self.current);
        }
        self.bytes
    }
}

pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read `num_bits` (MSB first). Pads with 0 if past end of data.
    pub fn read_bits(&mut self, num_bits: u8) -> u8 {
        let mut value: u8 = 0;
        for _ in 0..num_bits {
            value <<= 1;
            if self.byte_pos < self.data.len() {
                let bit = (self.data[self.byte_pos] >> (7 - self.bit_pos)) & 1;
                value |= bit;
                self.bit_pos += 1;
                if self.bit_pos == 8 {
                    self.bit_pos = 0;
                    self.byte_pos += 1;
                }
            }
        }
        value
    }
}

// ── Block painting / reading ────────────────────────────────────────────────

/// Paint a BxB block of pixels at logical position (lx, ly) with the given RGB values.
fn paint_block(img: &mut RgbImage, lx: usize, ly: usize, block_size: u32, r: u8, g: u8, b: u8) {
    let px = lx as u32 * block_size;
    let py = ly as u32 * block_size;
    for dy in 0..block_size {
        for dx in 0..block_size {
            img.put_pixel(px + dx, py + dy, Rgb([r, g, b]));
        }
    }
}

/// Read a BxB block at logical (lx, ly) and return the median-dequantized (r, g, b) level values.
fn read_block(img: &RgbImage, lx: usize, ly: usize, block_size: u32, levels: u8) -> (u8, u8, u8) {
    let px = lx as u32 * block_size;
    let py = ly as u32 * block_size;
    let mut rs: Vec<u8> = Vec::new();
    let mut gs: Vec<u8> = Vec::new();
    let mut bs: Vec<u8> = Vec::new();
    for dy in 0..block_size {
        for dx in 0..block_size {
            let p = img.get_pixel(px + dx, py + dy);
            rs.push(p[0]);
            gs.push(p[1]);
            bs.push(p[2]);
        }
    }
    rs.sort_unstable();
    gs.sort_unstable();
    bs.sort_unstable();
    let mid = rs.len() / 2;
    (
        dequantize(rs[mid], levels),
        dequantize(gs[mid], levels),
        dequantize(bs[mid], levels),
    )
}

// ── Frame encoding / decoding ───────────────────────────────────────────────

/// Encode header bytes and RS-encoded data into a 4K RGB image.
pub fn encode_frame_to_image(header_data: &[u8], rs_data: &[u8], config: &FrameConfig) -> RgbImage {
    let lw = config.logical_width();
    let lh = config.logical_height();
    let bpc = config.bits_per_channel();
    let bs = config.block_size as u32;
    let levels = config.levels;

    let mut img = RgbImage::new(config.width, config.height);

    // Header area: first HEADER_ROWS logical rows
    let mut reader = BitReader::new(header_data);
    for ly in 0..HEADER_ROWS {
        for lx in 0..lw {
            let r = reader.read_bits(bpc);
            let g = reader.read_bits(bpc);
            let b = reader.read_bits(bpc);
            paint_block(
                &mut img,
                lx,
                ly,
                bs,
                quantize(r, levels),
                quantize(g, levels),
                quantize(b, levels),
            );
        }
    }

    // Data area: remaining logical rows
    let mut reader = BitReader::new(rs_data);
    for ly in HEADER_ROWS..lh {
        for lx in 0..lw {
            let r = reader.read_bits(bpc);
            let g = reader.read_bits(bpc);
            let b = reader.read_bits(bpc);
            paint_block(
                &mut img,
                lx,
                ly,
                bs,
                quantize(r, levels),
                quantize(g, levels),
                quantize(b, levels),
            );
        }
    }

    img
}

/// Decode only the header area (first HEADER_ROWS logical rows) from an image.
pub fn decode_header_area(img: &RgbImage, block_size: u8, levels: u8) -> Vec<u8> {
    let lw = img.width() as usize / block_size as usize;
    let bpc = (levels as f64).log2() as u8;
    let bs = block_size as u32;

    let mut writer = BitWriter::new();
    for ly in 0..HEADER_ROWS {
        for lx in 0..lw {
            let (r, g, b) = read_block(img, lx, ly, bs, levels);
            writer.write_bits(r, bpc);
            writer.write_bits(g, bpc);
            writer.write_bits(b, bpc);
        }
    }
    writer.finish()
}

/// Decode the data area (rows after HEADER_ROWS) from an image.
pub fn decode_data_area(img: &RgbImage, config: &FrameConfig) -> Vec<u8> {
    let lw = config.logical_width();
    let lh = config.logical_height();
    let bpc = config.bits_per_channel();
    let bs = config.block_size as u32;
    let levels = config.levels;

    let mut writer = BitWriter::new();
    for ly in HEADER_ROWS..lh {
        for lx in 0..lw {
            let (r, g, b) = read_block(img, lx, ly, bs, levels);
            writer.write_bits(r, bpc);
            writer.write_bits(g, bpc);
            writer.write_bits(b, bpc);
        }
    }
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_roundtrip() {
        for levels in [2u8, 4, 8, 16] {
            for v in 0..levels {
                let pixel = quantize(v, levels);
                let recovered = dequantize(pixel, levels);
                assert_eq!(v, recovered, "levels={levels}, value={v}, pixel={pixel}");
            }
        }
    }

    #[test]
    fn test_noise_tolerance() {
        let levels = 4u8;
        for v in 0..levels {
            let pixel = quantize(v, levels);
            // Add noise up to ±30
            for noise in -30i16..=30 {
                let noisy = (pixel as i16 + noise).clamp(0, 255) as u8;
                let recovered = dequantize(noisy, levels);
                assert_eq!(
                    v, recovered,
                    "levels={levels}, value={v}, pixel={pixel}, noise={noise}, noisy={noisy}"
                );
            }
        }
    }

    #[test]
    fn test_bit_roundtrip() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut reader = BitReader::new(&data);
        let mut writer = BitWriter::new();
        for _ in 0..(data.len() * 8 / 2) {
            let bits = reader.read_bits(2);
            writer.write_bits(bits, 2);
        }
        assert_eq!(writer.finish(), data);
    }

    #[test]
    fn test_frame_encode_decode_roundtrip() {
        let config = crate::config::FrameConfig::new(2, 4, 32, 30, 18).unwrap();

        // Create some test data
        let header_data = vec![0xAB; crate::header::HEADER_SIZE * 3];
        let rs_data = vec![0x55; 1024];

        let img = encode_frame_to_image(&header_data, &rs_data, &config);

        // Decode header
        let decoded_header = decode_header_area(&img, config.block_size, config.levels);
        assert_eq!(
            &decoded_header[..header_data.len()],
            &header_data[..],
            "header roundtrip failed"
        );

        // Decode data
        let decoded_data = decode_data_area(&img, &config);
        assert_eq!(
            &decoded_data[..rs_data.len()],
            &rs_data[..],
            "data roundtrip failed"
        );
    }
}
