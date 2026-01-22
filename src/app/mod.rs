//! Application state module.
//!
//! This module defines `AppState`, the single owner of all mutable state
//! for the application. Every change to the program’s state flows through
//! this struct and is driven by the central event loop in `main.rs`.
//!
//! Design principles:
//! - Single owner of mutable state
//! - No direct I/O (terminal, filesystem, mpv)
//! - Pure data + small helpers
//!
//! Other modules may read from `AppState`, but only the event loop
//! mutates it.
//! 

#[derive(Debug)]
pub struct AppState {
    /// set to true when the application should exit
    pub should_quit: bool,
}

impl AppState {
    /// Create a new `AppState` with default values.
    pub fn new() -> Self {
        Self {
            should_quit: false,
        }
    }
}