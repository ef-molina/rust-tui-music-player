use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::model::{MetadataConfidence, TrackMetadata};

pub fn extract(path: &Path) -> Option<TrackMetadata> {
    let output = Command::new("exiftool").arg("-j").arg(path).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;
    let entry = json.as_array()?.first()?;

    let title = get_string(entry, "Title").unwrap_or_default();
    let artist = get_string(entry, "Artist").unwrap_or_default();
    let album = get_string(entry, "Album");

    let duration_secs = entry
        .get("Duration")
        .and_then(|d| d.as_f64())
        .unwrap_or(0.0);

    // Optional fields
    let date = get_string(entry, "Date").or_else(|| get_string(entry, "Year"));
    let track = get_string(entry, "Track").or_else(|| get_string(entry, "TrackNumber"));
    let purl = get_string(entry, "PURL").or_else(|| get_string(entry, "purl"));
    let comment = get_string(entry, "Comment");
    let synopsis = get_string(entry, "Synopsis");

    Some(TrackMetadata {
        title,
        artist,
        album_artist: None,
        album,
        duration_secs,

        date,
        track,
        purl,
        comment,
        synopsis,

        // Confidence assigned later by orchestrator
        confidence: MetadataConfidence::FilenameOnly,
    })
}

fn get_string(map: &Value, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
