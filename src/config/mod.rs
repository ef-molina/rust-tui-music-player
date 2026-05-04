//! User configuration loaded from `~/.config/rust-tui-music-player/config.toml`.
//!
//! All fields have sensible defaults so the file is fully optional.
//!
//! Example config:
//! ```toml
//! music_root = "~/Music"
//! browser    = "firefox"
//! ```

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root directory of the music library.
    pub music_root: String,
    /// Browser yt-dlp reads cookies from (brave, chrome, firefox, safari, …).
    pub browser: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_root: default_music_root(),
            browser: "brave".to_string(),
        }
    }
}

fn default_music_root() -> String {
    std::env::var("HOME")
        .map(|h| format!("{h}/Downloads/Media/Music"))
        .unwrap_or_else(|_| ".".to_string())
}

impl Config {
    /// Expand `~` at the start of `music_root` to the real home directory.
    pub fn music_root_path(&self) -> PathBuf {
        if let Some(rest) = self.music_root.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(format!("{home}/{rest}"));
            }
        }
        PathBuf::from(&self.music_root)
    }
}

/// Load config from `~/.config/rust-tui-music-player/config.toml`.
/// Returns defaults if the file doesn't exist or can't be parsed.
pub fn load() -> Config {
    let path = config_path();

    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };

    match toml::from_str::<Config>(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "Warning: could not parse config at {}: {e}. Using defaults.",
                path.display()
            );
            Config::default()
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("rust-tui-music-player")
        })
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("config.toml")
}
