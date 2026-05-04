# Developer README — rust-tui-music-player

---

## 1. Overview

**rust-tui-music-player** is a terminal-based music player built in Rust with a focus on:

- **Filesystem as source of truth** — no database, no indexing required for local playback
- **Deterministic state management** — single-owner event loop pattern
- **Non-blocking UI** — all potentially long-running I/O (network, downloads, library indexing) happens on background threads; the main thread only processes events
- **Time-synced lyrics** — local-first `.lrc` files with background network fallback
- **YouTube Music integration** — search, download, and normalize tracks via yt-dlp
- **Clean separation of concerns** — UI, state, events, input, player, and downloads are strictly decoupled

---

## 2. System Requirements

### Rust Toolchain

- **Rust 1.70+** with `edition = "2024"` in `Cargo.toml`
- Standard toolchain via `rustup`

### External Dependencies (Runtime)

These must be installed and available in `$PATH`:

| Tool        | Purpose                                    | Install (macOS)            |
| ----------- | ------------------------------------------ | -------------------------- |
| **mpv**     | Audio playback via JSON IPC Unix socket    | `brew install mpv`         |
| **yt-dlp**  | YouTube Music search and download          | `brew install yt-dlp`      |
| **ffprobe** | Primary metadata extraction (part of ffmpeg) | `brew install ffmpeg`    |
| **exiftool**| Fallback metadata extraction (optional)    | `brew install exiftool`    |

The app checks for `mpv` and `yt-dlp` at startup and exits with a clear error if either is missing.

### OS Compatibility

- **macOS** ✅ Fully supported
- **Linux** ✅ Fully supported (Unix sockets)
- **Windows** ❌ Not supported (mpv IPC uses a Unix socket path)

---

## 3. Quickstart

```bash
git clone <repo-url>
cd rust-tui-music-player
cargo run
```

### Environment Variables

```bash
# Logs are written to ./debug.log — never to stdout/stderr
RUST_LOG=debug cargo run
RUST_LOG=trace cargo run
tail -f debug.log
```

### Configuration

Copy `config.example.toml` to `~/.config/rust-tui-music-player/config.toml`. All fields are optional:

```toml
music_root = "~/Downloads/Media/Music"   # library root
browser    = "brave"                      # yt-dlp cookie source
```

---

## 4. Architecture Overview

### Event-Driven State Machine

```
Input Polling → Event Translation → State Mutation → UI Render
     ↓               ↓                    ↓              ↓
 crossterm      input/mod.rs         main.rs        ui/mod.rs
```

### Single Source of Truth

All mutable application state lives in `AppState` (`app/mod.rs`).
The event loop in `main.rs` is the **only** code that mutates `AppState`.

- **UI module** — read-only access, pure rendering, no side effects
- **Input module** — stateless translation from keyboard to `AppEvent`
- **Player module** — encapsulates mpv state but does not mutate `AppState`
- **Background threads** — communicate results back via `std::sync::mpsc` channels

---

### Concurrency Model

| Thread                     | Responsibility                                    | Blocking? |
| -------------------------- | ------------------------------------------------- | --------- |
| **Main thread**            | Event loop, state mutations, UI rendering         | No — 10ms poll |
| **Lyrics fetch worker**    | HTTP to lrclib with bounded timeouts              | Yes — isolated |
| **Library indexer**        | Recursive directory scan + metadata enrichment    | Yes — isolated |
| **YouTube search worker**  | yt-dlp flat-playlist JSON fetch                   | Yes — isolated |
| **Download worker**        | yt-dlp subprocess + stdout progress streaming     | Yes — isolated |
| **Album detail workers**   | Parallel yt-dlp fetches to resolve album titles   | Yes — isolated |

**Critical invariant**: The main thread never blocks on network, subprocess, or disk I/O beyond small bounded filesystem operations.

---

## 5. Module Map

### `app/mod.rs` — Application State

**Purpose**: Define `AppState`, the single owner of all mutable state.

