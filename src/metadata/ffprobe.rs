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
    let mut album_artist = None;
    let mut album = None;
    let mut duration = None;

    // New optional fields
    let mut date = None;
    let mut track = None;
    let mut purl = None;
    let mut comment = None;
    let mut synopsis = None;

    // Prefer format-level tags
    if let Some(format) = json.get("format") {
        if let Some(tags) = format.get("tags") {
            title = get_string(tags, "title");
            artist = get_string(tags, "artist");
            album_artist = get_string(tags, "album_artist")
                .or_else(|| get_string(tags, "ALBUMARTIST"))
                .or_else(|| get_string(tags, "album artist"));
            album = get_string(tags, "album");

            date = get_string(tags, "date").or_else(|| get_string(tags, "DATE"));
            track = get_string(tags, "track").or_else(|| get_string(tags, "tracknumber"));
            purl = get_string(tags, "purl");
            comment = get_string(tags, "comment");
            synopsis = get_string(tags, "synopsis");
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
            let is_attached_pic = stream
                .get("disposition")
                .and_then(|d| d.get("attached_pic"))
                .and_then(|v| v.as_i64())
                == Some(1);

            if is_attached_pic {
                continue;
            }

            if let Some(tags) = stream.get("tags") {
                if title.is_none() {
                    title = get_string(tags, "title");
                }
                if artist.is_none() {
                    artist = get_string(tags, "artist");
                }
                if album_artist.is_none() {
                    album_artist = get_string(tags, "album_artist")
                        .or_else(|| get_string(tags, "ALBUMARTIST"))
                        .or_else(|| get_string(tags, "album artist"));
                }
                if album.is_none() {
                    album = get_string(tags, "album");
                }
                if date.is_none() {
                    date = get_string(tags, "date").or_else(|| get_string(tags, "DATE"));
                }
                if track.is_none() {
                    track = get_string(tags, "track").or_else(|| get_string(tags, "tracknumber"));
                }
                if purl.is_none() {
                    purl = get_string(tags, "purl");
                }
                if comment.is_none() {
                    comment = get_string(tags, "comment");
                }
                if synopsis.is_none() {
                    synopsis = get_string(tags, "synopsis");
                }
            }
        }
    }

    let duration_secs = duration.unwrap_or(0.0);

    Some(TrackMetadata {
        title: title.unwrap_or_default(),
        artist: artist.unwrap_or_default(),
        album_artist,
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
