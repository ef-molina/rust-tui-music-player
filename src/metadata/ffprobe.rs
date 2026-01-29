use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::model::{MetadataConfidence, TrackMetadata};

pub fn extract(path: &Path) -> Option<TrackMetadata> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;

    let mut title = None;
    let mut artist = None;
    let mut album = None;
    let mut duration = None;

    // Prefer format-level tags
    if let Some(format) = json.get("format") {
        if let Some(tags) = format.get("tags") {
            title = get_string(tags, "title");
            artist = get_string(tags, "artist");
            album = get_string(tags, "album");
        }

        duration = format
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|s| s.parse::<f64>().ok());
    }

    // Fall back to stream-level tags only if needed
    if (title.is_none() || artist.is_none() || album.is_none())
        && let Some(streams) = json.get("streams").and_then(|s| s.as_array())
    {
        for stream in streams {
            if let Some(tags) = stream.get("tags") {
                if title.is_none() {
                    title = get_string(tags, "title");
                }
                if artist.is_none() {
                    artist = get_string(tags, "artist");
                }
                if album.is_none() {
                    album = get_string(tags, "album");
                }
            }
        }
    }

    let duration_secs = duration.unwrap_or(0.0);

    Some(TrackMetadata {
        title: title.unwrap_or_default(),
        artist: artist.unwrap_or_default(),
        album,
        duration_secs,
        // Confidence is assigned later by the orchestrator
        confidence: MetadataConfidence::FilenameOnly,
    })
}

fn get_string(map: &Value, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
