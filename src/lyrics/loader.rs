use std::io;
use std::path::{Path, PathBuf};

use super::{LyricLine, parse_lrc};

/// Load lyrics for a given audio track path.
///
/// Looks for a `.lrc` file with the same base name as the audio file.
/// Returns `Ok(None)` if no lyrics are found.
pub fn load_for_track(track_path: &Path) -> io::Result<Option<Vec<LyricLine>>> {
    // Defensive: only operate on real file paths
    if track_path.as_os_str().is_empty() {
        return Ok(None);
    }

    let lrc_path = lrc_path_for_track(track_path);

    if !lrc_path.exists() {
        return Ok(None);
    }

    let lyrics = parse_lrc(&lrc_path)?;
    if lyrics.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lyrics))
    }
}

/// Derive `track.lrc` from `track.ext`
fn lrc_path_for_track(track_path: &Path) -> PathBuf {
    let mut base = track_path.to_path_buf();
    base.set_extension("lrc");
    base
}
