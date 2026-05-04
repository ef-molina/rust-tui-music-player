# Changelog

All notable, user-facing changes to this project are documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

---

## [v0.3.0] – 2026-05-02

### Added

#### Playback Controls

- Volume control: `=` raises volume 5%, `-` lowers it (0–150% range via mpv). Current level shown live in the footer
- Repeat mode: `r` cycles through off → track → album. Current mode shown in the footer
- Shuffle mode: `z` toggles shuffle. When active, next track is picked pseudo-randomly from the album

#### Download Queue & Cancellation

- `d` opens a download queue overlay showing all recent jobs with status badges (active / done / failed / cancelled)
- `x` cancels the active download by terminating the yt-dlp process immediately
- Download history is retained for up to 20 jobs per session

#### Configuration

- Optional config file at `~/.config/rust-tui-music-player/config.toml`
- Supports `music_root` (library path) and `browser` (cookie source for yt-dlp)
- Falls back gracefully to built-in defaults when the file is absent or malformed
- `config.example.toml` included in the repository as a reference

#### Search Improvements

- `:ss` now searches YouTube Music instead of regular YouTube, returning clean audio tracks rather than music videos and reaction content
- `:salb` includes YouTube Music playlist entries (VL\*) alongside official album releases (MPREb\*), roughly doubling the number of results
- Selecting an `:sa` artist result now drills into their albums instead of attempting to download the whole channel
- Result hint line updates contextually — shows "browse albums" for artist results, "download" for songs and albums

### Fixed

- `yt-dlp` absence is detected before entering raw terminal mode so the error message is readable
- Per-job staging subdirectories are removed after all tracks normalize, preventing indefinite accumulation of temp files
- Command bar now closes immediately when a search command is submitted, making results navigable without an extra Escape
- Artist search now resolves real names instead of showing "Unknown Artist" for all results

---

## [v0.2.0] – 2026-05-02

### Added

#### YouTube Music Integration

- Artist, album, and song search via YouTube Music using `:sa`, `:salb`, and `:ss`
  (full-word aliases: `artistsearch`, `albumsearch`, `songsearch`)
- Results paginate in groups of 20 with a navigable "Load more" row
- Each result shows a kind badge (♪ song / ▣ album / ◉ artist) for clarity
- Selecting an album result downloads all tracks individually as separate files
- Selecting a song result downloads and normalizes a single track
- Real-time download progress shown in the status bar: track title, position in album (e.g. 3/12), and overall percentage bar

#### Download & Normalization

- Deterministic library layout: `Artist/Year - Album/Title.opus`
- Primary artist extracted from YouTube Music comment metadata so collaborative albums stay in one folder
- Each track in a multi-track album is individually normalized and indexed
- Per-job staging subdirectory prevents concurrent downloads from interfering with each other
- Browser and album pane refresh automatically when downloaded tracks land in the currently viewed directory

#### Command Bar

- Pressing Enter on a partial command (e.g. `d`) autofills the top suggestion instead of submitting unknown
- Active command name shown as a badge in the command bar title once a command is recognized
- Unknown and incomplete commands show a warning status instead of silently failing

#### Lyrics

- In-memory negative cache to avoid repeated network fetches for tracks with no synced lyrics
- Automatic background writing of fetched lyrics to `.lrc` files, even if the user skips tracks before fetch completes
- Deterministic handling of stale lyrics fetches without blocking the main event loop

#### Search & Library

- Recursive background library indexing for music search
- Metadata-enriched search over artist, title, album, file name, and path
- Search result ranking that prioritizes exact metadata matches over path-only matches
- Search result activation that jumps to and plays the selected track
- Incremental search upserts after successful normalization so newly downloaded tracks appear without restart
- Bounded navigation history for browser/search flows with `Backspace` back-navigation in normal mode

#### UI & Interaction

- Dedicated search mode opened with `/`, including in-buffer text editing and restore-on-`Esc`
- Centered modal search picker overlay with a dimmed backdrop
- Dedicated statusline for indexing, downloads, lyrics fetch state, and transient feedback
- Refined header/footer hierarchy with clearer status badges and now-playing metadata
- Larger compact lyrics pane with height-driven rendering for more surrounding lyric context
- Inline `:` command helper panel with discoverable command syntax and descriptions
- Cleaner visible track labels that strip numeric filename prefixes like `01. `
- Slower, more readable marquee timing for long labels
- Consistent pane styling, counts in pane titles, and richer search result presentation

### Fixed

- Unicode-safe middle truncation no longer panics on multibyte (non-ASCII) filenames
- Multi-track album downloads now save all tracks, not just the last one written to the staging directory

---

## [v0.1.0] – 2026-04-01

Initial release.

### Added

#### Core Browsing & Playback

- Filesystem-first music browser with deterministic, keyboard-driven navigation
- Album-aware playback based on directory structure (no database or indexing required)
- mpv-based playback backend using JSON IPC for stable, non-blocking audio control
- Standard playback controls: play, pause, stop, seek, next/previous track
- Automatic track advance within an album
- "Jump to now playing" navigation for fast context recovery

#### Lyrics

- Time-synced lyrics support via `.lrc` files
- Local-first lyrics loading with automatic background fetching when missing
- Lyrics fetched using track metadata and cached as `.lrc` files alongside audio
- Lyrics remain correctly synchronized when seeking forward or backward
- Dedicated full-screen lyrics view mode

#### UI & Interaction

- Non-blocking, flicker-free terminal UI
- Clear separation between browser, track list, lyrics, and now-playing views
- Real-time playback progress display with elapsed and total duration
- Visual highlighting of the currently playing track

### Guarantees

- The filesystem is the single source of truth (no database, no indexing)
- All mutable application state is owned by a single, explicit `AppState`
- UI rendering is pure and side-effect free
- The main event loop never blocks on I/O or network activity
- Background tasks (e.g. lyrics fetching) do not interfere with UI responsiveness
- Lyrics synchronization is deterministic and resynchronizes correctly after seeking

### Known Limitations at Release

- No playlist management (album-based playback only)
- No volume control within the UI _(added in v0.3.0)_
- No shuffle or repeat modes _(added in v0.3.0)_
- Music root directory was static _(configurable since v0.3.0)_
- No YouTube Music integration _(added in v0.2.0)_
- In-flight lyrics fetches are not actively cancelled when switching tracks (stale results are safely ignored)
- Unix-only support (mpv IPC via Unix sockets)

### Non-Goals

- Graphical (GUI) interface
- Streaming service integration
- Music library management or tagging tools
- Database-backed indexing
- Complex playlist editors or queue systems
