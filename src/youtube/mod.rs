//! YouTube search via yt-dlp — songs, albums, and artists.

use serde_json::Value;
use std::process::Command;

pub const PAGE_SIZE: usize = 20;
pub const SONG_ENRICH_LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Song,
    Album,
    Artist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongConfidence {
    High,
    Medium,
    Low,
}

impl SongConfidence {
    pub fn label(self) -> &'static str {
        match self {
            SongConfidence::High => "High confidence",
            SongConfidence::Medium => "Medium confidence",
            SongConfidence::Low => "Low confidence",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeSongMetadata {
    pub track: Option<String>,
    pub artist: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_secs: Option<u32>,
    pub source: Option<String>,
    pub confidence: SongConfidence,
}

#[derive(Debug, Clone)]
pub struct YoutubeResult {
    pub title: String,
    pub url: String,
    pub kind: SearchKind,
    /// Artist name (for songs), channel (for albums), subscriber count text (for artists)
    pub subtitle: Option<String>,
    /// Track count — meaningful for albums only
    pub track_count: Option<u32>,
    /// Direct metadata enrichment for song results only
    pub song_metadata: Option<YoutubeSongMetadata>,
}

impl YoutubeResult {
    pub fn display_title(&self) -> &str {
        self.song_metadata
            .as_ref()
            .and_then(|meta| meta.track.as_deref())
            .unwrap_or(&self.title)
    }
}

// ---------------------------------------------------------------------------
// Song search
// ---------------------------------------------------------------------------

/// Search YouTube Music for individual tracks matching `query`.
/// Uses the YouTube Music search page and filters to watch URLs only,
/// which avoids the mixed results (reaction videos, compilations) from ytsearch:.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_songs(query: &str, page: usize, browser: &str) -> Result<Vec<YoutubeResult>, String> {
    let search_url = format!(
        "https://music.youtube.com/search?q={}",
        urlencoding::encode(query)
    );

    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&search_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let skip = page * PAGE_SIZE;
    let entries = root["entries"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut results = collect_song_candidates(entries, skip, PAGE_SIZE);
    enrich_song_results(&mut results, browser);

    Ok(results)
}

// ---------------------------------------------------------------------------
// Album search
// ---------------------------------------------------------------------------

/// Search YouTube Music for official album releases matching `query`.
/// Uses MPREb_* IDs which are canonical YTM album release identifiers.
/// `page` is 0-based; fetches PAGE_SIZE albums per page.
pub fn search_albums(
    query: &str,
    page: usize,
    browser: &str,
) -> Result<Vec<YoutubeResult>, String> {
    let search_url = format!(
        "https://music.youtube.com/search?q={}",
        urlencoding::encode(query)
    );

    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&search_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let skip = page * PAGE_SIZE;
    let album_urls: Vec<String> = root["entries"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| {
            let id = e["id"].as_str()?;
            if id.starts_with("MPREb_") || id.starts_with("VL") {
                Some(e["url"].as_str()?.to_string())
            } else {
                None
            }
        })
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    if album_urls.is_empty() {
        return Ok(Vec::new());
    }

    let browser = browser.to_string();
    let handles: Vec<_> = album_urls
        .into_iter()
        .map(|url| {
            let b = browser.clone();
            std::thread::spawn(move || fetch_album_details(url, &b))
        })
        .collect();

    let mut results: Vec<YoutubeResult> = handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect();

    results.sort_by_key(|a| a.title.to_lowercase());
    Ok(results)
}

fn fetch_album_details(url: String, browser: &str) -> Option<YoutubeResult> {
    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&url)
        .output()
        .ok()?;

    let v: Value = serde_json::from_slice(&output.stdout).ok()?;
    let raw_title = v["title"].as_str()?;
    let title = raw_title
        .strip_prefix("Album - ")
        .unwrap_or(raw_title)
        .to_string();

    let track_count = v["entries"].as_array().map(|e| e.len() as u32);
    let subtitle = v["channel"]
        .as_str()
        .or_else(|| v["uploader"].as_str())
        .map(|s| s.to_string());

    Some(YoutubeResult {
        title,
        url,
        kind: SearchKind::Album,
        subtitle,
        track_count,
        song_metadata: None,
    })
}

// ---------------------------------------------------------------------------
// Artist search
// ---------------------------------------------------------------------------

/// Search YouTube Music for artist channels matching `query`.
/// Returns artist pages (UC* IDs) — selecting one can browse their discography.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_artists(
    query: &str,
    page: usize,
    browser: &str,
) -> Result<Vec<YoutubeResult>, String> {
    let search_url = format!(
        "https://music.youtube.com/search?q={}",
        urlencoding::encode(query)
    );

    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&search_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let skip = page * PAGE_SIZE;
    let channel_urls: Vec<String> = root["entries"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| {
            let id = e["id"].as_str()?;
            if id.starts_with("UC") {
                Some(e["url"].as_str()?.to_string())
            } else {
                None
            }
        })
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    if channel_urls.is_empty() {
        return Ok(Vec::new());
    }

    let browser = browser.to_string();
    let handles: Vec<_> = channel_urls
        .into_iter()
        .map(|url| {
            let b = browser.clone();
            std::thread::spawn(move || fetch_artist_details(url, &b))
        })
        .collect();

    let results: Vec<YoutubeResult> = handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect();

    Ok(results)
}

