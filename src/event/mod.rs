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

pub mod commands;
pub mod jobs;

#[derive(PartialEq, Eq)]
pub enum AppEvent {
    /// Request to quit the application.
    Quit,

    /// A tick event, used for periodic updates (UI refresh, etc.).
    Tick,

    /// Navigate the file browser.
    MoveUp,
    MoveDown,
    NavigateBack,
    Activate,

    /// Media playback controls.
    TogglePause,
    SeekForward,
    SeekBackward,
    Stop,
    JumpToNowPlaying,
    NextTrack,
    PrevTrack,
    ToggleRepeat,
    ToggleShuffle,
    VolumeUp,
    VolumeDown,
    ToggleDownloadQueue,
    CloseDownloadQueue,
    CancelDownload,

    /// Switch focus to diffent panes.
    FocusBrowser,
    FocusAlbum,
    FocusLyrics,

    // Command mode events
    EnterCommandMode,
    ExitCommandMode,
    CommandChar(char),
    CommandBackspace,
    SubmitCommand,
    TextMoveLeft,
    TextMoveRight,
    TextDelete,
    TextMoveHome,
    TextMoveEnd,

    // Search mode events
    EnterSearchMode,
    ExitSearchMode,
    SearchChar(char),
    SearchBackspace,
    SearchMoveUp,
    SearchMoveDown,
    SearchActivate,
}
