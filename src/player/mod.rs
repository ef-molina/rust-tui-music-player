//! Audio playback controller.
//!
//! Owns interaction with the playback backend (mpv).
//! No UI logic. No filesystem logic.

mod mpv;

use crate::player::mpv::MpvController;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

pub struct Player {
    pub state: PlaybackState,
    pub current_track: Option<PathBuf>,
    mpv: MpvController,
}

impl Player {
    pub fn new() -> Self {
        let mpv =
            MpvController::spawn().expect("Failed to spawn mpv. Is it installed and in your PATH?");
        Self {
            state: PlaybackState::Stopped,
            current_track: None,
            mpv,
        }
    }

    pub fn load(&mut self, track: PathBuf) {
        self.mpv.load_file(&track);
        self.current_track = Some(track);
        self.state = PlaybackState::Playing;
    }

    pub fn toggle_pause(&mut self) {
        match self.state {
            PlaybackState::Playing => {
                self.mpv.set_pause(true);
                self.state = PlaybackState::Paused;
            }
            PlaybackState::Paused => {
                self.mpv.set_pause(false);
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Stopped => {}
        }
    }

    pub fn seek(&mut self, seconds: i64) {
        // Only seek if something is playing or paused
        if matches!(self.state, PlaybackState::Playing | PlaybackState::Paused) {
            self.mpv.seek(seconds);
        }
    }

    pub fn stop(&mut self) {
        self.mpv.stop();
        self.state = PlaybackState::Stopped;
        self.current_track = None;
    }

    pub fn shutdown(&mut self) {
        // Stop playback if needed
        self.mpv.stop();

        // Explicitly kill mpv
        self.mpv.shutdown();

        self.state = PlaybackState::Stopped;
        self.current_track = None;
    }
}
