use std::path::Path;
use std::process::Command;

use crate::config::FrameConfig;
use crate::error::{Result, VstorageError};

/// Check that FFmpeg is available on PATH.
pub fn check_ffmpeg() -> Result<()> {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| {
            VstorageError::Ffmpeg(
                "FFmpeg not found. Please install FFmpeg and ensure it is in your PATH.".into(),
            )
        })?;
    Ok(())
}

/// Convert a directory of numbered PNGs into an MP4 video.
pub fn pngs_to_mp4(png_dir: &Path, output: &Path, config: &FrameConfig) -> Result<()> {
    let pattern = png_dir.join("frame_%06d.png");
    let fps_str = config.fps.to_string();
    let crf_str = config.crf.to_string();

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate",
            &fps_str,
            "-i",
            pattern.to_str().unwrap(),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv444p",
            "-color_range",
            "pc",
            "-crf",
            &crf_str,
            "-tune",
            "stillimage",
            "-preset",
            "medium",
            output.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| VstorageError::Ffmpeg(format!("failed to run ffmpeg: {e}")))?;

    if !status.success() {
        return Err(VstorageError::Ffmpeg(format!(
            "ffmpeg exited with status {status}"
        )));
    }

    Ok(())
}

/// Extract frames from an MP4 video into numbered PNGs.
pub fn mp4_to_pngs(input: &Path, output_dir: &Path) -> Result<()> {
    let pattern = output_dir.join("frame_%06d.png");

    let status = Command::new("ffmpeg")
        .args([
            "-i",
            input.to_str().unwrap(),
            "-pix_fmt",
            "rgb24",
            "-color_range",
            "pc",
            pattern.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| VstorageError::Ffmpeg(format!("failed to run ffmpeg: {e}")))?;

    if !status.success() {
        return Err(VstorageError::Ffmpeg(format!(
            "ffmpeg exited with status {status}"
        )));
    }

    Ok(())
}
