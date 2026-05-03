//! Application entry point.
//!
//! This file is responsible for:
//! - Initializing application state
//! - Owning the main event loop
//! - Dispatching events
//! - Coordinating state updates
//!
//! Important constraints:
//! - All mutable state lives in `AppState`
//! - This file does NOT contain UI rendering logic
//! - This file does NOT contain filesystem or player logic
//!
//! Over time, this loop will:
//! - read input events
//! - update application state
//! - send commands to the player
//! - trigger UI redraws

mod app;
mod event;
mod fs;
mod input;
mod lyrics;
mod lyrics_fetch;
mod metadata;
mod player;
mod search;
mod ui;
mod youtube;

use crate::event::commands::{Command, parse_command};
use crate::event::jobs::JobResult;
use crate::lyrics::{LyricsState, load_for_track};
use crate::lyrics_fetch::LyricsFetchResult;
use crate::lyrics_fetch::lrclib::fetch_lrc;
use crate::metadata::extract_metadata;
use crate::search::{SearchMessage, filter_results, spawn_index_update, spawn_indexer};
use app::{
    AppState, CommandState, FocusPane, InputMode, LyricsCacheKey, LyricsStatus, NavigationState,
    NowPlaying, SearchStatus, StatusLevel,
};
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use event::AppEvent;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs::File;
use std::io::stdout;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tracing::{debug, trace, warn};
use tracing_subscriber::EnvFilter;

const NAVIGATION_HISTORY_LIMIT: usize = 20;

fn download_staging_dir() -> PathBuf {
    std::path::PathBuf::from(
        std::env::var("HOME")
            .map(|h| format!("{}/Downloads/Media/Music/.staging", h))
            .unwrap_or_else(|_| ".staging".into()),
    )
}

fn truncate_status_url(url: &str) -> String {
    const MAX_LEN: usize = 56;
    if url.chars().count() <= MAX_LEN {
        return url.to_string();
    }

    let prefix: String = url.chars().take(36).collect();
    let suffix: String = url
        .chars()
        .rev()
        .take(14)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}…{suffix}")
}

/// Download a URL (single track or full playlist) with progress streaming.
/// Designed to run in a background thread; sends `JobResult` messages via `tx`.
fn spawn_playlist_download(url: String, staging: PathBuf, tx: Sender<JobResult>) {
    use std::io::{BufRead, BufReader};

    // Use a per-job subdirectory so concurrent downloads don't clobber each other
    // and so we can reliably collect every file that belongs to this job.
    let job_dir = staging.join({
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        format!("job_{:x}", h.finish())
    });
    let staging = job_dir;
    let _ = std::fs::create_dir_all(&staging);
    let output_template = staging.join("%(title)s [%(id)s].%(ext)s");

    let _ = tx.send(JobResult::DownloadStarted { url: url.clone() });

    let mut child = match std::process::Command::new("yt-dlp")
        .args([
            "-f", "bestaudio[ext=opus]/bestaudio",
            "-x",
            "--audio-format", "opus",
            "--audio-quality", "0",
            "--embed-metadata",
            "--embed-thumbnail",
            "--convert-thumbnails", "jpg",
            "--add-metadata",
            "--yes-playlist",
            "--newline",
            "--cookies-from-browser", "brave",
            "-o",
        ])
        .arg(&output_template)
        .arg(&url)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: format!("Failed to spawn yt-dlp: {e}"),
            });
            return;
        }
    };

    let stdout = child.stdout.take().expect("stdout piped");
    let reader = BufReader::new(stdout);

    let mut current_item: u32 = 0;
    let mut total_items: u32 = 0;
    let mut current_title = String::new();

    for line in reader.lines().map_while(Result::ok) {
        let Some(rest) = line.strip_prefix("[download]") else { continue };
        let trimmed = rest.trim();

        // "[download] Downloading item X of Y"
        if let Some(item_str) = trimmed.strip_prefix("Downloading item ") {
            if let Some((idx, tot)) = item_str.split_once(" of ") {
                if let (Ok(i), Ok(t)) = (idx.trim().parse::<u32>(), tot.trim().parse::<u32>()) {
                    current_item = i;
                    total_items = t;
                }
            }
        }
        // "[download] Destination: /full/path/Title [id].ext"
        else if let Some(dest) = trimmed.strip_prefix("Destination: ") {
            let filename = std::path::Path::new(dest)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(dest);
            // Output template is "%(title)s [%(id)s].%(ext)s" — strip " [id].ext"
            current_title = if let Some(pos) = filename.rfind(" [") {
                filename[..pos].to_string()
            } else {
                filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename).to_string()
            };
        }
        // "[download]  42.3% of 5.23MiB at ..."
        else if let Some(pct_str) = trimmed.split('%').next() {
            if let Ok(track_pct) = pct_str.trim().parse::<f32>() {
                let overall = if total_items > 0 {
                    ((current_item.saturating_sub(1)) as f32 + track_pct / 100.0)
                        / total_items as f32
                        * 100.0
                } else {
                    track_pct
                };
                let _ = tx.send(JobResult::DownloadProgress {
                    url: url.clone(),
                    track_percent: track_pct,
                    overall_percent: overall,
                    track_title: current_title.clone(),
                    track_index: current_item,
                    total_tracks: total_items,
                });
            }
        }
    }

    match child.wait() {
        Ok(status) if status.success() => {
            // Collect every file in the per-job staging dir and emit one
            // DownloadFinished per track so each is individually normalized.
            let files: Vec<_> = std::fs::read_dir(&staging)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .collect();

            if files.is_empty() {
                let _ = tx.send(JobResult::DownloadFailed {
                    url,
                    error: "Download succeeded but no files found".into(),
                });
            } else {
                let total = files.len();
                for (i, entry) in files.into_iter().enumerate() {
                    // Only the last event carries the URL so the active_download
                    // indicator clears once all tracks are queued.
                    let evt_url = if i + 1 == total {
                        url.clone()
                    } else {
                        String::new()
                    };
                    let _ = tx.send(JobResult::DownloadFinished {
                        url: evt_url,
                        temp_path: entry.path(),
                    });
                }
            }
        }
        Ok(status) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: format!("yt-dlp exited with code {:?}", status.code()),
            });
        }
        Err(e) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: e.to_string(),
            });
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --------------------------------------------------
    // CLI: one-shot normalization mode (EARLY EXIT)
    // --------------------------------------------------
    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if args.first().map(|s| s.as_str()) == Some("--normalize") {
        if args.len() != 2 {
            eprintln!("Usage: --normalize <path-to-audio-file>");
            std::process::exit(1);
        }

        let path = std::path::PathBuf::from(&args[1]);

        let library_root = std::path::PathBuf::from(
            std::env::var("HOME")
                .map(|h| format!("{}/Downloads/Media/Music", h))
                .unwrap_or_else(|_| ".".into()),
        );

        let normalized = crate::fs::normalize::normalize_downloaded_track(&path, &library_root)?;

        crate::metadata::write::write_clean_tags(&normalized.final_path, &normalized)?;

        // Visible output for confidence
        println!("{:#?}", normalized);

        return Ok(());
    }

    // Initialize logging to file
    let log_file = File::create("debug.log")?;

    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(log_file)
        .init();

    debug!("logging initialized");

    terminal::enable_raw_mode().expect("Failed to enable raw mode");
    execute!(stdout(), EnterAlternateScreen).expect("Failed to enter alt screen");

    let result = run_app();

    let _ = execute!(stdout(), LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();

    if let Err(err) = result {
        eprintln!("Application error: {err}");
    }
    Ok(())
}

