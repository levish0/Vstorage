use crate::error::{Result, VstorageError};

pub const FRAME_WIDTH: u32 = 3840;
pub const FRAME_HEIGHT: u32 = 2160;
pub const HEADER_ROWS: usize = 2;
pub const HEADER_COPIES: usize = 3;
pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct FrameConfig {
    pub width: u32,
    pub height: u32,
    pub block_size: u8,
    pub levels: u8,
    pub ecc_len: u8,
    pub fps: u32,
    pub crf: u8,
}

impl FrameConfig {
    pub fn new(block_size: u8, levels: u8, ecc_len: u8, fps: u32, crf: u8) -> Result<Self> {
        if block_size == 0 {
            return Err(VstorageError::Config("block_size must be > 0".into()));
        }
        if !levels.is_power_of_two() || levels < 2 {
            return Err(VstorageError::Config(
                "levels must be a power of 2 and >= 2".into(),
            ));
        }
        if ecc_len == 0 || ecc_len as u16 >= 255 {
            return Err(VstorageError::Config("ecc_len must be in 1..254".into()));
        }
        if FRAME_WIDTH % block_size as u32 != 0 || FRAME_HEIGHT % block_size as u32 != 0 {
            return Err(VstorageError::Config(
                "frame dimensions must be divisible by block_size".into(),
            ));
        }
        Ok(Self {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            block_size,
            levels,
            ecc_len,
            fps,
            crf,
        })
    }

    pub fn logical_width(&self) -> usize {
        self.width as usize / self.block_size as usize
    }

    pub fn logical_height(&self) -> usize {
        self.height as usize / self.block_size as usize
    }

    pub fn bits_per_channel(&self) -> u8 {
        (self.levels as f64).log2() as u8
    }

    pub fn bits_per_pixel(&self) -> u8 {
        self.bits_per_channel() * 3
    }

    /// Number of logical pixels available for data (excluding header rows)
    pub fn data_area_pixels(&self) -> usize {
        let lw = self.logical_width();
        let lh = self.logical_height();
        lw * (lh - HEADER_ROWS)
    }

    /// Number of bytes that fit in the data area
    pub fn data_area_bytes(&self) -> usize {
        self.data_area_pixels() * self.bits_per_pixel() as usize / 8
    }

    /// RS data length per block (255 - ecc_len)
    pub fn rs_data_len(&self) -> usize {
        255 - self.ecc_len as usize
    }

    /// Maximum number of complete RS blocks per frame
    pub fn max_rs_blocks_per_frame(&self) -> usize {
        self.data_area_bytes() / 255
    }

    /// Maximum raw (pre-RS) data bytes per frame
    pub fn max_raw_per_frame(&self) -> usize {
        self.max_rs_blocks_per_frame() * self.rs_data_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FrameConfig::new(2, 4, 32, 30, 18).unwrap();
        assert_eq!(config.logical_width(), 1920);
        assert_eq!(config.logical_height(), 1080);
        assert_eq!(config.bits_per_channel(), 2);
        assert_eq!(config.bits_per_pixel(), 6);
        assert_eq!(config.rs_data_len(), 223);
        // ~1.29MB raw data per frame
        assert!(config.max_raw_per_frame() > 1_200_000);
        assert!(config.max_raw_per_frame() < 1_400_000);
    }

    #[test]
    fn test_invalid_config() {
        assert!(FrameConfig::new(0, 4, 32, 30, 18).is_err());
        assert!(FrameConfig::new(2, 3, 32, 30, 18).is_err()); // not power of 2
        assert!(FrameConfig::new(2, 4, 0, 30, 18).is_err());
        assert!(FrameConfig::new(7, 4, 32, 30, 18).is_err()); // 3840 not divisible by 7
    }
}
