use crate::config::PROTOCOL_VERSION;
use crate::error::{Result, VstorageError};

pub const HEADER_SIZE: usize = 90;
pub const MAGIC: &[u8; 4] = b"VSTR";

/// Frame header containing metadata for one video frame.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub version: u8,
    pub frame_number: u32,
    pub total_frames: u32,
    pub block_size: u8,
    pub levels: u8,
    pub file_size: u64,
    pub data_length: u32,
    pub ecc_len: u8,
    pub rs_data_len: u16,
    pub nonce: [u8; 12],
    pub salt: [u8; 16],
    pub data_sha256: [u8; 32],
}

impl FrameHeader {
    /// Serialize to HEADER_SIZE bytes (big-endian).
    pub fn serialize(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(MAGIC);
        buf[4] = self.version;
        buf[5..9].copy_from_slice(&self.frame_number.to_be_bytes());
        buf[9..13].copy_from_slice(&self.total_frames.to_be_bytes());
        buf[13] = self.block_size;
        buf[14] = self.levels;
        buf[15..23].copy_from_slice(&self.file_size.to_be_bytes());
        buf[23..27].copy_from_slice(&self.data_length.to_be_bytes());
        buf[27] = self.ecc_len;
        buf[28..30].copy_from_slice(&self.rs_data_len.to_be_bytes());
        buf[30..42].copy_from_slice(&self.nonce);
        buf[42..58].copy_from_slice(&self.salt);
        buf[58..90].copy_from_slice(&self.data_sha256);
        buf
    }

    /// Deserialize from bytes.
    pub fn deserialize(buf: &[u8]) -> Result<Self> {
        if buf.len() < HEADER_SIZE {
            return Err(VstorageError::Header("buffer too short".into()));
        }
        if &buf[0..4] != MAGIC {
            return Err(VstorageError::Header(format!(
                "invalid magic: {:?}",
                &buf[0..4]
            )));
        }
        let version = buf[4];
        if version != PROTOCOL_VERSION {
            return Err(VstorageError::Header(format!(
                "unsupported version: {version}"
            )));
        }
        Ok(Self {
            version,
            frame_number: u32::from_be_bytes(buf[5..9].try_into().unwrap()),
            total_frames: u32::from_be_bytes(buf[9..13].try_into().unwrap()),
            block_size: buf[13],
            levels: buf[14],
            file_size: u64::from_be_bytes(buf[15..23].try_into().unwrap()),
            data_length: u32::from_be_bytes(buf[23..27].try_into().unwrap()),
            ecc_len: buf[27],
            rs_data_len: u16::from_be_bytes(buf[28..30].try_into().unwrap()),
            nonce: buf[30..42].try_into().unwrap(),
            salt: buf[42..58].try_into().unwrap(),
            data_sha256: buf[58..90].try_into().unwrap(),
        })
    }
}

/// Encode header with triple redundancy for error resilience.
pub fn encode_header_triple(header: &FrameHeader) -> Vec<u8> {
    let serialized = header.serialize();
    let mut result = Vec::with_capacity(HEADER_SIZE * 3);
    for _ in 0..3 {
        result.extend_from_slice(&serialized);
    }
    result
}

/// Decode header from triple-redundant data using byte-level majority vote.
pub fn decode_header_triple(data: &[u8]) -> Result<FrameHeader> {
    if data.len() < HEADER_SIZE * 3 {
        return Err(VstorageError::Header(
            "header data too short for triple decode".into(),
        ));
    }

    let h1 = &data[0..HEADER_SIZE];
    let h2 = &data[HEADER_SIZE..HEADER_SIZE * 2];
    let h3 = &data[HEADER_SIZE * 2..HEADER_SIZE * 3];

    let mut voted = vec![0u8; HEADER_SIZE];
    for i in 0..HEADER_SIZE {
        voted[i] = majority_vote(h1[i], h2[i], h3[i]);
    }

    FrameHeader::deserialize(&voted)
}

fn majority_vote(a: u8, b: u8, c: u8) -> u8 {
    if a == b || a == c {
        a
    } else if b == c {
        b
    } else {
        a // no majority — return first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> FrameHeader {
        FrameHeader {
            version: PROTOCOL_VERSION,
            frame_number: 42,
            total_frames: 100,
            block_size: 2,
            levels: 4,
            file_size: 123456789,
            data_length: 65535,
            ecc_len: 32,
            rs_data_len: 223,
            nonce: [1; 12],
            salt: [2; 16],
            data_sha256: [3; 32],
        }
    }

    #[test]
    fn test_serialize_roundtrip() {
        let h = sample_header();
        let buf = h.serialize();
        let h2 = FrameHeader::deserialize(&buf).unwrap();
        assert_eq!(h.frame_number, h2.frame_number);
        assert_eq!(h.total_frames, h2.total_frames);
        assert_eq!(h.file_size, h2.file_size);
        assert_eq!(h.data_length, h2.data_length);
        assert_eq!(h.nonce, h2.nonce);
        assert_eq!(h.salt, h2.salt);
        assert_eq!(h.data_sha256, h2.data_sha256);
    }

    #[test]
    fn test_triple_majority_vote() {
        let h = sample_header();
        let mut triple = encode_header_triple(&h);

        // Corrupt the second copy
        for i in HEADER_SIZE..HEADER_SIZE * 2 {
            triple[i] = 0xFF;
        }

        let recovered = decode_header_triple(&triple).unwrap();
        assert_eq!(recovered.frame_number, h.frame_number);
        assert_eq!(recovered.file_size, h.file_size);
        assert_eq!(recovered.data_sha256, h.data_sha256);
    }

    #[test]
    fn test_invalid_magic() {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(b"XXXX");
        assert!(FrameHeader::deserialize(&buf).is_err());
    }
}
