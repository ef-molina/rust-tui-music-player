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
/// Uses the YouTube Music search page and filters to watch URLs only,
/// which avoids the mixed results (reaction videos, compilations) from ytsearch:.
/// `page` is 0-based; fetches PAGE_SIZE results per page.
pub fn search_songs(query: &str, page: usize) -> Result<Vec<YoutubeResult>, String> {
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
            let url = e["url"].as_str()?;
            // Keep only individual track URLs — skip channels (UC*) and album releases (MPREb_*)
            if !url.contains("watch?v=") {
                return None;
            }
            let title = e["title"].as_str().filter(|s| !s.is_empty())?.to_string();
            let subtitle = e["uploader"]
                .as_str()
                .or_else(|| e["channel"].as_str())
                .map(|s| s.to_string());
            Some(YoutubeResult {
                title,
                url: url.to_string(),
                kind: SearchKind::Song,
                subtitle,
                track_count: None,
            })
        })
        .skip(skip)
        .take(PAGE_SIZE)
        .collect();

    Ok(results)
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
    // Collect both official releases (MPREb_*) and playlists (VL*).
    // MPREb_ = canonical YTM album releases; VL* = playlists that are often albums.
    // Both require a second fetch to resolve the title.
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

    // Fetch each channel page in parallel to resolve the artist name
    let handles: Vec<_> = channel_urls
        .into_iter()
        .map(|url| std::thread::spawn(move || fetch_artist_details(url)))
        .collect();

    let results: Vec<YoutubeResult> = handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect();

    Ok(results)
}

fn fetch_artist_details(url: String) -> Option<YoutubeResult> {
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
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_search_filters_to_watch_urls_only() {
        // Simulate the entries array from a YTM search page
        let entries = serde_json::json!([
            {"id": "dQw4w9WgXcQ", "title": "Never Gonna Give You Up", "url": "https://music.youtube.com/watch?v=dQw4w9WgXcQ", "uploader": "Rick Astley"},
            {"id": "UCxxx", "title": "Rick Astley", "url": "https://music.youtube.com/browse/UCxxx"},
            {"id": "MPREb_abc", "title": "Album", "url": "https://music.youtube.com/browse/MPREb_abc"},
        ]);

        let results: Vec<YoutubeResult> = entries.as_array().unwrap().iter().filter_map(|e| {
            let url = e["url"].as_str()?;
            if !url.contains("watch?v=") { return None; }
            let title = e["title"].as_str().filter(|s| !s.is_empty())?.to_string();
            let subtitle = e["uploader"].as_str().map(|s| s.to_string());
            Some(YoutubeResult { title, url: url.to_string(), kind: SearchKind::Song, subtitle, track_count: None })
        }).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Never Gonna Give You Up");
        assert_eq!(results[0].subtitle.as_deref(), Some("Rick Astley"));
    }

    #[test]
    fn page_size_is_20() {
        assert_eq!(PAGE_SIZE, 20);
    }
}
