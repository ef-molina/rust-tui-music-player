# Rust TUI Music Player

A terminal-based music player built in Rust, designed for fast, keyboard-driven music browsing and playback with a focus on **clean architecture**, **explicit state ownership**, and **predictable behavior**.

This project emphasizes filesystem-based music organization, album-aware playback, and a fully time-synced lyrics system — all within a responsive, flicker-free terminal UI.

---

## Features

- **Keyboard-driven navigation**
  Browse your music library using arrow keys and Enter, with explicit pane focus switching

- **Hierarchical album view**
  Navigate directory trees and automatically detect album folders (leaf directories with audio files)

- **Persistent album context**
  Album selection remains active even when browsing other directories

- **Playback controls**
  Play, pause, seek forward/backward, skip to next/previous track, and jump to now-playing

- **Real-time progress display**
  Shows current playback position and total duration

- **Now-playing highlighting**
  Visual indicator of the currently playing track across album and browser views

- **Time-synced lyrics (.lrc)**
  Lyrics are parsed, synced to playback time, and displayed in both mini and full-screen views

- **Clean separation of concerns**
  Modular architecture with strict boundaries between UI, state, events, and player control

---

## Architecture Overview

The application follows a strict **event-driven state machine** pattern:

```
Input Events → Event Loop → State Mutations → UI Rendering
```

### Key Design Principles

#### Single Source of Truth

All mutable state lives in `AppState`.
No other module mutates application data.

#### Pure Rendering

The UI module is read-only and produces no side effects.
Focus changes never mutate application data.

#### Decoupled Concerns

- **UI Module (`ui/`)**
  Pure rendering using `ratatui`; reads from `AppState` only

- **Event Module (`event/`)**
  Semantic application events (input-agnostic)

- **Input Module (`input/`)**
  Keyboard input mapped to semantic `AppEvent`s

- **Player Module (`player/`)**
  mpv subprocess management and JSON IPC communication

- **Lyrics Module (`lyrics/`)**
  LRC parsing and time-based lyric state tracking

- **Filesystem Module (`fs/`)**
  Directory traversal, album detection, and entry enumeration

- **App Module (`app/`)**
  Core application state and invariants

---

## Album State Management

The player distinguishes between **two independent navigation contexts**:

### Browser State

Tracks filesystem navigation:

- `current_dir`
- `browser_entries`
- `selected_index`

### Album State

Tracks playback context:

- `active_album_dir`
- `album_entries`
- `album_selected`

An **album** is defined as any directory that:

- Contains one or more audio files
- Contains no subdirectories

Directories with audio files but no subdirectories — including the root directory — are treated as **implicit albums**.

Once an album is activated, it remains active even if the user navigates elsewhere in the browser. This enables the mental model:

> _Navigate the filesystem with one hand, control playback with the other._

---

## Player Integration

