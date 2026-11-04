//! The async bridge between the synchronous UI loop and `libgaki`.
//!
//! [`Worker`] owns a tokio runtime and one Steam session. The UI thread sends a
//! [`Request`] with [`Worker::request`], gets a [`Ticket`] back, and picks up the
//! matching [`Reply`] later with [`Worker::poll`], which never blocks. Each
//! request runs as its own task, so a slow one (a download, a login) doesn't
//! hold up the others.
//!
//! The session is shared: requests run against cheap clones of one
//! [`SteamClient`], the way the CLI daemon serves commands over one connection.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use libgaki::ops::auth::SessionStatus;
use libgaki::steam_client::SteamClient;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, mpsc as async_mpsc};

/// Identifies one request, so its [`Reply`] can be matched up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ticket(u64);

/// What the UI asks the worker to do.
#[derive(Debug)]
pub enum Request {
    /// Who is signed in, without connecting to Steam.
    SessionStatus,
    /// Restore the stored session (connects when one is stored), then report
    /// the resulting status. A failed restore still leaves a logged-out
    /// session, so offline features keep working.
    Restore,
}

/// The worker's answer to one [`Request`].
#[derive(Debug)]
pub enum Response {
    SessionStatus(SessionStatus),
    Restored(Result<SessionStatus>),
}

/// A [`Response`] tagged with the [`Ticket`] of the request it answers.
#[derive(Debug)]
pub struct Reply {
    pub ticket: Ticket,
    pub response: Response,
}

/// How long dropping a [`Worker`] waits for in-flight tasks before abandoning them.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(1);

/// The UI's handle on the `libgaki` worker. Dropping it shuts the worker down.
pub struct Worker {
    runtime: Option<Runtime>,
    requests: async_mpsc::UnboundedSender<(Ticket, Request)>,
    replies: mpsc::Receiver<Reply>,
    next_ticket: AtomicU64,
}

impl Worker {
    /// Start the runtime and the request loop.
    pub fn start() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            // Steam connect/auth futures nest deeply; debug builds need the room
            // (the CLI does the same).
            .thread_stack_size(16 * 1024 * 1024)
            .thread_name("gaki-worker")
            .build()
            .context("could not start the async runtime")?;
        let (requests, inbox) = async_mpsc::unbounded_channel();
        let (outbox, replies) = mpsc::channel();
        runtime.spawn(serve(inbox, outbox));
        Ok(Self {
            runtime: Some(runtime),
            requests,
            replies,
            next_ticket: AtomicU64::new(0),
        })
    }

    /// Queue `request`. Never blocks; the answer arrives through [`Worker::poll`].
    pub fn request(&self, request: Request) -> Ticket {
        let ticket = Ticket(self.next_ticket.fetch_add(1, Ordering::Relaxed));
        // The loop only stops when the worker is dropped, so this can't fail
        // while `self` is alive.
        let _ = self.requests.send((ticket, request));
        ticket
    }

    /// The next finished reply, if any. Never blocks.
    pub fn poll(&self) -> Option<Reply> {
        self.replies.try_recv().ok()
    }

    /// Wait up to `timeout` for the next reply. For tests and shutdown paths;
    /// the UI loop uses [`Worker::poll`].
    pub fn wait(&self, timeout: Duration) -> Option<Reply> {
        self.replies.recv_timeout(timeout).ok()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN_GRACE);
        }
    }
}

/// The one Steam session the worker holds. Cloning shares it.
#[derive(Clone, Default)]
struct Session(Arc<Mutex<Option<SteamClient>>>);

impl Session {
    /// The current client, if a session was restored or logged in.
    async fn client(&self) -> Option<SteamClient> {
        self.0.lock().await.clone()
    }

    /// Restore the stored session and keep the result. Holding the lock
    /// across the restore makes concurrent restores wait for the first.
    async fn restore(&self) -> Result<SteamClient> {
        let mut slot = self.0.lock().await;
        let client = libgaki::ops::session::restore_client().await?;
        *slot = Some(client.clone());
        Ok(client)
    }
}

/// The request loop: one task per request, until the [`Worker`] is dropped.
async fn serve(
    mut inbox: async_mpsc::UnboundedReceiver<(Ticket, Request)>,
    outbox: mpsc::Sender<Reply>,
) {
    let session = Session::default();
    while let Some((ticket, request)) = inbox.recv().await {
        let session = session.clone();
        let outbox = outbox.clone();
        tokio::spawn(async move {
            let response = handle(request, &session).await;
            // The UI may already be gone; nothing to do then.
            let _ = outbox.send(Reply { ticket, response });
        });
    }
}

async fn handle(request: Request, session: &Session) -> Response {
    match request {
        Request::SessionStatus => {
            let client = session.client().await;
            Response::SessionStatus(libgaki::ops::auth::session_status(client.as_ref()).await)
        }
        Request::Restore => Response::Restored(match session.restore().await {
            Ok(client) => Ok(libgaki::ops::auth::session_status(Some(&client)).await),
            Err(e) => Err(e),
        }),
    }
}
