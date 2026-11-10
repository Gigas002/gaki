//! Local IPC endpoint: a Unix domain socket.
//!
//! The socket is `$XDG_RUNTIME_DIR/gaki/daemon.sock`: the runtime dir is the
//! user's own (tmpfs, `0700`, removed at logout), the `gaki/` directory in it
//! is `0700` and the socket `0600`, and every connection's peer must be this
//! uid (`SO_PEERCRED`). Without `XDG_RUNTIME_DIR` there is no daemon: there is
//! no `/tmp` fallback, which other users could reach. The global `--socket`
//! flag overrides the path so a driver can isolate its own daemon; the socket
//! is still `0600` and peer-checked there.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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

/// `$XDG_RUNTIME_DIR`, which must be set and absolute.
fn runtime_dir(value: Option<std::ffi::OsString>) -> io::Result<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "XDG_RUNTIME_DIR is not set, so there is no private place for the daemon \
                 socket; commands run without a daemon",
            )
        })
}

/// The socket path: `--socket`, else `$XDG_RUNTIME_DIR/gaki/daemon.sock`.
pub fn endpoint() -> io::Result<PathBuf> {
    if let Some(p) = ENDPOINT_OVERRIDE.get() {
        return Ok(PathBuf::from(p));
    }
    Ok(runtime_dir(std::env::var_os("XDG_RUNTIME_DIR"))?
        .join("gaki")
        .join("daemon.sock"))
}

/// The daemon's identity marker (version, pid, build), next to the socket
/// (`daemon.toml`), so a custom `--socket` gets its own.
pub fn version_marker_path() -> io::Result<PathBuf> {
    Ok(endpoint()?.with_extension("toml"))
}

/// Make `dir` a directory only this user can use: create it `0700`, or
/// tighten an existing one we own. One owned by someone else is refused.
fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != rustix::process::getuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a directory owned by this user; refusing to put the daemon socket there",
                dir.display()
            ),
        ));
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub struct Listener(UnixListener);

impl Listener {
    pub async fn bind() -> io::Result<Self> {
        let path = endpoint()?;
        if let Some(parent) = path.parent() {
            if endpoint_override().is_some() {
                std::fs::create_dir_all(parent)?;
            } else {
                ensure_private_dir(parent)?;
            }
        }
        // A stale socket file from a previous run blocks bind(); remove it.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self(listener))
    }

    /// The next connection from this uid. Others are refused and logged.
    pub async fn next(&mut self) -> io::Result<UnixStream> {
        let me = rustix::process::getuid().as_raw();
        loop {
            let (stream, _addr) = self.0.accept().await?;
            match stream.peer_cred() {
                Ok(cred) if cred.uid() == me => return Ok(stream),
                Ok(cred) => tracing::warn!(
                    "daemon: refused a connection from uid {} (pid {:?})",
                    cred.uid(),
                    cred.pid()
                ),
                Err(e) => tracing::warn!("daemon: refused a connection: no peer credentials ({e})"),
            }
        }
    }
}

pub async fn connect() -> io::Result<UnixStream> {
    UnixStream::connect(endpoint()?).await
}

#[cfg(test)]
mod tests;