Playback is handled by [mpv](https://mpv.io), controlled via Unix socket JSON IPC.

The player is launched with:

```
--no-video --idle=yes --input-ipc-server=/tmp/rust-tui-mpv.sock
```

The `Player` abstraction exposes:

- Playback state (Playing / Paused / Stopped)
- Current track path
- Real-time playback metrics (position, duration)
- Automatic track advancement
- Clean shutdown and process lifecycle handling

---

## Lyrics System

### Lyrics Loading

- Timestamped `.lrc` files are detected alongside audio files
- Lyrics are loaded automatically when a track starts playing
- Lyrics are cleared on stop or track change

Expected file layout:

```
Music/
├── Album/
│   ├── Track01.mp3
│   ├── Track01.lrc
│   └── Track02.mp3
```

### LRC Format

```
[00:12.00]First line of lyrics
[00:17.20]Second line of lyrics
[00:21.10]Third line of lyrics
```

### Display Modes

- **Mini lyrics view**
  Displayed beneath the album track list, always time-synced

- **Full lyrics view** (`l`)
  Full-height lyrics pane with:
  - Time-synced highlighting
  - Centered active line
  - Manual scrolling with automatic resume

If no lyrics are available, the lyrics pane displays a clear fallback message.

---

## Installation & Running

### Prerequisites

- **Rust 1.70+** (Edition 2021)
- **mpv** (audio backend)
- **Unix socket support** (Linux, macOS, or WSL2)

### Install mpv

**macOS (Homebrew)**

```bash
brew install mpv
```

**Debian / Ubuntu**

```bash
sudo apt-get install mpv
```

**Arch Linux**

```bash
sudo pacman -S mpv
```

### Build and Run

```bash
git clone https://github.com/yourusername/rust-tui-music-player.git
cd rust-tui-music-player
cargo build --release
./target/release/rust-tui-music-player
```

By default, the player starts with the music library rooted at:

```
~/Downloads/Media/Music
```

---

## Usage

### Keyboard Controls

| Key         | Action                                     |
| ----------- | ------------------------------------------ |
| `↑` / `↓`   | Move selection / scroll lyrics             |
| `Enter`     | Open directory or play track               |
| `Backspace` | Navigate to parent directory / exit lyrics |
| `Space`     | Play / pause                               |
| `←` / `→`   | Seek backward / forward                    |
| `s`         | Stop playback                              |
| `n`         | Jump to now-playing                        |
| `b`         | Focus browser pane                         |
| `t`         | Focus album pane                           |
| `l`         | Focus lyrics pane                          |
| `[` / `]`   | Previous / next track                      |
| `q`         | Quit                                       |

### Typical Workflow

1. Launch the player
2. Browse directories with `↑` / `↓`
3. Press `Enter` on an album directory
4. Press `Enter` on a track to play
5. Navigate elsewhere using the browser pane
6. Return to the album pane — the album remains active
7. Press `l` to view synced lyrics (if available)
8. Control playback at any time using keyboard shortcuts

---

## Configuration

### Music Library Path

The music library root is currently hardcoded in `src/app/mod.rs`.

To change it, update:

```rust
let root_dir = PathBuf::from(
    std::env::var("HOME")
        .map(|h| format!("{}/your/custom/path", h))
        .unwrap_or_else(|_| ".".into()),
);
```

Future versions may support configuration files or environment variables.

---

## Limitations & Known Issues

- **Audio-only playback** (video disabled in mpv)
- **Filesystem-based albums only** (no metadata-based grouping)
- **No tag parsing** (artist, album, year not read)
- **Single mpv instance** (shared IPC socket)
- **No playlists or queue system**
- **No shuffle or repeat modes**
- **Unix-only IPC** (Windows support not yet implemented)

---

## State Management Notes

Phase 1 established explicit album state ownership:

- `active_album_dir` is the authoritative album source
- Album state persists independently of browser navigation
- Focus changes (`b`, `t`, `l`) are pure and data-safe

Phase 2 decoupled rendering from focus:

- Album pane remains visible as long as an album is active
- Browser navigation does not affect playback context
- Visual focus indicators communicate interaction state without mutating data

This architecture ensures predictable, intuitive behavior even as features grow.

---

## Future Improvements

- Metadata parsing (ID3 / Vorbis tags)
- Fuzzy search for albums and tracks
- Shuffle and repeat modes
- Playlist support
- Configuration file support
- Lyrics enhancements (online fetch, karaoke-style highlighting, lyric-based seeking)
- Performance improvements for large libraries
- Windows support
- Unit and integration tests

---

## Contributing

Contributions are welcome. Please follow these guidelines:

- Preserve the existing architecture
- Keep all state mutations in the event loop
- UI code must remain pure
- Explain **why**, not just **what**
- Run `cargo check` and `cargo build --release` before submitting
- Route new behavior through `AppEvent` → `AppState` → rendering

---

## License

MIT License. See the `LICENSE` file for details.

---

## Acknowledgments

- Built with [ratatui](https://ratatui.rs)
- Terminal input via [crossterm](https://github.com/crossterm-rs/crossterm)
- Audio playback powered by [mpv](https://mpv.io)
