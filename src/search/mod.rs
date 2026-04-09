use crate::app::SearchEntry;
use crate::metadata::extract_metadata;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

const SEARCH_BATCH_SIZE: usize = 100;
const MAX_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub enum SearchMessage {
    Batch { entries: Vec<SearchEntry>, scanned: usize },
    EnrichedBatch { entries: Vec<SearchEntry> },
    Upsert { entry: SearchEntry },
    Finished { scanned: usize },
    Failed(String),
}

pub fn spawn_indexer(root: PathBuf, tx: Sender<SearchMessage>) {
    std::thread::spawn(move || {
        let mut batch = Vec::new();
        let mut scanned = 0usize;
        let mut paths = Vec::new();

        if let Err(err) = walk_dir(&root, &root, &tx, &mut batch, &mut scanned, &mut paths) {
            let _ = tx.send(SearchMessage::Failed(err.to_string()));
            return;
        }

        if !batch.is_empty() {
            let _ = tx.send(SearchMessage::Batch {
                entries: batch,
                scanned,
            });
        }

        let mut enriched_batch = Vec::new();

        for path in paths {
            if let Some(entry) = build_enriched_search_entry(&root, path) {
                enriched_batch.push(entry);
            }

            if enriched_batch.len() >= SEARCH_BATCH_SIZE {
                let entries = std::mem::take(&mut enriched_batch);
                let _ = tx.send(SearchMessage::EnrichedBatch { entries });
            }
        }

        if !enriched_batch.is_empty() {
            let _ = tx.send(SearchMessage::EnrichedBatch {
                entries: enriched_batch,
            });
        }

        let _ = tx.send(SearchMessage::Finished { scanned });
    });
}

pub fn spawn_index_update(root: PathBuf, path: PathBuf, tx: Sender<SearchMessage>) {
    std::thread::spawn(move || {
        let entry = build_enriched_search_entry(&root, path.clone())
            .or_else(|| build_search_entry(&root, path));

        if let Some(entry) = entry {
            let _ = tx.send(SearchMessage::Upsert { entry });
        }
    });
}

pub fn filter_results(index_entries: &[SearchEntry], query: &str) -> Vec<SearchEntry> {
    let query = query.trim().to_lowercase();
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    if terms.is_empty() {
        return Vec::new();
    }

    let mut matches: Vec<(i32, SearchEntry)> = index_entries
        .iter()
        .filter_map(|entry| score_entry(entry, &query, &terms).map(|score| (score, entry.clone())))
        .collect();

    matches.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| metadata_field_count(&b.1).cmp(&metadata_field_count(&a.1)))
            .then_with(|| a.1.relative_path.len().cmp(&b.1.relative_path.len()))
            .then_with(|| a.1.relative_path.cmp(&b.1.relative_path))
    });

    matches
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, entry)| entry)
        .collect()
}

fn walk_dir(
    root: &Path,
    dir: &Path,
    tx: &Sender<SearchMessage>,
    batch: &mut Vec<SearchEntry>,
    scanned: &mut usize,
    paths: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            walk_dir(root, &path, tx, batch, scanned, paths)?;
            continue;
        }

        if !is_supported_audio_path(&path) {
            continue;
        }

        *scanned += 1;
        paths.push(path.clone());

        if let Some(search_entry) = build_search_entry(root, path) {
            batch.push(search_entry);
        }

        if batch.len() >= SEARCH_BATCH_SIZE {
            let entries = std::mem::take(batch);
            let _ = tx.send(SearchMessage::Batch {
                entries,
                scanned: *scanned,
            });
        }
    }

    Ok(())
}

fn build_search_entry(root: &Path, path: PathBuf) -> Option<SearchEntry> {
    let file_name = path.file_name()?.to_string_lossy().to_string();
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path.as_path())
        .display()
        .to_string();

    let search_blob = format!(
        "{} {}",
        file_name.to_lowercase(),
        relative_path.to_lowercase()
    );

    Some(SearchEntry {
        path,
        relative_path,
        file_name,
        artist: None,
        title: None,
        album: None,
        search_blob,
    })
}

fn build_enriched_search_entry(root: &Path, path: PathBuf) -> Option<SearchEntry> {
    let mut entry = build_search_entry(root, path.clone())?;
    let meta = extract_metadata(&path)?;

    let artist = (!meta.artist.is_empty()).then_some(meta.artist);
    let title = (!meta.title.is_empty()).then_some(meta.title);
    let album = meta.album.filter(|value| !value.is_empty());

    entry.artist = artist;
    entry.title = title;
    entry.album = album;

    let mut search_blob_parts = vec![entry.file_name.to_lowercase(), entry.relative_path.to_lowercase()];

    if let Some(artist) = &entry.artist {
        search_blob_parts.push(artist.to_lowercase());
    }
    if let Some(title) = &entry.title {
        search_blob_parts.push(title.to_lowercase());
    }
    if let Some(album) = &entry.album {
        search_blob_parts.push(album.to_lowercase());
    }

    entry.search_blob = search_blob_parts.join(" ");
    Some(entry)
}

