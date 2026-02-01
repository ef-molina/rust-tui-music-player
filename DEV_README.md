# Developer README — rust-tui-music-player

---

## 1. Overview

**rust-tui-music-player** is a terminal-based music player built in Rust with a focus on:

- **Filesystem as source of truth** — no database, no indexing
- **Deterministic state management** — single-owner event loop pattern
- **Non-blocking UI** — all potentially long-running I/O (network, IPC) happens off the main thread; filesystem operations are scoped and bounded
- **Time-synced lyrics** — local-first `.lrc` files with background network fallback
- **Clean separation of concerns** — UI, state, events, input, and player are strictly decoupled

This is a developer-focused project emphasizing maintainability, debuggability, and architectural clarity over feature completeness.

---

## 2. System Requirements

### Rust Toolchain

- **Rust 1.70+**
- The codebase assumes Rust 2021 semantics  
  (`Cargo.toml` currently specifies `edition = "2024"` and should be aligned intentionally)
- Standard toolchain via `rustup`

### External Dependencies (Runtime)

These must be installed and available in `$PATH`:

- **mpv** — audio playback via JSON IPC over Unix sockets
  - macOS: `brew install mpv`
  - Linux: `apt install mpv` or equivalent
  - Verify: `mpv --version`

- **ffprobe** (part of ffmpeg) — primary metadata extraction
  - macOS: `brew install ffmpeg`
  - Linux: `apt install ffmpeg`
  - Verify: `ffprobe -version`

- **exiftool** (optional) — fallback metadata extraction
  - macOS: `brew install exiftool`
  - Linux: `apt install libimage-exiftool-perl`
  - Verify: `exiftool -ver`

### OS Compatibility

- **macOS** ✅ Fully supported
- **Linux** ✅ Fully supported (Unix sockets)
- **Windows** ❌ Not supported (mpv IPC uses a Unix socket path)

---

## 3. Quickstart

### Build and Run

```bash
git clone <repo-url>
cd rust-tui-music-player

cargo build --release
cargo run --release

# Or directly in dev mode
cargo run
```

### Environment Variables

```bash
RUST_LOG=trace cargo run
RUST_LOG=debug cargo run

# Logs are written to ./debug.log (NOT terminal)
tail -f debug.log
```

### Default Music Directory

The app defaults to `$HOME/Downloads/Media/Music`.
This is intentionally hardcoded during early development to reduce configuration complexity while core architecture stabilizes.

```rust
let root_dir = PathBuf::from(
    std::env::var("HOME")
        .map(|h| format!("{}/Downloads/Media/Music", h))
        .unwrap_or_else(|_| ".".into()),
);
```

To use a different directory, modify this path and rebuild.

---

## 4. Architecture Overview

### Event-Driven State Machine

The application follows a strict **synchronous event loop** pattern:

```
Input Polling → Event Translation → State Mutation → UI Render
     ↓               ↓                    ↓              ↓
 crossterm      input/mod.rs         main.rs        ui/mod.rs
```

### Single Source of Truth

All mutable application state lives in `AppState` (`app/mod.rs`).
The event loop in `main.rs` is the **only** code that mutates `AppState`.

- **UI module**: read-only access, pure rendering
- **Input module**: stateless translation from keyboard to `AppEvent`
- **Player module**: encapsulates mpv state, but does not mutate `AppState`

---

### Concurrency Model

| Thread                  | Responsibility                                         | Blocking?                                 |
| ----------------------- | ------------------------------------------------------ | ----------------------------------------- |
| **Main thread**         | Event loop, state mutations, UI rendering, mpv polling | No — uses non-blocking `poll_event(10ms)` |
| **Lyrics fetch worker** | HTTP requests with bounded timeouts                    | Yes — isolated from UI                    |

**Critical invariant**:
The main thread never blocks on network or IPC I/O.

Filesystem operations are synchronous but intentionally scoped to small, bounded operations.

---

### mpv Integration

- mpv is spawned as a child process on startup with
  `--input-ipc-server=/tmp/rust-tui-mpv.sock`
- Communication occurs via JSON IPC over a Unix socket
- Commands: `loadfile`, `set_property pause`, `seek`, `stop`, `quit`
- Queries: `get_property time-pos`, `get_property duration`
- Polling strategy: every 10ms tick (tunable; favors responsiveness over efficiency)

---

## 5. Module Map

