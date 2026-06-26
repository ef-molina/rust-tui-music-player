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
mod config;
mod download;
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

use crate::app::navigation::{
    jump_to_track_path, pop_navigation_history, push_navigation_history, restore_search_context,
};
use crate::app::search_helpers::{merge_search_entries, refresh_search_results};
use crate::event::commands::{Command, active_command_spec, parse_command, top_command_spec};
use crate::event::jobs::JobResult;
use crate::input::text::{
    backspace_char, delete_char, insert_char, move_cursor_left, move_cursor_right,
};
use crate::lyrics::LyricsState;
use crate::lyrics_fetch::LyricsFetchResult;
use crate::search::{SearchMessage, spawn_index_update, spawn_indexer};
use crate::youtube::spawn_youtube_search;
use app::{
    AppState, CommandState, FocusPane, InputMode, LyricsStatus, RepeatMode, SearchStatus,
    StatusLevel,
};
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use event::AppEvent;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs::File;
use std::io::stdout;
use std::time::Duration;
use tracing::{debug, trace};
use tracing_subscriber::EnvFilter;

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

    // Verify yt-dlp is available before entering raw mode so the error is readable
    if std::process::Command::new("yt-dlp")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("Error: yt-dlp is not installed or not in PATH.");
        eprintln!("Install it with:  brew install yt-dlp");
        eprintln!("See: https://github.com/yt-dlp/yt-dlp");
        std::process::exit(1);
    }

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
    let Some(album_dir) = &app.album.dir else {
        return;
    };
    let Some(entry) = app.album.entries.get(index) else {
        return;
    };

    let track_path = album_dir.join(&entry.name);

    debug!(
        path = %track_path.display(),
        "Starting playback for track"
    );

    app.album.selected = index;
    app.player.load(track_path.clone());

    crate::lyrics::orchestrator::start_lyrics_load(app, &track_path);
}

fn play_next_or_stop(app: &mut AppState) {
    let count = app.album.entries.len();
    if count == 0 {
        app.player.stop();
        app.clear_playback();
        return;
    }

    match app.playback.repeat_mode {
        RepeatMode::Track => {
            // Replay the same track
            play_album_index(app, app.album.selected);
        }
        RepeatMode::Album => {
            let next = if app.playback.shuffle {
                shuffle_next_index(app)
            } else {
                (app.album.selected + 1) % count
            };
            play_album_index(app, next);
        }
        RepeatMode::Off => {
            if app.playback.shuffle {
                let next = shuffle_next_index(app);
                // In shuffle without repeat, stop when we've cycled through all tracks.
                // Simple approach: just pick a random different track.
                play_album_index(app, next);
            } else {
                let next = app.album.selected + 1;
                if next < count {
                    play_album_index(app, next);
                } else {
                    app.player.stop();
                    app.clear_playback();
                }
            }
        }
    }
}

/// Pick a pseudo-random track index different from the current one.
fn shuffle_next_index(app: &AppState) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let count = app.album.entries.len();
    if count <= 1 {
        return 0;
    }
    let mut h = DefaultHasher::new();
    app.ui.ui_tick.hash(&mut h);
    app.album.selected.hash(&mut h);
    let candidate = (h.finish() as usize) % (count - 1);
    // Shift to avoid replaying the current track
    if candidate >= app.album.selected {
        candidate + 1
    } else {
        candidate
    }
}

