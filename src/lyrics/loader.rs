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

// ==============================================================
// Inline Unit Tests
// ==============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::model::MetadataConfidence;
    use std::fs;
    use tempfile::tempdir;

    fn complete_metadata() -> TrackMetadata {
        TrackMetadata {
            title: "Song".into(),
            artist: "Artist".into(),
            album: Some("Album".into()),
            duration_secs: 10.0,
            confidence: MetadataConfidence::Exact,
            date: None,
            track: None,
            purl: None,
            comment: None,
            synopsis: None,
        }
    }

    fn incomplete_metadata() -> TrackMetadata {
        TrackMetadata {
            title: "".into(),
            artist: "".into(),
            album: None,
            duration_secs: 0.0,
            confidence: MetadataConfidence::FilenameOnly,
            date: None,
            track: None,
            purl: None,
            comment: None,
            synopsis: None,
        }
    }

    #[test]
    fn skips_loading_when_metadata_incomplete() {
        let dir = tempdir().unwrap();
        let track = dir.path().join("song.mp3");
        fs::write(&track, b"fake audio").unwrap();

        let result = load_for_track(&track, &incomplete_metadata()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_when_lrc_missing() {
        let dir = tempdir().unwrap();
        let track = dir.path().join("song.mp3");
        fs::write(&track, b"fake audio").unwrap();

        let result = load_for_track(&track, &complete_metadata()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn loads_valid_lrc_file() {
        let dir = tempdir().unwrap();
        let track = dir.path().join("song.mp3");
        let lrc = dir.path().join("song.lrc");

        fs::write(&track, b"fake audio").unwrap();
        fs::write(&lrc, "[00:01.00]Hello\n").unwrap();

        let lines = load_for_track(&track, &complete_metadata())
            .unwrap()
            .expect("lyrics should load");

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello");
    }

    #[test]
    fn empty_lrc_file_returns_none() {
        let dir = tempdir().unwrap();
        let track = dir.path().join("song.mp3");
        let lrc = dir.path().join("song.lrc");

        fs::write(&track, b"fake audio").unwrap();
        fs::write(&lrc, "").unwrap();

        let result = load_for_track(&track, &complete_metadata()).unwrap();
        assert!(result.is_none());
    }
}
