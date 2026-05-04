# Rust TUI Music Player

A terminal-based music player built in Rust, designed for fast, keyboard-driven music browsing, playback, and downloading — with a focus on **clean architecture**, **explicit state ownership**, and **predictable behavior**.

This project emphasizes filesystem-based music organization, album-aware playback, a fully time-synced lyrics system, and YouTube Music integration — all within a responsive, flicker-free terminal UI.

---

## Screenshots

### Browser, Album, and Mini Lyrics

![Browser and Album View](docs/screenshots/browser.png)

### Full Lyrics View

![Full Lyrics View](docs/screenshots/lyrics.png)

---

## Features

### Browsing & Playback

- **Keyboard-driven navigation** — browse your music library with arrow keys and Enter
- **Hierarchical album view** — automatically detects album folders (leaf directories with audio files)
- **Persistent album context** — album stays active while you browse other directories
- **Playback controls** — play, pause, seek, skip, jump to now-playing
- **Real-time progress display** — elapsed time and total duration
- **Now-playing highlighting** — visual indicator across album and browser views
- **Volume control** — raise/lower in 5% steps, displayed live in the footer
- **Repeat mode** — cycle through off, track, and album repeat
- **Shuffle** — pseudo-random next track within the current album

### Lyrics

- **Time-synced lyrics (.lrc)** — parsed, synced to playback, displayed in mini and full-screen views
- **Background lyrics fetching** — if no local `.lrc` exists, lyrics are fetched from [lrclib](https://github.com/tranxuanthang/lrclib) and cached locally
- **In-memory negative cache** — avoids repeated network requests for tracks with no synced lyrics

### YouTube Music Integration

- **Song search** (`:ss`) — search YouTube Music for individual tracks
- **Album search** (`:salb`) — search YouTube Music for official album releases and playlists
- **Artist search** (`:sa`) — search YouTube Music for artist pages; selecting one drills into their albums
- **Paginated results** — 20 results per page with a navigable "Load more" row
- **Typed result badges** — ♪ song / ▣ album / ◉ artist so you know what you're selecting before downloading
- **Full album download** — selecting an album downloads every track individually and normalizes each into your library
- **Real-time download progress** — current track name, position (e.g. 3/12), and overall percentage shown in the status bar
- **Download queue** — press `d` to view all recent downloads with status badges
- **Cancel download** — press `x` to kill the active yt-dlp process immediately

### Library & Organization

- **Deterministic library layout** — downloaded tracks are normalized to `Artist/Year - Album/Title.opus`
- **Primary artist extraction** — collaborative albums land under the primary artist folder, not a separate folder per featured artist
- **Auto-refresh** — browser and album pane update automatically when downloaded tracks land in the current directory
- **Background library indexing** — local search across artist, title, album, file name, and path
- **Incremental search upserts** — newly downloaded tracks appear in search without restarting

---

## Keyboard Controls

| Key       | Action                                         |
| --------- | ---------------------------------------------- |
| ↑ / ↓     | Move selection / scroll lyrics                 |
| Enter     | Open directory or play track                   |
| Backspace | Navigate to parent / go back                   |
| Space     | Play / pause                                   |
| ← / →     | Seek backward / forward                        |
| `s`       | Stop playback                                  |
| `n`       | Jump to now-playing                            |
| `b`       | Focus browser pane                             |
| `t`       | Focus album pane                               |
| `l`       | Focus lyrics pane                              |
| `[` / `]` | Previous / next track                          |
| `r`       | Cycle repeat mode (off → track → album)        |
| `z`       | Toggle shuffle                                 |
| `=`       | Volume up (+5%)                                |
| `-`       | Volume down (−5%)                              |
| `d`       | Toggle download queue overlay                  |
| `x`       | Cancel active download                         |
| `/`       | Open local library search                      |
| `:`       | Open command mode                              |
| `q`       | Quit                                           |

### Command Mode (`:`)

| Command          | Action                                               |
| ---------------- | ---------------------------------------------------- |
| `download <url>` | Download and normalize a track or playlist from URL  |
| `ss <song>`      | Search YouTube Music for a song                      |
| `salb <album>`   | Search YouTube Music for an album                    |
| `sa <artist>`    | Search YouTube Music for an artist                   |

Full-word aliases also work: `songsearch`, `albumsearch`, `artistsearch`.

---

## Configuration

Copy `config.example.toml` to `~/.config/rust-tui-music-player/config.toml` and edit as needed. All settings are optional — defaults are shown:

```toml
# Root directory of your music library
music_root = "~/Downloads/Media/Music"

# Browser yt-dlp reads cookies from for YouTube authentication
# Supported: brave, chrome, firefox, safari, edge, chromium
browser = "brave"
```

---

## Installation & Running

### Prerequisites

- **Rust 1.70+**
- **mpv** — audio playback backend

  ```bash
  brew install mpv          # macOS
  sudo apt install mpv      # Debian / Ubuntu
  sudo pacman -S mpv        # Arch
  ```

- **yt-dlp** — YouTube Music search and download *(required for `:ss`, `:salb`, `:sa`, `download`)*

  ```bash
  brew install yt-dlp       # macOS
  pip install yt-dlp        # pip
  ```

- **ffprobe** (part of ffmpeg) — metadata extraction

  ```bash
  brew install ffmpeg       # macOS
  sudo apt install ffmpeg   # Debian / Ubuntu
  ```

### Build and Run

```bash
git clone https://github.com/ef-molina/rust-tui-music-player.git
cd rust-tui-music-player
cargo run --release
```

The app checks for `yt-dlp` and `mpv` at startup and exits with a clear error message if either is missing.

---

## Architecture Overview

The application follows a strict **event-driven state machine** pattern:

```
Input Events → Event Loop → State Mutations → UI Rendering
```

- **Single source of truth** — all mutable state lives in `AppState`; only the event loop mutates it
- **Pure rendering** — the UI module is read-only and produces no side effects
- **Non-blocking main thread** — network, downloads, and library indexing run on background threads; channels deliver results back to the event loop
- **Filesystem first** — no database; the directory structure is the library

See `DEV_README.md` for deep architecture, module map, logging, and extension notes.

---

## Lyrics System

- Timestamped `.lrc` files are detected alongside audio files and loaded automatically
- If no local `.lrc` file exists, lyrics are fetched in the background from lrclib and cached
- Expected layout: `Music/Album/Track01.mp3` + `Music/Album/Track01.lrc`
- **Mini lyrics view** — beneath the album track list, always time-synced
- **Full lyrics view** (`l`) — full-height pane with time-synced highlighting and manual scrolling

---

## Limitations

- Audio-only playback (video disabled via mpv flag)
- Unix-only IPC (Windows not supported)
- Single mpv instance per session
- YouTube authentication requires a supported browser (Brave, Chrome, Firefox, etc.) to be signed into YouTube

---

## Contributing

Contributions are welcome. Please follow these guidelines:

- Preserve the existing architecture
- Keep all state mutations in the event loop
- UI code must remain pure (no side effects in `ui/mod.rs`)
- Route new behavior through `AppEvent` → `AppState` → rendering
- See `CONTRIBUTING.md` for full guidelines

---

## License

MIT License. See the `LICENSE` file for details.

---

## Acknowledgments

- Built with [ratatui](https://ratatui.rs)
- Terminal input via [crossterm](https://github.com/crossterm-rs/crossterm)
- Audio playback powered by [mpv](https://mpv.io)
- Synced lyrics provided by [lrclib](https://github.com/tranxuanthang/lrclib)
- YouTube Music download via [yt-dlp](https://github.com/yt-dlp/yt-dlp)
