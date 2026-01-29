pub mod exiftool;
pub mod ffprobe;
pub mod model;

use std::path::Path;

use model::{MetadataConfidence, TrackMetadata};

/// Extract metadata using ffprobe with an exiftool fallback.
/// This function performs no filesystem heuristics.
pub fn extract_metadata(path: &Path) -> Option<TrackMetadata> {
    let mut meta = ffprobe::extract(path)?;

    let needs_fallback = !meta.is_complete();

    if needs_fallback && let Some(extra) = exiftool::extract(path) {
        merge_metadata(&mut meta, extra);
    }

    meta.confidence = determine_confidence(&meta);

    Some(meta)
}

fn merge_metadata(base: &mut TrackMetadata, extra: TrackMetadata) {
    if base.title.is_empty() {
        base.title = extra.title;
    }

    if base.artist.is_empty() {
        base.artist = extra.artist;
    }

    if base.album.is_none() {
        base.album = extra.album;
    }

    if base.duration_secs <= 0.0 && extra.duration_secs > 0.0 {
        base.duration_secs = extra.duration_secs;
    }
}

fn determine_confidence(meta: &TrackMetadata) -> MetadataConfidence {
    if meta.is_complete() {
        MetadataConfidence::Exact
    } else {
        MetadataConfidence::FilenameOnly
    }
}
