//! Input handling module.

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::event::AppEvent;

/// Poll for an input event and translate it into an `AppEvent`.
///
/// `in_command_mode` controls whether keys are interpreted as text-editing
/// events (command mode) or normal navigation/playback events (normal mode).
pub fn poll_event(timeout: Duration, in_command_mode: bool) -> std::io::Result<Option<AppEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if in_command_mode {
                // -----------------------------
                // Command mode: text entry only
                // -----------------------------
                return Ok(match key.code {
                    KeyCode::Esc => Some(AppEvent::ExitCommandMode),
                    KeyCode::Enter => Some(AppEvent::SubmitCommand),
                    KeyCode::Backspace => Some(AppEvent::CommandBackspace),
                    KeyCode::Char(c) => Some(AppEvent::CommandChar(c)),
                    _ => None,
                });
            }

            // -----------------------------
            // Normal mode: app controls
            // -----------------------------
            Ok(match key.code {
                KeyCode::Char('q') => Some(AppEvent::Quit),
                KeyCode::Up => Some(AppEvent::MoveUp),
                KeyCode::Down => Some(AppEvent::MoveDown),
                KeyCode::Backspace => Some(AppEvent::NavigateUp),
                KeyCode::Enter => Some(AppEvent::Activate),
                KeyCode::Char(' ') => Some(AppEvent::TogglePause),
                KeyCode::Left => Some(AppEvent::SeekBackward),
                KeyCode::Right => Some(AppEvent::SeekForward),
                KeyCode::Char('s') => Some(AppEvent::Stop),
                KeyCode::Char('n') => Some(AppEvent::JumpToNowPlaying),
                KeyCode::Char('b') => Some(AppEvent::FocusBrowser),
                KeyCode::Char('t') => Some(AppEvent::FocusAlbum),
                KeyCode::Char(']') => Some(AppEvent::NextTrack),
                KeyCode::Char('[') => Some(AppEvent::PrevTrack),
                KeyCode::Char('l') => Some(AppEvent::FocusLyrics),
                KeyCode::Char('/') => Some(AppEvent::EnterCommandMode),
                _ => None,
            })
        }
        _ => Ok(None),
    }
}
