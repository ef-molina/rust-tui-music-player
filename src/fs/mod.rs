//! Filesystem access module.
//!
//! Responsible for reading directory contents and converting them
//! into browser-friendly data structures.
//!
//! Design rules:
//! - No UI logic
//! - No terminal access
//! - No global state
//! - All paths are provided by the caller

use std::fs;
use std::path::Path;

use crate::app::BrowserEntry;

/// Check if a file name corresponds to a supported audio file.
fn is_supported_audio_file(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();

    matches!(
        ext.as_str(),
        "mp3" | "flac" | "wav" | "opus" | "ogg" | "m4a" | "aac"
    )
}

/// Read a directory and return a list of browser entries.
///
/// - Directories are listed first
/// - Files are listed second
/// - Entries are sorted alphabetically
pub fn read_dir(path: &Path) -> std::io::Result<Vec<BrowserEntry>> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();

        if file_type.is_dir() {
            dirs.push(BrowserEntry { name, is_dir: true });
        } else if is_supported_audio_file(&name) {
            files.push(BrowserEntry {
                name,
                is_dir: false,
            });
        }
    }

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    dirs.extend(files);
    Ok(dirs)
}
