//! YouTube search via yt-dlp — songs, albums, and artists.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub const PAGE_SIZE: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Song,
    Album,
    Artist,
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
}

// ---------------------------------------------------------------------------
// Song search
// ---------------------------------------------------------------------------

/// Search YouTube Music for individual tracks matching `query`.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_songs(query: &str, page: usize) -> Result<Vec<YoutubeResult>, String> {
    let total = (page + 1) * PAGE_SIZE;
    let search_term = format!("ytsearch{}:{}", total, query);

    let mut child = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--dump-json",
            "--yes-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            "brave",
            &search_term,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let stdout = child.stdout.take().expect("stdout piped");
    let reader = BufReader::new(stdout);
    let skip = page * PAGE_SIZE;

    let results: Vec<YoutubeResult> = reader
        .lines()
        .filter_map(|l| l.ok())
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| parse_song_entry(&l))
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    let _ = child.wait();
    Ok(results)
}

fn parse_song_entry(json: &str) -> Option<YoutubeResult> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let id = v["id"].as_str()?;
    let title = v["title"].as_str().unwrap_or("Unknown").to_string();
    let subtitle = v["uploader"]
        .as_str()
        .or_else(|| v["channel"].as_str())
        .map(|s| s.to_string());

    let url = v["url"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));

    Some(YoutubeResult {
        title,
        url,
        kind: SearchKind::Song,
        subtitle,
        track_count: None,
    })
}

// ---------------------------------------------------------------------------
// Album search
// ---------------------------------------------------------------------------

/// Search YouTube Music for official album releases matching `query`.
/// Uses MPREb_* IDs which are canonical YTM album release identifiers.
/// `page` is 0-based; fetches PAGE_SIZE albums per page.
pub fn search_albums(query: &str, page: usize) -> Result<Vec<YoutubeResult>, String> {
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
            "brave",
        ])
        .arg(&search_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let skip = page * PAGE_SIZE;
    let album_urls: Vec<String> = root["entries"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| {
            let id = e["id"].as_str()?;
            if id.starts_with("MPREb_") {
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

    let handles: Vec<_> = album_urls
        .into_iter()
        .map(|url| std::thread::spawn(move || fetch_album_details(url)))
        .collect();

    let mut results: Vec<YoutubeResult> = handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect();

    results.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    Ok(results)
}

fn fetch_album_details(url: String) -> Option<YoutubeResult> {
    let output = Command::new("yt-dlp")
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--quiet",
            "--cookies-from-browser",
            "brave",
        ])
        .arg(&url)
        .output()
        .ok()?;

    let v: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
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
    })
}

// ---------------------------------------------------------------------------
// Artist search
// ---------------------------------------------------------------------------

/// Search YouTube Music for artist channels matching `query`.
/// Returns artist pages (UC* IDs) — selecting one can browse their discography.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_artists(query: &str, page: usize) -> Result<Vec<YoutubeResult>, String> {
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
            "brave",
        ])
        .arg(&search_url)
        .output()
        .map_err(|e| format!("Failed to spawn yt-dlp: {e}"))?;

    let root: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse yt-dlp output: {e}"))?;

    let skip = page * PAGE_SIZE;
    let results: Vec<YoutubeResult> = root["entries"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| {
            let id = e["id"].as_str()?;
            // UC* = YouTube channel/artist page
            if !id.starts_with("UC") {
                return None;
            }
            let title = e["title"].as_str().unwrap_or("Unknown Artist").to_string();
            let url = e["url"].as_str()?.to_string();
            Some(YoutubeResult {
                title,
                url,
                kind: SearchKind::Artist,
                subtitle: None,
                track_count: None,
            })
        })
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_song_entry_with_explicit_url() {
        let json = r#"{"id":"dQw4w9WgXcQ","title":"Never Gonna Give You Up","url":"https://www.youtube.com/watch?v=dQw4w9WgXcQ","uploader":"Rick Astley"}"#;
        let result = parse_song_entry(json).expect("should parse");
        assert_eq!(result.title, "Never Gonna Give You Up");
        assert_eq!(result.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(result.subtitle.as_deref(), Some("Rick Astley"));
        assert_eq!(result.kind, SearchKind::Song);
    }

    #[test]
    fn parse_song_entry_constructs_watch_url_from_id() {
        let json = r#"{"id":"dQw4w9WgXcQ","title":"Never Gonna Give You Up"}"#;
        let result = parse_song_entry(json).expect("should parse");
        assert_eq!(result.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    }

    #[test]
    fn parse_song_entry_returns_none_for_missing_id() {
        let json = r#"{"title":"No ID here"}"#;
        assert!(parse_song_entry(json).is_none());
    }

    #[test]
    fn parse_song_entry_returns_none_for_invalid_json() {
        assert!(parse_song_entry("not json").is_none());
    }

    #[test]
    fn page_size_is_20() {
        assert_eq!(PAGE_SIZE, 20);
    }
}
