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

use crate::event::jobs::JobResult;
use crate::lyrics::LyricsState;
use crate::lyrics_fetch::LyricsFetchResult;
use crate::metadata::model::TrackMetadata;
use crate::player::Player;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

/// Stable identity for lyrics caching decisions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LyricsCacheKey {
    pub artist: String,
    pub title: String,
    pub duration_secs: u32,
}

impl LyricsCacheKey {
    pub fn from_metadata(meta: &TrackMetadata) -> Self {
        Self {
            artist: meta.artist.clone(),
            title: meta.title.clone(),
            duration_secs: meta.duration_secs.floor() as u32,
        }
    }
}

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

// Currently playing track information
#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs_meta: u64,
}

pub enum LyricsStatus {
    None,
    Loading,
    Loaded(LyricsState),
}

#[derive(Debug, Clone)]
pub struct CommandState {
    pub buffer: String,
    pub cursor: usize,
}

pub enum InputMode {
    Normal,
    Command(CommandState),
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

    /// UI tick used for render-time effects (like blinking cursor, marquee, etc)
    pub ui_tick: u64,
    pub selection_anchor_tick: u64,

    /// Sender and receiver for background lyrics fetch results
    pub lyrics_rx: Receiver<LyricsFetchResult>,
    pub lyrics_tx: Sender<LyricsFetchResult>,

    /// Monotonically increasing request ID for lyrics fetches
    pub lyrics_request_id: u64,

    /// In-memory negative cache for tracks known to have no synced lyrics
    pub lyrics_negative_cache: HashSet<LyricsCacheKey>,

    /// Cache key associated with the currently in-flight lyrics request
    pub lyrics_pending_cache_key: Option<LyricsCacheKey>,

    /// Playback state and mpv integration
    pub player: Player,

    /// Currently playing track information
    pub now_playing: Option<NowPlaying>,

    /// Current input mode (normal vs command)
    pub input_mode: InputMode,

    /// Background job results (downloads, normalization, etc.)
    pub jobs_rx: Receiver<JobResult>,
    pub jobs_tx: Sender<JobResult>,
}

impl AppState {
    /// Create a new application state with default values.
    pub fn new(
        lyrics_rx: Receiver<LyricsFetchResult>,
        lyrics_tx: Sender<LyricsFetchResult>,
        jobs_rx: Receiver<JobResult>,
        jobs_tx: Sender<JobResult>,
    ) -> Self {
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
            lyrics_request_id: 0,
            lyrics_negative_cache: HashSet::new(),
            lyrics_pending_cache_key: None,
            lyrics_rx,
            lyrics_tx,
            jobs_rx,
            jobs_tx,
            ui_tick: 0,
            selection_anchor_tick: 0,
            now_playing: None,
            input_mode: InputMode::Normal,
        }
    }

    pub fn clear_playback(&mut self) {
        self.player.stop();
        self.lyrics = LyricsStatus::None;
        self.now_playing = None;
        self.lyric_scroll = 0;
    }
}
