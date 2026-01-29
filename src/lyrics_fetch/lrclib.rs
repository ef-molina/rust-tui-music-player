use crate::metadata::model::TrackMetadata;
use serde::Deserialize;
use tracing::{debug, trace, warn};

#[derive(Debug, Deserialize)]
struct LrclibResponse {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

#[derive(Debug, Copy, Clone)]
enum FetchTier {
    ExactAlbum,
    AlbumOmitted,
    SingleCanonical,
}

/// Fetch synced lyrics from lrclib.net using a tiered identity strategy.
///
/// Tier 1: title + artist + duration + album
/// Tier 2: title + artist + duration
/// Tier 3: title + artist + duration + album = title
pub fn fetch_lrc(meta: &TrackMetadata) -> Result<Option<String>, ()> {
    let title = meta.title.trim();
    let artist = meta.artist.trim();
    let duration = meta.duration_secs.round() as u64;

    if title.is_empty() || artist.is_empty() || duration == 0 {
        warn!(
            title = %title,
            artist = %artist,
            duration,
            "Insufficient metadata for lrclib lookup"
        );
        return Ok(None);
    }

    trace!(
        title = %title,
        artist = %artist,
        album = meta.album.as_deref().unwrap_or("<none>"),
        duration,
        confidence = ?meta.confidence,
        "Starting lrclib lookup"
    );

    // ------------------------------------------------------------
    // Tier 1 — Exact album
    // ------------------------------------------------------------
    if let Some(album) = meta.album.as_deref()
        && let Some(lrc) = try_fetch(title, artist, duration, Some(album), FetchTier::ExactAlbum)?
    {
        return Ok(Some(lrc));
    }

    // ------------------------------------------------------------
    // Tier 2 — Album omitted
    // ------------------------------------------------------------
    if let Some(lrc) = try_fetch(title, artist, duration, None, FetchTier::AlbumOmitted)? {
        return Ok(Some(lrc));
    }

    // ------------------------------------------------------------
    // Tier 3 — Canonical single (album = title)
    // ------------------------------------------------------------
    if let Some(lrc) = try_fetch(
        title,
        artist,
        duration,
        Some(title),
        FetchTier::SingleCanonical,
    )? {
        return Ok(Some(lrc));
    }

    debug!("lrclib lookup exhausted — no synced lyrics found");
    Ok(None)
}

/// Perform a single lrclib identity lookup.
fn try_fetch(
    title: &str,
    artist: &str,
    duration: u64,
    album: Option<&str>,
    tier: FetchTier,
) -> Result<Option<String>, ()> {
    let mut url = format!(
        "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
        duration
    );

    if let Some(album) = album {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }

    debug!(?tier, %url, "Sending lrclib request");

    let response = match ureq::get(&url).call() {
        Ok(resp) => resp,
        Err(_) => {
            warn!(?tier, "lrclib HTTP request failed");
            return Ok(None);
        }
    };

    let body = match response.into_string() {
        Ok(body) => body,
        Err(_) => {
            warn!(?tier, "Failed to read lrclib response body");
            return Ok(None);
        }
    };

    let parsed: LrclibResponse = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(_) => {
            warn!(?tier, "Failed to parse lrclib JSON");
            return Ok(None);
        }
    };

    match parsed.synced_lyrics {
        Some(lrc) if !lrc.trim().is_empty() => {
            debug!(?tier, bytes = lrc.len(), "lrclib returned synced lyrics");
            Ok(Some(lrc))
        }
        _ => {
            debug!(?tier, "lrclib returned no synced lyrics");
            Ok(None)
        }
    }
}
