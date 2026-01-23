//! mpv IPC controller.
//!
//! Responsible for spawning mpv and sending JSON IPC commands.
//! No UI logic. No filesystem logic.

use std::path::Path;
use std::process::{Child, Command, Stdio};

pub struct MpvController {
    child: Child,
}

/// Spawn a new mpv process with IPC enabled.
/// The caller is responsible for managing the MpvController instance.
impl MpvController {
    pub fn spawn() -> std::io::Result<Self> {
        let child = Command::new("mpv")
            .arg("--no-video") // Audio only
            .arg("--idle=yes") // Keep mpv running even when no file is loaded
            .arg("--no-terminal") // Disable mpv's own terminal UI
            .arg("--input-ipc-server=/tmp/rust-tui-mpv.sock") // IPC socket
            .stdin(Stdio::null()) // We won't use stdin
            .stdout(Stdio::null()) // We won't use stdout
            .stderr(Stdio::null()) // We won't use stderr
            .spawn()?;

        Ok(Self { child })
    }

    pub fn load_file(&self, _path: &Path) {
        // Stub — we’ll add JSON IPC next
    }

    pub fn set_pause(&self, _pause: bool) {
        // Stub — we’ll add JSON IPC next
    }
}

/// Ensure mpv process is killed when MpvController is dropped.
/// This prevents orphaned mpv processes.
impl Drop for MpvController {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