**Key types**:
- `AppState`
- `BrowserEntry`, `FocusPane`, `LyricsStatus`
- `RepeatMode` — `Off | Track | Album`
- `DownloadJob`, `DownloadJobStatus` — download queue entries
- `DownloadState` — live progress shown in status bar
- `SearchState` — local library search query and results
- `InputMode` — `Normal | Command | Search`

**Invariants**: no I/O, no UI logic, only the event loop mutates this struct.

---

### `config/mod.rs` — User Configuration

**Purpose**: Load `~/.config/rust-tui-music-player/config.toml` at startup.

**Fields**: `music_root: String`, `browser: String`

Falls back to built-in defaults when the file is absent or malformed. A warning is printed to stderr (before raw mode) on parse error.

---

### `event/mod.rs` — Semantic Events

**Purpose**: Define `AppEvent` — abstract representation of all application actions, input-agnostic.

Includes: navigation, playback, volume, repeat/shuffle, search, command mode, YouTube results, download queue/cancel.

---

### `event/commands.rs` — Command Parsing

**Purpose**: Parse and autocomplete `:command` input.

**Commands**: `download <url>`, `ss <song>`, `salb <album>`, `sa <artist>`

`filtered_command_specs(query)` powers the inline command helper. `parse_command(input)` returns a typed `Command` variant.

---

### `event/jobs.rs` — Background Job Results

**Purpose**: Messages sent from worker threads back to the main event loop via `jobs_rx`.

**Variants**: `DownloadStarted` (with PID), `DownloadProgress`, `DownloadFinished`, `DownloadFailed`, `DownloadCancelled`, `YoutubeSearchDone`, `YoutubeSearchFailed`

---

### `input/mod.rs` — Input Handling

**Purpose**: Poll terminal input and translate raw key events into `AppEvent`s. Behaviour changes based on `InputMode`.

---

### `ui/mod.rs` — UI Rendering

**Purpose**: Pure rendering using `ratatui`. Read-only access to `AppState`.

**Invariants**: no logging to terminal, no state mutation, no side effects.

---

### `player/mod.rs` + `mpv.rs` — Playback Control

**Purpose**: Manage the mpv subprocess and JSON IPC.

**Interface**: `load(path)`, `toggle_pause()`, `seek(seconds)`, `stop()`, `adjust_volume(delta)`, `shutdown()`

Volume is stored in `Player.volume` (0–150) and synced to mpv via `set_property volume`.

**Known limitation**: hardcoded socket path `/tmp/rust-tui-mpv.sock`; stale socket on crash.

---

### `fs/mod.rs` — Filesystem Operations

**Purpose**: Directory scanning and album detection.

**`fs/normalize.rs`** — Download normalization pipeline:
- Reads embedded metadata via ffprobe
- Classifies track kind (AutoGeneratedAlbum, OfficialAudioSingle, OfficialVideo, CreatorUpload)
- Extracts primary artist from YouTube Music comment `·` format
- Moves file to canonical path: `{library}/{Artist}/{Year} - {Album}/{Title}.opus`
- Writes clean tags via exiftool

---

### `metadata/` — Metadata Extraction

**Purpose**: ffprobe primary, exiftool fallback. Produces `TrackMetadata` including `album_artist` which is used to determine the library folder.

---

### `lyrics/` — Lyrics Parsing & State

**Purpose**: Parse `.lrc` files and sync lyric lines to playback time.

---

### `lyrics_fetch/` — Network Lyrics

**Purpose**: Background lyrics fetch via lrclib.net with tiered lookup and stale-result protection.

---

### `search/mod.rs` — Local Library Search

**Purpose**: Background indexer that walks the library directory and enriches entries with metadata. Results are filtered in real-time as the user types.

**Messages**: `Batch`, `EnrichedBatch`, `Upsert`, `Finished`, `Failed`

`Upsert` is sent after a successful download normalization so new tracks appear without restarting the indexer.

---

### `youtube/mod.rs` — YouTube Music Search

**Purpose**: Wraps yt-dlp to search YouTube Music for songs, albums, and artists.

