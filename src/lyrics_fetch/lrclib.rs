use crate::metadata::model::TrackMetadata;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LrclibResponse {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
}

/// Fetch synced lyrics from lrclib.net.
/// Returns raw LRC text if found.
pub fn fetch_synced_lyrics(meta: &TrackMetadata) -> Option<String> {
    // Respect metadata guarantees
    let title: &str = &meta.title;
    let artist: &str = &meta.artist;
    let duration: f64 = meta.duration_secs;

    let mut url = String::from("https://lrclib.net/api/get?");
    url.push_str(&format!(
        "track_name={}&artist_name={}&duration={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
        duration.round() as u64
    ));

    // Album is optional
    if let Some(album) = meta.album.as_deref() {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }

    let response = ureq::get(&url).call().ok()?;
    let body = response.into_string().ok()?;
    let parsed: LrclibResponse = serde_json::from_str(&body).ok()?;

    parsed.synced_lyrics
}
