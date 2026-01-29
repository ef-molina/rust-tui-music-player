# Contributing to rust-tui-music-player

Thank you for your interest in contributing!  
This project values **architectural clarity**, **predictable behavior**, and **explicit state ownership** over rapid feature growth.

Before submitting changes, please read this document carefully.

---

## Project Philosophy

Maintainer Authority

The project maintainer has final say on design, architecture, and scope decisions.
Discussion is welcome, but architectural direction is not decided by consensus.

This is a **filesystem-first, event-driven TUI music player**.  
It is not designed as a generic UI playground or a streaming client.

Key principles that must be preserved:

- **Single source of truth** for all mutable state
- **Pure UI rendering** (no side effects)
- **Non-blocking terminal UI**
- **Explicit, debuggable behavior**
- **Incremental, surgical changes**

If a change violates these principles, it will not be accepted.

---

## High-Level Architecture Rules (Non-Negotiable)

### 1. State Ownership

- All mutable application state **must live in `AppState`**
- Only the **main event loop** may mutate `AppState`
- Worker threads may compute or fetch data, but must communicate results via channels

❌ Do not mutate state from:

- UI code
- input handlers
- background threads
- helper modules

---

### 2. Event-Driven Flow

All behavior must follow this flow:

```

Input → AppEvent → AppState mutation → UI render

```

- New behavior requires a new or extended `AppEvent`
- State transitions must be explicit and traceable
- Avoid “hidden” behavior in helper functions

---

### 3. UI Code Must Be Pure

The UI module:

- Reads from `AppState`
- Produces widgets
- Has **no side effects**

❌ UI code must not:

- Perform I/O
- Spawn threads
- Log to stdout/stderr
- Mutate state
- Make decisions that belong in the event loop

---

### 4. Terminal Safety

- **Never log to stdout or stderr**
- All logging must go through `tracing` and write to `debug.log`

Violating this will corrupt the TUI and is considered a breaking change.

---

### 5. Concurrency & I/O

- The **main thread must never block** on:
  - Network I/O
  - IPC
  - Long-running filesystem operations
- Background threads are allowed for:
  - Lyrics fetching
  - External tool invocation
- Background threads must:
  - Communicate via channels
  - Be bounded in scope
  - Clean up after themselves

Do not introduce async runtimes unless explicitly discussed.

---

## What Kinds of Contributions Are Welcome?

### ✅ Good Contributions

- Bug fixes with clear reproduction steps
- Performance improvements (especially UI responsiveness)
- Robustness improvements (timeouts, cancellation, cleanup)
- Test coverage for parsers and detectors
- Documentation improvements
- Small, incremental features that fit the existing model

### ⚠️ Discuss First

- New playback modes (shuffle, repeat)
- Configuration systems
- Cross-platform abstractions
- Refactors affecting multiple modules

Open an issue or discussion before starting these.

### ❌ Not Acceptable Without Explicit Approval

- Architectural rewrites
- Introducing a database
- Moving state out of `AppState`
- Logging to the terminal
- Replacing mpv as the backend
- Large dependency additions without justification

---

## Development Workflow

1. **Fork the repository**
2. Create a feature branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

````

3. Make small, focused commits
4. Run:

   ```bash
   cargo check
   cargo build --release
   ```
5. Test manually (see checklist below)
6. Open a pull request with a clear description

---

## Commit & PR Guidelines

### Commit Messages

* Use clear, descriptive messages
* Explain *why*, not just *what*

Examples:

```
Fix lyrics fetch blocking UI by moving HTTP to worker thread
Add negative cache marker for failed lyrics fetch
```

### Pull Request Description

Your PR should include:

* What problem this solves
* Why this approach fits the existing architecture
* What files/modules are affected
* Any limitations or follow-ups

---

## Manual Testing Checklist (Required)

Before submitting a PR, verify:

* [ ] UI remains responsive under normal use
* [ ] No terminal corruption (no stdout/stderr logging)
* [ ] Playback starts/stops correctly
* [ ] Lyrics load correctly (local and/or fetched)
* [ ] No orphaned mpv processes after quitting
* [ ] No crashes when dependencies are missing (if applicable)
* [ ] New behavior is traceable via `debug.log`

---

## Logging Expectations

* Use `tracing::{trace, debug, info, warn, error}`
* Prefer structured fields:

  ```rust
  debug!(path = %track_path.display(), "Loading track");
  ```
* Do not log excessively in tight loops without good reason

---

## Documentation

If your change affects behavior or architecture:

* Update **DEV_README.md** if internal behavior changes
* Update **README.md** if user-facing behavior changes
* Add comments only where they clarify *why*, not *what*

---

## Code Style

* Prefer clarity over cleverness
* Avoid unnecessary abstractions
* Keep functions small and focused
* Follow existing naming conventions

---

## Getting Help

If you’re unsure about an approach:

* Open an issue describing the problem
* Ask clarifying questions in the PR
* Reference `DEV_README.md` when discussing architecture

Thoughtful questions are always welcome.

---

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for helping keep **rust-tui-music-player** clean, predictable, and maintainable.

```

---
````
