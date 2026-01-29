use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::lyrics_fetch::lrclib::fetch_synced_lyrics;
use crate::metadata::model::TrackMetadata;

use super::{LyricLine, parse_lrc};

/// Load lyrics for a given audio track path.
///
/// Order:
/// 1. Try local `.lrc`
/// 2. Fetch from lrclib if missing
/// 3. Write `.lrc` to disk
/// 4. Parse and return
pub fn load_for_track(
    track_path: &Path,
    metadata: &TrackMetadata,
) -> io::Result<Option<Vec<LyricLine>>> {
    if track_path.as_os_str().is_empty() {
        return Ok(None);
    }

    let lrc_path = lrc_path_for_track(track_path);

    // --- Step 1: Local lyrics ---
    if lrc_path.exists() {
        let lyrics = parse_lrc(&lrc_path)?;
        return Ok(if lyrics.is_empty() {
            None
        } else {
            Some(lyrics)
        });
    }

    // --- Step 2: Metadata gate ---
    if !metadata.is_complete() {
        return Ok(None);
    }

    // --- Step 3: Fetch from lrclib ---
    let Some(lrc_text) = fetch_synced_lyrics(metadata) else {
        return Ok(None);
    };

    // --- Step 4: Write atomically ---
    write_lrc_atomic(&lrc_path, &lrc_text)?;

    // --- Step 5: Parse written file ---
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

/// Write `.lrc` safely using a temp file + rename
fn write_lrc_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let tmp_path = path.with_extension("lrc.tmp");
    fs::write(&tmp_path, contents)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}