/// --------------------------------------------------
/// Album playback helpers (no fs / mpv policy)
/// --------------------------------------------------
fn play_album_index(app: &mut AppState, index: usize) {
    let Some(album_dir) = &app.active_album_dir else {
        return;
    };
    let Some(entry) = app.album_entries.get(index) else {
        return;
    };

    let track_path = album_dir.join(&entry.name);

    debug!(
        path = %track_path.display(),
        "Starting playback for track"
    );

    app.album_selected = index;
    app.player.load(track_path.clone());

    // reset lyrics state immediately
    app.lyrics = LyricsStatus::Loading;
    app.lyric_scroll = 0;
    let tx = app.lyrics_tx.clone();
    app.lyrics_pending_cache_key = None;
    app.lyrics_request_id += 1;

    // extract metadata once
    let metadata = match extract_metadata(&track_path) {
        Some(m) if m.is_complete() => m,
        Some(m) => {
            warn!(
                path = %track_path.display(),
                confidence = ?m.confidence,
                "Metadata incomplete — skipping lyrics fetch"
            );
            app.lyrics = LyricsStatus::None;
            return;
        }
        None => {
            warn!(
                path = %track_path.display(),
                "Failed to extract metadata — skipping lyrics fetch"
            );
            app.lyrics = LyricsStatus::None;
            return;
        }
    };

    // Guardrail: lyrics require normalized tags
    if metadata.confidence != crate::metadata::model::MetadataConfidence::Exact {
        debug!(
            path = %track_path.display(),
            confidence = ?metadata.confidence,
            "Skipping lyrics fetch for non-normalized track"
        );
        app.lyrics = LyricsStatus::None;
        return;
    }

    // update now playing information
    app.now_playing = Some(NowPlaying {
        title: metadata.title.clone(),
        artist: metadata.artist.clone(),
        album: metadata.album.clone().unwrap_or_default(),
        duration_secs_meta: metadata.duration_secs as u64,
    });

    // build cache key once
    let cache_key = LyricsCacheKey::from_metadata(&metadata);

    // negative cache gate
    if app.lyrics_negative_cache.contains(&cache_key) {
        debug!(
            artist = %cache_key.artist,
            title = %cache_key.title,
            "Skipping lyrics fetch (negative cache hit)"
        );
        app.lyrics = LyricsStatus::None;
        return;
    }

    // try local lyrics
    match load_for_track(&track_path, &metadata) {
        Ok(Some(lines)) => {
            app.lyrics = LyricsStatus::Loaded(LyricsState::new(lines));
            app.lyric_scroll = 0;
        }

        Ok(None) => {
            debug!(
                path = %track_path.display(),
                "No local lyrics found — spawning background fetch"
            );

            app.lyrics_pending_cache_key = Some(cache_key.clone());

            let meta = metadata.clone();
            let path = track_path.clone();
            let request_id = app.lyrics_request_id;

            // LOG: spawn fetch thread
            debug!(
                request_id = app.lyrics_request_id,
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
            app.lyrics = LyricsStatus::None;
        }
    }
}

/// Start a YouTube search of the given kind at the given page.
/// Resets results when page == 0; appends when page > 0 (load-more).
fn spawn_youtube_search(
    app: &mut AppState,
    query: String,
    kind: crate::youtube::SearchKind,
    page: usize,
) {
    use crate::youtube::PAGE_SIZE;

    if page == 0 {
        app.youtube_results.clear();
        app.youtube_selected = 0;
    }
    app.youtube_searching = true;
    app.youtube_search_kind = kind;
    app.youtube_page = page;
    app.youtube_query = query.clone();
    app.youtube_has_more = false;
    app.focus = FocusPane::YoutubeResults;

    let label = match kind {
        crate::youtube::SearchKind::Song => "songs",
        crate::youtube::SearchKind::Album => "albums",
        crate::youtube::SearchKind::Artist => "artists",
    };
    app.set_status(
        StatusLevel::Info,
        format!("Searching {label} for \"{query}\"…"),
        None,
    );

    let tx = app.jobs_tx.clone();
    std::thread::spawn(move || {
        let result = match kind {
            crate::youtube::SearchKind::Song => crate::youtube::search_songs(&query, page),
            crate::youtube::SearchKind::Album => crate::youtube::search_albums(&query, page),
            crate::youtube::SearchKind::Artist => crate::youtube::search_artists(&query, page),
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

fn play_next_or_stop(app: &mut AppState) {
    let next = app.album_selected + 1;

    if next < app.album_entries.len() {
        play_album_index(app, next);
    } else {
        app.player.stop();
        app.clear_playback();
    }
}

fn refresh_search_results(app: &mut AppState) {
    app.search.results = filter_results(&app.search.index_entries, &app.search.query);

    if app.search.results.is_empty() {
        app.search.selected = 0;
    } else {
        app.search.selected = app.search.selected.min(app.search.results.len() - 1);
    }
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }

    let ch = text[cursor..].chars().next().unwrap_or_default();
    cursor + ch.len_utf8()
}

fn insert_char(buffer: &mut String, cursor: &mut usize, ch: char) {
    buffer.insert(*cursor, ch);
    *cursor += ch.len_utf8();
}

fn backspace_char(buffer: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }

    let start = previous_char_boundary(buffer, *cursor);
    buffer.drain(start..*cursor);
    *cursor = start;
}

fn delete_char(buffer: &mut String, cursor: &mut usize) {
    if *cursor >= buffer.len() {
        return;
    }

    let end = next_char_boundary(buffer, *cursor);
    buffer.drain(*cursor..end);
}

fn move_cursor_left(buffer: &str, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor = previous_char_boundary(buffer, *cursor);
    }
}

fn move_cursor_right(buffer: &str, cursor: &mut usize) {
    if *cursor < buffer.len() {
        *cursor = next_char_boundary(buffer, *cursor);
    }
}

fn restore_search_context(app: &mut AppState) {
    load_browser_dir(app, app.search.last_browser_dir.clone());
    let dir_count = app.browser_entries.iter().filter(|entry| entry.is_dir).count();
    app.selected_index = if dir_count == 0 {
        0
    } else {
        app.search.last_browser_selected.min(dir_count - 1)
    };
    app.active_album_dir = app.search.last_active_album_dir.clone();
    app.album_entries = app.search.last_album_entries.clone();
    app.album_selected = if app.album_entries.is_empty() {
        0
    } else {
        app.search
            .last_album_selected
            .min(app.album_entries.len() - 1)
    };
    app.focus = app.search.last_focus;
}

fn push_navigation_history(app: &mut AppState) {
    let snapshot = app.current_navigation_state();
    if app.navigation_history.last() == Some(&snapshot) {
        return;
    }

    if app.navigation_history.len() == NAVIGATION_HISTORY_LIMIT {
        app.navigation_history.remove(0);
    }
    app.navigation_history.push(snapshot);
}

fn restore_navigation_state(app: &mut AppState, state: &NavigationState) {
    load_browser_dir(app, state.current_dir.clone());

    let dir_count = app.browser_entries.iter().filter(|entry| entry.is_dir).count();
    app.selected_index = if dir_count == 0 {
        0
    } else {
        state.selected_index.min(dir_count - 1)
    };

    app.active_album_dir = state.active_album_dir.clone();
    app.album_entries = state.album_entries.clone();
    app.album_selected = if app.album_entries.is_empty() {
        0
    } else {
        state.album_selected.min(app.album_entries.len() - 1)
    };
    app.focus = state.focus;
    app.selection_anchor_tick = app.ui_tick;
}

fn pop_navigation_history(app: &mut AppState) -> bool {
    while let Some(state) = app.navigation_history.pop() {
        if state != app.current_navigation_state() {
            restore_navigation_state(app, &state);
            return true;
        }
    }

    false
}

fn merge_search_entries(app: &mut AppState, entries: Vec<app::SearchEntry>) {
    for entry in entries {
        if let Some(existing) = app
            .search
            .index_entries
            .iter_mut()
            .find(|existing| existing.path == entry.path)
        {
            *existing = entry;
        } else {
            app.search.index_entries.push(entry);
        }
    }
}

fn load_browser_dir(app: &mut AppState, dir: PathBuf) {
    app.current_dir = dir;
    app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
    app.selected_index = 0;
    app.selection_anchor_tick = app.ui_tick;
}

fn sync_album_for_directory(app: &mut AppState, dir: &std::path::Path) {
    if let Ok(Some(tracks)) = fs::detect_loose_tracks(dir) {
        app.active_album_dir = Some(dir.to_path_buf());
        app.album_entries = tracks;
        app.album_selected = 0;
    } else {
        app.active_album_dir = None;
        app.album_entries.clear();
        app.album_selected = 0;
    }
}

fn jump_to_track_path(app: &mut AppState, track_path: &std::path::Path) {
    let Some(track_dir) = track_path.parent() else {
        return;
    };

    if !track_dir.starts_with(&app.root_dir) {
        return;
    }

    let track_name = track_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    if let Ok(Some(tracks)) = fs::detect_album(track_dir) {
        let browser_dir = track_dir.parent().unwrap_or(&app.root_dir).to_path_buf();
        load_browser_dir(app, browser_dir);

        let browser_dirs: Vec<_> = app
            .browser_entries
            .iter()
            .filter(|e| e.is_dir)
            .collect();

        if let Some(dir_name) = track_dir.file_name().and_then(|s| s.to_str())
            && let Some(index) = browser_dirs.iter().position(|e| e.name == dir_name)
        {
            app.selected_index = index;
        }

        app.active_album_dir = Some(track_dir.to_path_buf());
        app.album_entries = tracks;
        app.album_selected = app
            .album_entries
            .iter()
            .position(|entry| entry.name == track_name)
            .unwrap_or(0);
    } else {
        load_browser_dir(app, track_dir.to_path_buf());
        sync_album_for_directory(app, track_dir);
        app.album_selected = app
            .album_entries
            .iter()
            .position(|entry| entry.name == track_name)
            .unwrap_or(0);
    }

    app.focus = FocusPane::Album;
}

fn handle_tick(app: &mut AppState) {
    app.ui_tick = app.ui_tick.wrapping_add(1);
    app.clear_expired_status();
    app.player.poll_metrics();

    while let Ok(message) = app.search_rx.try_recv() {
        match message {
            SearchMessage::Batch { entries, scanned } => {
                merge_search_entries(app, entries);
                app.search.status = SearchStatus::Indexing { scanned };
                refresh_search_results(app);
            }
            SearchMessage::EnrichedBatch { entries } => {
                merge_search_entries(app, entries);
                refresh_search_results(app);
            }
            SearchMessage::Upsert { entry } => {
                merge_search_entries(app, vec![entry]);
                refresh_search_results(app);
            }
            SearchMessage::Finished { scanned } => {
                app.search.status = SearchStatus::Ready;
                app.set_status(
                    StatusLevel::Success,
                    format!("Library index ready: {} tracks", app.search.index_entries.len()),
                    Some(500),
                );
                debug!(
                    scanned,
                    indexed = app.search.index_entries.len(),
                    "Search indexing complete"
                );
                refresh_search_results(app);
            }
            SearchMessage::Failed(error) => {
                app.set_status(
                    StatusLevel::Error,
                    format!("Search indexing failed: {error}"),
                    None,
                );
                app.search.status = SearchStatus::Failed(error);
                refresh_search_results(app);
            }
        }
    }

    if let Ok(result) = app.lyrics_rx.try_recv() {
        match result {
            LyricsFetchResult::RawLrc {
                request_id,
                path,
                contents,
            } => {
                let is_current = request_id == app.lyrics_request_id;

                debug!(
                    path = %path.display(),
                    bytes = contents.len(),
                    current = is_current,
                    "Lyrics fetch succeeded"
                );

                let lrc_path = path.with_extension("lrc");
                let tmp = lrc_path.with_extension("lrc.tmp");

                if std::fs::write(&tmp, &contents).is_ok()
                    && std::fs::rename(&tmp, &lrc_path).is_ok()
                {
                    debug!(
                        path = %lrc_path.display(),
                        current = is_current,
                        "Lyrics written to sidecar file"
                    );

                    if is_current {
                        app.lyrics_pending_cache_key = None;

                        if let Ok(lines) = crate::lyrics::parse_lrc(&lrc_path) {
                            if !lines.is_empty() {
                                app.lyrics = LyricsStatus::Loaded(LyricsState::new(lines));
                                app.lyric_scroll = 0;
                                app.set_status(StatusLevel::Success, "Lyrics loaded", Some(250));
                            } else {
                                app.lyrics = LyricsStatus::None;
                            }
                        } else {
                            app.lyrics = LyricsStatus::None;
                        }
                    } else {
                        trace!(
                            request_id,
                            current = app.lyrics_request_id,
                            "Stale lyrics fetch — saved to disk only"
                        );
                    }
                } else if is_current {
                    app.lyrics = LyricsStatus::None;
                    app.set_status(
                        StatusLevel::Warning,
                        "Couldn't save fetched lyrics",
                        Some(350),
                    );
                }
            }
            LyricsFetchResult::NotFound { request_id } => {
                if request_id != app.lyrics_request_id {
                    trace!(
                        request_id,
                        current = app.lyrics_request_id,
                        "Ignoring stale lyrics fetch result"
                    );
                } else {
                    if let Some(key) = app.lyrics_pending_cache_key.take() {
                        trace!(
                            artist = %key.artist,
                            title = %key.title,
                            "Inserting lyrics negative cache entry"
                        );
                        app.lyrics_negative_cache.insert(key);
                    }

                    app.lyrics = LyricsStatus::None;
                    app.set_status(StatusLevel::Warning, "No lyrics found", Some(250));
                }
            }
            LyricsFetchResult::Failed { request_id } => {
                if request_id != app.lyrics_request_id {
                    trace!(
                        request_id,
                        current = app.lyrics_request_id,
                        "Ignoring stale lyrics fetch result"
                    );
                } else {
                    app.lyrics_pending_cache_key = None;
                    app.lyrics = LyricsStatus::None;
                    app.set_status(StatusLevel::Warning, "Lyrics fetch failed", Some(350));
                }
            }
        }
    }

    if let Ok(job) = app.jobs_rx.try_recv() {
        match job {
            JobResult::DownloadStarted { url } => {
                app.active_download_url = Some(url.clone());
                app.set_status(
                    StatusLevel::Info,
                    format!("Downloading: {}", truncate_status_url(&url)),
                    None,
                );
                tracing::info!(url = %url, "Download job started");
            }
            JobResult::DownloadFinished { url, temp_path } => {
                app.active_download_url = None;
                // Only clear the progress bar on the final track (url is empty for intermediate ones)
                if !url.is_empty() {
                    app.active_download = None;
                }
                tracing::info!(
                    url = %url,
                    path = %temp_path.display(),
                    "Download job finished"
                );

                let library_root = app.root_dir.clone();

                match crate::fs::normalize::normalize_downloaded_track(
                    &temp_path,
                    &library_root,
                ) {
                    Ok(normalized) => {
                        app.set_status(
                            StatusLevel::Success,
                            format!("Added {} - {}", normalized.artist, normalized.title),
                            Some(500),
                        );
                        tracing::info!(
                            from = %temp_path.display(),
                            to = %normalized.final_path.display(),
                            artist = %normalized.artist,
                            title = %normalized.title,
                            "Track normalized successfully"
                        );

                        spawn_index_update(
                            app.root_dir.clone(),
                            normalized.final_path.clone(),
                            app.search_tx.clone(),
                        );

                        // Refresh browser if the new track landed in or under the current dir
                        let track_album_dir = normalized.final_path.parent();
                        let track_artist_dir = track_album_dir.and_then(|d| d.parent());

                        let browser_stale = track_artist_dir
                            .map(|d| d == app.current_dir)
                            .unwrap_or(false);

                        if browser_stale {
                            app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
                        }

                        // Refresh album pane if this track landed in the active album dir
                        let album_stale = track_album_dir
                            .zip(app.active_album_dir.as_deref())
                            .map(|(track_dir, album_dir)| track_dir == album_dir)
                            .unwrap_or(false);

                        if album_stale {
                            if let Ok(Some(tracks)) = fs::detect_album(
                                app.active_album_dir.as_deref().unwrap()
                            ) {
                                app.album_entries = tracks;
                            }
                        }
                    }
                    Err(err) => {
                        app.set_status(
                            StatusLevel::Error,
                            format!("Normalization failed: {err}"),
                            Some(600),
                        );
                        tracing::warn!(
                            path = %temp_path.display(),
                            error = %err,
                            "Track moved but metadata write failed"
                        );
                    }
                }
            }
            JobResult::DownloadProgress {
                url,
                overall_percent,
                track_title,
                track_index,
                total_tracks,
                ..
            } => {
                app.active_download = Some(crate::app::DownloadState {
                    track_title,
                    track_index,
                    total_tracks,
                    overall_percent,
                });
                // Keep the URL visible in the status bar label
                app.active_download_url = Some(url);
            }
            JobResult::DownloadFailed { url, error } => {
                app.active_download_url = None;
                app.active_download = None;
                app.set_status(
                    StatusLevel::Error,
                    format!("Download failed: {} ({error})", truncate_status_url(&url)),
                    Some(600),
                );
                tracing::error!(
                    url = %url,
                    error = %error,
                    "Download job failed"
                );
            }
            JobResult::YoutubeSearchDone { results, has_more } => {
                app.youtube_searching = false;
                let appending = app.youtube_page > 0 && !app.youtube_results.is_empty();
                let new_count = results.len();
                if appending {
                    app.youtube_results.extend(results);
                } else {
                    app.youtube_results = results;
                    app.youtube_selected = 0;
                }
                app.youtube_has_more = has_more;
                app.focus = FocusPane::YoutubeResults;
                let total = app.youtube_results.len();
                app.set_status(
                    StatusLevel::Success,
                    if appending {
                        format!("Loaded {new_count} more — {total} total")
                    } else {
                        format!("Found {total} result(s) — Enter to select, Tab for more")
                    },
                    Some(300),
                );
                tracing::info!(total, "YouTube search completed");
            }
            JobResult::YoutubeSearchFailed(error) => {
                app.youtube_searching = false;
                app.set_status(
                    StatusLevel::Error,
                    format!("YouTube search failed: {error}"),
                    Some(600),
                );
                tracing::error!(error = %error, "YouTube search failed");
            }
        }
    }

    if let (LyricsStatus::Loaded(lyrics), Some(position)) =
        (&mut app.lyrics, app.player.metrics.position)
    {
        let prev_index = lyrics.current_index;
        lyrics.update(position);

        if lyrics.current_index != prev_index {
            trace!(
                index = lyrics.current_index,
                time = position,
                "Lyric line advanced"
            );
            app.lyric_scroll = lyrics.current_index;
        }
    }

    if app.player.is_track_finished() {
        debug!("Track finished — auto advancing");
        play_next_or_stop(app);
    }
}

/// --------------------------------------------------
/// Main application loop
/// --------------------------------------------------
fn run_app() -> std::io::Result<()> {
    let (lyrics_tx, lyrics_rx) = std::sync::mpsc::channel();
    let (search_tx, search_rx) = std::sync::mpsc::channel();
    let (jobs_tx, jobs_rx) = std::sync::mpsc::channel();
    let mut app = AppState::new(lyrics_rx, lyrics_tx, search_rx, search_tx.clone(), jobs_rx, jobs_tx);
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
    app.search.status = SearchStatus::Indexing { scanned: 0 };
    spawn_indexer(app.root_dir.clone(), search_tx);

    if let Ok(Some(tracks)) = fs::detect_loose_tracks(&app.current_dir) {
        app.active_album_dir = Some(app.current_dir.clone());
        app.album_entries = tracks;
        app.album_selected = 0;
    }

    loop {
        let event = match input::poll_event(Duration::from_millis(10), &app.input_mode) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        if matches!(event, AppEvent::Tick) {
            handle_tick(&mut app);
        }

        match &mut app.input_mode {
            InputMode::Command(_) => {
                match event {
                    AppEvent::ExitCommandMode => {
                        app.input_mode = InputMode::Normal;
                    }

                    AppEvent::CommandChar(c) => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            insert_char(&mut cmd.buffer, &mut cmd.cursor, c);
                        }
                    }

                    AppEvent::CommandBackspace => {
                        if let InputMode::Command(cmd) = &mut app.input_mode
                        {
                            backspace_char(&mut cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveLeft => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            move_cursor_left(&cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveRight => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            move_cursor_right(&cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextDelete => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            delete_char(&mut cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveHome => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            cmd.cursor = 0;
                        }
                    }

                    AppEvent::TextMoveEnd => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            cmd.cursor = cmd.buffer.len();
                        }
                    }

                    AppEvent::SubmitCommand => {
                        if let InputMode::Command(cmd) = &app.input_mode {
                            let raw = cmd.buffer.clone();

                            let command = parse_command(&raw);

                            tracing::debug!(
                                raw = %raw,
                                parsed = ?command,
                                "Parsed command"
                            );

                            match command {
                                Command::Download { url } => {
                                    tracing::info!(url = %url, "Spawning download job");
                                    app.active_download_url = Some(url.clone());
                                    let tx = app.jobs_tx.clone();
                                    let staging = download_staging_dir();
                                    std::thread::spawn(move || {
                                        spawn_playlist_download(url, staging, tx);
                                    });
                                }

                                Command::SearchSong { query } => {
                                    spawn_youtube_search(&mut app, query, crate::youtube::SearchKind::Song, 0);
                                }

                                Command::SearchAlbum { query } => {
                                    spawn_youtube_search(&mut app, query, crate::youtube::SearchKind::Album, 0);
                                }

                                Command::SearchArtist { query } => {
                                    spawn_youtube_search(&mut app, query, crate::youtube::SearchKind::Artist, 0);
                                }

                                Command::Unknown(input) => {
                                    tracing::warn!(
                                        input = %input,
                                        "Unknown command"
                                    );
                                }
                            }
                        }

                        app.input_mode = InputMode::Normal;
                    }

                    // Ignore all other events while in command mode
                    _ => {}
                }
            }

            InputMode::Search => match event {
                AppEvent::ExitSearchMode => {
                    restore_search_context(&mut app);
                    app.input_mode = InputMode::Normal;
                }

                AppEvent::SearchChar(c) => {
                    insert_char(&mut app.search.query, &mut app.search.cursor, c);
                    refresh_search_results(&mut app);
                }

                AppEvent::SearchBackspace => {
                    backspace_char(&mut app.search.query, &mut app.search.cursor);
                    refresh_search_results(&mut app);
                }

                AppEvent::SearchMoveUp => {
                    if app.search.selected > 0 {
                        app.search.selected -= 1;
                        app.selection_anchor_tick = app.ui_tick;
                    }
                }

                AppEvent::SearchMoveDown => {
                    if app.search.selected + 1 < app.search.results.len() {
                        app.search.selected += 1;
                        app.selection_anchor_tick = app.ui_tick;
                    }
                }

                AppEvent::SearchActivate => {
                    let path = app
                        .search
                        .results
                        .get(app.search.selected)
                        .map(|entry| entry.path.clone());

                    if let Some(path) = path {
                        push_navigation_history(&mut app);
                        jump_to_track_path(&mut app, &path);
                        let index = app.album_selected;
                        play_album_index(&mut app, index);
                        app.input_mode = InputMode::Normal;
                    }
                }

                AppEvent::TextMoveLeft => {
                    move_cursor_left(&app.search.query, &mut app.search.cursor);
                }

                AppEvent::TextMoveRight => {
                    move_cursor_right(&app.search.query, &mut app.search.cursor);
                }

                AppEvent::TextDelete => {
                    delete_char(&mut app.search.query, &mut app.search.cursor);
                    refresh_search_results(&mut app);
                }

                AppEvent::TextMoveHome => {
                    app.search.cursor = 0;
                }

                AppEvent::TextMoveEnd => {
                    app.search.cursor = app.search.query.len();
                }

                AppEvent::Tick => {}

                // Ignore non-search events while the search bar is focused.
                _ => {}
            },

            // -----------------------------------------------------------------
            InputMode::Normal => {
                match event {
                    AppEvent::EnterSearchMode => {
                        app.search.query.clear();
                        app.search.cursor = 0;
                        app.search.selected = 0;
                        app.search.last_focus = app.focus;
                        app.search.last_browser_dir = app.current_dir.clone();
                        app.search.last_browser_selected = app.selected_index;
                        app.search.last_active_album_dir = app.active_album_dir.clone();
                        app.search.last_album_entries = app.album_entries.clone();
                        app.search.last_album_selected = app.album_selected;
                        refresh_search_results(&mut app);
                        app.input_mode = InputMode::Search;
                    }
                    AppEvent::EnterCommandMode => {
                        app.input_mode = InputMode::Command(CommandState {
                            buffer: String::new(),
                            cursor: 0,
                        });
                    }
                    AppEvent::Quit => {
                        app.player.shutdown();
                        app.now_playing = None;
                        break;
                    }

                    AppEvent::Tick => {}

                    // -----------------------------------------------------------------
                    // Focus switching
                    AppEvent::FocusBrowser => {
                        if app.focus != FocusPane::Browser {
                            push_navigation_history(&mut app);
                            app.focus = FocusPane::Browser;
                        }
                    }
                    AppEvent::FocusAlbum => {
                        if (app.active_album_dir.is_some() || app.current_dir == app.root_dir)
                            && app.focus != FocusPane::Album
                        {
                            push_navigation_history(&mut app);
                            app.focus = FocusPane::Album;
                        }
                    }
                    AppEvent::FocusLyrics => {
                        let current_index = if let LyricsStatus::Loaded(lyrics) = &app.lyrics {
                            Some(lyrics.current_index)
                        } else {
                            None
                        };

                        if let Some(current_index) = current_index {
                            if app.focus != FocusPane::Lyrics {
                                push_navigation_history(&mut app);
                            }
                            app.lyric_scroll = current_index;
                            app.focus = FocusPane::Lyrics;
                        }
                    }

                    // -----------------------------------------------------------------
                    // Navigation (focus-dependent)
                    AppEvent::MoveUp => match app.focus {
                        FocusPane::Browser => {
                            if app.selected_index > 0 {
                                app.selected_index -= 1;
                                app.selection_anchor_tick = app.ui_tick;
                            }
                        }
                        FocusPane::Album => {
                            if app.album_selected > 0 {
                                app.album_selected -= 1;
                                app.selection_anchor_tick = app.ui_tick;
                            }
                        }
                        FocusPane::Lyrics => {
                            app.lyric_scroll = app.lyric_scroll.saturating_sub(1);
                        }
                        FocusPane::YoutubeResults => {
                            if app.youtube_selected > 0 {
                                app.youtube_selected -= 1;
                            }
                        }
                    },

                    AppEvent::MoveDown => match app.focus {
                        FocusPane::Browser => {
                            let dir_count = app.browser_entries.iter().filter(|e| e.is_dir).count();
                            if app.selected_index + 1 < dir_count {
                                app.selected_index += 1;
                                app.selection_anchor_tick = app.ui_tick;
                            }
                        }
                        FocusPane::Album => {
                            if app.album_selected + 1 < app.album_entries.len() {
                                app.album_selected += 1;
                                app.selection_anchor_tick = app.ui_tick;
                            }
                        }
                        FocusPane::Lyrics => {
                            if let LyricsStatus::Loaded(lyrics) = &app.lyrics
                                && app.lyric_scroll + 1 < lyrics.lines.len()
                            {
                                app.lyric_scroll += 1;
                            }
                        }
                        FocusPane::YoutubeResults => {
                            let max = if app.youtube_has_more {
                                app.youtube_results.len() // can reach the "Load more" row
                            } else {
                                app.youtube_results.len().saturating_sub(1)
                            };
                            if app.youtube_selected < max {
                                app.youtube_selected += 1;
                            }
                        }
                    },

                    // -----------------------------------------------------------------
                    AppEvent::NavigateBack => {
                        if app.focus == FocusPane::YoutubeResults {
                            app.focus = FocusPane::Browser;
                        } else if !pop_navigation_history(&mut app) {
                            app.set_status(StatusLevel::Info, "No previous view", Some(250));
                        }
                    }

                    // -----------------------------------------------------------------
                    AppEvent::JumpToNowPlaying => {
                        let Some(track_path) = app.player.current_track.clone() else {
                            continue;
                        };

                        let Some(album_dir) = track_path.parent() else {
                            continue;
                        };

                        if !album_dir.starts_with(&app.root_dir) {
                            continue;
                        }

                        push_navigation_history(&mut app);

                        if let Ok(Some(tracks)) = fs::detect_album(album_dir) {
                            app.active_album_dir = Some(album_dir.to_path_buf());
                            app.album_entries = tracks;
                            app.album_selected = track_path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .and_then(|name| {
                                    app.album_entries.iter().position(|e| e.name == name)
                                })
                                .unwrap_or(0);
                            app.focus = FocusPane::Album;
                        }

                        let browser_dir = album_dir.parent().unwrap_or(&app.root_dir);
                        app.current_dir = browser_dir.to_path_buf();
                        app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
                        app.selected_index = 0;
                        app.selection_anchor_tick = app.ui_tick;
                    }

                    // -----------------------------------------------------------------
                    // Player controls
                    AppEvent::Activate => match app.focus {
                        FocusPane::Browser => {
                            let browser_dirs: Vec<_> = app
                                .browser_entries
                                .iter()
                                .filter(|e| e.is_dir)
                                .cloned()
                                .collect();

                            let Some(entry) = browser_dirs.get(app.selected_index) else {
                                continue;
                            };

                            let new_path = app.current_dir.join(&entry.name);
                            if !new_path.starts_with(&app.root_dir) {
                                continue;
                            }

                            push_navigation_history(&mut app);

                            if let Ok(Some(tracks)) = fs::detect_album(&new_path) {
                                app.active_album_dir = Some(new_path);
                                app.album_entries = tracks;
                                app.album_selected = 0;
                                app.focus = FocusPane::Album;
                            } else {
                                app.current_dir = new_path;
                                app.browser_entries =
                                    fs::read_dir(&app.current_dir).unwrap_or_default();
                                app.selected_index = 0;
                                app.selection_anchor_tick = app.ui_tick;

                                // Refresh album pane for directories that contain loose tracks
                                if let Ok(Some(tracks)) = fs::detect_loose_tracks(&app.current_dir)
                                {
                                    app.active_album_dir = Some(app.current_dir.clone());
                                    app.album_entries = tracks;
                                    app.album_selected = 0;
                                } else {
                                    // Clear album pane if this directory has no playable tracks
                                    app.active_album_dir = None;
                                    app.album_entries.clear();
                                    app.album_selected = 0;
                                }
                            }
                        }

                        FocusPane::Album => {
                            let index = app.album_selected;
                            play_album_index(&mut app, index);
                        }
                        FocusPane::Lyrics => {}
                        FocusPane::YoutubeResults => {
                            // The virtual "Load more" row sits after all real results
                            let load_more_idx = app.youtube_results.len();
                            if app.youtube_selected == load_more_idx && app.youtube_has_more {
                                let query = app.youtube_query.clone();
                                let kind = app.youtube_search_kind;
                                let next_page = app.youtube_page + 1;
                                spawn_youtube_search(&mut app, query, kind, next_page);
                            } else if let Some(result) = app.youtube_results.get(app.youtube_selected) {
                                let url = result.url.clone();
                                let title = result.title.clone();
                                tracing::info!(url = %url, "Queuing download from YouTube results");
                                app.active_download_url = Some(url.clone());
                                app.set_status(StatusLevel::Info, format!("Downloading: {title}"), None);
                                let tx = app.jobs_tx.clone();
                                let staging = download_staging_dir();
                                std::thread::spawn(move || {
                                    spawn_playlist_download(url, staging, tx);
                                });
                                app.focus = FocusPane::Browser;
                            }
                        }
                    },

                    AppEvent::TogglePause => app.player.toggle_pause(),
                    AppEvent::SeekForward => app.player.seek(5),
                    AppEvent::SeekBackward => app.player.seek(-5),
                    AppEvent::Stop => {
                        app.clear_playback();
                    }
                    AppEvent::NextTrack => play_next_or_stop(&mut app),
                    AppEvent::PrevTrack => {
                        let restart_current = app
                            .player
                            .metrics
                            .position
                            .map(|p| p > 2.0)
                            .unwrap_or(false);

                        let target = if restart_current || app.album_selected == 0 {
                            app.album_selected
                        } else {
                            app.album_selected - 1
                        };

                        play_album_index(&mut app, target);
                    }

                    // Ignore command-mode events in normal mode
                    AppEvent::ExitCommandMode
                    | AppEvent::CommandChar(_)
                    | AppEvent::CommandBackspace
                    | AppEvent::SubmitCommand
                    | AppEvent::TextMoveLeft
                    | AppEvent::TextMoveRight
                    | AppEvent::TextDelete
                    | AppEvent::TextMoveHome
                    | AppEvent::TextMoveEnd
                    | AppEvent::ExitSearchMode
                    | AppEvent::SearchChar(_)
                    | AppEvent::SearchBackspace
                    | AppEvent::SearchMoveUp
                    | AppEvent::SearchMoveDown
                    | AppEvent::SearchActivate => {}
                }
            }
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn test_app() -> AppState {
        let (lyrics_tx, lyrics_rx) = channel();
        let (search_tx, search_rx) = channel();
        let (jobs_tx, jobs_rx) = channel();
        AppState::new(lyrics_rx, lyrics_tx, search_rx, search_tx, jobs_rx, jobs_tx)
    }

    #[test]
    fn previous_and_next_char_boundaries_handle_unicode() {
        let text = "A—B";

        assert_eq!(next_char_boundary(text, 0), 1);
        assert_eq!(next_char_boundary(text, 1), 4);
        assert_eq!(previous_char_boundary(text, 4), 1);
        assert_eq!(previous_char_boundary(text, text.len()), 4);
    }

    #[test]
    fn insert_and_backspace_char_track_utf8_cursor_positions() {
        let mut buffer = String::from("AB");
        let mut cursor = 1;

        insert_char(&mut buffer, &mut cursor, '—');
        assert_eq!(buffer, "A—B");
        assert_eq!(cursor, 4);

        backspace_char(&mut buffer, &mut cursor);
        assert_eq!(buffer, "AB");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn delete_char_removes_full_unicode_scalar() {
        let mut buffer = String::from("A—B");
        let mut cursor = 1;

        delete_char(&mut buffer, &mut cursor);
        assert_eq!(buffer, "AB");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn cursor_movement_stops_at_character_boundaries() {
        let buffer = String::from("A—B");
        let mut cursor = 0;

        move_cursor_right(&buffer, &mut cursor);
        assert_eq!(cursor, 1);

        move_cursor_right(&buffer, &mut cursor);
        assert_eq!(cursor, 4);

        move_cursor_left(&buffer, &mut cursor);
        assert_eq!(cursor, 1);

        move_cursor_left(&buffer, &mut cursor);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn navigation_history_dedupes_and_stays_bounded() {
        let mut app = test_app();
        app.current_dir = PathBuf::from("/tmp/root");

        push_navigation_history(&mut app);
        push_navigation_history(&mut app);
        assert_eq!(app.navigation_history.len(), 1);

        for index in 0..(NAVIGATION_HISTORY_LIMIT + 5) {
            app.current_dir = PathBuf::from(format!("/tmp/root/{index}"));
            push_navigation_history(&mut app);
        }

        assert_eq!(app.navigation_history.len(), NAVIGATION_HISTORY_LIMIT);
        assert_eq!(
            app.navigation_history.first().map(|state| state.current_dir.clone()),
            Some(PathBuf::from("/tmp/root/5"))
        );
        assert_eq!(
            app.navigation_history.last().map(|state| state.current_dir.clone()),
            Some(PathBuf::from(format!(
                "/tmp/root/{}",
                NAVIGATION_HISTORY_LIMIT + 4
            )))
        );
    }
}
