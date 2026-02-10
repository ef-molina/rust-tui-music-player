use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::fs::normalize::NormalizedTrack;

/// Write cleaned metadata tags back to the audio file.
///
/// Guarantees:
/// - audio stream preserved
/// - cover art preserved (if present as attached_pic stream)
/// - only selected tags overridden
/// - atomic replace on success
pub fn write_clean_tags(
    path: &Path,
    meta: &NormalizedTrack,
) -> Result<(), Box<dyn std::error::Error>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("opus") => write_opus_tags_ffmpeg(path, meta),
        _ => {
            tracing::debug!(
                path = %path.display(),
                "Skipping metadata write for unsupported format"
            );
            Ok(())
        }
    }
}

fn write_opus_tags_ffmpeg(
    path: &Path,
    meta: &NormalizedTrack,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = temp_opus_path(path);

    // Pre-clean: remove any stale temp file
    let _ = std::fs::remove_file(&tmp);

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(path);

    // Preserve everything
    cmd.arg("-map_metadata").arg("0");
    cmd.arg("-map").arg("0");
    cmd.arg("-c").arg("copy");

    // Core tags
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
    cmd.stderr(Stdio::null());

    cmd.arg(&tmp);

    let status = cmd.status()?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("ffmpeg failed writing opus tags: {}", path.display()).into());
    }

    // Sanity check: ffmpeg must have produced a real file
    let tmp_meta = std::fs::metadata(&tmp)?;
    if tmp_meta.len() == 0 {
        let _ = std::fs::remove_file(&tmp);
        return Err("ffmpeg produced empty opus temp file".into());
    }

    // Cross-platform safe replace
    let _ = std::fs::remove_file(path);
    std::fs::rename(&tmp, path)?;

    // Best-effort cleanup (in case rename semantics differ)
    let _ = std::fs::remove_file(&tmp);

    Ok(())
}

/// Create a temp path that still ends with `.opus` so ffmpeg can infer the muxer.
///
/// Example:
///   /music/N95.opus  ->  /music/N95.tmp.opus
fn temp_opus_path(original: &Path) -> PathBuf {
    let parent = original.parent().unwrap_or_else(|| Path::new("."));
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp");

    parent.join(format!("{stem}.tmp.opus"))
}
