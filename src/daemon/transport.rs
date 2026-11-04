//! Local IPC endpoint: a Unix domain socket.
//!
//! The endpoint path can be overridden with `AURELIA_DAEMON_SOCKET` (used by the
//! `--socket` flag) so a driver can isolate its own daemon.

use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};

/// Socket path, also used for logging.
pub fn endpoint() -> String {
    if let Ok(p) = std::env::var("AURELIA_DAEMON_SOCKET") {
        return p;
    }
    let base = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let uid = unsafe { libc::getuid() };
    base.join(format!("aurelia-{uid}.sock"))
        .to_string_lossy()
        .into_owned()
}

/// Filesystem path of the daemon's identity marker (version + pid), written by the
/// running daemon and read by thin clients to detect a version mismatch. Kept next to
/// the socket (`aurelia-{uid}.info`) so a custom `AURELIA_DAEMON_SOCKET` gets its own
/// isolated marker.
pub fn version_marker_path() -> PathBuf {
    PathBuf::from(endpoint()).with_extension("info")
}

pub struct Listener(UnixListener);

impl Listener {
    pub async fn bind() -> std::io::Result<Self> {
        let path = endpoint();
        // A stale socket file from a previous run blocks bind(); remove it.
        let _ = std::fs::remove_file(&path);
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Ok(Self(UnixListener::bind(&path)?))
    }

    pub async fn next(&mut self) -> std::io::Result<UnixStream> {
        let (stream, _addr) = self.0.accept().await?;
        Ok(stream)
    }
}

pub async fn connect() -> std::io::Result<UnixStream> {
    UnixStream::connect(endpoint()).await
}
