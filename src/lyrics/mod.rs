//! Lyrics module.
//!
//! Responsibilities:
//! - Parse .lrc files (timestamped lyrics)
//! - Load lyrics for a given audio track path
//! - Maintain a small state machine for current lyric line
//!
//! Design rules:
//! - No UI logic
//! - No mpv IPC
//! - No AppState mutation (main.rs owns state)

mod loader;
mod parser;
mod state;

pub use loader::load_for_track;
pub use parser::{LyricLine, parse_lrc};
pub use state::LyricsState;
