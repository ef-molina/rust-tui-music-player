use crate::app::{AppState, LyricsCacheKey, LyricsStatus, NowPlaying};
use crate::lyrics::{LyricsState, load_for_track};
use crate::lyrics_fetch::LyricsFetchResult;
use crate::lyrics_fetch::lrclib::fetch_lrc;
use crate::metadata::extract_metadata;
use std::path::Path;
use tracing::{debug, warn};

/// Orchestrate lyrics loading for a track that is about to play.
///
/// This function handles the full lyrics lifecycle:
/// 1. Reset lyrics state and increment request ID
/// 2. Extract and validate metadata
/// 3. Update now-playing information
/// 4. Check the negative cache
/// 5. Try loading local .lrc file
/// 6. Spawn a background fetch if no local lyrics exist
///
/// Called by `play_album_index()` after the player has loaded the track.
pub fn start_lyrics_load(app: &mut AppState, track_path: &Path) {
    app.lyrics_state.status = LyricsStatus::Loading;
    app.lyrics_state.scroll = 0;
    let tx = app.channels.lyrics_tx.clone();
    app.lyrics_state.pending_cache_key = None;
    app.lyrics_state.request_id += 1;

    let metadata = match extract_metadata(track_path) {
        Some(m) if m.is_complete() => m,
        Some(m) => {
            warn!(
                path = %track_path.display(),
                confidence = ?m.confidence,
                "Metadata incomplete — skipping lyrics fetch"
            );
            app.lyrics_state.status = LyricsStatus::None;
            return;
        }
        None => {
            warn!(
                path = %track_path.display(),
                "Failed to extract metadata — skipping lyrics fetch"
            );
            app.lyrics_state.status = LyricsStatus::None;
            return;
        }
    };

    if metadata.confidence != crate::metadata::model::MetadataConfidence::Exact {
        debug!(
            path = %track_path.display(),
            confidence = ?metadata.confidence,
            "Skipping lyrics fetch for non-normalized track"
        );
        app.lyrics_state.status = LyricsStatus::None;
        return;
    }

    let mut lyrics_meta = metadata.clone();
    lyrics_meta.artist = primary_artist_for_lyrics(&metadata.artist).to_string();
    lyrics_meta.title = clean_title_for_lyrics(&metadata.title);

    app.playback.now_playing = Some(NowPlaying {
        title: lyrics_meta.title.clone(),
        artist: lyrics_meta.artist.clone(),
        album: metadata.album.clone().unwrap_or_default(),
    });

    let cache_key = LyricsCacheKey::from_metadata(&lyrics_meta);

    if app.lyrics_state.negative_cache.contains(&cache_key) {
        debug!(
            artist = %cache_key.artist,
            title = %cache_key.title,
            "Skipping lyrics fetch (negative cache hit)"
        );
        app.lyrics_state.status = LyricsStatus::None;
        return;
    }

    match load_for_track(track_path, &lyrics_meta) {
        Ok(Some(lines)) => {
            app.lyrics_state.status = LyricsStatus::Loaded(LyricsState::new(lines));
            app.lyrics_state.scroll = 0;
        }

        Ok(None) => {
            debug!(
                path = %track_path.display(),
                "No local lyrics found — spawning background fetch"
            );

            app.lyrics_state.pending_cache_key = Some(cache_key.clone());

            let meta = lyrics_meta.clone();
            let path = track_path.to_path_buf();
            let request_id = app.lyrics_state.request_id;

            debug!(
                request_id = app.lyrics_state.request_id,
                artist = %cache_key.artist,
                title = %cache_key.title,
                "Spawning lyrics fetch"
            );

            debug!(
                raw_title = %metadata.title,
                lyrics_title = %lyrics_meta.title,
                raw_artist = %metadata.artist,
                lyrics_artist = %lyrics_meta.artist,
                raw_album = ?metadata.album,
                duration = metadata.duration_secs,
                confidence = ?metadata.confidence,
                "Lyrics fetch input metadata"
            );

            std::thread::spawn(move || {
                let result = match fetch_lrc(&meta) {
                    Ok(Some(lrc_text)) => LyricsFetchResult::RawLrc {
                        request_id,
                        path,
                        contents: lrc_text,
                    },
                    Ok(None) => LyricsFetchResult::NotFound { request_id },
                    Err(_) => LyricsFetchResult::Failed { request_id },
                };

                let _ = tx.send(result);
            });
        }

        Err(_) => {
            app.lyrics_state.status = LyricsStatus::None;
        }
    }
}

/// Extract the primary artist for lyrics lookup.
///
/// Splits on ", " (comma-space) only, preserving names like "Simon & Garfunkel".
/// Returns the full string unchanged if there is no comma-space separator.
/// Recognizes known artists whose official name contains ", " (e.g. "Tyler, The Creator").
fn primary_artist_for_lyrics(artist: &str) -> &str {
    const COMMA_NAMES: &[&str] = &["Tyler, The Creator"];

    let trimmed = artist.trim();
    for name in COMMA_NAMES {
        if trimmed.len() >= name.len()
            && trimmed[..name.len()].eq_ignore_ascii_case(name)
            && (trimmed.len() == name.len() || trimmed[name.len()..].starts_with(", "))
        {
            return &trimmed[..name.len()];
        }
    }

    artist
        .split(", ")
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(artist)
}