fn handle_tick(app: &mut AppState) {
    app.ui.ui_tick = app.ui.ui_tick.wrapping_add(1);
    app.clear_expired_status();
    app.player.poll_metrics();

    while let Ok(message) = app.channels.search_rx.try_recv() {
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
                    format!(
                        "Library index ready: {} tracks",
                        app.search.index_entries.len()
                    ),
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

    if let Ok(result) = app.channels.lyrics_rx.try_recv() {
        match result {
            LyricsFetchResult::RawLrc {
                request_id,
                path,
                contents,
            } => {
                let is_current = request_id == app.lyrics_state.request_id;

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
                        app.lyrics_state.pending_cache_key = None;

                        if let Ok(lines) = crate::lyrics::parse_lrc(&lrc_path) {
                            if !lines.is_empty() {
                                app.lyrics_state.status =
                                    LyricsStatus::Loaded(LyricsState::new(lines));
                                app.lyrics_state.scroll = 0;
                                app.set_status(StatusLevel::Success, "Lyrics loaded", Some(250));
                            } else {
                                app.lyrics_state.status = LyricsStatus::None;
                            }
                        } else {
                            app.lyrics_state.status = LyricsStatus::None;
                        }
                    } else {
                        trace!(
                            request_id,
                            current = app.lyrics_state.request_id,
                            "Stale lyrics fetch — saved to disk only"
                        );
                    }
                } else if is_current {
                    app.lyrics_state.status = LyricsStatus::None;
                    app.set_status(
                        StatusLevel::Warning,
                        "Couldn't save fetched lyrics",
                        Some(350),
                    );
                }
            }
            LyricsFetchResult::NotFound { request_id } => {
                if request_id != app.lyrics_state.request_id {
                    trace!(
                        request_id,
                        current = app.lyrics_state.request_id,
                        "Ignoring stale lyrics fetch result"
                    );
                } else {
                    if let Some(key) = app.lyrics_state.pending_cache_key.take() {
                        trace!(
                            artist = %key.artist,
                            title = %key.title,
                            "Inserting lyrics negative cache entry"
                        );
                        app.lyrics_state.negative_cache.insert(key);
                    }

                    app.lyrics_state.status = LyricsStatus::None;
                    app.set_status(StatusLevel::Warning, "No lyrics found", Some(250));
                }
            }
            LyricsFetchResult::Failed { request_id } => {
                if request_id != app.lyrics_state.request_id {
                    trace!(
                        request_id,
                        current = app.lyrics_state.request_id,
                        "Ignoring stale lyrics fetch result"
                    );
                } else {
                    app.lyrics_state.pending_cache_key = None;
                    app.lyrics_state.status = LyricsStatus::None;
                    app.set_status(StatusLevel::Warning, "Lyrics fetch failed", Some(350));
                }
            }
        }
    }

    if let Ok(job) = app.channels.jobs_rx.try_recv() {
        match job {
            JobResult::DownloadStarted { url, title, pid } => {
                app.downloads.active_url = Some(url.clone());
                app.downloads.active_pid = Some(pid);
                app.set_status(
                    StatusLevel::Info,
                    format!("Downloading: {}", truncate_status_url(&url)),
                    None,
                );
                // Add to queue (cap at 20)
                app.downloads.jobs.push(crate::app::DownloadJob {
                    title,
                    url: url.clone(),
                    status: crate::app::DownloadJobStatus::Active,
                });
                if app.downloads.jobs.len() > 20 {
                    app.downloads.jobs.remove(0);
                }
                tracing::info!(url = %url, pid, "Download job started");
            }

            JobResult::DownloadFinished { url, temp_path } => {
                app.downloads.active_url = None;
                if !url.is_empty() {
                    app.downloads.active_progress = None;
                    app.downloads.active_pid = None;
                    // Mark job done in queue
                    if let Some(job) = app.downloads.jobs.iter_mut().find(|j| j.url == url) {
                        job.status = crate::app::DownloadJobStatus::Done;
                    }
                }
                tracing::info!(
                    url = %url,
                    path = %temp_path.display(),
                    "Download job finished"
                );

                let library_root = app.browser_state.root_dir.clone();

                match crate::fs::normalize::normalize_downloaded_track(&temp_path, &library_root) {
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
                            app.browser_state.root_dir.clone(),
                            normalized.final_path.clone(),
                            app.channels.search_tx.clone(),
                        );

                        // Refresh browser if the new track landed in or under the current dir
                        let track_album_dir = normalized.final_path.parent();
                        let track_artist_dir = track_album_dir.and_then(|d| d.parent());

                        let browser_stale = track_artist_dir
                            .map(|d| d == app.browser_state.current_dir)
                            .unwrap_or(false);

                        if browser_stale {
                            app.browser_state.entries =
                                fs::read_dir(&app.browser_state.current_dir).unwrap_or_default();
                        }

                        // Refresh album pane if this track landed in the active album dir
                        let album_stale = track_album_dir
                            .zip(app.album.dir.as_deref())
                            .map(|(track_dir, album_dir)| track_dir == album_dir)
                            .unwrap_or(false);

                        if album_stale
                            && let Ok(Some(tracks)) =
                                fs::detect_album(app.album.dir.as_deref().unwrap())
                        {
                            app.album.entries = tracks;
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
                app.downloads.active_progress = Some(crate::app::DownloadState {
                    track_title,
                    track_index,
                    total_tracks,
                    overall_percent,
                });
                // Keep the URL visible in the status bar label
                app.downloads.active_url = Some(url);
            }
            JobResult::DownloadFailed { url, error } => {
                app.downloads.active_url = None;
                app.downloads.active_progress = None;
                app.downloads.active_pid = None;
                if let Some(job) = app.downloads.jobs.iter_mut().find(|j| j.url == url) {
                    job.status = crate::app::DownloadJobStatus::Failed(error.clone());
                }
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
                app.youtube.searching = false;
                let appending = app.youtube.page > 0 && !app.youtube.results.is_empty();
                let new_count = results.len();
                if appending {
                    app.youtube.results.extend(results);
                } else {
                    app.youtube.results = results;
                    app.youtube.selected = 0;
                }
                app.youtube.has_more = has_more;
                app.ui.focus = FocusPane::YoutubeResults;
                let total = app.youtube.results.len();
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
                app.youtube.searching = false;
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
        (&mut app.lyrics_state.status, app.player.metrics.position)
    {
        let prev_index = lyrics.current_index;
        lyrics.update(position);

        if lyrics.current_index != prev_index {
            trace!(
                index = lyrics.current_index,
                time = position,
                "Lyric line advanced"
            );
            app.lyrics_state.scroll = lyrics.current_index;
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
    let cfg = config::load();
    let (lyrics_tx, lyrics_rx) = std::sync::mpsc::channel();
    let (search_tx, search_rx) = std::sync::mpsc::channel();
    let (jobs_tx, jobs_rx) = std::sync::mpsc::channel();
    let search_tx_clone = search_tx.clone();
    let mut app = AppState::new(
        &cfg,
        crate::app::Channels {
            lyrics_rx,
            lyrics_tx,
            search_rx,
            search_tx: search_tx_clone,
            jobs_rx,
            jobs_tx,
        },
    );
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    app.browser_state.entries = fs::read_dir(&app.browser_state.current_dir).unwrap_or_default();
    app.search.status = SearchStatus::Indexing { scanned: 0 };
    spawn_indexer(app.browser_state.root_dir.clone(), search_tx);

    if let Ok(Some(tracks)) = fs::detect_loose_tracks(&app.browser_state.current_dir) {
        app.album.dir = Some(app.browser_state.current_dir.clone());
        app.album.entries = tracks;
        app.album.selected = 0;
    }

    loop {
        let event = match input::poll_event(Duration::from_millis(10), &app.ui.input_mode) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        if matches!(event, AppEvent::Tick) {
            handle_tick(&mut app);
        }

        match &mut app.ui.input_mode {
            InputMode::Command(_) => {
                match event {
                    AppEvent::ExitCommandMode => {
                        app.ui.input_mode = InputMode::Normal;
                    }

                    AppEvent::CommandChar(c) => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            insert_char(&mut cmd.buffer, &mut cmd.cursor, c);
                        }
                    }

                    AppEvent::CommandBackspace => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            backspace_char(&mut cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveLeft => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            move_cursor_left(&cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveRight => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            move_cursor_right(&cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextDelete => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            delete_char(&mut cmd.buffer, &mut cmd.cursor);
                        }
                    }

                    AppEvent::TextMoveHome => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            cmd.cursor = 0;
                        }
                    }

                    AppEvent::TextMoveEnd => {
                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
                            cmd.cursor = cmd.buffer.len();
                        }
                    }

                    AppEvent::SubmitCommand => {
                        let mut close_command_mode = false;
                        let mut pending_status: Option<(StatusLevel, String, Option<u64>)> = None;

                        if let InputMode::Command(cmd) = &mut app.ui.input_mode {
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
                                    app.downloads.active_url = Some(url.clone());
                                    let tx = app.channels.jobs_tx.clone();
                                    let staging = download::staging_dir();
                                    let browser = app.browser.clone();
                                    let title = truncate_status_url(&url);
                                    std::thread::spawn(move || {
                                        download::spawn_playlist_download(
                                            url, title, staging, browser, tx,
                                        );
                                    });

                                    close_command_mode = true;
                                }

                                Command::SearchSong { query } => {
                                    spawn_youtube_search(
                                        &mut app,
                                        query,
                                        crate::youtube::SearchKind::Song,
                                        0,
                                    );
                                    close_command_mode = true;
                                }

                                Command::SearchAlbum { query } => {
                                    spawn_youtube_search(
                                        &mut app,
                                        query,
                                        crate::youtube::SearchKind::Album,
                                        0,
                                    );
                                    close_command_mode = true;
                                }

                                Command::SearchArtist { query } => {
                                    spawn_youtube_search(
                                        &mut app,
                                        query,
                                        crate::youtube::SearchKind::Artist,
                                        0,
                                    );
                                    close_command_mode = true;
                                }

                                Command::Unknown(_) => {
                                    if let Some(spec) = top_command_spec(&raw)
                                        && active_command_spec(&raw).is_none()
                                    {
                                        cmd.buffer = format!("{} ", spec.name);
                                        cmd.cursor = cmd.buffer.len();
                                        pending_status = Some((
                                            StatusLevel::Info,
                                            format!("Command selected: {}", spec.syntax),
                                            Some(250),
                                        ));
                                    } else if let Some(spec) = active_command_spec(&raw) {
                                        pending_status = Some((
                                            StatusLevel::Warning,
                                            format!("{} requires more input", spec.syntax),
                                            Some(350),
                                        ));
                                    } else {
                                        tracing::warn!(input = %raw, "Unknown command");
                                        pending_status = Some((
                                            StatusLevel::Warning,
                                            format!("Unknown command: {}", raw.trim()),
                                            Some(350),
                                        ));
                                    }
                                }
                            }
                        }

                        if let Some((level, text, ttl)) = pending_status {
                            app.set_status(level, text, ttl);
                        }

                        if close_command_mode {
                            app.ui.input_mode = InputMode::Normal;
                        }
                    }

                    // Ignore all other events while in command mode
                    _ => {}
                }
            }

            InputMode::Search => match event {
                AppEvent::ExitSearchMode => {
                    restore_search_context(&mut app);
                    app.ui.input_mode = InputMode::Normal;
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
                        app.ui.selection_anchor_tick = app.ui.ui_tick;
                    }
                }

                AppEvent::SearchMoveDown => {
                    if app.search.selected + 1 < app.search.results.len() {
                        app.search.selected += 1;
                        app.ui.selection_anchor_tick = app.ui.ui_tick;
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
                        let index = app.album.selected;
                        play_album_index(&mut app, index);
                        app.ui.input_mode = InputMode::Normal;
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
                        app.search.last_focus = app.ui.focus;
                        app.search.last_browser_dir = app.browser_state.current_dir.clone();
                        app.search.last_browser_selected = app.browser_state.selected_index;
                        app.search.last_active_album_dir = app.album.dir.clone();
                        app.search.last_album_entries = app.album.entries.clone();
                        app.search.last_album_selected = app.album.selected;
                        refresh_search_results(&mut app);
                        app.ui.input_mode = InputMode::Search;
                    }
                    AppEvent::EnterCommandMode => {
                        app.ui.input_mode = InputMode::Command(CommandState {
                            buffer: String::new(),
                            cursor: 0,
                        });
                    }
                    AppEvent::Quit => {
                        app.player.shutdown();
                        app.playback.now_playing = None;
                        break;
                    }

                    AppEvent::Tick => {}

                    // -----------------------------------------------------------------
                    // Focus switching
                    AppEvent::FocusBrowser => {
                        if app.ui.focus != FocusPane::Browser {
                            push_navigation_history(&mut app);
                            app.ui.focus = FocusPane::Browser;
                        }
                    }
                    AppEvent::FocusAlbum => {
                        if (app.album.dir.is_some()
                            || app.browser_state.current_dir == app.browser_state.root_dir)
                            && app.ui.focus != FocusPane::Album
                        {
                            push_navigation_history(&mut app);
                            app.ui.focus = FocusPane::Album;
                        }
                    }
                    AppEvent::FocusLyrics => {
                        let current_index =
                            if let LyricsStatus::Loaded(lyrics) = &app.lyrics_state.status {
                                Some(lyrics.current_index)
                            } else {
                                None
                            };

                        if let Some(current_index) = current_index {
                            if app.ui.focus != FocusPane::Lyrics {
                                push_navigation_history(&mut app);
                            }
                            app.lyrics_state.scroll = current_index;
                            app.ui.focus = FocusPane::Lyrics;
                        }
                    }

                    // -----------------------------------------------------------------
                    // Navigation (focus-dependent)
                    AppEvent::MoveUp => match app.ui.focus {
                        FocusPane::Browser => {
                            if app.browser_state.selected_index > 0 {
                                app.browser_state.selected_index -= 1;
                                app.ui.selection_anchor_tick = app.ui.ui_tick;
                            }
                        }
                        FocusPane::Album => {
                            if app.album.selected > 0 {
                                app.album.selected -= 1;
                                app.ui.selection_anchor_tick = app.ui.ui_tick;
                            }
                        }
                        FocusPane::Lyrics => {
                            app.lyrics_state.scroll = app.lyrics_state.scroll.saturating_sub(1);
                        }
                        FocusPane::YoutubeResults => {
                            if app.youtube.selected > 0 {
                                app.youtube.selected -= 1;
                            }
                        }
                    },

                    AppEvent::MoveDown => match app.ui.focus {
                        FocusPane::Browser => {
                            let dir_count = app
                                .browser_state
                                .entries
                                .iter()
                                .filter(|e| e.is_dir)
                                .count();
                            if app.browser_state.selected_index + 1 < dir_count {
                                app.browser_state.selected_index += 1;
                                app.ui.selection_anchor_tick = app.ui.ui_tick;
                            }
                        }
                        FocusPane::Album => {
                            if app.album.selected + 1 < app.album.entries.len() {
                                app.album.selected += 1;
                                app.ui.selection_anchor_tick = app.ui.ui_tick;
                            }
                        }
                        FocusPane::Lyrics => {
                            if let LyricsStatus::Loaded(lyrics) = &app.lyrics_state.status
                                && app.lyrics_state.scroll + 1 < lyrics.lines.len()
                            {
                                app.lyrics_state.scroll += 1;
                            }
                        }
                        FocusPane::YoutubeResults => {
                            let max = if app.youtube.has_more {
                                app.youtube.results.len() // can reach the "Load more" row
                            } else {
                                app.youtube.results.len().saturating_sub(1)
                            };
                            if app.youtube.selected < max {
                                app.youtube.selected += 1;
                            }
                        }
                    },

                    // -----------------------------------------------------------------
                    AppEvent::NavigateBack => {
                        if app.ui.focus == FocusPane::YoutubeResults {
                            app.ui.focus = FocusPane::Browser;
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

                        if !album_dir.starts_with(&app.browser_state.root_dir) {
                            continue;
                        }

                        push_navigation_history(&mut app);

                        if let Ok(Some(tracks)) = fs::detect_album(album_dir) {
                            app.album.dir = Some(album_dir.to_path_buf());
                            app.album.entries = tracks;
                            app.album.selected = track_path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .and_then(|name| {
                                    app.album.entries.iter().position(|e| e.name == name)
                                })
                                .unwrap_or(0);
                            app.ui.focus = FocusPane::Album;
                        }

                        let browser_dir = album_dir.parent().unwrap_or(&app.browser_state.root_dir);
                        app.browser_state.current_dir = browser_dir.to_path_buf();
                        app.browser_state.entries =
                            fs::read_dir(&app.browser_state.current_dir).unwrap_or_default();
                        app.browser_state.selected_index = 0;
                        app.ui.selection_anchor_tick = app.ui.ui_tick;
                    }

                    // -----------------------------------------------------------------
                    // Player controls
                    AppEvent::Activate => match app.ui.focus {
                        FocusPane::Browser => {
                            let browser_dirs: Vec<_> = app
                                .browser_state
                                .entries
                                .iter()
                                .filter(|e| e.is_dir)
                                .cloned()
                                .collect();

                            let Some(entry) = browser_dirs.get(app.browser_state.selected_index)
                            else {
                                continue;
                            };

                            let new_path = app.browser_state.current_dir.join(&entry.name);
                            if !new_path.starts_with(&app.browser_state.root_dir) {
                                continue;
                            }

                            push_navigation_history(&mut app);

                            if let Ok(Some(tracks)) = fs::detect_album(&new_path) {
                                app.album.dir = Some(new_path);
                                app.album.entries = tracks;
                                app.album.selected = 0;
                                app.ui.focus = FocusPane::Album;
                            } else {
                                app.browser_state.current_dir = new_path;
                                app.browser_state.entries =
                                    fs::read_dir(&app.browser_state.current_dir)
                                        .unwrap_or_default();
                                app.browser_state.selected_index = 0;
                                app.ui.selection_anchor_tick = app.ui.ui_tick;

                                // Refresh album pane for directories that contain loose tracks
                                if let Ok(Some(tracks)) =
                                    fs::detect_loose_tracks(&app.browser_state.current_dir)
                                {
                                    app.album.dir = Some(app.browser_state.current_dir.clone());
                                    app.album.entries = tracks;
                                    app.album.selected = 0;
                                } else {
                                    // Clear album pane if this directory has no playable tracks
                                    app.album.dir = None;
                                    app.album.entries.clear();
                                    app.album.selected = 0;
                                }
                            }
                        }

                        FocusPane::Album => {
                            let index = app.album.selected;
                            play_album_index(&mut app, index);
                        }
                        FocusPane::Lyrics => {}
                        FocusPane::YoutubeResults => {
                            // The virtual "Load more" row sits after all real results
                            let load_more_idx = app.youtube.results.len();
                            if app.youtube.selected == load_more_idx && app.youtube.has_more {
                                let query = app.youtube.query.clone();
                                let kind = app.youtube.search_kind;
                                let next_page = app.youtube.page + 1;
                                spawn_youtube_search(&mut app, query, kind, next_page);
                            } else if let Some(result) =
                                app.youtube.results.get(app.youtube.selected)
                            {
                                let url = result.url.clone();
                                let title = result.title.clone();
                                let kind = result.kind;

                                if kind == crate::youtube::SearchKind::Artist {
                                    // Drill into this artist's albums rather than downloading their channel
                                    tracing::info!(artist = %title, "Browsing artist albums");
                                    spawn_youtube_search(
                                        &mut app,
                                        title,
                                        crate::youtube::SearchKind::Album,
                                        0,
                                    );
                                } else {
                                    tracing::info!(url = %url, "Queuing download from YouTube results");
                                    app.downloads.active_url = Some(url.clone());
                                    app.set_status(
                                        StatusLevel::Info,
                                        format!("Downloading: {title}"),
                                        None,
                                    );
                                    let tx = app.channels.jobs_tx.clone();
                                    let staging = download::staging_dir();
                                    let browser = app.browser.clone();
                                    let dl_title = title.clone();
                                    std::thread::spawn(move || {
                                        download::spawn_playlist_download(
                                            url, dl_title, staging, browser, tx,
                                        );
                                    });
                                    app.ui.focus = FocusPane::Browser;
                                }
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

                        let target = if restart_current || app.album.selected == 0 {
                            app.album.selected
                        } else {
                            app.album.selected - 1
                        };

                        play_album_index(&mut app, target);
                    }

                    AppEvent::ToggleRepeat => {
                        app.playback.repeat_mode = app.playback.repeat_mode.cycle();
                        app.set_status(
                            StatusLevel::Info,
                            format!("Repeat: {}", app.playback.repeat_mode.label()),
                            Some(200),
                        );
                    }

                    AppEvent::ToggleShuffle => {
                        app.playback.shuffle = !app.playback.shuffle;
                        app.set_status(
                            StatusLevel::Info,
                            format!(
                                "Shuffle: {}",
                                if app.playback.shuffle { "on" } else { "off" }
                            ),
                            Some(200),
                        );
                    }

                    AppEvent::VolumeUp => {
                        app.player.adjust_volume(5);
                        app.set_status(
                            StatusLevel::Info,
                            format!("Volume: {}%", app.player.volume),
                            Some(150),
                        );
                    }

                    AppEvent::VolumeDown => {
                        app.player.adjust_volume(-5);
                        app.set_status(
                            StatusLevel::Info,
                            format!("Volume: {}%", app.player.volume),
                            Some(150),
                        );
                    }

                    AppEvent::ToggleDownloadQueue => {
                        app.downloads.show_queue = !app.downloads.show_queue;
                    }

                    AppEvent::CancelDownload => {
                        if let Some(pid) = app.downloads.active_pid {
                            let _ = std::process::Command::new("kill")
                                .args(["-TERM", &pid.to_string()])
                                .status();
                            let url = app.downloads.active_url.clone().unwrap_or_default();
                            app.downloads.active_url = None;
                            app.downloads.active_progress = None;
                            app.downloads.active_pid = None;
                            if let Some(job) = app.downloads.jobs.iter_mut().find(|j| j.url == url)
                            {
                                job.status = crate::app::DownloadJobStatus::Cancelled;
                            }
                            app.set_status(
                                StatusLevel::Warning,
                                "Download cancelled".to_string(),
                                Some(300),
                            );
                        }
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
