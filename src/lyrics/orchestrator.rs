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

    app.playback.now_playing = Some(NowPlaying {
        title: metadata.title.clone(),
        artist: metadata.artist.clone(),
        album: metadata.album.clone().unwrap_or_default(),
    });

    let cache_key = LyricsCacheKey::from_metadata(&metadata);

    if app.lyrics_state.negative_cache.contains(&cache_key) {
        debug!(
            artist = %cache_key.artist,
            title = %cache_key.title,
            "Skipping lyrics fetch (negative cache hit)"
        );
        app.lyrics_state.status = LyricsStatus::None;
        return;
    }

    match load_for_track(track_path, &metadata) {
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

            let meta = metadata.clone();
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
                raw_artist = %metadata.artist,
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
