//! Background job results (download/search/etc).

use crate::youtube::YoutubeResult;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum JobResult {
    DownloadStarted {
        url: String,
        /// Title shown in the queue (best-effort from the URL or result title)
        title: String,
        /// yt-dlp process ID — used to cancel by killing the process
        pid: u32,
    },

    DownloadProgress {
        url: String,
        /// 0–100 for the current track (kept for future per-track display)
        #[allow(dead_code)]
        track_percent: f32,
        /// 0–100 across the whole album/playlist
        overall_percent: f32,
        track_title: String,
        track_index: u32,
        total_tracks: u32,
    },

    DownloadFinished {
        url: String,
        temp_path: PathBuf,
    },

    DownloadFailed {
        url: String,
        error: String,
    },

    /// Search completed — results are appended to existing list when paginating
    YoutubeSearchDone {
        results: Vec<YoutubeResult>,
        /// true if there may be more results available (another page exists)
        has_more: bool,
    },

    YoutubeSearchFailed(String),
}
