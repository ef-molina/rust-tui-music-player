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

use app::AppState;
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use event::AppEvent;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;
use std::time::Duration;

fn main() {
    if let Err(err) = terminal::enable_raw_mode() {
        eprintln!("Failed to enable raw mode: {err}");
        return;
    }

    let mut stdout = stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen) {
        eprintln!("Failed to enter alternate screen: {err}");
        return;
    }

    let result = run_app();

    // Restore terminal
    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();

    if let Err(err) = result {
        eprintln!("Application error: {err}");
    }
}

fn run_app() -> std::io::Result<()> {
    let mut app = AppState::new();
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_else(|_| Vec::new());

    loop {
        let event = match input::poll_event(Duration::from_millis(10)) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        match event {
            AppEvent::Quit => {
                app.player.shutdown();
                break;
            }
            AppEvent::Tick => {
                app.player.poll_metrics();
            }
            AppEvent::MoveUp => {
                if app.selected_index > 0 {
                    app.selected_index -= 1;
                }
            }
            AppEvent::MoveDown => {
                if app.selected_index + 1 < app.browser_entries.len() {
                    app.selected_index += 1;
                }
            }
            AppEvent::NavigateUp => {
                if app.current_dir != app.root_dir
                    && let Some(parent) = app.current_dir.parent()
                {
                    app.current_dir = parent.to_path_buf();
                    app.selected_index = 0;
                    app.browser_entries =
                        fs::read_dir(&app.current_dir).unwrap_or_else(|_| Vec::new());
                }
            }
            AppEvent::Activate => {
                if let Some(entry) = app.browser_entries.get(app.selected_index) {
                    if entry.is_dir {
                        let new_path = app.current_dir.join(&entry.name);

                        if new_path.starts_with(&app.root_dir) {
                            app.current_dir = new_path;
                            app.selected_index = 0;
                            app.browser_entries =
                                fs::read_dir(&app.current_dir).unwrap_or_else(|_| Vec::new());
                        }
                    } else {
                        let track_path = app.current_dir.join(&entry.name);
                        app.player.load(track_path);
                    }
                }
            }
            AppEvent::JumpToNowPlaying => {
                let Some(track_path) = &app.player.current_track else {
                    return Ok(());
                };

                let Some(parent) = track_path.parent() else {
                    return Ok(());
                };

                // Safety: ensure we stay inside the music root
                if !parent.starts_with(&app.root_dir) {
                    return Ok(());
                }

                app.current_dir = parent.to_path_buf();
                app.browser_entries = fs::read_dir(&app.current_dir).unwrap_or_else(|_| Vec::new());

                // Select the currently playing file
                let Some(file_name) = track_path.file_name().and_then(|s| s.to_str()) else {
                    return Ok(());
                };

                if let Some(index) = app
                    .browser_entries
                    .iter()
                    .position(|e| !e.is_dir && e.name == file_name)
                {
                    app.selected_index = index;
                }
            }

            AppEvent::TogglePause => {
                app.player.toggle_pause();
            }
            AppEvent::SeekForward => {
                app.player.seek(5);
            }
            AppEvent::SeekBackward => {
                app.player.seek(-5);
            }
            AppEvent::Stop => {
                app.player.stop();
            }
        }

        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;
    }

    Ok(())
}
