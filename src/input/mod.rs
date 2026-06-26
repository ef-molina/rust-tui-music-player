//! Input handling module.

pub mod text;

use crate::app::InputMode;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

use crate::event::AppEvent;

fn command_mode_event(key_code: KeyCode) -> Option<AppEvent> {
    match key_code {
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
    }
}

fn search_mode_event(key_code: KeyCode) -> Option<AppEvent> {
    match key_code {
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
    }
}

fn normal_mode_event(key_code: KeyCode) -> Option<AppEvent> {
    match key_code {
        KeyCode::Esc => Some(AppEvent::CloseDownloadQueue),
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
        KeyCode::Char('d') => Some(AppEvent::ToggleDownloadQueue),
        KeyCode::Char('x') => Some(AppEvent::CancelDownload),
        KeyCode::Char('/') => Some(AppEvent::EnterSearchMode),
        KeyCode::Char(':') => Some(AppEvent::EnterCommandMode),
        _ => None,
    }
}

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
                    return Ok(command_mode_event(key.code));
                }
                InputMode::Search => {
                    return Ok(search_mode_event(key.code));
                }
                InputMode::Normal => {}
            }

            // -----------------------------
            // Normal mode: app controls
            // -----------------------------
            Ok(normal_mode_event(key.code))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_mode_event, normal_mode_event, search_mode_event};
    use crate::event::AppEvent;
    use crossterm::event::KeyCode;

    #[test]
    fn normal_mode_esc_maps_to_close_download_queue() {
        assert!(matches!(
            normal_mode_event(KeyCode::Esc),
            Some(AppEvent::CloseDownloadQueue)
        ));
    }

    #[test]
    fn normal_mode_d_still_maps_to_toggle_download_queue() {
        assert!(matches!(
            normal_mode_event(KeyCode::Char('d')),
            Some(AppEvent::ToggleDownloadQueue)
        ));
    }

    #[test]
    fn command_and_search_modes_keep_existing_escape_behavior() {
        assert!(matches!(
            command_mode_event(KeyCode::Esc),
            Some(AppEvent::ExitCommandMode)
        ));
        assert!(matches!(
            search_mode_event(KeyCode::Esc),
            Some(AppEvent::ExitSearchMode)
        ));
    }
}
