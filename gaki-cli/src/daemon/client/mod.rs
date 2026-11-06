//! Thin-client side: forward this invocation to a running daemon (auto-spawning one
//! if needed) and relay stdio + exit code. Returns `Ok(None)` to mean "no daemon
//! available — run the command locally instead".

use anyhow::{Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::transport;
use super::{Header, proto};

/// How long to wait for an auto-spawned daemon to come up before giving up and
/// running locally.
const SPAWN_WAIT: std::time::Duration = std::time::Duration::from_millis(100);
const SPAWN_ATTEMPTS: u32 = 50; // ~5s total

/// Outcome of forwarding to the daemon.
pub enum Forwarded {
    /// Daemon ran it; its exit code.
    Done(i32),
    /// Our stdio died mid-relay; parent gone.
    OutputClosed,
    /// No daemon reachable; run locally.
    Unavailable,
}

/// Run this command via the daemon. With `no_spawn`, only use one that is
/// already running.
pub async fn try_forward(no_spawn: bool) -> Result<Forwarded> {
    let Some(stream) = connect_or_spawn(no_spawn).await else {
        return Ok(Forwarded::Unavailable);
    };
    let argv: Vec<String> = std::env::args().collect();
    forward(stream, argv).await
}

/// Connect to the daemon; if none is listening, spawn one and wait for it.
///
/// A daemon left over from a previous `gaki-cli` build parses forwarded commands with its
/// own (stale) CLI, so it rejects newly added subcommands with "unrecognized subcommand".
/// So when an existing daemon is a different version from this binary, stop it and start a
/// fresh one before forwarding.
async fn connect_or_spawn(no_spawn: bool) -> Option<impl AsyncRead + AsyncWrite + Unpin + Send> {
    // No private socket location (no `XDG_RUNTIME_DIR`): no daemon to reach or
    // start, so the command runs here.
    if let Err(e) = transport::endpoint() {
        tracing::debug!("{e}");
        return None;
    }
    if let Ok(stream) = transport::connect().await {
        let current = env!("CARGO_PKG_VERSION");
        let info = super::read_daemon_info();
        if !daemon_needs_restart(info.as_ref(), current) {
            return Some(stream);
        }
        // Version mismatch, or an old daemon predating the marker (absent -> "unknown").
        drop(stream);
        let reported = info.as_ref().map_or("unknown", |i| i.version.as_str());
        tracing::info!(
            "restarting gaki-cli daemon: it is running v{reported} but this binary is v{current}"
        );
        restart_daemon(info.as_ref().map(|i| i.pid));

        // Wait (bounded) for the stopped daemon to release the socket, so the replacement
        // we spawn below doesn't bow out to it and we don't reconnect to the very daemon
        // we just stopped.
        for _ in 0..SPAWN_ATTEMPTS {
            if transport::connect().await.is_err() {
                break;
            }
            tokio::time::sleep(SPAWN_WAIT).await;
        }
    }

    // Never auto-spawn when the driver manages the daemon lifecycle itself
    // (`--no-spawn`, e.g. Heroic): some driver spawn chains — like
    // `powershell Start-Process -Wait` — wait on the whole descendant tree, so
    // a daemon spawned from this client process would keep that wait blocked
    // for the daemon's lifetime. Run the command locally instead.
    if no_spawn {
        return None;
    }

    spawn_daemon().ok()?;
    for _ in 0..SPAWN_ATTEMPTS {
        tokio::time::sleep(SPAWN_WAIT).await;
        if let Ok(stream) = transport::connect().await {
            return Some(stream);
        }
    }
    None
}

/// Whether an existing daemon described by `info` must be restarted before use.
///
/// True when its version differs from `current`, or the marker is absent/unparseable
/// (`info == None`) — a daemon predating this marker can't be trusted to parse newer
/// commands either, so treat "unknown" as a mismatch. The same applies to the
/// binary identity: a rebuild at an unchanged crate version must also restart the
/// daemon, or it keeps serving stale code.
fn daemon_needs_restart(info: Option<&super::DaemonInfo>, current: &str) -> bool {
    let Some(info) = info else { return true };
    if info.version != current {
        return true;
    }
    match (&info.build_id, super::current_build_id()) {
        (Some(daemon), Some(client)) => *daemon != client,
        // Old marker without identity: conservative restart.
        (None, _) => true,
        // Own identity unreadable: don't restart-loop on it.
        (_, None) => false,
    }
}

/// Stop a stale-version daemon so the next connect spawns a fresh one. `pid` is the
/// daemon's own pid from its marker when available — kill exactly that process. Without a
/// marker (a daemon predating this mechanism) fall back to stopping every session daemon,
/// since we can't otherwise identify which process owns the socket.
fn restart_daemon(pid: Option<u32>) {
    let killed = match pid {
        Some(pid) => crate::proc_admin::kill_pids(&[pid]),
        None => {
            let daemons: Vec<u32> = crate::proc_admin::find_gaki_processes()
                .into_iter()
                .filter(|p| p.is_daemon)
                .map(|p| p.pid)
                .collect();
            crate::proc_admin::kill_pids(&daemons)
        }
    };
    tracing::info!("stopped {killed} stale gaki-cli daemon(s)");
    super::clear_daemon_info();
}

/// Launch a detached `gaki-cli daemon` process.
fn spawn_daemon() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    // The daemon serves the same config dir and socket as this client.
    if let Some(dir) = super::config_dir_override() {
        cmd.arg("--config-dir").arg(dir);
    }
    if let Some(socket) = transport::endpoint_override() {
        cmd.arg("--socket").arg(socket);
    }
    cmd.arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    detach(&mut cmd);
    cmd.spawn()?;
    Ok(())
}

