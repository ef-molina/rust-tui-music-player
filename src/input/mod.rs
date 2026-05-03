//! Input handling module.

use crate::app::InputMode;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::event::AppEvent;

/// Poll for an input event and translate it into an `AppEvent`.
///
/// `input_mode` controls whether keys are interpreted as text-editing
/// events or normal navigation/playback events.
pub fn poll_event(timeout: Duration, input_mode: &InputMode) -> std::io::Result<Option<AppEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            match input_mode {
                InputMode::Command(_) => {
                    return Ok(match key.code {
                        KeyCode::Esc => Some(AppEvent::ExitCommandMode),
                        KeyCode::Enter => Some(AppEvent::SubmitCommand),
                        KeyCode::Backspace => Some(AppEvent::CommandBackspace),
                        KeyCode::Left => Some(AppEvent::TextMoveLeft),
                        KeyCode::Right => Some(AppEvent::TextMoveRight),
                        KeyCode::Delete => Some(AppEvent::TextDelete),
                        KeyCode::Home => Some(AppEvent::TextMoveHome),
                        KeyCode::End => Some(AppEvent::TextMoveEnd),
                        KeyCode::Char(c) => Some(AppEvent::CommandChar(c)),
                        _ => None,
                    });
                }
                InputMode::Search => {
                    return Ok(match key.code {
                        KeyCode::Esc => Some(AppEvent::ExitSearchMode),
                        KeyCode::Enter => Some(AppEvent::SearchActivate),
                        KeyCode::Up => Some(AppEvent::SearchMoveUp),
                        KeyCode::Down => Some(AppEvent::SearchMoveDown),
                        KeyCode::Backspace => Some(AppEvent::SearchBackspace),
                        KeyCode::Left => Some(AppEvent::TextMoveLeft),
                        KeyCode::Right => Some(AppEvent::TextMoveRight),
                        KeyCode::Delete => Some(AppEvent::TextDelete),
                        KeyCode::Home => Some(AppEvent::TextMoveHome),
                        KeyCode::End => Some(AppEvent::TextMoveEnd),
                        KeyCode::Char(c) => Some(AppEvent::SearchChar(c)),
                        _ => None,
                    });
                }
                InputMode::Normal => {}
            }

            // -----------------------------
            // Normal mode: app controls
            // -----------------------------
            Ok(match key.code {
                KeyCode::Char('q') => Some(AppEvent::Quit),
                KeyCode::Up => Some(AppEvent::MoveUp),
                KeyCode::Down => Some(AppEvent::MoveDown),
                KeyCode::Backspace => Some(AppEvent::NavigateBack),
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
                KeyCode::Char('r') => Some(AppEvent::ToggleRepeat),
                KeyCode::Char('z') => Some(AppEvent::ToggleShuffle),
                KeyCode::Char('=') => Some(AppEvent::VolumeUp),
                KeyCode::Char('-') => Some(AppEvent::VolumeDown),
                KeyCode::Char('/') => Some(AppEvent::EnterSearchMode),
                KeyCode::Char(':') => Some(AppEvent::EnterCommandMode),
                _ => None,
            })
        }
        _ => Ok(None),
    }
}
