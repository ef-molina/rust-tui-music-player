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

pub mod normalize;

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

/// If `dir` is a leaf album directory, return its track entries.
/// A leaf album:
/// - contains at least one audio file
/// - contains NO subdirectories
pub fn detect_album(dir: &Path) -> std::io::Result<Option<Vec<BrowserEntry>>> {
    let entries = read_dir(dir)?;

    let has_subdirs = entries.iter().any(|e| e.is_dir);
    if has_subdirs {
        return Ok(None);
    }

    let tracks: Vec<_> = entries.into_iter().filter(|e| !e.is_dir).collect();
    if tracks.is_empty() {
        return Ok(None);
    }

    Ok(Some(tracks))
}

/// Unlike `detect_album`, this allows subdirectories.
pub fn detect_loose_tracks(dir: &Path) -> std::io::Result<Option<Vec<BrowserEntry>>> {
    let entries = read_dir(dir)?;

    let tracks: Vec<_> = entries.into_iter().filter(|e| !e.is_dir).collect();

    if tracks.is_empty() {
        Ok(None)
    } else {
        Ok(Some(tracks))
    }
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
            if name.starts_with('.') {
                continue;
            }
            dirs.push(BrowserEntry { name, is_dir: true });
        } else if is_supported_audio_file(&name) {
            files.push(BrowserEntry {
                name,
                is_dir: false,
            });
        }
    }

    dirs.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());

    dirs.extend(files);
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::read_dir;
    use crate::app::BrowserEntry;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn read_dir_excludes_dot_directories_but_keeps_normal_dirs_and_audio_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join(".staging")).unwrap();
        fs::create_dir(root.join("Albums")).unwrap();
        fs::write(root.join("song.opus"), b"audio").unwrap();
        fs::write(root.join("cover.jpg"), b"image").unwrap();

        let entries = read_dir(root).unwrap();

        assert_eq!(
            entries,
            vec![
                BrowserEntry {
                    name: "Albums".into(),
                    is_dir: true,
                },
                BrowserEntry {
                    name: "song.opus".into(),
                    is_dir: false,
                },
            ]
        );
    }
}
