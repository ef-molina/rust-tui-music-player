use std::path::Path;
use std::process::Command;

use crate::fs::normalize::NormalizedTrack;

/// Write cleaned metadata tags back to the audio file.
///
/// This function is deterministic and idempotent.
pub fn write_clean_tags(path: &Path, track: &NormalizedTrack) -> std::io::Result<()> {
    let mut cmd = Command::new("exiftool");

    // Overwrite in-place, no backup files
    cmd.arg("-overwrite_original");

    cmd.arg(format!("-Title={}", track.title));
    cmd.arg(format!("-Artist={}", track.artist));

    if let Some(album) = &track.album {
        cmd.arg(format!("-Album={}", album));
    }

    if let Some(year) = track.year {
        cmd.arg(format!("-Date={}", year));
    }

    if let Some(n) = track.track_number {
        cmd.arg(format!("-TrackNumber={}", n));
    }

    // Store YouTube ID in a stable, searchable way
    cmd.arg(format!("-Comment=yt:{}", track.youtube_id));

    cmd.arg(path);

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}
