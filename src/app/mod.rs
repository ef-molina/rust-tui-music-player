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

pub mod navigation;
pub mod search_helpers;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadJobStatus {
    Active,
    Done,
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub title: String,
    pub url: String,
    pub status: DownloadJobStatus,
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

pub struct PlaybackState {
    pub now_playing: Option<NowPlaying>,
    pub repeat_mode: RepeatMode,
    pub shuffle: bool,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            now_playing: None,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
        }
    }
}

pub struct LyricsManager {
    pub status: LyricsStatus,
    pub scroll: usize,
    pub request_id: u64,
    pub negative_cache: HashSet<LyricsCacheKey>,
    pub pending_cache_key: Option<LyricsCacheKey>,
}

impl Default for LyricsManager {
    fn default() -> Self {
        Self {
            status: LyricsStatus::None,
            scroll: 0,
            request_id: 0,
            negative_cache: HashSet::new(),
            pending_cache_key: None,
        }
    }
}

#[derive(Default)]
pub struct DownloadManager {
    pub active_url: Option<String>,
    pub active_progress: Option<DownloadState>,
    pub active_pid: Option<u32>,
    pub jobs: Vec<DownloadJob>,
    pub show_queue: bool,
}

pub struct YoutubeState {
    pub results: Vec<YoutubeResult>,
    pub selected: usize,
    pub searching: bool,
    pub search_kind: SearchKind,
    pub page: usize,
    pub query: String,
    pub has_more: bool,
}

impl Default for YoutubeState {
    fn default() -> Self {
        Self {
            results: Vec::new(),
            selected: 0,
            searching: false,
            search_kind: SearchKind::Song,
            page: 0,
            query: String::new(),
            has_more: false,
        }
    }
}

pub struct UiState {
    pub ui_tick: u64,
    pub selection_anchor_tick: u64,
    pub focus: FocusPane,
    pub input_mode: InputMode,
    pub status_message: Option<StatusMessage>,
}

pub struct Channels {
    pub lyrics_rx: Receiver<LyricsFetchResult>,
    pub lyrics_tx: Sender<LyricsFetchResult>,
    pub search_rx: Receiver<SearchMessage>,
    pub search_tx: Sender<SearchMessage>,
    pub jobs_rx: Receiver<JobResult>,
    pub jobs_tx: Sender<JobResult>,
}

#[derive(Default)]
pub struct AlbumState {
    pub dir: Option<PathBuf>,
    pub entries: Vec<BrowserEntry>,
    pub selected: usize,
}

pub struct BrowserState {
    pub root_dir: PathBuf,
    pub current_dir: PathBuf,
    pub selected_index: usize,
    pub entries: Vec<BrowserEntry>,
}

pub struct AppState {
    /// File browser state
    pub browser_state: BrowserState,

    /// UI state (focus, input mode, tick counters, status message)
    pub ui: UiState,

    /// Album/playlist state
    pub album: AlbumState,

    /// Lyrics state (status, scroll, caching, request tracking)
    pub lyrics_state: LyricsManager,

    /// Channel handles for background work (lyrics, search, downloads)
    pub channels: Channels,

    /// Playback state and mpv integration
    pub player: Player,

    /// Playback metadata (now playing, repeat, shuffle)
    pub playback: PlaybackState,

    /// Search query, results, and last pre-search browser context
    pub search: SearchState,

    /// Download state (active download, job queue, overlay visibility)
    pub downloads: DownloadManager,

    /// Bounded history of previous navigation states
    pub navigation_history: Vec<NavigationState>,

    /// YouTube search state
    pub youtube: YoutubeState,

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
    pub fn new(cfg: &crate::config::Config, channels: Channels) -> Self {
        let root_dir = cfg.music_root_path();

        Self {
            browser_state: BrowserState {
                root_dir: root_dir.clone(),
                current_dir: root_dir.clone(),
                selected_index: 0,
                entries: Vec::new(),
            },
            ui: UiState {
                focus: FocusPane::Browser,
                input_mode: InputMode::Normal,
                ui_tick: 0,
                selection_anchor_tick: 0,
                status_message: None,
            },
            album: AlbumState::default(),
            player: Player::new(),
            lyrics_state: LyricsManager::default(),
            channels,
            playback: PlaybackState::default(),
            search: SearchState::new(root_dir.clone(), 0),
            downloads: DownloadManager::default(),
            navigation_history: Vec::new(),
            youtube: YoutubeState::default(),
            browser: cfg.browser.clone(),
        }
    }

    pub fn clear_playback(&mut self) {
        self.player.stop();
        self.lyrics_state.status = LyricsStatus::None;
        self.playback.now_playing = None;
        self.lyrics_state.scroll = 0;
    }

    pub fn set_status(
        &mut self,
        level: StatusLevel,
        text: impl Into<String>,
        ttl_ticks: Option<u64>,
    ) {
        self.ui.status_message = Some(StatusMessage {
            level,
            text: text.into(),
            expires_at_tick: ttl_ticks.map(|ttl| self.ui.ui_tick.saturating_add(ttl)),
        });
    }

    pub fn clear_expired_status(&mut self) {
        if self
            .ui
            .status_message
            .as_ref()
            .and_then(|status| status.expires_at_tick)
            .is_some_and(|expires_at| self.ui.ui_tick >= expires_at)
        {
            self.ui.status_message = None;
        }
    }

    pub fn current_navigation_state(&self) -> NavigationState {
        NavigationState {
            focus: self.ui.focus,
            current_dir: self.browser_state.current_dir.clone(),
            selected_index: self.browser_state.selected_index,
            active_album_dir: self.album.dir.clone(),
            album_entries: self.album.entries.clone(),
            album_selected: self.album.selected,
        }
    }
}
