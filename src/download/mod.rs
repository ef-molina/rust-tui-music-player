use crate::event::jobs::JobResult;
use crate::metadata::model::TrustedSearchMetadata;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::Sender;

pub fn staging_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("HOME")
            .map(|h| format!("{}/Downloads/Media/Music/.staging", h))
            .unwrap_or_else(|_| ".staging".into()),
    )
}

/// Download a URL (single track or full playlist) with progress streaming.
/// Designed to run in a background thread; sends `JobResult` messages via `tx`.
///
/// `search_metadata` carries trusted metadata from YouTube search enrichment.
/// Pass `Some` for individual song downloads from `:ss` results.
/// Pass `None` for direct URL downloads, album downloads, and playlist downloads.
pub fn spawn_playlist_download(
    url: String,
    title: String,
    staging: PathBuf,
    browser: String,
    tx: Sender<JobResult>,
    search_metadata: Option<TrustedSearchMetadata>,
) {
    // Use a per-job subdirectory so concurrent downloads don't clobber each other
    // and so we can reliably collect every file that belongs to this job.
    let job_dir = staging.join({
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        url.hash(&mut h);
        format!("job_{:x}", h.finish())
    });
    let staging = job_dir;
    let _ = std::fs::create_dir_all(&staging);
    let output_template = staging.join("%(title)s [%(id)s].%(ext)s");

    let mut child = match std::process::Command::new("yt-dlp")
        .args([
            "-f",
            "bestaudio[ext=opus]/bestaudio",
            "-x",
            "--audio-format",
            "opus",
            "--audio-quality",
            "0",
            "--embed-metadata",
            "--embed-thumbnail",
            "--convert-thumbnails",
            "jpg",
            "--add-metadata",
            "--yes-playlist",
            "--newline",
            "--cookies-from-browser",
            &browser,
            "-o",
        ])
        .arg(&output_template)
        .arg(&url)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: format!("Failed to spawn yt-dlp: {e}"),
            });
            return;
        }
    };

    // Emit started now that we have a PID
    let _ = tx.send(JobResult::DownloadStarted {
        url: url.clone(),
        title: title.clone(),
        pid: child.id(),
    });

    let stdout = child.stdout.take().expect("stdout piped");
    let reader = BufReader::new(stdout);

    let mut current_item: u32 = 0;
    let mut total_items: u32 = 0;
    let mut current_title = String::new();

    for line in reader.lines().map_while(Result::ok) {
        let Some(rest) = line.strip_prefix("[download]") else {
            continue;
        };
        let trimmed = rest.trim();

        // "[download] Downloading item X of Y"
        if let Some(item_str) = trimmed.strip_prefix("Downloading item ")
            && let Some((idx, tot)) = item_str.split_once(" of ")
            && let (Ok(i), Ok(t)) = (idx.trim().parse::<u32>(), tot.trim().parse::<u32>())
        {
            current_item = i;
            total_items = t;
        }
        // "[download] Destination: /full/path/Title [id].ext"
        else if let Some(dest) = trimmed.strip_prefix("Destination: ") {
            let filename = std::path::Path::new(dest)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(dest);
            // Output template is "%(title)s [%(id)s].%(ext)s" — strip " [id].ext"
            current_title = if let Some(pos) = filename.rfind(" [") {
                filename[..pos].to_string()
            } else {
                filename
                    .rsplit_once('.')
                    .map(|(s, _)| s)
                    .unwrap_or(filename)
                    .to_string()
            };
        }
        // "[download]  42.3% of 5.23MiB at ..."
        else if let Some(pct_str) = trimmed.split('%').next()
            && let Ok(track_pct) = pct_str.trim().parse::<f32>()
        {
            let overall = if total_items > 0 {
                ((current_item.saturating_sub(1)) as f32 + track_pct / 100.0) / total_items as f32
                    * 100.0
            } else {
                track_pct
            };
            let _ = tx.send(JobResult::DownloadProgress {
                url: url.clone(),
                track_percent: track_pct,
                overall_percent: overall,
                track_title: current_title.clone(),
                track_index: current_item,
                total_tracks: total_items,
            });
        }
    }

    match child.wait() {
        Ok(status) if status.success() => {
            // Collect every file in the per-job staging dir and emit one
            // DownloadFinished per track so each is individually normalized.
            let files: Vec<_> = std::fs::read_dir(&staging)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .collect();

            if files.is_empty() {
                let _ = tx.send(JobResult::DownloadFailed {
                    url,
                    error: "Download succeeded but no files found".into(),
                });
            } else {
                let total = files.len();
                for (i, entry) in files.into_iter().enumerate() {
                    // Only the last event carries the URL so the active_download
                    // indicator clears once all tracks are queued.
                    let evt_url = if i + 1 == total {
                        url.clone()
                    } else {
                        String::new()
                    };
                    let _ = tx.send(JobResult::DownloadFinished {
                        url: evt_url,
                        temp_path: entry.path(),
                        search_metadata: search_metadata.clone(),
                    });
                }
                // Remove the now-empty per-job staging directory
                let _ = std::fs::remove_dir(&staging);
            }
        }
        Ok(status) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: format!("yt-dlp exited with code {:?}", status.code()),
            });
        }
        Err(e) => {
            let _ = tx.send(JobResult::DownloadFailed {
                url,
                error: e.to_string(),
            });
        }
    }
}
