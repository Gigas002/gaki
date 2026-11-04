//! Local IPC endpoint: a Unix domain socket.
//!
//! The endpoint path can be overridden with the global `--socket` flag so a driver
//! can isolate its own daemon.

use std::path::PathBuf;
use std::sync::OnceLock;
use tokio::net::{UnixListener, UnixStream};

/// The `--socket` path, when given.
static ENDPOINT_OVERRIDE: OnceLock<String> = OnceLock::new();

/// Use `path` as the socket for this process (both serving and connecting).
/// Call once at startup.
pub fn set_endpoint(path: String) {
    let _ = ENDPOINT_OVERRIDE.set(path);
}

/// The `--socket` path, if one was set.
pub fn endpoint_override() -> Option<&'static str> {
    ENDPOINT_OVERRIDE.get().map(String::as_str)
}

/// Socket path, also used for logging.
pub fn endpoint() -> String {
    if let Some(p) = ENDPOINT_OVERRIDE.get() {
        return p.clone();
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
/// the socket (`aurelia-{uid}.info`) so a custom `--socket` gets its own
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
