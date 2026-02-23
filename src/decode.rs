use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use crate::config::FrameConfig;
use crate::error::{Result, VstorageError};
use crate::header::FrameHeader;
use crate::{crypto, ecc, frame, header, video};

/// Run the full decoding pipeline: MP4 → PNGs → frames → decrypt → file.
pub fn decode(input_path: &Path, output_path: &Path, password: Option<&str>) -> Result<()> {
    video::check_ffmpeg()?;

    // 1. Extract PNGs from video
    let temp_dir = tempfile::tempdir()?;
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(format!(
        "Extracting frames from {}...",
        input_path.display()
    ));
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    video::mp4_to_pngs(input_path, temp_dir.path())?;
    pb.finish_and_clear();

    // 2. List extracted frames
    let frame_paths = list_frame_paths(temp_dir.path())?;
    if frame_paths.is_empty() {
        return Err(VstorageError::Ffmpeg("no frames extracted".into()));
    }

    // 3. Read first frame to detect config
    let first_img = load_png(&frame_paths[0])?;
    let (first_header, config) = detect_config_from_frame(&first_img)?;
    let total_frames = first_header.total_frames as usize;
    let file_size = first_header.file_size;
    let nonce = first_header.nonce;
    let salt = first_header.salt;

    eprintln!(
        "Detected: {} frames, block_size={}, levels={}, ecc={}, file_size={}",
        total_frames, config.block_size, config.levels, config.ecc_len, file_size
    );

    // 4. Decode all frames
    let mut ciphertext = Vec::new();

    let pb = ProgressBar::new(total_frames as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} frames ({eta} remaining)")
            .unwrap()
            .progress_chars("=>-"),
    );

    let max_raw = config.max_raw_per_frame();

    for (i, frame_path) in frame_paths.iter().enumerate() {
        if i >= total_frames {
            break;
        }

        let img = load_png(frame_path)?;

        // Try to read per-frame header; fall back to max capacity
        let header_bytes = frame::decode_header_area(&img, config.block_size, config.levels);
        let data_len = match header::decode_header_triple(&header_bytes) {
            Ok(fh) => fh.data_length as usize,
            Err(e) => {
                eprintln!(
                    "  frame {}: header unreadable ({e}), using max capacity",
                    i + 1
                );
                max_raw
            }
        };

        // Decode data area
        let data_bytes = frame::decode_data_area(&img, &config);

        // RS decode
        let rs_decoded = ecc::rs_decode(
            &data_bytes,
            config.ecc_len as usize,
            config.rs_data_len(),
            data_len,
        )?;

        ciphertext.extend_from_slice(&rs_decoded);
        pb.inc(1);
    }
    pb.finish_with_message(format!("{total_frames} frames decoded"));

    // 5. Decrypt (or pass through if no encryption)
    let encrypted = nonce != [0u8; 12] || salt != [0u8; 16];
    let plaintext = if encrypted {
        let pw = password.ok_or_else(|| {
            VstorageError::Crypto("this video is encrypted — provide -p <PASSWORD>".into())
        })?;
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Decrypting...");
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        let pt = crypto::decrypt(&ciphertext, pw, &nonce, &salt)?;
        pb.finish_and_clear();
        pt
    } else {
        eprintln!("No encryption detected — skipping decryption");
        ciphertext
    };

    // 6. Truncate to original file size and write
    let output_data = &plaintext[..file_size as usize];
    std::fs::write(output_path, output_data)?;
    eprintln!(
        "Wrote {} bytes to {}",
        output_data.len(),
        output_path.display()
    );

    Ok(())
}

fn load_png(path: &Path) -> Result<image::RgbImage> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let decoder = image::codecs::png::PngDecoder::new(reader)?;
    let img = image::DynamicImage::from_decoder(decoder)?;
    Ok(img.to_rgb8())
}

fn list_frame_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "png"))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Try combinations of block_size and levels to find a valid header.
fn detect_config_from_frame(img: &image::RgbImage) -> Result<(FrameHeader, FrameConfig)> {
    let width = img.width();
    let height = img.height();

    for &block_size in &[1u8, 2, 4, 8, 16] {
        if width % block_size as u32 != 0 || height % block_size as u32 != 0 {
            continue;
        }
        for &levels in &[2u8, 4, 8, 16] {
            let header_bytes = frame::decode_header_area(img, block_size, levels);
            if let Ok(hdr) = header::decode_header_triple(&header_bytes) {
                if hdr.block_size == block_size && hdr.levels == levels {
                    let config = FrameConfig {
                        width,
                        height,
                        block_size,
                        levels,
                        ecc_len: hdr.ecc_len,
                        fps: 30,
                        crf: 18,
                    };
                    return Ok((hdr, config));
                }
            }
        }
    }

    // Debug: print first few pixel values to help diagnose
    eprintln!("Header detection failed. First frame: {}x{}", width, height);
    eprintln!("First 8 pixel RGB values:");
    for x in 0..8u32.min(width) {
        let p = img.get_pixel(x, 0);
        eprint!("  ({},{},{}) ", p[0], p[1], p[2]);
    }
    eprintln!();

    Err(VstorageError::Header(
        "could not detect frame configuration from video".into(),
    ))
}
