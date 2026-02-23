use reed_solomon::{Decoder, Encoder};

use crate::error::{Result, VstorageError};

/// Reed-Solomon encode data.
/// Splits `data` into chunks of `rs_data_len`, pads the last chunk with zeros,
/// and encodes each chunk into a 255-byte RS block (rs_data_len + ecc_len).
pub fn rs_encode(data: &[u8], ecc_len: usize, rs_data_len: usize) -> Vec<u8> {
    let enc = Encoder::new(ecc_len);
    let mut result = Vec::new();

    for chunk in data.chunks(rs_data_len) {
        let input: Vec<u8> = if chunk.len() < rs_data_len {
            let mut padded = vec![0u8; rs_data_len];
            padded[..chunk.len()].copy_from_slice(chunk);
            padded
        } else {
            chunk.to_vec()
        };

        let encoded = enc.encode(&input);
        // encoded is a Buffer of length rs_data_len + ecc_len = 255
        for i in 0..rs_data_len + ecc_len {
            result.push(encoded[i]);
        }
    }

    result
}

/// Reed-Solomon decode data.
/// Reads complete 255-byte RS blocks from `data`, corrects errors, and
/// returns the reassembled raw payload truncated to `expected_data_len`.
pub fn rs_decode(
    data: &[u8],
    ecc_len: usize,
    rs_data_len: usize,
    expected_data_len: usize,
) -> Result<Vec<u8>> {
    let dec = Decoder::new(ecc_len);
    let block_len = rs_data_len + ecc_len; // 255
    let num_blocks = (expected_data_len + rs_data_len - 1) / rs_data_len;
    let mut result = Vec::new();

    for i in 0..num_blocks {
        let start = i * block_len;
        let end = start + block_len;
        if end > data.len() {
            return Err(VstorageError::Ecc(format!(
                "insufficient data for RS block {i}: need {end} bytes, have {}",
                data.len()
            )));
        }

        // We need to encode dummy data to get a Buffer of the right size,
        // then overwrite it with our received data.
        let enc = Encoder::new(ecc_len);
        let dummy = vec![0u8; rs_data_len];
        let mut buf = enc.encode(&dummy);

        // Copy received data into the buffer
        for j in 0..block_len {
            buf[j] = data[start + j];
        }

        match dec.correct(&mut buf, None) {
            Ok(corrected) => {
                result.extend_from_slice(corrected.data());
            }
            Err(e) => {
                return Err(VstorageError::Ecc(format!(
                    "RS correction failed on block {i}: {e:?}"
                )));
            }
        }
    }

    result.truncate(expected_data_len);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rs_encode_decode_clean() {
        let ecc_len = 32;
        let rs_data_len = 223;
        let data = b"Hello, Reed-Solomon!";

        let encoded = rs_encode(data, ecc_len, rs_data_len);
        assert_eq!(encoded.len(), 255); // one block, padded

        let decoded = rs_decode(&encoded, ecc_len, rs_data_len, data.len()).unwrap();
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_rs_error_correction() {
        let ecc_len = 32;
        let rs_data_len = 223;
        let data = b"Error correction test data!!!!!";

        let mut encoded = rs_encode(data, ecc_len, rs_data_len);

        // Corrupt up to ecc_len/2 = 16 bytes (maximum correctable)
        for i in 0..15 {
            encoded[i] = encoded[i].wrapping_add(1);
        }

        let decoded = rs_decode(&encoded, ecc_len, rs_data_len, data.len()).unwrap();
        assert_eq!(&decoded, data);
    }

    #[test]
    fn test_rs_multiple_blocks() {
        let ecc_len = 32;
        let rs_data_len = 223;
        // Create data that spans 3 blocks
        let data: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();

        let encoded = rs_encode(&data, ecc_len, rs_data_len);
        assert_eq!(encoded.len(), 255 * 3); // 3 padded blocks

        let decoded = rs_decode(&encoded, ecc_len, rs_data_len, data.len()).unwrap();
        assert_eq!(decoded, data);
    }
}
