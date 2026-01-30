# Changelog

All notable, user-facing changes to this project are documented in this file.

This project follows [Semantic Versioning](https://semver.org/).

---

## [v0.1.1] – Unreleased

### Planned

- Improvements to album and directory name filtering
- More robust lyrics fetching and caching behavior
- Better handling of mpv lifecycle and cleanup on exit
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
- No negative cache for failed lyrics fetches
- No cancellation of in-flight lyrics fetch when switching tracks
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
