use std::io;
use std::path::{Path, PathBuf};

use super::{LyricLine, parse_lrc};
use crate::metadata::model::TrackMetadata;

/// Load lyrics for a given audio track path.
///
/// Order:
/// 1. Try local `.lrc`
///
/// This function is intentionally:
/// - synchronous
/// - non-blocking
/// - network-free
pub fn load_for_track(
    track_path: &Path,
    metadata: &TrackMetadata,
) -> io::Result<Option<Vec<LyricLine>>> {
    if track_path.as_os_str().is_empty() {
        return Ok(None);
    }

    // Metadata gate (cheap and fast)
    if !metadata.is_complete() {
        return Ok(None);
    }

    let lrc_path = lrc_path_for_track(track_path);

    if !lrc_path.exists() {
        return Ok(None);
    }

    let lyrics = parse_lrc(&lrc_path)?;
    Ok(if lyrics.is_empty() {
        None
    } else {
        Some(lyrics)
    })
}

/// Derive `track.lrc` from `track.ext`
fn lrc_path_for_track(track_path: &Path) -> PathBuf {
    let mut base = track_path.to_path_buf();
    base.set_extension("lrc");
    base
}