/// Clean a title for lyrics lookup by stripping noise that prevents matching.
///
/// 1. Strip leading "Artist - " prefix.
/// 2. Strip trailing junk suffixes in `(...)` or `[...]` form, iteratively.
///
/// Does NOT strip `(feat. ...)`, `(ft. ...)`, `(with ...)`, or `(Remastered)`.
fn clean_title_for_lyrics(title: &str) -> String {
    let mut t = title.to_string();

    // Strip "Artist - " prefix
    if let Some((_, rest)) = t.split_once(" - ") {
        t = rest.to_string();
    }

    // Iteratively strip trailing junk suffixes
    const JUNK: &[&str] = &[
        "official video",
        "official audio",
        "visualizer",
        "lyrics",
        "high quality",
        "hq",
        "hd",
        "audio",
    ];

    loop {
        let lower = t.to_lowercase();
        let mut removed = false;

        for j in JUNK {
            let paren = format!("({})", j);
            let bracket = format!("[{}]", j);

            if lower.ends_with(&paren) {
                t = t[..t.len() - paren.len()].trim().to_string();
                removed = true;
            } else if lower.ends_with(&bracket) {
                t = t[..t.len() - bracket.len()].trim().to_string();
                removed = true;
            }
        }

        if !removed {
            break;
        }
    }

    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // primary_artist_for_lyrics tests
    // ---------------------------------------------------------------

    #[test]
    fn extracts_primary_from_multi_artist() {
        assert_eq!(
            primary_artist_for_lyrics("Kanye West, GLC, Consequence"),
            "Kanye West"
        );
    }

    #[test]
    fn single_artist_unchanged() {
        assert_eq!(
            primary_artist_for_lyrics("Kendrick Lamar"),
            "Kendrick Lamar"
        );
    }

    #[test]
    fn ampersand_group_preserved() {
        assert_eq!(
            primary_artist_for_lyrics("Simon & Garfunkel"),
            "Simon & Garfunkel"
        );
    }

    #[test]
    fn bare_comma_without_space_not_split() {
        assert_eq!(
            primary_artist_for_lyrics("Tyler,The Creator"),
            "Tyler,The Creator"
        );
    }

    #[test]
    fn tyler_the_creator_preserved() {
        assert_eq!(
            primary_artist_for_lyrics("Tyler, The Creator"),
            "Tyler, The Creator"
        );
    }

    #[test]
    fn tyler_the_creator_as_primary_in_multi_artist() {
        assert_eq!(
            primary_artist_for_lyrics("Tyler, The Creator, Pharrell Williams"),
            "Tyler, The Creator"
        );
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            primary_artist_for_lyrics("  Dr. Dre , Snoop Dogg"),
            "Dr. Dre"
        );
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(primary_artist_for_lyrics(""), "");
    }

    // ---------------------------------------------------------------
    // clean_title_for_lyrics tests
    // ---------------------------------------------------------------

    #[test]
    fn strips_artist_prefix_and_high_quality() {
        assert_eq!(
            clean_title_for_lyrics("Kanye West - Spaceship (High Quality)"),
            "Spaceship"
        );
    }

    #[test]
    fn strips_official_audio() {
        assert_eq!(
            clean_title_for_lyrics("Song Title (Official Audio)"),
            "Song Title"
        );
    }

    #[test]
    fn strips_lyrics_in_brackets() {
        assert_eq!(clean_title_for_lyrics("Song Title [Lyrics]"), "Song Title");
    }

    #[test]
    fn strips_hq_suffix() {
        assert_eq!(clean_title_for_lyrics("Song Title (HQ)"), "Song Title");
    }

    #[test]
    fn preserves_remastered() {
        assert_eq!(
            clean_title_for_lyrics("Song Title (Remastered)"),
            "Song Title (Remastered)"
        );
    }

    #[test]
    fn preserves_feat() {
        assert_eq!(
            clean_title_for_lyrics("Song Title (feat. Artist)"),
            "Song Title (feat. Artist)"
        );
    }

    #[test]
    fn clean_title_unchanged() {
        assert_eq!(clean_title_for_lyrics("Song Title"), "Song Title");
    }

    #[test]
    fn strips_stacked_suffixes() {
        assert_eq!(
            clean_title_for_lyrics("Song Title (Official Video) (HQ)"),
            "Song Title"
        );
    }

    // ---------------------------------------------------------------
    // NowPlaying consistency tests
    // ---------------------------------------------------------------

    #[test]
    fn cleaned_identity_suitable_for_now_playing() {
        let artist = primary_artist_for_lyrics("Kanye West, GLC, Consequence");
        let title = clean_title_for_lyrics("Kanye West - Spaceship (High Quality)");

        assert_eq!(artist, "Kanye West");
        assert_eq!(title, "Spaceship");
    }

    #[test]
    fn clean_track_identity_unchanged_for_now_playing() {
        let artist = primary_artist_for_lyrics("Kendrick Lamar");
        let title = clean_title_for_lyrics("N95");

        assert_eq!(artist, "Kendrick Lamar");
        assert_eq!(title, "N95");
    }
}