fn fetch_artist_details(url: String, browser: &str) -> Option<YoutubeResult> {
    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&url)
        .output()
        .ok()?;

    let v: Value = serde_json::from_slice(&output.stdout).ok()?;
    let title = v["title"].as_str().filter(|s| !s.is_empty())?.to_string();
    let subtitle = v["description"].as_str().map(|s| {
        // Description can be long — take just the first line
        s.lines().next().unwrap_or("").trim().to_string()
    });

    Some(YoutubeResult {
        title,
        url,
        kind: SearchKind::Artist,
        subtitle,
        track_count: None,
        song_metadata: None,
    })
}

fn collect_song_candidates(entries: &[Value], skip: usize, take: usize) -> Vec<YoutubeResult> {
    entries
        .iter()
        .filter_map(song_result_from_entry)
        .skip(skip)
        .take(take)
        .collect()
}

fn song_result_from_entry(entry: &Value) -> Option<YoutubeResult> {
    let url = entry["url"].as_str()?;
    if !url.contains("watch?v=") {
        return None;
    }

    let title = entry["title"]
        .as_str()
        .filter(|s| !s.is_empty())?
        .to_string();
    let subtitle = get_string(entry, "uploader").or_else(|| get_string(entry, "channel"));
    let song_metadata = Some(build_fallback_song_metadata(entry));

    Some(YoutubeResult {
        title,
        url: url.to_string(),
        kind: SearchKind::Song,
        subtitle,
        track_count: None,
        song_metadata,
    })
}

fn build_fallback_song_metadata(entry: &Value) -> YoutubeSongMetadata {
    let source = get_string(entry, "uploader").or_else(|| get_string(entry, "channel"));
    let mut meta = YoutubeSongMetadata {
        track: None,
        artist: None,
        artists: Vec::new(),
        album: None,
        duration_secs: None,
        source,
        confidence: SongConfidence::Low,
    };
    meta.confidence = score_song_confidence(&meta);
    meta
}

fn enrich_song_results(results: &mut [YoutubeResult], browser: &str) {
    let browser = browser.to_string();
    let handles: Vec<_> = results
        .iter()
        .take(SONG_ENRICH_LIMIT)
        .enumerate()
        .map(|(idx, result)| {
            let url = result.url.clone();
            let browser = browser.clone();
            std::thread::spawn(move || (idx, fetch_song_metadata(&url, &browser)))
        })
        .collect();

    for handle in handles {
        let Ok((idx, Some(meta))) = handle.join() else {
            continue;
        };

        if let Some(result) = results.get_mut(idx) {
            apply_song_metadata(result, meta);
        }
    }
}

fn fetch_song_metadata(url: &str, browser: &str) -> Option<YoutubeSongMetadata> {
    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(url)
        .output()
        .ok()?;

    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    Some(parse_song_metadata(&value))
}

