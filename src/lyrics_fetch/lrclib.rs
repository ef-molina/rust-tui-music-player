use crate::metadata::model::TrackMetadata;
use serde::Deserialize;
use tracing::{debug, trace, warn};

#[derive(Debug, Deserialize)]
struct LrclibResponse {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

/// Fetch synced lyrics from lrclib.net.
/// Returns raw LRC text if found.
pub fn fetch_lrc(meta: &TrackMetadata) -> Result<Option<String>, ()> {
    let title = &meta.title;
    let artist = &meta.artist;
    let duration = meta.duration_secs;
    let rounded_duration = duration.round() as u64;

    trace!(
        title = %title,
        artist = %artist,
        album = meta.album.as_deref().unwrap_or("<none>"),
        duration_raw = duration,
        duration_rounded = rounded_duration,
        confidence = ?meta.confidence,
        "Preparing lrclib request"
    );

    if artist.trim().is_empty() {
        warn!(
            title = %title,
            "Artist is empty — lrclib lookup likely to fail"
        );
    }

    if rounded_duration == 0 {
        warn!(
            title = %title,
            "Track duration rounded to 0 — invalid for lrclib"
        );
    }

    let mut url = String::from("https://lrclib.net/api/get?");
    url.push_str(&format!(
        "track_name={}&artist_name={}&duration={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
        rounded_duration
    ));

    if let Some(album) = meta.album.as_deref() {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }

    debug!(%url, "Sending lrclib HTTP request");

    let response = ureq::get(&url).call().map_err(|_| {
        warn!("lrclib HTTP request failed");
        ()
    })?;

    let body = response.into_string().map_err(|_| {
        warn!("Failed to read lrclib response body");
        ()
    })?;

    let parsed: LrclibResponse = serde_json::from_str(&body).map_err(|_| {
        warn!("Failed to parse lrclib JSON response");
        ()
    })?;

    match &parsed.synced_lyrics {
        Some(lrc) => {
            debug!(bytes = lrc.len(), "lrclib returned synced lyrics");
        }
        None => {
            debug!("lrclib returned no synced lyrics");
        }
    }

    Ok(parsed.synced_lyrics)
}
