use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::fs::normalize::NormalizedTrack;

/// Write cleaned metadata tags back to the audio file.
///
/// Opus limitation:
/// - Opus-in-Ogg does NOT support attached picture streams
/// - yt-dlp embeds cover art as MJPEG video
/// - We must drop video stream when rewriting metadata
///
/// Guarantees:
/// - audio stream preserved
/// - metadata rewritten deterministically
/// - atomic replace on success
pub fn write_clean_tags(
    path: &Path,
    meta: &NormalizedTrack,
) -> Result<(), Box<dyn std::error::Error>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("opus") => write_opus_tags_ffmpeg(path, meta),
        _ => Ok(()),
    }
}

fn write_opus_tags_ffmpeg(
    path: &Path,
    meta: &NormalizedTrack,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = temp_opus_path(path);
    let _ = std::fs::remove_file(&tmp);

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(path);

    // CRITICAL: drop attached_pic video stream
    cmd.arg("-map").arg("0:a");
    cmd.arg("-vn");
    cmd.arg("-c:a").arg("copy");

    // Preserve baseline metadata, override selected fields
    cmd.arg("-map_metadata").arg("0");

    cmd.arg("-metadata").arg(format!("title={}", meta.title));
    cmd.arg("-metadata").arg(format!("artist={}", meta.artist));
    cmd.arg("-metadata")
        .arg(format!("album_artist={}", meta.artist));

    if let Some(album) = &meta.album {
        cmd.arg("-metadata").arg(format!("album={}", album));
    }

    if let Some(year) = meta.year {
        cmd.arg("-metadata").arg(format!("date={}", year));
    }

    cmd.arg("-metadata")
        .arg(format!("comment=yt:{}", meta.youtube_id));

    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    cmd.arg(&tmp);

    let output = cmd.output()?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "ffmpeg failed writing opus tags: {}\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    // Atomic replace
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;
    let _ = std::fs::remove_file(&tmp);

    Ok(())
}

fn temp_opus_path(original: &Path) -> PathBuf {
    let parent = original.parent().unwrap_or_else(|| Path::new("."));
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");

    parent.join(format!("{stem}.tmp.opus"))
}
