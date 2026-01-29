//! Lyrics fetching module (network + caching).
//!
//! This module is responsible for fetching synced lyrics (LRC) from a provider
//! (currently lrclib) and returning parsed lyric lines.
//!
//! Design rules:
//! - No AppState mutation here
//! - No UI work here
//! - Callers decide when/how to fetch (sync vs async)

pub mod lrclib;

/// Result type returned from a lyrics fetch attempt.
///
/// This is intentionally small and message-friendly so it can be sent across
/// threads via `std::sync::mpsc`.
#[derive(Debug)]
pub enum LyricsFetchResult {
    /// Raw LRC text fetched from provider.
    /// Writing + parsing happens on the main thread.
    RawLrc {
        path: std::path::PathBuf,
        contents: String,
    },

    /// Provider responded but no synced lyrics exist.
    NotFound,

    /// Network, parsing, or unexpected failure.
    Failed,
}
