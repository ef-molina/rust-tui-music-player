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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeArtistMetadata {
    pub handle: Option<String>,
    pub verified: bool,
    pub follower_count: Option<u64>,
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
    /// Channel metadata for artist results only
    pub artist_metadata: Option<YoutubeArtistMetadata>,
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
    rank_song_results(&mut results, query);

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
        artist_metadata: None,
    })
}

// ---------------------------------------------------------------------------
// Artist search
// ---------------------------------------------------------------------------

/// Search YouTube for artist channels matching `query`.
/// Uses the YouTube channel search filter for richer metadata (verification,
/// follower count, handle) than YouTube Music's UC* topic channels.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_artists(
    query: &str,
    page: usize,
    browser: &str,
) -> Result<Vec<YoutubeResult>, String> {
    let search_url = format!(
        "https://www.youtube.com/results?search_query={}&sp=EgIQAg%3D%3D",
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
    let mut results: Vec<YoutubeResult> = entries
        .iter()
        .filter_map(artist_result_from_entry)
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    rank_artist_results(&mut results);
    Ok(results)
}

fn artist_result_from_entry(entry: &Value) -> Option<YoutubeResult> {
    let title = entry["title"]
        .as_str()
        .filter(|s| !s.is_empty())?
        .to_string();
    let channel_url = get_string(entry, "channel_url").or_else(|| get_string(entry, "url"))?;
    let handle = get_string(entry, "uploader_id");
    let verified = entry["channel_is_verified"].as_bool().unwrap_or(false);
    let follower_count = get_u64(entry, "channel_follower_count");

    let subtitle = Some(format_artist_subtitle(&handle, verified, follower_count));

    Some(YoutubeResult {
        title,
        url: channel_url,
        kind: SearchKind::Artist,
        subtitle,
        track_count: None,
        song_metadata: None,
        artist_metadata: Some(YoutubeArtistMetadata {
            handle,
            verified,
            follower_count,
        }),
    })
}

fn format_artist_subtitle(
    handle: &Option<String>,
    verified: bool,
    follower_count: Option<u64>,
) -> String {
    let mut parts = Vec::new();
    if let Some(h) = handle {
        parts.push(h.clone());
    }
    if verified {
        parts.push("Verified".into());
    }
    if let Some(count) = follower_count {
        parts.push(format_follower_count(count));
    }
    if parts.is_empty() {
        "Channel".into()
    } else {
        parts.join(" · ")
    }
}

fn format_follower_count(count: u64) -> String {
    if count >= 1_000_000 {
        let millions = count as f64 / 1_000_000.0;
        format!("{millions:.1}M followers")
    } else if count >= 1_000 {
        let thousands = count as f64 / 1_000.0;
        format!("{thousands:.1}K followers")
    } else {
        format!("{count} followers")
    }
}

fn rank_artist_results(results: &mut [YoutubeResult]) {
    results.sort_by_key(|r| std::cmp::Reverse(artist_rank_key(r)));
}

fn artist_rank_key(result: &YoutubeResult) -> (u8, u8, u64) {
    let meta = result.artist_metadata.as_ref();
    let verified = meta.map(|m| m.verified).unwrap_or(false) as u8;
    let is_not_topic = (!result.title.ends_with(" - Topic")) as u8;
    let followers = meta.and_then(|m| m.follower_count).unwrap_or(0);
    (verified, is_not_topic, followers)
}

// ---------------------------------------------------------------------------
// Artist releases
// ---------------------------------------------------------------------------

/// Fetch an artist's release catalog from their YouTube channel releases tab.
/// Returns album results with OLAK5uy_* playlist URLs when available.
pub fn fetch_artist_releases(handle: &str, browser: &str) -> Result<Vec<YoutubeResult>, String> {
    let releases_url = releases_url_for_handle(handle);

    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(&releases_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let artist_name = get_string(&root, "channel").or_else(|| get_string(&root, "uploader"));

    let entries = root["entries"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let results = entries
        .iter()
        .filter_map(|e| release_entry_to_album(e, artist_name.as_deref()))
        .collect();

    Ok(results)
}

fn release_entry_to_album(entry: &Value, artist: Option<&str>) -> Option<YoutubeResult> {
    let title = entry["title"]
        .as_str()
        .filter(|s| !s.is_empty())?
        .to_string();
    let url = get_string(entry, "url")?;
    let subtitle = artist.map(|a| a.to_string());

    Some(YoutubeResult {
        title,
        url,
        kind: SearchKind::Album,
        subtitle,
        track_count: None,
        song_metadata: None,
        artist_metadata: None,
    })
}

/// Build the releases tab URL for a YouTube channel handle.
pub fn releases_url_for_handle(handle: &str) -> String {
    let bare = handle.strip_prefix('@').unwrap_or(handle);
    format!("https://www.youtube.com/@{bare}/releases")
}

// ---------------------------------------------------------------------------
// Album preview
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AlbumPreviewTrack {
    pub title: String,
    pub duration_secs: Option<u32>,
    pub video_id: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AlbumPreview {
    pub album_title: String,
    pub album_url: String,
    pub artist: Option<String>,
    pub tracks: Vec<AlbumPreviewTrack>,
}

/// Returns true when the URL points to an official OLAK5uy_* album playlist.
/// Used to gate preview behavior: only OLAK release playlist URLs from
/// `/@handle/releases` trigger preview. `:salb` results (MPREb_*, VL*) do not.
pub fn is_olak_playlist_url(url: &str) -> bool {
    url.contains("OLAK5uy_")
}

/// Fetch an album tracklist preview from an OLAK5uy_* playlist URL.
///
/// Uses `yt-dlp --flat-playlist --dump-single-json` for a single cheap request.
/// Does not full-resolve the playlist.
///
/// `album_title_hint` and `artist_hint` are carried from the selected
/// `YoutubeResult` because the OLAK flat-playlist root may have missing
/// or empty `title`/`channel`/`uploader` fields.
pub fn fetch_album_preview(
    url: &str,
    album_title_hint: Option<&str>,
    artist_hint: Option<&str>,
    browser: &str,
) -> Result<AlbumPreview, String> {
    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            browser,
        ])
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    parse_album_preview_json(&root, url, album_title_hint, artist_hint)
}

fn parse_album_preview_json(
    root: &Value,
    url: &str,
    album_title_hint: Option<&str>,
    artist_hint: Option<&str>,
) -> Result<AlbumPreview, String> {
    let album_title = album_title_hint
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| get_string(root, "title"))
        .unwrap_or_default();

    let artist = artist_hint
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| get_string(root, "channel"))
        .or_else(|| get_string(root, "uploader"));

    let entries = root["entries"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let tracks: Vec<AlbumPreviewTrack> = entries
        .iter()
        .filter_map(parse_album_preview_track)
        .collect();

    Ok(AlbumPreview {
        album_title,
        album_url: url.to_string(),
        artist,
        tracks,
    })
}

fn parse_album_preview_track(entry: &Value) -> Option<AlbumPreviewTrack> {
    let title = entry["title"]
        .as_str()
        .filter(|s| !s.is_empty())?
        .to_string();
    let duration_secs = get_u32(entry, "duration");
    let video_id = get_string(entry, "id");
    let url = get_string(entry, "url");

    Some(AlbumPreviewTrack {
        title,
        duration_secs,
        video_id,
        url,
    })
}

// ---------------------------------------------------------------------------
// Song search helpers
// ---------------------------------------------------------------------------

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
        artist_metadata: None,
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

fn rank_song_results(results: &mut [YoutubeResult], query: &str) {
    let normalized_query = normalize_search_text(query);
    let query_terms = query_terms(&normalized_query);

    results.sort_by(|left, right| {
        song_rank_key(right, &normalized_query, &query_terms).cmp(&song_rank_key(
            left,
            &normalized_query,
            &query_terms,
        ))
    });
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

fn song_rank_key(
    result: &YoutubeResult,
    normalized_query: &str,
    query_terms: &[String],
) -> (u8, u8, u8, u8, u8, u8) {
    let meta = result.song_metadata.as_ref();
    let confidence = meta
        .map(|meta| song_confidence_rank(meta.confidence))
        .unwrap_or_else(|| song_confidence_rank(SongConfidence::Low));

    (
        confidence,
        core_field_count(meta),
        song_query_relevance(result, normalized_query, query_terms),
        (!looks_like_creator_upload(meta)) as u8,
        (!looks_like_noisy_song_result(result)) as u8,
        has_sane_duration(meta) as u8,
    )
}

fn song_confidence_rank(confidence: SongConfidence) -> u8 {
    match confidence {
        SongConfidence::High => 3,
        SongConfidence::Medium => 2,
        SongConfidence::Low => 1,
    }
}

fn core_field_count(meta: Option<&YoutubeSongMetadata>) -> u8 {
    let Some(meta) = meta else {
        return 0;
    };

    let has_track = meta.track.as_deref().is_some_and(non_empty);
    let has_artist = meta.artist.as_deref().is_some_and(non_empty) || !meta.artists.is_empty();
    let has_album = meta.album.as_deref().is_some_and(non_empty);

    [has_track, has_artist, has_album]
        .into_iter()
        .filter(|present| *present)
        .count() as u8
}

fn song_query_relevance(
    result: &YoutubeResult,
    normalized_query: &str,
    query_terms: &[String],
) -> u8 {
    if normalized_query.is_empty() {
        return 0;
    }

    let Some(meta) = result.song_metadata.as_ref() else {
        return 0;
    };

    let mut haystack_parts = vec![normalize_search_text(result.display_title())];

    if let Some(track) = meta.track.as_deref() {
        haystack_parts.push(normalize_search_text(track));
    }
    if let Some(artist) = meta.artist.as_deref() {
        haystack_parts.push(normalize_search_text(artist));
    }
    if !meta.artists.is_empty() {
        haystack_parts.push(normalize_search_text(&meta.artists.join(" ")));
    }
    if let Some(album) = meta.album.as_deref() {
        haystack_parts.push(normalize_search_text(album));
    }
    if let Some(source) = meta.source.as_deref() {
        haystack_parts.push(normalize_search_text(source));
    }

    let haystack = haystack_parts.join(" ");
    let full_query_bonus = haystack.contains(normalized_query) as u8;
    let term_hits = query_terms
        .iter()
        .filter(|term| haystack.contains(*term))
        .count() as u8;

    full_query_bonus.saturating_mul(2).saturating_add(term_hits)
}

fn looks_like_creator_upload(meta: Option<&YoutubeSongMetadata>) -> bool {
    let Some(meta) = meta else {
        return true;
    };

    let has_artist = meta.artist.as_deref().is_some_and(non_empty) || !meta.artists.is_empty();
    let has_album = meta.album.as_deref().is_some_and(non_empty);
    let has_source = meta.source.as_deref().is_some_and(non_empty);

    !has_artist && !has_album && has_source
}

fn looks_like_noisy_song_result(result: &YoutubeResult) -> bool {
    let title = normalize_search_text(result.display_title());

    [
        "high quality",
        "lyrics",
        "lyric video",
        "live",
        "live version",
        "remix",
    ]
    .iter()
    .any(|marker| title.contains(marker))
}

fn has_sane_duration(meta: Option<&YoutubeSongMetadata>) -> bool {
    meta.and_then(|meta| meta.duration_secs)
        .is_some_and(|duration| (90..=900).contains(&duration))
}

fn normalize_search_text(text: &str) -> String {
    text.to_lowercase()
}

fn query_terms(normalized_query: &str) -> Vec<String> {
    normalized_query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(|term| term.to_string())
        .collect()
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

fn get_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|inner| {
        inner
            .as_u64()
            .or_else(|| inner.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| inner.as_f64().map(|n| n.round().max(0.0) as u64))
    })
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
    app.youtube.clear_album_preview();
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

    fn test_song_result(url: &str, title: &str, meta: YoutubeSongMetadata) -> YoutubeResult {
        YoutubeResult {
            title: title.to_string(),
            url: url.to_string(),
            kind: SearchKind::Song,
            subtitle: meta.artist.clone().or_else(|| meta.source.clone()),
            track_count: None,
            song_metadata: Some(meta),
            artist_metadata: None,
        }
    }

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

    #[test]
    fn ranking_prefers_high_confidence_official_over_low_confidence_creator_upload() {
        let mut results = vec![
            test_song_result(
                "https://example.com/creator",
                "Spaceship (High Quality)",
                YoutubeSongMetadata {
                    track: Some("Spaceship (High Quality)".into()),
                    artist: None,
                    artists: Vec::new(),
                    album: None,
                    duration_secs: Some(325),
                    source: Some("Red System Of U Day".into()),
                    confidence: SongConfidence::Low,
                },
            ),
            test_song_result(
                "https://example.com/official",
                "Spaceship",
                YoutubeSongMetadata {
                    track: Some("Spaceship".into()),
                    artist: Some("Kanye West, GLC, Consequence".into()),
                    artists: vec!["Kanye West".into(), "GLC".into(), "Consequence".into()],
                    album: Some("The College Dropout".into()),
                    duration_secs: Some(324),
                    source: Some("Kanye West".into()),
                    confidence: SongConfidence::High,
                },
            ),
        ];

        rank_song_results(&mut results, "spaceship kanye");

        assert_eq!(results[0].url, "https://example.com/official");
        assert_eq!(results[1].url, "https://example.com/creator");
    }

    #[test]
    fn ranking_places_medium_confidence_between_high_and_low() {
        let mut results = vec![
            test_song_result(
                "https://example.com/low",
                "Spaceship (High Quality)",
                YoutubeSongMetadata {
                    track: Some("Spaceship (High Quality)".into()),
                    artist: None,
                    artists: Vec::new(),
                    album: None,
                    duration_secs: Some(325),
                    source: Some("Red System Of U Day".into()),
                    confidence: SongConfidence::Low,
                },
            ),
            test_song_result(
                "https://example.com/high",
                "Spaceship",
                YoutubeSongMetadata {
                    track: Some("Spaceship".into()),
                    artist: Some("Kanye West".into()),
                    artists: vec!["Kanye West".into()],
                    album: Some("The College Dropout".into()),
                    duration_secs: Some(324),
                    source: Some("Kanye West".into()),
                    confidence: SongConfidence::High,
                },
            ),
            test_song_result(
                "https://example.com/medium",
                "Spaceship",
                YoutubeSongMetadata {
                    track: Some("Spaceship".into()),
                    artist: Some("Kanye West".into()),
                    artists: vec!["Kanye West".into()],
                    album: None,
                    duration_secs: Some(324),
                    source: Some("Kanye West".into()),
                    confidence: SongConfidence::Medium,
                },
            ),
        ];

        rank_song_results(&mut results, "spaceship kanye");

        assert_eq!(results[0].url, "https://example.com/high");
        assert_eq!(results[1].url, "https://example.com/medium");
        assert_eq!(results[2].url, "https://example.com/low");
    }

    #[test]
    fn ranking_keeps_low_confidence_results_present() {
        let mut results = vec![
            test_song_result(
                "https://example.com/low-a",
                "Spaceship (High Quality)",
                YoutubeSongMetadata {
                    track: Some("Spaceship (High Quality)".into()),
                    artist: None,
                    artists: Vec::new(),
                    album: None,
                    duration_secs: Some(325),
                    source: Some("Red System Of U Day".into()),
                    confidence: SongConfidence::Low,
                },
            ),
            test_song_result(
                "https://example.com/low-b",
                "Spaceship Lyrics",
                YoutubeSongMetadata {
                    track: Some("Spaceship Lyrics".into()),
                    artist: None,
                    artists: Vec::new(),
                    album: None,
                    duration_secs: Some(325),
                    source: Some("Lyrics Channel".into()),
                    confidence: SongConfidence::Low,
                },
            ),
        ];

        rank_song_results(&mut results, "spaceship kanye");

        let urls: Vec<_> = results.iter().map(|result| result.url.as_str()).collect();
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com/low-a"));
        assert!(urls.contains(&"https://example.com/low-b"));
    }

    #[test]
    fn ranking_is_stable_for_equal_scores() {
        let mut results = vec![
            test_song_result(
                "https://example.com/one",
                "Track One",
                YoutubeSongMetadata {
                    track: Some("Track One".into()),
                    artist: Some("Artist".into()),
                    artists: vec!["Artist".into()],
                    album: None,
                    duration_secs: Some(300),
                    source: Some("Artist".into()),
                    confidence: SongConfidence::Medium,
                },
            ),
            test_song_result(
                "https://example.com/two",
                "Track Two",
                YoutubeSongMetadata {
                    track: Some("Track Two".into()),
                    artist: Some("Artist".into()),
                    artists: vec!["Artist".into()],
                    album: None,
                    duration_secs: Some(300),
                    source: Some("Artist".into()),
                    confidence: SongConfidence::Medium,
                },
            ),
        ];

        rank_song_results(&mut results, "unrelated query");

        assert_eq!(results[0].url, "https://example.com/one");
        assert_eq!(results[1].url, "https://example.com/two");
    }

    #[test]
    fn query_relevance_breaks_ties_when_confidence_matches() {
        let mut results = vec![
            test_song_result(
                "https://example.com/other",
                "Spaceship",
                YoutubeSongMetadata {
                    track: Some("Spaceship".into()),
                    artist: Some("Other Artist".into()),
                    artists: vec!["Other Artist".into()],
                    album: Some("Loose Songs".into()),
                    duration_secs: Some(324),
                    source: Some("Other Artist".into()),
                    confidence: SongConfidence::Medium,
                },
            ),
            test_song_result(
                "https://example.com/kanye",
                "Spaceship",
                YoutubeSongMetadata {
                    track: Some("Spaceship".into()),
                    artist: Some("Kanye West".into()),
                    artists: vec!["Kanye West".into()],
                    album: Some("Loose Songs".into()),
                    duration_secs: Some(324),
                    source: Some("Kanye West".into()),
                    confidence: SongConfidence::Medium,
                },
            ),
        ];

        rank_song_results(&mut results, "spaceship kanye");

        assert_eq!(results[0].url, "https://example.com/kanye");
        assert_eq!(results[1].url, "https://example.com/other");
    }

    // ---------------------------------------------------------------
    // Artist search result parsing and ranking tests
    // ---------------------------------------------------------------

    #[test]
    fn parses_verified_channel_result() {
        let entry = json!({
            "title": "Kanye West",
            "channel_url": "https://www.youtube.com/channel/UCs6eXM7s8Vl5WcECcRHc2qQ",
            "uploader_id": "@kanyewest",
            "channel_is_verified": true,
            "channel_follower_count": 12500000
        });

        let result = artist_result_from_entry(&entry).unwrap();
        assert_eq!(result.title, "Kanye West");
        assert_eq!(result.kind, SearchKind::Artist);

        let meta = result.artist_metadata.as_ref().unwrap();
        assert_eq!(meta.handle.as_deref(), Some("@kanyewest"));
        assert!(meta.verified);
        assert_eq!(meta.follower_count, Some(12_500_000));
    }

    #[test]
    fn parses_topic_channel_result() {
        let entry = json!({
            "title": "Kanye West - Topic",
            "channel_url": "https://www.youtube.com/channel/UCRY5dYsbIN5TylSbd7gVnZg",
            "channel_is_verified": true,
            "channel_follower_count": 559
        });

        let result = artist_result_from_entry(&entry).unwrap();
        assert_eq!(result.title, "Kanye West - Topic");

        let meta = result.artist_metadata.as_ref().unwrap();
        assert!(meta.handle.is_none());
        assert!(meta.verified);
        assert_eq!(meta.follower_count, Some(559));
    }

    #[test]
    fn parses_unverified_channel_with_handle() {
        let entry = json!({
            "title": "Kanye Archival Project",
            "channel_url": "https://www.youtube.com/channel/UCf19qC07K5EzjIOieNgVoxQ",
            "uploader_id": "@KanyeArchivalProject",
            "channel_follower_count": 2580
        });

        let result = artist_result_from_entry(&entry).unwrap();
        assert_eq!(result.title, "Kanye Archival Project");

        let meta = result.artist_metadata.as_ref().unwrap();
        assert_eq!(meta.handle.as_deref(), Some("@KanyeArchivalProject"));
        assert!(!meta.verified);
        assert_eq!(meta.follower_count, Some(2580));
    }

    #[test]
    fn verified_channel_ranks_above_topic() {
        let mut results = vec![
            YoutubeResult {
                title: "Kanye West - Topic".into(),
                url: "https://example.com/topic".into(),
                kind: SearchKind::Artist,
                subtitle: None,
                track_count: None,
                song_metadata: None,
                artist_metadata: Some(YoutubeArtistMetadata {
                    handle: None,
                    verified: true,
                    follower_count: Some(559),
                }),
            },
            YoutubeResult {
                title: "Kanye West".into(),
                url: "https://example.com/official".into(),
                kind: SearchKind::Artist,
                subtitle: None,
                track_count: None,
                song_metadata: None,
                artist_metadata: Some(YoutubeArtistMetadata {
                    handle: Some("@kanyewest".into()),
                    verified: true,
                    follower_count: Some(12_500_000),
                }),
            },
        ];

        rank_artist_results(&mut results);

        assert_eq!(results[0].title, "Kanye West");
        assert_eq!(results[1].title, "Kanye West - Topic");
    }

    #[test]
    fn verified_ranks_above_unverified() {
        let mut results = vec![
            YoutubeResult {
                title: "Fake Kanye".into(),
                url: "https://example.com/fake".into(),
                kind: SearchKind::Artist,
                subtitle: None,
                track_count: None,
                song_metadata: None,
                artist_metadata: Some(YoutubeArtistMetadata {
                    handle: Some("@fakekanye".into()),
                    verified: false,
                    follower_count: Some(1000),
                }),
            },
            YoutubeResult {
                title: "Kanye West".into(),
                url: "https://example.com/official".into(),
                kind: SearchKind::Artist,
                subtitle: None,
                track_count: None,
                song_metadata: None,
                artist_metadata: Some(YoutubeArtistMetadata {
                    handle: Some("@kanyewest".into()),
                    verified: true,
                    follower_count: Some(12_500_000),
                }),
            },
        ];

        rank_artist_results(&mut results);

        assert_eq!(results[0].title, "Kanye West");
        assert_eq!(results[1].title, "Fake Kanye");
    }

    #[test]
    fn format_follower_count_millions() {
        assert_eq!(format_follower_count(12_500_000), "12.5M followers");
    }

    #[test]
    fn format_follower_count_thousands() {
        assert_eq!(format_follower_count(2_580), "2.6K followers");
    }

    #[test]
    fn format_follower_count_small() {
        assert_eq!(format_follower_count(559), "559 followers");
    }

    #[test]
    fn artist_subtitle_includes_handle_verified_and_followers() {
        let subtitle = format_artist_subtitle(&Some("@kanyewest".into()), true, Some(12_500_000));
        assert_eq!(subtitle, "@kanyewest · Verified · 12.5M followers");
    }

    #[test]
    fn artist_subtitle_without_handle_or_verification() {
        let subtitle = format_artist_subtitle(&None, false, Some(559));
        assert_eq!(subtitle, "559 followers");
    }

    #[test]
    fn artist_subtitle_empty_fallback() {
        let subtitle = format_artist_subtitle(&None, false, None);
        assert_eq!(subtitle, "Channel");
    }

    // ---------------------------------------------------------------
    // Artist releases tests
    // ---------------------------------------------------------------

    #[test]
    fn releases_url_from_handle_with_at() {
        assert_eq!(
            releases_url_for_handle("@kanyewest"),
            "https://www.youtube.com/@kanyewest/releases"
        );
    }

    #[test]
    fn releases_url_from_handle_without_at() {
        assert_eq!(
            releases_url_for_handle("kanyewest"),
            "https://www.youtube.com/@kanyewest/releases"
        );
    }

    #[test]
    fn parses_release_entry_with_olak_id() {
        let entry = json!({
            "id": "OLAK5uy_l139P2p521JCZVZX8S_PuGFUKyD1brXWY",
            "title": "The College Dropout",
            "url": "https://www.youtube.com/playlist?list=OLAK5uy_l139P2p521JCZVZX8S_PuGFUKyD1brXWY",
            "ie_key": "YoutubeTab"
        });

        let result = release_entry_to_album(&entry, Some("Kanye West")).unwrap();
        assert_eq!(result.title, "The College Dropout");
        assert_eq!(result.kind, SearchKind::Album);
        assert_eq!(result.subtitle.as_deref(), Some("Kanye West"));
        assert!(result.url.contains("OLAK5uy_"));
    }

    #[test]
    fn release_entry_without_title_is_skipped() {
        let entry = json!({
            "id": "OLAK5uy_l139P2p521JCZVZX8S_PuGFUKyD1brXWY",
            "url": "https://www.youtube.com/playlist?list=OLAK5uy_abc"
        });

        assert!(release_entry_to_album(&entry, Some("Kanye West")).is_none());
    }

    #[test]
    fn release_entry_without_url_is_skipped() {
        let entry = json!({
            "title": "The College Dropout"
        });

        assert!(release_entry_to_album(&entry, Some("Kanye West")).is_none());
    }

    #[test]
    fn release_entry_without_artist_has_no_subtitle() {
        let entry = json!({
            "title": "The College Dropout",
            "url": "https://www.youtube.com/playlist?list=OLAK5uy_abc"
        });

        let result = release_entry_to_album(&entry, None).unwrap();
        assert_eq!(result.title, "The College Dropout");
        assert!(result.subtitle.is_none());
    }

    // ---------------------------------------------------------------
    // Album preview parsing tests
    // ---------------------------------------------------------------

    #[test]
    fn parses_album_preview_track_with_title_duration_id_and_url() {
        let entry = json!({
            "title": "Donda Chant",
            "duration": 53.0,
            "id": "J8k-73s2fHw",
            "url": "https://music.youtube.com/watch?v=J8k-73s2fHw"
        });

        let track = parse_album_preview_track(&entry).unwrap();
        assert_eq!(track.title, "Donda Chant");
        assert_eq!(track.duration_secs, Some(53));
        assert_eq!(track.video_id.as_deref(), Some("J8k-73s2fHw"));
        assert_eq!(
            track.url.as_deref(),
            Some("https://music.youtube.com/watch?v=J8k-73s2fHw")
        );
    }

    #[test]
    fn parses_album_preview_track_without_duration() {
        let entry = json!({
            "title": "Jail",
            "id": "abc123"
        });

        let track = parse_album_preview_track(&entry).unwrap();
        assert_eq!(track.title, "Jail");
        assert!(track.duration_secs.is_none());
        assert_eq!(track.video_id.as_deref(), Some("abc123"));
        assert!(track.url.is_none());
    }

    #[test]
    fn album_preview_track_without_title_is_skipped() {
        let entry = json!({
            "duration": 120.0,
            "id": "abc123"
        });

        assert!(parse_album_preview_track(&entry).is_none());
    }

    #[test]
    fn album_preview_track_with_empty_title_is_skipped() {
        let entry = json!({
            "title": "",
            "duration": 120.0
        });

        assert!(parse_album_preview_track(&entry).is_none());
    }

    #[test]
    fn album_preview_prefers_title_and_artist_hints() {
        let root = json!({
            "title": "Root Title",
            "channel": "Root Channel",
            "entries": [
                {"title": "Track 1", "duration": 180.0, "id": "v1"},
                {"title": "Track 2", "duration": 240.0, "id": "v2"}
            ]
        });

        let preview = parse_album_preview_json(
            &root,
            "https://www.youtube.com/playlist?list=OLAK5uy_abc",
            Some("The College Dropout"),
            Some("Kanye West"),
        )
        .unwrap();

        assert_eq!(preview.album_title, "The College Dropout");
        assert_eq!(preview.artist.as_deref(), Some("Kanye West"));
        assert_eq!(preview.tracks.len(), 2);
        assert_eq!(preview.tracks[0].title, "Track 1");
        assert_eq!(preview.tracks[0].duration_secs, Some(180));
        assert_eq!(preview.tracks[1].title, "Track 2");
        assert_eq!(
            preview.album_url,
            "https://www.youtube.com/playlist?list=OLAK5uy_abc"
        );
    }

    #[test]
    fn album_preview_falls_back_to_root_title_when_hint_missing() {
        let root = json!({
            "title": "Donda",
            "channel": "Kanye West",
            "entries": [
                {"title": "Donda Chant", "duration": 53.0, "id": "v1"}
            ]
        });

        let preview = parse_album_preview_json(
            &root,
            "https://www.youtube.com/playlist?list=OLAK5uy_xyz",
            None,
            None,
        )
        .unwrap();

        assert_eq!(preview.album_title, "Donda");
        assert_eq!(preview.artist.as_deref(), Some("Kanye West"));
        assert_eq!(preview.tracks.len(), 1);
    }

    #[test]
    fn detects_olak_playlist_url() {
        assert!(is_olak_playlist_url(
            "https://www.youtube.com/playlist?list=OLAK5uy_l139P2p521JCZVZX8S_PuGFUKyD1brXWY"
        ));
        assert!(is_olak_playlist_url(
            "https://www.youtube.com/playlist?list=OLAK5uy_abc"
        ));
    }

    #[test]
    fn rejects_non_olak_playlist_url_for_preview() {
        assert!(!is_olak_playlist_url(
            "https://music.youtube.com/browse/MPREb_KCNeTnK02S7"
        ));
        assert!(!is_olak_playlist_url(
            "https://music.youtube.com/browse/VLPLEbPTbJsrTelVjk5XOA9b8ihNV9WQVvh8"
        ));
        assert!(!is_olak_playlist_url(
            "https://www.youtube.com/playlist?list=PLGeJR8ZOrTZfWkXN2AD"
        ));
        assert!(!is_olak_playlist_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
    }
}
