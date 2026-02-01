//! Application state module.
//!
//! This module defines `AppState`, the single owner of all mutable state
//! for the application. Every change to the program’s state flows through
//! this struct and is driven by the central event loop in `main.rs`.
//!
//! Design principles:
//! - Single owner of mutable state
//! - No direct I/O (terminal, filesystem, mpv)
//! - Pure data + small helpers
//!
//! Other modules may read from `AppState`, but only the event loop
//! mutates it.
//!

use crate::lyrics::LyricsState;
use crate::lyrics_fetch::LyricsFetchResult;
use crate::player::Player;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

/// Represent a single entry in file browser
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    /// Display name of the entry
    pub name: String,

    /// True if the entry is a directory
    pub is_dir: bool,
}

/// Focused application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    /// File browser is focused
    Browser,
    /// Album controls are focused
    Album,
    /// Lyrics pane is focused
    Lyrics,
}

pub enum LyricsStatus {
    None,
    Loading,
    Loaded(LyricsState),
}

pub struct AppState {
    /// Root directory of the file browser
    pub root_dir: PathBuf,

    /// Current directory in the file browser
    pub current_dir: PathBuf,

    /// Index of the currently selected browser entry
    pub selected_index: usize,

    /// List of entries in the current directory
    pub browser_entries: Vec<BrowserEntry>,

    /// Which pane currently has focus
    pub focus: FocusPane,

    /// Directory of active album/playlist
    pub active_album_dir: Option<PathBuf>,

    /// Tracks shown in album/playlist view
    pub album_entries: Vec<BrowserEntry>,

    /// Index of the currently selected album entry
    pub album_selected: usize,

    /// State for synced lyrics
    pub lyrics: LyricsStatus,
    pub lyric_scroll: usize,
    pub lyrics_rx: Option<Receiver<LyricsFetchResult>>,
    pub lyrics_request_id: u64,

    /// Currently selected file or directory
    pub player: Player,
}

impl AppState {
    /// Create a new application state with default values.
    pub fn new() -> Self {
        let root_dir = PathBuf::from(
            std::env::var("HOME")
                .map(|h| format!("{}/Downloads/Media/Music", h))
                .unwrap_or_else(|_| ".".into()),
        );
        Self {
            root_dir: root_dir.clone(),
            current_dir: root_dir,
            browser_entries: Vec::new(),
            selected_index: 0,
            focus: FocusPane::Browser,
            active_album_dir: None,
            album_entries: Vec::new(),
            album_selected: 0,
            player: Player::new(),
            lyrics: LyricsStatus::None,
            lyric_scroll: 0,
            lyrics_rx: None,
            lyrics_request_id: 0,
        }
    }
}
