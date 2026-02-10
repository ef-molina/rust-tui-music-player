//! Background job results (download/normalize/etc).
//!
//! These are messages sent from worker threads back to the main loop.
//! This is intentionally separate from AppEvent (which represents user/UI events).

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum JobResult {
    DownloadStarted {
        url: String,
    },

    /// yt-dlp finished and produced a file at this path (usually inside a staging dir)
    DownloadFinished {
        url: String,
        temp_path: PathBuf,
    },

    DownloadFailed {
        url: String,
        error: String,
    },
}