fn score_entry(entry: &SearchEntry, query: &str, terms: &[String]) -> Option<i32> {
    let file_name = entry.file_name.to_lowercase();
    let relative_path = entry.relative_path.to_lowercase();
    let artist = entry.artist.as_deref().map(str::to_lowercase);
    let title = entry.title.as_deref().map(str::to_lowercase);
    let album = entry.album.as_deref().map(str::to_lowercase);

    let mut total = 0;

    if title.as_deref() == Some(query) {
        total += 220;
    }
    if artist.as_deref() == Some(query) {
        total += 210;
    }
    if album.as_deref() == Some(query) {
        total += 180;
    }

    for term in terms {
        let score = [
            title
                .as_deref()
                .map(|value| score_field(value, term, 120, 80))
                .unwrap_or(0),
            artist
                .as_deref()
                .map(|value| score_field(value, term, 110, 70))
                .unwrap_or(0),
            album
                .as_deref()
                .map(|value| score_field(value, term, 100, 60))
                .unwrap_or(0),
            score_field(&file_name, term, 50, 30),
            score_field(&relative_path, term, 20, 10),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);

        if score == 0 {
            return None;
        }

        total += score;
    }

    Some(total)
}

fn score_field(field: &str, term: &str, exact_score: i32, partial_score: i32) -> i32 {
    if field == term {
        exact_score
    } else if field.contains(term) {
        partial_score
    } else {
        0
    }
}

fn metadata_field_count(entry: &SearchEntry) -> usize {
    usize::from(entry.artist.is_some())
        + usize::from(entry.title.is_some())
        + usize::from(entry.album.is_some())
}

fn is_supported_audio_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "wav" | "opus" | "ogg" | "m4a" | "aac"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        relative_path: &str,
        file_name: &str,
        artist: Option<&str>,
        title: Option<&str>,
        album: Option<&str>,
    ) -> SearchEntry {
        let mut parts = vec![file_name.to_lowercase(), relative_path.to_lowercase()];
        if let Some(artist) = artist {
            parts.push(artist.to_lowercase());
        }
        if let Some(title) = title {
            parts.push(title.to_lowercase());
        }
        if let Some(album) = album {
            parts.push(album.to_lowercase());
        }

        SearchEntry {
            path: PathBuf::from(format!("/music/{relative_path}")),
            relative_path: relative_path.to_string(),
            file_name: file_name.to_string(),
            artist: artist.map(str::to_string),
            title: title.map(str::to_string),
            album: album.map(str::to_string),
            search_blob: parts.join(" "),
        }
    }

    #[test]
    fn blank_query_returns_no_results() {
        let results = filter_results(&[entry("A/track.mp3", "track.mp3", None, None, None)], "  ");
        assert!(results.is_empty());
    }

    #[test]
    fn exact_metadata_match_outranks_path_only_match() {
        let entries = vec![
            entry(
                "Loose/01. Something Else.mp3",
                "01. Something Else.mp3",
                None,
                None,
                None,
            ),
            entry(
                "Artist/Album/01. Track.mp3",
                "01. Track.mp3",
                Some("Drake"),
                Some("Nokia"),
                Some("Album"),
            ),
        ];

        let results = filter_results(&entries, "nokia");
        assert_eq!(results.first().and_then(|entry| entry.title.as_deref()), Some("Nokia"));
    }

    #[test]
    fn all_query_terms_must_match() {
        let entries = vec![
            entry(
                "Artist/Album/01. Track.mp3",
                "01. Track.mp3",
                Some("Kendrick Lamar"),
                Some("DNA"),
                Some("DAMN."),
            ),
            entry(
                "Artist/Album/02. Track.mp3",
                "02. Track.mp3",
                Some("Kendrick Lamar"),
                Some("HUMBLE."),
                Some("DAMN."),
            ),
        ];

        let results = filter_results(&entries, "kendrick dna");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title.as_deref(), Some("DNA"));
    }

    #[test]
    fn exact_artist_query_outranks_path_only_artist_name() {
        let entries = vec![
            entry(
                "SearchHits/kendrick-lamar-mix.mp3",
                "kendrick-lamar-mix.mp3",
                None,
                None,
                None,
            ),
            entry(
                "Kendrick Lamar/DAMN./01. DNA.mp3",
                "01. DNA.mp3",
                Some("Kendrick Lamar"),
                Some("DNA"),
                Some("DAMN."),
            ),
        ];

        let results = filter_results(&entries, "kendrick lamar");
        assert_eq!(results.first().and_then(|entry| entry.artist.as_deref()), Some("Kendrick Lamar"));
    }

    #[test]
    fn tie_break_prefers_richer_metadata_when_scores_match() {
        let entries = vec![
            entry(
                "Artist/Album/track.mp3",
                "track.mp3",
                None,
                Some("Nokia"),
                None,
            ),
            entry(
                "Artist/Album/other-track.mp3",
                "other-track.mp3",
                Some("Drake"),
                Some("Nokia"),
                Some("Some Album"),
            ),
        ];

        let results = filter_results(&entries, "nokia");
        assert_eq!(results.first().and_then(|entry| entry.artist.as_deref()), Some("Drake"));
    }

    #[test]
    fn build_search_entry_uses_relative_path_from_root() {
        let root = Path::new("/music");
        let path = PathBuf::from("/music/Artist/Album/track.mp3");

        let entry = build_search_entry(root, path).expect("entry");

        assert_eq!(entry.relative_path, "Artist/Album/track.mp3");
        assert!(entry.search_blob.contains("track.mp3"));
        assert!(entry.search_blob.contains("artist/album/track.mp3"));
    }

    #[test]
    fn supported_audio_extensions_are_detected_case_insensitively() {
        assert!(is_supported_audio_path(Path::new("track.MP3")));
        assert!(is_supported_audio_path(Path::new("track.flac")));
        assert!(!is_supported_audio_path(Path::new("cover.jpg")));
    }
}
