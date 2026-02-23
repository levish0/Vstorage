use vstorage::config::FrameConfig;
use vstorage::header::FrameHeader;
use vstorage::{config, crypto, ecc, frame, header};

use sha2::{Digest, Sha256};

/// Full pipeline test without FFmpeg: file → encrypt → frames → decode → decrypt → compare.
#[test]
fn test_roundtrip_no_ffmpeg() {
    let original: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    let password = "test-password-123";

    let config = FrameConfig::new(2, 4, 32, 30, 18).unwrap();

    // ── Encode ──────────────────────────────────────────────────────
    let (ciphertext, nonce, salt) = crypto::encrypt(&original, password).unwrap();
    let file_size = original.len() as u64;

    let max_raw = config.max_raw_per_frame();
    let num_frames = (ciphertext.len() + max_raw - 1) / max_raw;

    let mut frame_images = Vec::new();

    for i in 0..num_frames {
        let start = i * max_raw;
        let end = std::cmp::min(start + max_raw, ciphertext.len());
        let frame_data = &ciphertext[start..end];

        let rs_encoded = ecc::rs_encode(frame_data, config.ecc_len as usize, config.rs_data_len());
        let data_hash: [u8; 32] = Sha256::digest(&rs_encoded).into();

        let hdr = FrameHeader {
            version: config::PROTOCOL_VERSION,
            frame_number: i as u32,
            total_frames: num_frames as u32,
            block_size: config.block_size,
            levels: config.levels,
            file_size,
            data_length: frame_data.len() as u32,
            ecc_len: config.ecc_len,
            rs_data_len: config.rs_data_len() as u16,
            nonce,
            salt,
            data_sha256: data_hash,
        };

        let header_bytes = header::encode_header_triple(&hdr);
        let img = frame::encode_frame_to_image(&header_bytes, &rs_encoded, &config);
        frame_images.push(img);
    }

    // ── Decode ──────────────────────────────────────────────────────
    let first_header_bytes =
        frame::decode_header_area(&frame_images[0], config.block_size, config.levels);
    let first_header = header::decode_header_triple(&first_header_bytes).unwrap();

    assert_eq!(first_header.total_frames, num_frames as u32);
    assert_eq!(first_header.file_size, file_size);

    let mut recovered_ciphertext = Vec::new();

    for (i, img) in frame_images.iter().enumerate() {
        let header_bytes = frame::decode_header_area(img, config.block_size, config.levels);
        let frame_hdr = header::decode_header_triple(&header_bytes).unwrap();
        assert_eq!(frame_hdr.frame_number, i as u32);

        let data_bytes = frame::decode_data_area(img, &config);
        let data_len = frame_hdr.data_length as usize;
        let rs_decoded = ecc::rs_decode(
            &data_bytes,
            config.ecc_len as usize,
            config.rs_data_len(),
            data_len,
        )
        .unwrap();

        recovered_ciphertext.extend_from_slice(&rs_decoded);
    }

    let plaintext = crypto::decrypt(
        &recovered_ciphertext,
        password,
        &first_header.nonce,
        &first_header.salt,
    )
    .unwrap();

    let recovered = &plaintext[..file_size as usize];
    assert_eq!(recovered, &original[..], "roundtrip mismatch!");
}

/// Test that the pipeline survives moderate noise (simulating lossy compression).
#[test]
fn test_roundtrip_with_noise() {
    let original: Vec<u8> = b"Vstorage noise resilience test data!".to_vec();
    let password = "noisy";

    let config = FrameConfig::new(4, 4, 32, 30, 18).unwrap();

    // Encode
    let (ciphertext, nonce, salt) = crypto::encrypt(&original, password).unwrap();
    let file_size = original.len() as u64;
    let max_raw = config.max_raw_per_frame();
    let num_frames = (ciphertext.len() + max_raw - 1) / max_raw;

    let mut frame_images = Vec::new();
    for i in 0..num_frames {
        let start = i * max_raw;
        let end = std::cmp::min(start + max_raw, ciphertext.len());
        let frame_data = &ciphertext[start..end];

        let rs_encoded = ecc::rs_encode(frame_data, config.ecc_len as usize, config.rs_data_len());
        let data_hash: [u8; 32] = Sha256::digest(&rs_encoded).into();

        let hdr = header::FrameHeader {
            version: config::PROTOCOL_VERSION,
            frame_number: i as u32,
            total_frames: num_frames as u32,
            block_size: config.block_size,
            levels: config.levels,
            file_size,
            data_length: frame_data.len() as u32,
            ecc_len: config.ecc_len,
            rs_data_len: config.rs_data_len() as u16,
            nonce,
            salt,
            data_sha256: data_hash,
        };

        let header_bytes = header::encode_header_triple(&hdr);
        let mut img = frame::encode_frame_to_image(&header_bytes, &rs_encoded, &config);

        // Add noise to a subset of pixels (±10, well within tolerance for levels=4)
        let (w, h) = (img.width(), img.height());
        for y in (0..h).step_by(7) {
            for x in (0..w).step_by(11) {
                let p = img.get_pixel(x, y).0;
                let noisy = image::Rgb([
                    p[0].saturating_add(10),
                    p[1].saturating_sub(8),
                    p[2].saturating_add(5),
                ]);
                img.put_pixel(x, y, noisy);
            }
        }

        frame_images.push(img);
    }

    // Decode
    let first_hdr_bytes =
        frame::decode_header_area(&frame_images[0], config.block_size, config.levels);
    let first_hdr = header::decode_header_triple(&first_hdr_bytes).unwrap();

    let mut recovered_ct = Vec::new();
    for (i, img) in frame_images.iter().enumerate() {
        if i >= first_hdr.total_frames as usize {
            break;
        }
        let hdr_bytes = frame::decode_header_area(img, config.block_size, config.levels);
        let fhdr = header::decode_header_triple(&hdr_bytes).unwrap();

        let data_bytes = frame::decode_data_area(img, &config);
        let rs_decoded = ecc::rs_decode(
            &data_bytes,
            config.ecc_len as usize,
            config.rs_data_len(),
            fhdr.data_length as usize,
        )
        .unwrap();
        recovered_ct.extend_from_slice(&rs_decoded);
    }

    let plaintext =
        crypto::decrypt(&recovered_ct, password, &first_hdr.nonce, &first_hdr.salt).unwrap();
    let recovered = &plaintext[..file_size as usize];
    assert_eq!(recovered, &original[..], "noisy roundtrip failed!");
}
