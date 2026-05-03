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

#[derive(Debug, Clone, Copy, Default)]
pub struct PlaybackMetrics {
    pub position: Option<f64>, // seconds
    pub duration: Option<f64>, // seconds
}

pub struct Player {
    pub state: PlaybackState,
    pub metrics: PlaybackMetrics,
    pub current_track: Option<PathBuf>,
    pub volume: u32,
    mpv: MpvController,
    has_started: bool,
}

impl Player {
    pub fn new() -> Self {
        let mpv =
            MpvController::spawn().expect("Failed to spawn mpv. Is it installed and in your PATH?");
        Self {
            state: PlaybackState::Stopped,
            metrics: PlaybackMetrics::default(),
            current_track: None,
            volume: 100,
            mpv,
            has_started: false,
        }
    }

    pub fn adjust_volume(&mut self, delta: i32) {
        self.volume = (self.volume as i32 + delta).clamp(0, 150) as u32;
        self.mpv.set_volume(self.volume);
    }

    pub fn load(&mut self, track: PathBuf) {
        self.mpv.load_file(&track);
        self.current_track = Some(track);
        self.state = PlaybackState::Playing;
        self.metrics = PlaybackMetrics::default();
        self.has_started = false;
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

    pub fn is_track_finished(&self) -> bool {
        matches!(self.state, PlaybackState::Playing)
            && self.has_started
            && self.metrics.position.is_none()
            && self.current_track.is_some()
    }

    pub fn stop(&mut self) {
        self.mpv.stop();
        self.state = PlaybackState::Stopped;
        self.current_track = None;
        self.metrics = PlaybackMetrics::default();
        self.has_started = false;
    }

    pub fn shutdown(&mut self) {
        // Stop playback if needed
        self.mpv.stop();

        // Explicitly kill mpv
        self.mpv.shutdown();

        self.state = PlaybackState::Stopped;
        self.current_track = None;
    }

    pub fn poll_metrics(&mut self) {
        if !matches!(self.state, PlaybackState::Playing | PlaybackState::Paused) {
            return;
        }

        let pos = self.mpv.get_property_f64("time-pos");
        let dur = self.mpv.get_property_f64("duration");

        if pos.is_some() {
            self.has_started = true;
        }

        self.metrics.position = pos;
        self.metrics.duration = dur;
    }
}
