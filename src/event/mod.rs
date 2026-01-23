//! Application event definitions.
//!
//! This module defines `AppEvent`, an abstract representation of
//! everything that can happen in the application.
//!
//! Raw inputs (keyboard, timers, mpv IPC messages) are translated
//! into these events before reaching the core event loop.
//!
//! Design principles:
//! - Events are semantic, not tied to input libraries
//! - The event loop reacts to events and mutates `AppState`
//! - This keeps UI, input, and player logic decoupled

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// Request to quit the application.
    Quit,

    /// A tick event, used for periodic updates (UI refresh, etc.).
    Tick,

    /// Move the selection up in the browser.
    MoveUp,

    /// Move the selection down in the browser.
    MoveDown,

    /// Back into the parent directory or close current file.
    NavigateUp,

    /// Enter the selected directory or open the selected file.
    Activate,

    /// Toggle pause/playback state.
    TogglePause,

    SeekForward,
    SeekBackward,
    Stop,
    JumpToNowPlaying,
}