fn detach(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // New process group so the daemon outlives the spawning shell / Heroic process.
    cmd.process_group(0);
}

/// Write a relayed output chunk to a local stream and flush it immediately, so the
/// daemon's stdout/stderr appears with the same interleaving the user would see when
/// running locally.
async fn write_and_flush<W: AsyncWrite + Unpin>(w: &mut W, data: &[u8]) -> std::io::Result<()> {
    w.write_all(data).await?;
    w.flush().await
}

/// Did our local output stream close?
fn is_output_closed(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::UnexpectedEof
    )
}

/// Send the header + our stdin, relay the daemon's stdout/stderr, return its exit code.
async fn forward<S>(stream: S, argv: Vec<String>) -> Result<Forwarded>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, writer) = proto::split_shared(stream);

    // Header first.
    let header = serde_json::to_vec(&Header { argv })?;
    proto::send_frame(&writer, proto::C_HEADER, &header)
        .await
        .context("failed sending request to daemon")?;

    // Pump our stdin → daemon. Runs concurrently; aborted once the command exits, so
    // a command that never reads stdin doesn't block on it.
    let writer_in = writer.clone();
    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) => {
                    let _ = proto::send_frame(&writer_in, proto::C_STDIN_EOF, &[]).await;
                    break;
                }
                Ok(n) => {
                    if proto::send_frame(&writer_in, proto::C_STDIN, &buf[..n])
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Relay daemon output until the exit frame.
    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    let mut code = 0;
    loop {
        // Broken pipe means the parent left.
        let relayed = match proto::read_frame(&mut reader).await? {
            Some((proto::C_STDOUT, data)) => write_and_flush(&mut stdout, &data).await,
            Some((proto::C_STDERR, data)) => write_and_flush(&mut stderr, &data).await,
            Some((proto::C_EXIT, data)) => {
                code = i32::from_be_bytes(
                    data.get(..4)
                        .and_then(|b| b.try_into().ok())
                        .unwrap_or([0; 4]),
                );
                break;
            }
            Some(_) => Ok(()),
            None => break,
        };
        if let Err(e) = relayed {
            stdin_task.abort();
            if is_output_closed(&e) {
                return Ok(Forwarded::OutputClosed);
            }
            return Err(e).context("failed relaying daemon output");
        }
    }
    stdin_task.abort();
    Ok(Forwarded::Done(code))
}

#[cfg(test)]
mod tests;
