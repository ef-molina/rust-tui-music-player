//! mpv IPC controller.
//!
//! Responsible for spawning mpv and sending JSON IPC commands.
//! No UI logic. No filesystem logic.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};

const MPV_SOCKET: &str = "/tmp/rust-tui-mpv.sock";

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
            .arg(format!("--input-ipc-server={}", MPV_SOCKET)) // IPC socket
            .stdin(Stdio::null()) // We won't use stdin
            .stdout(Stdio::null()) // We won't use stdout
            .stderr(Stdio::null()) // We won't use stderr
            .spawn()?;

        Ok(Self { child })
    }

    fn send(&self, json: &str) {
        if let Ok(mut stream) = UnixStream::connect(MPV_SOCKET) {
            let _ = stream.write_all(json.as_bytes());
            let _ = stream.write_all(b"\n");
        }
        // Silent failure for now — we’ll add logging later
    }

    pub fn load_file(&self, path: &Path) {
        let cmd = format!(
            r#"{{ "command": ["loadfile", "{}", "replace"] }}"#,
            path.display()
        );
        self.send(&cmd);
    }

    pub fn set_pause(&self, pause: bool) {
        let cmd = format!(r#"{{ "command": ["set_property", "pause", {}] }}"#, pause);
        self.send(&cmd);
    }

    pub fn stop(&self) {
        let cmd = r#"{ "command": ["stop"] }"#;
        self.send(cmd);
    }

    pub fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(MPV_SOCKET);
    }
}
