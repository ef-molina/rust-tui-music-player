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
use crate::search::SearchMessage;
use crate::youtube::{SearchKind, YoutubeResult};
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// YouTube search results pane
    YoutubeResults,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub level: StatusLevel,
    pub text: String,
    pub expires_at_tick: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct CommandState {
    pub buffer: String,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct SearchEntry {
    pub path: PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub search_blob: String,
}

#[derive(Debug, Clone)]
pub enum SearchStatus {
    Idle,
    Indexing { scanned: usize },
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
    pub cursor: usize,
    pub selected: usize,
    pub index_entries: Vec<SearchEntry>,
    pub results: Vec<SearchEntry>,
    pub status: SearchStatus,
    pub last_focus: FocusPane,
    pub last_browser_dir: PathBuf,
    pub last_browser_selected: usize,
    pub last_active_album_dir: Option<PathBuf>,
    pub last_album_entries: Vec<BrowserEntry>,
    pub last_album_selected: usize,
}

impl SearchState {
    pub fn new(current_dir: PathBuf, selected_index: usize) -> Self {
        Self {
            query: String::new(),
            cursor: 0,
            selected: 0,
            index_entries: Vec::new(),
            results: Vec::new(),
            status: SearchStatus::Idle,
            last_focus: FocusPane::Browser,
            last_browser_dir: current_dir,
            last_browser_selected: selected_index,
            last_active_album_dir: None,
            last_album_entries: Vec::new(),
            last_album_selected: 0,
        }
    }
}

pub enum InputMode {
    Normal,
    Command(CommandState),
    Search,
}

#[derive(Debug, Clone)]
pub struct DownloadState {
    pub track_title: String,
    pub track_index: u32,
    pub total_tracks: u32,
    pub overall_percent: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationState {
    pub focus: FocusPane,
    pub current_dir: PathBuf,
    pub selected_index: usize,
    pub active_album_dir: Option<PathBuf>,
    pub album_entries: Vec<BrowserEntry>,
    pub album_selected: usize,
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

    /// Search query, results, and last pre-search browser context
    pub search: SearchState,

    /// Receiver and sender for background search indexing
    pub search_rx: Receiver<SearchMessage>,
    pub search_tx: Sender<SearchMessage>,

    /// Background job results (downloads, normalization, etc.)
    pub jobs_rx: Receiver<JobResult>,
    pub jobs_tx: Sender<JobResult>,

    /// Transient statusline message for background activity and feedback
    pub status_message: Option<StatusMessage>,

    /// Active download URL for long-running job visibility
    pub active_download_url: Option<String>,

    /// Live download progress shown in the footer
    pub active_download: Option<DownloadState>,

    /// Bounded history of previous navigation states
    pub navigation_history: Vec<NavigationState>,

    /// YouTube search results
    pub youtube_results: Vec<YoutubeResult>,
    pub youtube_selected: usize,
    /// True while a background YouTube search thread is running
    pub youtube_searching: bool,
    /// Which type of search produced the current results
    pub youtube_search_kind: SearchKind,
    /// 0-based page number of the currently loaded results
    pub youtube_page: usize,
    /// The query that produced the current results (used for load-more)
    pub youtube_query: String,
    /// True if the last search returned a full page (more may exist)
    pub youtube_has_more: bool,

    /// Playback repeat mode
    pub repeat_mode: RepeatMode,
    /// True when shuffle is active
    pub shuffle: bool,

    /// Browser yt-dlp reads cookies from (from config)
    pub browser: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    Track,
    Album,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::Track,
            RepeatMode::Track => RepeatMode::Album,
            RepeatMode::Album => RepeatMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RepeatMode::Off => "off",
            RepeatMode::Track => "track",
            RepeatMode::Album => "album",
        }
    }
}

impl AppState {
    /// Create a new application state from a loaded config.
    pub fn new(
        cfg: &crate::config::Config,
        lyrics_rx: Receiver<LyricsFetchResult>,
        lyrics_tx: Sender<LyricsFetchResult>,
        search_rx: Receiver<SearchMessage>,
        search_tx: Sender<SearchMessage>,
        jobs_rx: Receiver<JobResult>,
        jobs_tx: Sender<JobResult>,
    ) -> Self {
        let root_dir = cfg.music_root_path();

        Self {
            root_dir: root_dir.clone(),
            current_dir: root_dir.clone(),
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
            search: SearchState::new(root_dir.clone(), 0),
            search_rx,
            search_tx,
            status_message: None,
            active_download_url: None,
            active_download: None,
            navigation_history: Vec::new(),
            youtube_results: Vec::new(),
            youtube_selected: 0,
            youtube_searching: false,
            youtube_search_kind: SearchKind::Song,
            youtube_page: 0,
            youtube_query: String::new(),
            youtube_has_more: false,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
            browser: cfg.browser.clone(),
        }
    }

    pub fn clear_playback(&mut self) {
        self.player.stop();
        self.lyrics = LyricsStatus::None;
        self.now_playing = None;
        self.lyric_scroll = 0;
    }

    pub fn set_status(
        &mut self,
        level: StatusLevel,
        text: impl Into<String>,
        ttl_ticks: Option<u64>,
    ) {
        self.status_message = Some(StatusMessage {
            level,
            text: text.into(),
            expires_at_tick: ttl_ticks.map(|ttl| self.ui_tick.saturating_add(ttl)),
        });
    }

    pub fn clear_expired_status(&mut self) {
        if self
            .status_message
            .as_ref()
            .and_then(|status| status.expires_at_tick)
            .is_some_and(|expires_at| self.ui_tick >= expires_at)
        {
            self.status_message = None;
        }
    }

    pub fn current_navigation_state(&self) -> NavigationState {
        NavigationState {
            focus: self.focus,
            current_dir: self.current_dir.clone(),
            selected_index: self.selected_index,
            active_album_dir: self.active_album_dir.clone(),
            album_entries: self.album_entries.clone(),
            album_selected: self.album_selected,
        }
    }
}
