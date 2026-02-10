use std::path::Path;

use super::model::TrackMetadata;

/// Extract YouTube ID.
///
/// Priority:
/// 1. From metadata `purl` (preferred)
/// 2. From filename `[id]` suffix
pub fn extract_youtube_id(meta: &TrackMetadata, path: &Path) -> Option<String> {
    // 1) Prefer purl
    if let Some(purl) = meta.purl.as_deref()
        && let Some(id) = parse_youtube_id_from_url(purl)
    {
        return Some(id);
    }

    // 2) Fallback: filename "[id]"
    extract_youtube_id_from_filename(path)
}

/// Extract track number.
///
/// Priority:
/// 1. Metadata `track` / `tracknumber`
/// 2. Filename prefix written by yt-dlp (`NN. `)
pub fn extract_track_number(meta: &TrackMetadata, path: &Path) -> Option<u32> {
    // 1) Metadata track tag
    if let Some(track) = meta.track.as_deref()
        && let Some(n) = parse_track_tag(track)
    {
        return Some(n);
    }

    // 2) Filename prefix (playlist index)
    extract_track_number_from_filename(path)
}

/// Extract release year.
///
/// Priority:
/// 1. "Released on: YYYY-.." inside comment/synopsis
/// 2. First 4 digits of `date` tag
pub fn extract_release_year(meta: &TrackMetadata) -> Option<u32> {
    // Trusted auto-generated block
    if let Some(c) = meta.comment.as_deref()
        && let Some(y) = parse_released_on_year(c)
    {
        return Some(y);
    }
    if let Some(s) = meta.synopsis.as_deref()
        && let Some(y) = parse_released_on_year(s)
    {
        return Some(y);
    }

    // Fallback: date tag
    meta.date.as_deref().and_then(parse_year_from_date)
}

/* ---------------- helpers ---------------- */

fn parse_youtube_id_from_url(url: &str) -> Option<String> {
    // Expect: https://www.youtube.com/watch?v=ID
    let idx = url.find("v=")?;
    let rest = &url[idx + 2..];
    let end = rest.find('&').unwrap_or(rest.len());
    let id = &rest[..end];
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn extract_youtube_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    let s = stem.as_ref();

    let lb = s.rfind('[')?;
    let rb = s.rfind(']')?;
    if rb <= lb + 1 {
        return None;
    }

    Some(s[lb + 1..rb].trim().to_string())
}

fn parse_track_tag(track: &str) -> Option<u32> {
    // Handles: "1", "01", "01/12"
    let t = track.trim();
    if t.is_empty() {
        return None;
    }

    let first = t.split('/').next().unwrap_or(t).trim();
    first.parse::<u32>().ok().filter(|n| *n > 0)
}

fn extract_track_number_from_filename(path: &Path) -> Option<u32> {
    // Expect: "NN. Title.ext"
    let stem = path.file_stem()?.to_string_lossy();
    let s = stem.as_ref();

    let mut chars = s.chars();
    let d1 = chars.next()?.to_digit(10)?;
    let d2 = chars.next()?.to_digit(10)?;
    let dot = chars.next()?;
    if dot != '.' {
        return None;
    }

    Some(d1 * 10 + d2)
}

fn parse_released_on_year(text: &str) -> Option<u32> {
    // Example: "Released on: 2017-03-10"
    let idx = text.find("Released on: ")?;
    let after = &text[idx + "Released on: ".len()..];
    let year = after.get(0..4)?;
    year.parse::<u32>().ok()
}

fn parse_year_from_date(date: &str) -> Option<u32> {
    if date.len() < 4 {
        return None;
    }
    date.get(0..4)?.parse::<u32>().ok()
}
