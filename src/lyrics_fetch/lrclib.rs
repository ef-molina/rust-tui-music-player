use crate::metadata::model::TrackMetadata;
use serde::Deserialize;

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

    let mut url = String::from("https://lrclib.net/api/get?");
    url.push_str(&format!(
        "track_name={}&artist_name={}&duration={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
        duration.round() as u64
    ));

    if let Some(album) = meta.album.as_deref() {
        url.push_str(&format!("&album_name={}", urlencoding::encode(album)));
    }

    let response = ureq::get(&url).call().map_err(|_| ())?;
    let body = response.into_string().map_err(|_| ())?;
    let parsed: LrclibResponse = serde_json::from_str(&body).map_err(|_| ())?;

    Ok(parsed.synced_lyrics)
}
