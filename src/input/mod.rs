//! Input handling module.
//!
//! This module is responsible for:
//! - Reading raw terminal input events
//! - Translating them into semantic `AppEvent`s
//!
//! It does NOT:
//! - Mutate application state
//! - Perform rendering
//! - Contain business logic
//!
//! This separation allows input sources to change
//! (keyboard, IPC, tests) without affecting the core logic.

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::event::AppEvent;

/// Poll for an input event and translate it into an `AppEvent`.
///
/// Returns `None` if no relevant event occurred.
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<AppEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
            KeyCode::Char('q') => Ok(Some(AppEvent::Quit)),
            KeyCode::Up => Ok(Some(AppEvent::MoveUp)),
            KeyCode::Down => Ok(Some(AppEvent::MoveDown)),
            KeyCode::Backspace => Ok(Some(AppEvent::NavigateUp)),
            KeyCode::Enter => Ok(Some(AppEvent::Activate)),
            KeyCode::Char(' ') => Ok(Some(AppEvent::TogglePause)),
            KeyCode::Left => Ok(Some(AppEvent::SeekBackward)),
            KeyCode::Right => Ok(Some(AppEvent::SeekForward)),
            KeyCode::Char('s') => Ok(Some(AppEvent::Stop)),
            KeyCode::Char('n') => Ok(Some(AppEvent::JumpToNowPlaying)),
            KeyCode::Char('b') => Ok(Some(AppEvent::FocusBrowser)),
            KeyCode::Char('t') => Ok(Some(AppEvent::FocusAlbum)),
            KeyCode::Char(']') => Ok(Some(AppEvent::NextTrack)),
            KeyCode::Char('[') => Ok(Some(AppEvent::PrevTrack)),
            KeyCode::Char('l') => Ok(Some(AppEvent::FocusLyrics)),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}
