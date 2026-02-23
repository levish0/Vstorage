use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::config::{FrameConfig, PROTOCOL_VERSION};
use crate::error::Result;
use crate::{crypto, ecc, frame, header, video};

/// Run the full encoding pipeline: file → encrypt → frames → PNGs → MP4.
pub fn encode(
    input_path: &Path,
    output_path: &Path,
    password: Option<&str>,
    config: &FrameConfig,
) -> Result<()> {
    video::check_ffmpeg()?;

    // 1. Read file
    let data = std::fs::read(input_path)?;
    let file_size = data.len() as u64;
    eprintln!("Read {} bytes from {}", data.len(), input_path.display());

    // 2. Encrypt (or pass through)
    let (payload, nonce, salt) = if let Some(pw) = password {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Encrypting (Argon2 + AES-256-GCM)...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        let (ct, n, s) = crypto::encrypt(&data, &pw)?;
        pb.finish_with_message(format!("Encrypted: {} bytes", ct.len()));
        (ct, n, s)
    } else {
        eprintln!("No password — skipping encryption");
        (data, [0u8; 12], [0u8; 16])
    };

    // 3. Calculate frame count
    let max_raw = config.max_raw_per_frame();
    if max_raw == 0 {
        return Err(crate::error::VstorageError::Config(
            "frame capacity is zero — check block_size/levels/ecc settings".into(),
        ));
    }
    let num_frames = (payload.len() + max_raw - 1) / max_raw;
    eprintln!(
        "Encoding into {} frames ({} bytes/frame, RS({},{}), ecc={})",
        num_frames,
        max_raw,
        config.rs_data_len() + config.ecc_len as usize,
        config.rs_data_len(),
        config.ecc_len
    );

    // 4. Create temp dir for PNGs
    let temp_dir = tempfile::tempdir()?;

    // 5. Encode each frame
    let pb = ProgressBar::new(num_frames as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} frames ({eta} remaining)")
            .unwrap()
            .progress_chars("=>-"),
    );

    for i in 0..num_frames {
        let start = i * max_raw;
        let end = std::cmp::min(start + max_raw, payload.len());
        let frame_data = &payload[start..end];

        // RS encode (pads last chunk to full block)
        let rs_encoded = ecc::rs_encode(frame_data, config.ecc_len as usize, config.rs_data_len());

        // SHA-256 of the RS-encoded data
        let data_hash: [u8; 32] = Sha256::digest(&rs_encoded).into();

        // Build header
        let hdr = header::FrameHeader {
            version: PROTOCOL_VERSION,
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
        let img = frame::encode_frame_to_image(&header_bytes, &rs_encoded, config);

        let png_path = temp_dir.path().join(format!("frame_{:06}.png", i + 1));
        img.save(&png_path)?;

        pb.inc(1);
    }
    pb.finish_with_message(format!("{num_frames} frames encoded"));

    // 6. FFmpeg: PNGs → MP4
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("FFmpeg: producing {}...", output_path.display()));
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    video::pngs_to_mp4(temp_dir.path(), output_path, config)?;
    pb.finish_with_message("Done.");

    Ok(())
}
