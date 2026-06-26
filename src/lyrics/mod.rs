//! Lyrics module.
//!
//! Responsibilities:
//! - Parse .lrc files (timestamped lyrics)
//! - Load lyrics for a given audio track path
//! - Maintain a small state machine for current lyric line
//! - Orchestrate lyrics loading on track change (orchestrator submodule)
//!
//! Design rules:
//! - No UI logic
//! - No mpv IPC

mod loader;
pub mod orchestrator;
mod parser;
mod state;

pub use loader::load_for_track;
pub use parser::{LyricLine, parse_lrc};
pub use state::LyricsState;
