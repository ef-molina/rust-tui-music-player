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
mod player;
mod ui;

use app::{AppState, FocusPane};
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use event::AppEvent;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use std::time::Duration;

fn main() {
    terminal::enable_raw_mode().expect("Failed to enable raw mode");
    execute!(stdout(), EnterAlternateScreen).expect("Failed to enter alt screen");

    let result = run_app();

    let _ = execute!(stdout(), LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();

    if let Err(err) = result {
        eprintln!("Application error: {err}");
    }
}

fn run_app() -> std::io::Result<()> {
    let mut app = AppState::new();
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();

    // Initialize album with root loose tracks if any
    let root_tracks: Vec<_> = app
        .browser_entries
        .iter()
        .filter(|e| !e.is_dir)
        .cloned()
        .collect();

    if !root_tracks.is_empty() {
        app.active_album_dir = Some(app.root_dir.clone());
        app.album_entries = root_tracks;
        app.album_selected = 0;
    }

    loop {
        let event = match input::poll_event(Duration::from_millis(10)) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        match event {
            // -----------------------------------------------------------------
            AppEvent::Quit => {
                app.player.shutdown();
                break;
            }

            AppEvent::Tick => {
                app.player.poll_metrics();
            }

            // -----------------------------------------------------------------
            // Focus switching
            AppEvent::FocusBrowser => {
                app.focus = FocusPane::Browser;
            }

            AppEvent::FocusAlbum => {
                if app.active_album_dir.is_some() || app.current_dir == app.root_dir {
                    app.focus = FocusPane::Album;
                }
            }

            // -----------------------------------------------------------------
            // Navigation (focus-dependent)
            AppEvent::MoveUp => match app.focus {
                FocusPane::Browser => {
                    if app.selected_index > 0 {
                        app.selected_index -= 1;
                    }
                }
                FocusPane::Album => {
                    if app.album_selected > 0 {
                        app.album_selected -= 1;
                    }
                }
            },

            AppEvent::MoveDown => match app.focus {
                FocusPane::Browser => {
                    let dir_count = app.browser_entries.iter().filter(|e| e.is_dir).count();
                    if app.selected_index + 1 < dir_count {
                        app.selected_index += 1;
                    }
                }
                FocusPane::Album => {
                    if app.album_selected + 1 < app.album_entries.len() {
                        app.album_selected += 1;
                    }
                }
            },

            // -----------------------------------------------------------------
            AppEvent::NavigateUp => match app.focus {
                FocusPane::Album => {
                    // Exit album mode
                    app.focus = FocusPane::Browser;
                }
                FocusPane::Browser => {
                    if app.current_dir != app.root_dir
                        && let Some(parent) = app.current_dir.parent()
                    {
                        app.current_dir = parent.to_path_buf();
                        app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
                        app.selected_index = 0;

                        // CHANGED: Rehydrate root album when returning home
                        if app.current_dir == app.root_dir && app.active_album_dir.is_none() {
                            let root_tracks: Vec<_> = app
                                .browser_entries
                                .iter()
                                .filter(|e| !e.is_dir)
                                .cloned()
                                .collect();

                            if !root_tracks.is_empty() {
                                app.active_album_dir = Some(app.root_dir.clone());
                                app.album_entries = root_tracks;
                                app.album_selected = 0;
                            }
                        }
                    }
                }
            },

            // -----------------------------------------------------------------
            AppEvent::Activate => match app.focus {
                FocusPane::Browser => {
                    // CHANGED: browser activation uses directories only
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

                    let entries = fs::read_dir(&new_path).unwrap_or_default();
                    let tracks: Vec<_> = entries.iter().filter(|e| !e.is_dir).cloned().collect();
                    let has_subdirs = entries.iter().any(|e| e.is_dir);

                    if !tracks.is_empty() && !has_subdirs {
                        // CHANGED: Album detected — DO NOT navigate browser
                        app.active_album_dir = Some(new_path);
                        app.album_entries = tracks;
                        app.album_selected = 0;
                        app.focus = FocusPane::Album;
                    } else {
                        // Normal directory navigation — DO NOT clear album
                        app.current_dir = new_path;
                        app.browser_entries = entries;
                        app.selected_index = 0;
                    }
                }

                FocusPane::Album => {
                    // CHANGED: Root album is playable
                    if let (Some(entry), Some(album_dir)) = (
                        app.album_entries.get(app.album_selected),
                        &app.active_album_dir,
                    ) {
                        app.player.load(album_dir.join(&entry.name));
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

                let entries = fs::read_dir(album_dir).unwrap_or_default();
                let tracks: Vec<_> = entries.iter().filter(|e| !e.is_dir).cloned().collect();
                let has_subdirs = entries.iter().any(|e| e.is_dir);

                if !tracks.is_empty() && !has_subdirs {
                    // CHANGED: Restore album context
                    app.active_album_dir = Some(album_dir.to_path_buf());
                    app.album_entries = tracks;
                    app.album_selected = track_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .and_then(|name| app.album_entries.iter().position(|e| e.name == name))
                        .unwrap_or(0);
                    app.focus = FocusPane::Album;
                }

                // CHANGED: Browser shows sibling albums, not album contents
                let browser_dir = album_dir.parent().unwrap_or(&app.root_dir);
                app.current_dir = browser_dir.to_path_buf();
                app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_default();
                app.selected_index = 0;
            }

            // -----------------------------------------------------------------
            AppEvent::TogglePause => app.player.toggle_pause(),
            AppEvent::SeekForward => app.player.seek(5),
            AppEvent::SeekBackward => app.player.seek(-5),
            AppEvent::Stop => app.player.stop(),
        }

        terminal.draw(|frame| ui::draw(frame, &app))?;
    }

    Ok(())
}