fn apply_song_metadata(result: &mut YoutubeResult, mut meta: YoutubeSongMetadata) {
    if meta.source.is_none() {
        meta.source = result.subtitle.clone();
    }
    meta.confidence = score_song_confidence(&meta);

    if let Some(track) = meta.track.clone() {
        result.title = track;
    }

    result.subtitle = meta.artist.clone().or_else(|| meta.source.clone());
    result.song_metadata = Some(meta);
}

fn parse_song_metadata(value: &Value) -> YoutubeSongMetadata {
    let mut artists = get_string_array(value, "artists");
    if artists.is_empty()
        && let Some(artist_text) = get_string(value, "artist")
    {
        artists = artist_text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
    }

    let artist = get_string(value, "artist").or_else(|| {
        if artists.is_empty() {
            None
        } else {
            Some(artists.join(", "))
        }
    });

    let mut meta = YoutubeSongMetadata {
        track: get_string(value, "track").or_else(|| get_string(value, "title")),
        artist,
        artists,
        album: get_string(value, "album"),
        duration_secs: get_u32(value, "duration"),
        source: get_string(value, "uploader").or_else(|| get_string(value, "channel")),
        confidence: SongConfidence::Low,
    };
    meta.confidence = score_song_confidence(&meta);
    meta
}

fn score_song_confidence(meta: &YoutubeSongMetadata) -> SongConfidence {
    let has_track = meta.track.as_deref().is_some_and(non_empty);
    let has_artist = meta.artist.as_deref().is_some_and(non_empty) || !meta.artists.is_empty();
    let has_album = meta.album.as_deref().is_some_and(non_empty);

    let core_field_count = [has_track, has_artist, has_album]
        .into_iter()
        .filter(|present| *present)
        .count();

    if core_field_count == 3 {
        SongConfidence::High
    } else if core_field_count >= 2 {
        SongConfidence::Medium
    } else {
        SongConfidence::Low
    }
}

fn get_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|inner| inner.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn get_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|inner| inner.as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::trim))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn get_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(|inner| {
            inner
                .as_u64()
                .or_else(|| inner.as_i64().and_then(|n| u64::try_from(n).ok()))
                .or_else(|| inner.as_f64().map(|n| n.round().max(0.0) as u64))
        })
        .and_then(|n| u32::try_from(n).ok())
}

fn non_empty(s: &str) -> bool {
    !s.trim().is_empty()
}

// ---------------------------------------------------------------------------
// Search spawning (mutates AppState, spawns background thread)
// ---------------------------------------------------------------------------

