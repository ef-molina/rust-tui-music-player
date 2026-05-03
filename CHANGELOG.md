# Changelog

All notable, user-facing changes to this project are documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

---

## [v0.1.1] – Unreleased

### Added

#### Lyrics

- In-memory negative cache to avoid repeated network fetches for tracks with no synced lyrics
- Automatic background writing of fetched lyrics to `.lrc` files, even if the user skips tracks before fetch completion
- Deterministic handling of stale lyrics fetches without blocking the main event loop

#### Search & Library

- Recursive background library indexing for music search
- Metadata-enriched search over artist, title, album, file name, and path
- Search result ranking that prioritizes exact metadata matches over path-only matches
- Search result activation that jumps to and plays the selected track
- Incremental search upserts after successful normalization so newly added tracks appear without restart
- Bounded navigation history for browser/search flows with `Backspace` back-navigation in normal mode

#### UI & Interaction

- Dedicated search mode opened with `/`, including in-buffer text editing and restore-on-`Esc`
- Centered modal search picker overlay with a dimmed backdrop
- Dedicated statusline for indexing, downloads, lyrics fetch state, and transient feedback
- Refined header/footer hierarchy with clearer status badges and now-playing metadata
- Larger compact lyrics pane for better readability in the split view
- Height-driven compact lyrics rendering that fills the larger pane with more surrounding lyric context
- Inline `:` command helper panel with discoverable command syntax and descriptions
- Command-helper autofill so `Enter` accepts the top matching command before execution
- Visual active-command state in the command bar once a helper suggestion is accepted
- Cleaner visible track labels that hide numeric filename prefixes like `01. `
- Slower, more readable marquee timing for long labels
- Unicode-safe text truncation in the UI to avoid crashes on multibyte characters
- Consistent pane styling, counts in pane titles, and richer search result presentation

### Planned

- Optional persistent lyrics cache (e.g. `~/.cache/rust-tui-music-player`)
- User-configurable controls for automatic lyrics downloading/writing
- Improvements to album and directory name filtering
- Responsive layout improvements for smaller terminal sizes
- Richer metadata density in browser/track/search rows
- Internal robustness and bug fixes identified after v0.1.0 release

This release focuses on polish, correctness, and addressing known limitations
before wider distribution.

---

## [v0.1.0] – Initial Release

### Added

#### Core Browsing & Playback

- Filesystem-first music browser with deterministic, keyboard-driven navigation
- Album-aware playback based on directory structure (no database or indexing)
- mpv-based playback backend using JSON IPC for stable, non-blocking audio control
- Standard playback controls: play, pause, stop, seek, next/previous track
- Automatic track advance within an album
- “Jump to now playing” navigation for fast context recovery

#### Lyrics (Primary Feature)

- Time-synced lyrics support via `.lrc` files
- Local-first lyrics loading with automatic background fetching when missing
- Lyrics fetched using track metadata and cached as `.lrc` files alongside audio
- Spotify-like synced lyrics experience without a graphical UI
- Lyrics remain correctly synchronized when seeking forward or backward
- Dedicated lyrics view mode for focused reading

#### UI & Interaction

- Non-blocking, flicker-free terminal UI
- Clear separation between browser, track list, lyrics, and now-playing views
- Real-time playback progress display with elapsed and total duration
- Visual highlighting of the currently playing track

---

### Guarantees

- The filesystem is the single source of truth (no database, no indexing)
- All mutable application state is owned by a single, explicit `AppState`
- UI rendering is pure and side-effect free
- The main event loop never blocks on I/O or network activity
- Background tasks (e.g. lyrics fetching) do not interfere with UI responsiveness
- Lyrics synchronization is deterministic and resynchronizes correctly after seeking

---

### Known Limitations

- No playlist management (album-based playback only)
- No volume control within the UI (delegated to mpv defaults)
- No streaming or remote file support
- Music root directory is currently static
- Limited metadata display beyond basic track and album information
- In-flight lyrics fetches are not cancelled when switching tracks (results are safely handled instead)
- Unix-only support (mpv IPC via Unix sockets)

---

### Non-Goals

- Graphical (GUI) interface
- Streaming service integration
- Music library management or tagging tools
- Database-backed indexing
- Complex playlist editors or queue systems

---

This release establishes a stable architectural foundation focused on
clarity, determinism, and a lightweight, lyrics-centric listening experience.
