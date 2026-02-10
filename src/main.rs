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
mod ui;

use crate::event::commands::{Command, parse_command};
use crate::event::jobs::JobResult;
use crate::fs::normalize::NormalizedTrack;
use crate::lyrics::{LyricsState, load_for_track};
use crate::lyrics_fetch::LyricsFetchResult;
use crate::lyrics_fetch::lrclib::fetch_lrc;
use crate::metadata::extract_metadata;
use crate::metadata::model::TrackMetadata;
use app::{AppState, CommandState, FocusPane, InputMode, LyricsCacheKey, LyricsStatus, NowPlaying};
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
use std::time::Duration;
use tracing::{debug, trace, warn};
use tracing_subscriber::EnvFilter;

fn download_staging_dir() -> PathBuf {
    std::path::PathBuf::from(
        std::env::var("HOME")
            .map(|h| format!("{}/Downloads/Media/Music/.staging", h))
            .unwrap_or_else(|_| ".staging".into()),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --------------------------------------------------
    // CLI: one-shot normalization mode (EARLY EXIT)
    // --------------------------------------------------
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();

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

fn play_next_or_stop(app: &mut AppState) {
    let next = app.album_selected + 1;

    if next < app.album_entries.len() {
        play_album_index(app, next);
    } else {
        app.player.stop();
        app.clear_playback();
    }
}

/// --------------------------------------------------
/// Main application loop
/// --------------------------------------------------
fn run_app() -> std::io::Result<()> {
    let (lyrics_tx, lyrics_rx) = std::sync::mpsc::channel();
    let (jobs_tx, jobs_rx) = std::sync::mpsc::channel();
    let mut app = AppState::new(lyrics_rx, lyrics_tx, jobs_rx, jobs_tx);
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();

    if let Ok(Some(tracks)) = fs::detect_loose_tracks(&app.current_dir) {
        app.active_album_dir = Some(app.current_dir.clone());
        app.album_entries = tracks;
        app.album_selected = 0;
    }

    loop {
        let in_command = matches!(app.input_mode, InputMode::Command(_));
        let event = match input::poll_event(Duration::from_millis(10), in_command) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        match &mut app.input_mode {
            InputMode::Command(_) => {
                match event {
                    AppEvent::ExitCommandMode => {
                        app.input_mode = InputMode::Normal;
                    }

                    AppEvent::CommandChar(c) => {
                        if let InputMode::Command(cmd) = &mut app.input_mode {
                            cmd.buffer.insert(cmd.cursor, c);
                            cmd.cursor += 1;
                        }
                    }

                    AppEvent::CommandBackspace => {
                        if let InputMode::Command(cmd) = &mut app.input_mode
                            && cmd.cursor > 0
                        {
                            cmd.cursor -= 1;
                            cmd.buffer.remove(cmd.cursor);
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
                                    tracing::info!(
                                        url = %url,
                                        "Spawning download job"
                                    );

                                    let tx = app.jobs_tx.clone();

                                    std::thread::spawn(move || {
                                        tracing::debug!(
                                            url = %url,
                                            "Download thread started"
                                        );

                                        let staging = download_staging_dir();
                                        let _ = std::fs::create_dir_all(&staging);

                                        let output_template =
                                            staging.join("%(title)s [%(id)s].%(ext)s");

                                        let status = std::process::Command::new("yt-dlp")
                                            .arg("-f")
                                            .arg("bestaudio[ext=opus]/bestaudio")
                                            .arg("-x")
                                            .arg("--audio-format")
                                            .arg("opus")
                                            .arg("--audio-quality")
                                            .arg("0")
                                            .arg("--embed-metadata")
                                            .arg("--embed-thumbnail")
                                            .arg("--convert-thumbnails")
                                            .arg("jpg")
                                            .arg("--add-metadata")
                                            .arg("-o")
                                            .arg(output_template)
                                            .arg(&url)
                                            .stdout(Stdio::null())
                                            .stderr(Stdio::null())
                                            .status();

                                        match status {
                                            Ok(s) if s.success() => {
                                                tracing::debug!(
                                                    url = %url,
                                                    "yt-dlp exited successfully"
                                                );

                                                if let Ok(entries) = std::fs::read_dir(&staging)
                                                    && let Some(latest) = entries
                                                        .filter_map(|e| e.ok())
                                                        .max_by_key(|e| {
                                                            e.metadata()
                                                                .and_then(|m| m.modified())
                                                                .ok()
                                                        })
                                                {
                                                    tracing::info!(
                                                        url = %url,
                                                        path = %latest.path().display(),
                                                        "Download finished"
                                                    );

                                                    let _ = tx.send(JobResult::DownloadFinished {
                                                        url,
                                                        temp_path: latest.path(),
                                                    });
                                                    return;
                                                }

                                                tracing::error!(
                                                    url = %url,
                                                    "Download succeeded but no file found"
                                                );

                                                let _ = tx.send(JobResult::DownloadFailed {
                                                    url,
                                                    error: "Download succeeded but no file found"
                                                        .into(),
                                                });
                                            }

                                            Ok(s) => {
                                                tracing::error!(
                                                    url = %url,
                                                    status = ?s.code(),
                                                    "yt-dlp exited with failure"
                                                );

                                                let _ = tx.send(JobResult::DownloadFailed {
                                                    url,
                                                    error: "yt-dlp failed".into(),
                                                });
                                            }

                                            Err(e) => {
                                                tracing::error!(
                                                    url = %url,
                                                    error = %e,
                                                    "Failed to spawn yt-dlp"
                                                );

                                                let _ = tx.send(JobResult::DownloadFailed {
                                                    url,
                                                    error: e.to_string(),
                                                });
                                            }
                                        }
                                    });
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

            // -----------------------------------------------------------------
            InputMode::Normal => {
                match event {
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

                    AppEvent::Tick => {
                        // Update UI tick
                        app.ui_tick = app.ui_tick.wrapping_add(1);
                        app.player.poll_metrics();

                        // Resolve background lyrics fetch (non-blocking)
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

                                    // Always write .lrc file (even if stale)
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

                                        // Only update UI if this is the current track
                                        if is_current {
                                            app.lyrics_pending_cache_key = None;

                                            if let Ok(lines) = crate::lyrics::parse_lrc(&lrc_path) {
                                                if !lines.is_empty() {
                                                    app.lyrics = LyricsStatus::Loaded(
                                                        LyricsState::new(lines),
                                                    );
                                                    app.lyric_scroll = 0;
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
                                        // Write failed AND this was current
                                        app.lyrics = LyricsStatus::None;
                                    }
                                }

                                LyricsFetchResult::NotFound { request_id } => {
                                    if request_id != app.lyrics_request_id {
                                        trace!(
                                            request_id,
                                            current = app.lyrics_request_id,
                                            "Ignoring stale lyrics fetch result"
                                        );
                                        continue;
                                    }

                                    if let Some(key) = app.lyrics_pending_cache_key.take() {
                                        trace!(
                                            artist = %key.artist,
                                            title = %key.title,
                                            "Inserting lyrics negative cache entry"
                                        );
                                        app.lyrics_negative_cache.insert(key);
                                    }

                                    app.lyrics = LyricsStatus::None;
                                }

                                LyricsFetchResult::Failed { request_id } => {
                                    if request_id != app.lyrics_request_id {
                                        trace!(
                                            request_id,
                                            current = app.lyrics_request_id,
                                            "Ignoring stale lyrics fetch result"
                                        );
                                        continue;
                                    }

                                    app.lyrics_pending_cache_key = None;
                                    app.lyrics = LyricsStatus::None;
                                }
                            }
                        }

                        // --------------------------------------------------
                        // Resolve background jobs (downloads, normalization)
                        // --------------------------------------------------
                        if let Ok(job) = app.jobs_rx.try_recv() {
                            match job {
                                crate::event::jobs::JobResult::DownloadStarted { url } => {
                                    tracing::info!(
                                        url = %url,
                                        "Download job started"
                                    );
                                }

                                crate::event::jobs::JobResult::DownloadFinished {
                                    url,
                                    temp_path,
                                } => {
                                    tracing::info!(
                                        url = %url,
                                        path = %temp_path.display(),
                                        "Download job finished"
                                    );

                                    // ---------------------------------------------
                                    // Normalize downloaded track
                                    // ---------------------------------------------
                                    let library_root = app.root_dir.clone();

                                    match crate::fs::normalize::normalize_downloaded_track(
                                        &temp_path,
                                        &library_root,
                                    ) {
                                        Ok(normalized) => {
                                            tracing::info!(
                                                from = %temp_path.display(),
                                                to = %normalized.final_path.display(),
                                                artist = %normalized.artist,
                                                title = %normalized.title,
                                                "Track normalized successfully"
                                            );
                                        }

                                        Err(err) => {
                                            tracing::warn!(
                                                path = %temp_path.display(),
                                                error = %err,
                                                "Track moved but metadata write failed"
                                            );
                                        }
                                    }
                                }

                                crate::event::jobs::JobResult::DownloadFailed { url, error } => {
                                    tracing::error!(
                                        url = %url,
                                        error = %error,
                                        "Download job failed"
                                    );
                                }
                            }
                        }

                        // Update lyrics state
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

                        // Auto-advance when track finishes
                        if app.player.is_track_finished() {
                            debug!("Track finished — auto advancing");
                            play_next_or_stop(&mut app);
                        }
                    }

                    // -----------------------------------------------------------------
                    // Focus switching
                    AppEvent::FocusBrowser => app.focus = FocusPane::Browser,
                    AppEvent::FocusAlbum => {
                        if app.active_album_dir.is_some() || app.current_dir == app.root_dir {
                            app.focus = FocusPane::Album;
                        }
                    }
                    AppEvent::FocusLyrics => {
                        if let LyricsStatus::Loaded(lyrics) = &app.lyrics {
                            app.lyric_scroll = lyrics.current_index;
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
                    },

                    // -----------------------------------------------------------------
                    AppEvent::NavigateUp => match app.focus {
                        FocusPane::Lyrics => app.focus = FocusPane::Album,
                        FocusPane::Album => app.focus = FocusPane::Browser,
                        FocusPane::Browser => {
                            if app.current_dir != app.root_dir
                                && let Some(parent) = app.current_dir.parent()
                            {
                                app.current_dir = parent.to_path_buf();
                                app.browser_entries =
                                    fs::read_dir(&app.current_dir).unwrap_or_default();
                                app.selected_index = 0;
                                app.selection_anchor_tick = app.ui_tick;

                                if let Ok(Some(tracks)) = fs::detect_loose_tracks(&app.current_dir)
                                {
                                    app.active_album_dir = Some(app.current_dir.clone());
                                    app.album_entries = tracks;
                                    app.album_selected = 0;
                                }
                            }
                        }
                    },

                    // -----------------------------------------------------------------
                    AppEvent::JumpToNowPlaying => {
                        let Some(track_path) = &app.player.current_track else {
                            continue;
                        };

                        let Some(album_dir) = track_path.parent() else {
                            continue;
                        };

                        if !album_dir.starts_with(&app.root_dir) {
                            continue;
                        }

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
                    | AppEvent::SubmitCommand => {}
                }
            }
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;
    }

    Ok(())
}