/// Start a YouTube search of the given kind at the given page.
/// Resets results when page == 0; appends when page > 0 (load-more).
pub fn spawn_youtube_search(
    app: &mut crate::app::AppState,
    query: String,
    kind: SearchKind,
    page: usize,
) {
    use crate::app::{FocusPane, StatusLevel};
    use crate::event::jobs::JobResult;

    if page == 0 {
        app.youtube.results.clear();
        app.youtube.selected = 0;
    }
    app.youtube.searching = true;
    app.youtube.search_kind = kind;
    app.youtube.page = page;
    app.youtube.query = query.clone();
    app.youtube.has_more = false;
    app.ui.focus = FocusPane::YoutubeResults;

    let label = match kind {
        SearchKind::Song => "songs",
        SearchKind::Album => "albums",
        SearchKind::Artist => "artists",
    };
    app.set_status(
        StatusLevel::Info,
        format!("Searching {label} for \"{query}\"…"),
        None,
    );

    let tx = app.channels.jobs_tx.clone();
    let browser = app.browser.clone();
    std::thread::spawn(move || {
        let result = match kind {
            SearchKind::Song => search_songs(&query, page, &browser),
            SearchKind::Album => search_albums(&query, page, &browser),
            SearchKind::Artist => search_artists(&query, page, &browser),
        };
        match result {
            Ok(results) => {
                let has_more = results.len() >= PAGE_SIZE;
                let _ = tx.send(JobResult::YoutubeSearchDone { results, has_more });
            }
            Err(e) => {
                let _ = tx.send(JobResult::YoutubeSearchFailed(e));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn song_search_filters_to_watch_urls_only() {
        let entries = json!([
            {"id": "dQw4w9WgXcQ", "title": "Never Gonna Give You Up", "url": "https://music.youtube.com/watch?v=dQw4w9WgXcQ", "uploader": "Rick Astley"},
            {"id": "UCxxx", "title": "Rick Astley", "url": "https://music.youtube.com/browse/UCxxx"},
            {"id": "MPREb_abc", "title": "Album", "url": "https://music.youtube.com/browse/MPREb_abc"},
        ]);

        let results = collect_song_candidates(entries.as_array().unwrap(), 0, PAGE_SIZE);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Never Gonna Give You Up");
        assert_eq!(results[0].subtitle.as_deref(), Some("Rick Astley"));
        assert_eq!(
            results[0]
                .song_metadata
                .as_ref()
                .map(|meta| meta.confidence),
            Some(SongConfidence::Low)
        );
    }

    #[test]
    fn page_size_is_20() {
        assert_eq!(PAGE_SIZE, 20);
    }

    #[test]
    fn song_enrich_limit_is_bounded() {
        assert_eq!(SONG_ENRICH_LIMIT, 8);
    }

    #[test]
    fn parses_official_song_metadata() {
        let value = json!({
            "title": "Spaceship",
            "track": "Spaceship",
            "artist": "Kanye West, GLC, Consequence",
            "artists": ["Kanye West", "GLC", "Consequence"],
            "album": "The College Dropout",
            "duration": 324,
            "uploader": "Kanye West",
            "channel": "Kanye West"
        });

        let meta = parse_song_metadata(&value);

        assert_eq!(meta.track.as_deref(), Some("Spaceship"));
        assert_eq!(meta.artist.as_deref(), Some("Kanye West, GLC, Consequence"));
        assert_eq!(
            meta.artists,
            vec![
                "Kanye West".to_string(),
                "GLC".to_string(),
                "Consequence".to_string()
            ]
        );
        assert_eq!(meta.album.as_deref(), Some("The College Dropout"));
        assert_eq!(meta.duration_secs, Some(324));
        assert_eq!(meta.source.as_deref(), Some("Kanye West"));
        assert_eq!(meta.confidence, SongConfidence::High);
    }

    #[test]
    fn parses_creator_upload_metadata_with_fallbacks() {
        let value = json!({
            "title": "Kanye West - Spaceship (High Quality)",
            "uploader": "Red System Of U Day",
            "channel": "Red System Of U Day",
            "duration": 325
        });

        let meta = parse_song_metadata(&value);

        assert_eq!(
            meta.track.as_deref(),
            Some("Kanye West - Spaceship (High Quality)")
        );
        assert_eq!(meta.artist, None);
        assert!(meta.artists.is_empty());
        assert_eq!(meta.album, None);
        assert_eq!(meta.duration_secs, Some(325));
        assert_eq!(meta.source.as_deref(), Some("Red System Of U Day"));
        assert_eq!(meta.confidence, SongConfidence::Low);
    }

    #[test]
    fn trust_scoring_marks_official_metadata_high() {
        let meta = YoutubeSongMetadata {
            track: Some("Spaceship".into()),
            artist: Some("Kanye West, GLC, Consequence".into()),
            artists: vec!["Kanye West".into(), "GLC".into(), "Consequence".into()],
            album: Some("The College Dropout".into()),
            duration_secs: Some(324),
            source: Some("Kanye West".into()),
            confidence: SongConfidence::Low,
        };

        assert_eq!(score_song_confidence(&meta), SongConfidence::High);
    }

    #[test]
    fn trust_scoring_marks_creator_metadata_low() {
        let meta = YoutubeSongMetadata {
            track: Some("Kanye West - Spaceship (High Quality)".into()),
            artist: None,
            artists: Vec::new(),
            album: None,
            duration_secs: Some(325),
            source: Some("Red System Of U Day".into()),
            confidence: SongConfidence::High,
        };

        assert_eq!(score_song_confidence(&meta), SongConfidence::Low);
    }
}
