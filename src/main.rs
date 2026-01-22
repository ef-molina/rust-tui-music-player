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
mod input;
mod ui;

use app::AppState;
use event::AppEvent;
use std::time::Duration;
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::stdout;


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

    loop {
        let event = match input::poll_event(Duration::from_millis(50)) {
            Ok(Some(ev)) => ev,
            Ok(None) => AppEvent::Tick,
            Err(err) => return Err(err),
        };

        match event {
            AppEvent::Quit => {
                app.should_quit = true;
            }
            AppEvent::Tick => {
                // Future: time-based updates, UI refresh, player polling.
            }
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|frame| {
            ui::draw(frame, &app);
        })?;
    }

    Ok(())
}