**Public API**:
- `search_songs(query, page, browser)` — YouTube Music search filtered to `watch?v=` URLs
- `search_albums(query, page, browser)` — fetches MPREb_* and VL* entries in parallel to resolve titles
- `search_artists(query, page, browser)` — fetches UC* channel pages in parallel to resolve artist names

All functions are synchronous and designed to run on a background thread. Browser name is passed from config rather than hardcoded.

---

## 6. Download Pipeline

```
:salb Pink Floyd
    → spawn_youtube_search (background thread)
        → yt-dlp --flat-playlist (search)
        → parallel fetch per MPREb_/VL* entry to resolve titles
    → YoutubeSearchDone → populate results pane

Enter on result
    → spawn_playlist_download (background thread)
        → yt-dlp (download all tracks to per-job staging subdir)
        → DownloadStarted (with PID, for cancellation)
        → DownloadProgress (per [download] line from stdout)
        → DownloadFinished × N (one per file)
        → staging dir removed

DownloadFinished (main thread)
    → normalize_downloaded_track (move + rename + write tags)
    → spawn_index_update (upsert into search index)
    → browser/album pane refreshed if current dir matches
```

---

## 7. Logging & Debugging

**Critical constraint**: Logging must **never** write to stdout/stderr during the TUI session.

```rust
let log_file = File::create("debug.log")?;
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .with_writer(log_file)
    .init();
```

```bash
RUST_LOG=debug cargo run
tail -f debug.log
```

---

## 8. Testing

The project has **55 automated unit tests** covering:

- Lyrics parsing and synchronization (`lyrics/`)
- LRC file loading and edge cases (`lyrics/loader.rs`)
- Download normalization pipeline (`fs/normalize.rs`)
- YouTube JSON entry parsing (`youtube/mod.rs`)
- Command parsing and autocomplete (`event/commands.rs`)
- Navigation history (`main.rs`)
- UI helpers (`ui/mod.rs`)

```bash
cargo test
```

Manual testing is still recommended for:
- Full download → normalize → playback flow
- YouTube search result navigation
- mpv cleanup on quit
- yt-dlp cookie authentication across browsers

---

## 9. Known Limitations & Tech Debt

### Active

- Hardcoded mpv socket path (`/tmp/rust-tui-mpv.sock`) — stale on crash
- Single mpv instance shared across all playback
- No active cancellation of in-flight lyrics fetches (stale results are safely ignored)
- Orphaned `.lrc.tmp` files possible on crash

### Resolved in recent releases

- ~~Hardcoded music root directory~~ — configurable via `config.toml` (v0.3.0)
- ~~No shuffle or repeat~~ — added in v0.3.0
- ~~No volume control~~ — added in v0.3.0
- ~~No download queue visibility~~ — added in v0.3.0

---

## 10. Roadmap

### Near-Term

- Optional persistent lyrics cache (disk-level reuse across sessions)
- User-configurable control over automatic lyrics downloading
- mpv socket lifecycle improvements (crash recovery)

### Medium-Term

- Metadata-driven browsing and display
- Best-effort lyrics fetch cancellation

### Long-Term

- Windows support
- Responsive layout for small terminal sizes

---

## Appendix: Project Structure

```
src/
├── main.rs              # Event loop, startup, download orchestration
├── app/                 # AppState and all mutable state types
├── config/              # Config file loading (config.toml)
├── event/
│   ├── mod.rs           # AppEvent enum
│   ├── commands.rs      # Command parsing and autocomplete
│   └── jobs.rs          # Background job result types
├── input/               # Keyboard → AppEvent translation
├── ui/                  # Pure rendering (ratatui)
├── player/              # mpv subprocess and IPC
├── fs/
│   ├── mod.rs           # Directory scanning and album detection
│   └── normalize.rs     # Download normalization pipeline
├── metadata/            # ffprobe + exiftool metadata extraction
├── lyrics/              # LRC parsing and time-sync state
├── lyrics_fetch/        # Background lyrics fetch (lrclib)
├── search/              # Background library indexer
└── youtube/             # YouTube Music search (yt-dlp wrapper)
```