### app/mod.rs — Application State

**Purpose**: Define `AppState`, the single owner of all mutable state.

**Key types**:

- `AppState`
- `BrowserEntry { name: String, is_dir: bool }`
- `FocusPane` — `Browser | Album | Lyrics`
- `LyricsStatus` — `None | Loading | Loaded(LyricsState)`

**Invariants**:

- No I/O operations
- No UI logic
- Only the event loop mutates this struct

---

### event/mod.rs — Semantic Events

**Purpose**: Define `AppEvent` enum — abstract representation of all application actions.

**Design principle**: Events are semantic and input-agnostic.

---

### input/mod.rs — Input Handling

**Purpose**: Poll terminal input and translate raw key events into `AppEvent`s.

```rust
pub fn poll_event(timeout: Duration) -> io::Result<Option<AppEvent>>
```

---

### ui/mod.rs — UI Rendering

**Purpose**: Pure rendering using `ratatui`.

**Invariants**:

- Read-only access to `AppState`
- No logging to terminal
- No state mutation

---

### player/mod.rs + mpv.rs — Playback Control

**Purpose**: Manage mpv subprocess and IPC.

**Known limitations**:

- Hardcoded socket path
- Stale socket on crash
- Silent failure if mpv is missing

---

### fs/mod.rs — Filesystem Operations

**Purpose**: Directory scanning and album detection.

**Album detection rules**:

- Leaf album: audio files + no subdirectories
- Loose tracks: audio files allowed with subdirectories

---

### metadata/ — Metadata Extraction

**Purpose**: ffprobe primary, exiftool fallback.

**Lyrics gate**:
Only complete metadata triggers lyrics fetching.

---

### lyrics/ — Lyrics Parsing & State

**Purpose**: Parse `.lrc` files and sync lyric lines to playback time.

---

### lyrics_fetch/ — Network Lyrics

**Purpose**: Background lyrics fetch via lrclib.net.

**Behavior**:

- Tiered lookup strategy:
  1. Title + artist + duration + album
  2. Title + artist + duration
  3. Canonical single fallback

- Blocking HTTP requests executed on a worker thread
- Explicit network timeouts (connect + read)
- Results guarded by a monotonically increasing request ID to prevent stale fetch results from applying after rapid track changes
- Atomic write: `.lrc.tmp` → `.lrc` when committing lyrics

A failed fetch is terminal for the current track activation; retries occur only on subsequent activations.

**Known limitations**:

- No active cancellation of in-flight fetches (stale results are safely ignored)
- No negative cache for repeated fetch failures
- Orphaned `.lrc.tmp` files possible on crash

---

## 6. Logging & Debugging

**Critical constraint**: Logging must **never** write to stdout/stderr.

```rust
let log_file = File::create("debug.log")?;
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .with_writer(log_file)
    .init();
```

Structured logging is used to enable postmortem debugging without impacting UI responsiveness.

Use:

```bash
RUST_LOG=debug cargo run
RUST_LOG=trace cargo run
```

---

## 7. Testing & Verification

**Automated tests**: None currently.

**Manual testing recommended**:

- Navigation responsiveness
- Lyrics fetch success/failure
- Album persistence
- mpv cleanup on quit
- Metadata fallback behavior

---

## 8. Known Limitations & Tech Debt

### Critical

- Hardcoded mpv socket path
- Stale socket on crash
- Rust edition mismatch (2021 semantics vs 2024 declaration)

### High Priority

- No negative lyrics cache
- No active cancellation for in-flight lyrics fetches (stale results are ignored safely)
- Silent mpv IPC failures
- Hardcoded music root directory

### Medium / Low

- Aggressive metrics polling (10ms)
- Orphaned `.lrc.tmp` files on crash
- No dependency validation
- No UI error reporting

---

## 9. Roadmap

### Near-Term

- In-memory negative cache for failed lyrics lookups
- mpv socket lifecycle fixes

### Medium-Term

- Central disk cache for opportunistic lyrics reuse
- Configurable music directory
- Dependency validation
- Best-effort lyrics fetch cancellation

### Long-Term

- Windows support
- Automated tests
- Performance tuning

---

## Appendix: Project Structure

```
src/
├── main.rs
├── app/
├── event/
├── input/
├── ui/
├── player/
├── fs/
├── metadata/
├── lyrics/
└── lyrics_fetch/
```
