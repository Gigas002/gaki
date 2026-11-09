mod filter;
pub(crate) mod raw;
pub(crate) mod unauthenticated;

use crate::auth::{AuthConfirmationHandler, ClientInfo, GuardDataStore, RefreshToken};
pub use crate::base::{ConnectionTrait, ReadonlyConnection};
use crate::base::{
    EncodableMessage, JobId, NetMessageHeader, RawSteamId, ReceivableMessage, SendableMessage,
    ServiceMethodRequest, ServiceNotification,
};
use crate::message::{ServiceMethodMessage, ServiceMethodResponseMessage};
use crate::net::{NetworkError, RawNetMessage};
use crate::serverlist::ServerList;
use crate::session::{ConnectionError, RawSession, SessionAuthenticationDetails};
pub(crate) use filter::MessageFilter;
use futures::{FutureExt, Sink, SinkExt};
use gaki_steam_vent_proto::{JobMultiple, MsgKindEnum};
use raw::RawConnection;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use steamid_ng::SteamID;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tracing::instrument;
pub use unauthenticated::UnAuthenticatedConnection;

pub(crate) type Result<T, E = NetworkError> = std::result::Result<T, E>;

type TransportWriter = Arc<Mutex<dyn Sink<RawNetMessage, Error = NetworkError> + Unpin + Send>>;

/// Send raw messages to steam
#[derive(Clone)]
pub(crate) struct MessageSender {
    write: TransportWriter,
}

impl MessageSender {
    pub async fn send_raw(&self, raw_message: RawNetMessage) -> Result<()> {
        self.write.lock().await.send(raw_message).await?;
        Ok(())
    }
}

/// A connection to the steam server
#[derive(Clone)]
pub struct Connection {
    raw: RawConnection,
    pub(crate) auth: SessionAuthenticationDetails,
}

impl Debug for Connection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Connection {
    pub(self) fn new(raw: RawConnection, auth: SessionAuthenticationDetails) -> Self {
        Self { raw, auth }
    }

    /// Start an anonymous client session on a new connection
    pub async fn anonymous(server_list: &ServerList) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .anonymous()
            .await
    }

    /// Start an anonymous server session on a new connection
    pub async fn anonymous_server(server_list: &ServerList) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .anonymous_server()
            .await
    }

    /// Start an authenticated client session on a new connection by logging in with username, password and steam guard.
    pub async fn login<H: AuthConfirmationHandler, G: GuardDataStore>(
        server_list: &ServerList,
        account: &str,
        password: &str,
        guard_data_store: G,
        confirmation_handler: H,
        client_info: &ClientInfo,
    ) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .login(
                account,
                password,
                guard_data_store,
                confirmation_handler,
                client_info,
            )
            .await
    }

    /// Create a new authenticated session with a previously obtained refresh token.
    ///
    /// You can get the refresh token after login from [`Connection::refresh_token`].
    pub async fn login_with_refresh_token(
        server_list: &ServerList,
        token: &RefreshToken,
    ) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .login_with_refresh_token(token)
            .await
    }

    /// Password logon with a logon ID.
    ///
    /// Same-ID logons replace each other.
    pub async fn login_with_logon_id<H: AuthConfirmationHandler, G: GuardDataStore>(
        server_list: &ServerList,
        account: &str,
        password: &str,
        guard_data_store: G,
        confirmation_handler: H,
        client_info: &ClientInfo,
        logon_id: u32,
    ) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .login_with_logon_id(
                account,
                password,
                guard_data_store,
                confirmation_handler,
                client_info,
                logon_id,
            )
            .await
    }

    /// Refresh-token logon with a logon ID.
    ///
    /// See [`Self::login_with_logon_id`].
    pub async fn login_with_refresh_token_and_logon_id(
        server_list: &ServerList,
        token: &RefreshToken,
        logon_id: u32,
    ) -> Result<Self, ConnectionError> {
        UnAuthenticatedConnection::connect(server_list)
            .await?
            .login_with_refresh_token_and_logon_id(token, logon_id)
            .await
    }

    /// Get the refresh token for the current session.
    ///
    /// This can be used for future authentication with [`Connection::login_with_refresh_token`].
    pub fn refresh_token(&self) -> &RefreshToken {
        &self.auth.refresh_token
    }

    /// Get the steam id of the user or anonymous account that is connected.
    pub fn steam_id(&self) -> SteamID {
        self.auth.steam_id
    }

    /// Get a numeric identifier for the session
    pub fn session_id(&self) -> i32 {
        self.raw.session.session_id
    }

    /// Get the id of the "cell" the client is in.
    ///
    /// This can be used by [`ServerList`] discovery to potentially connect to a more optimal steam server.
    ///
    /// [`ServerList`]: `crate::ServerList`
    pub fn cell_id(&self) -> u32 {
        self.raw.session.cell_id
    }

    /// Get the ip address of the client, as determined by steam.
    pub fn public_ip(&self) -> IpAddr {
        self.auth.public_ip
    }

    /// Get the country code of the client, based on the IP address as determined by steam.
    pub fn ip_country_code(&self) -> &str {
        &self.auth.ip_country_code
    }

    /// Change how long we wait for a response from methods that require a response before considering it failed.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.raw.timeout = timeout;
    }

    /// Get all messages that haven't been filtered by any of the filters
    ///
    /// Note that at most 32 unprocessed connections are stored and calling
    /// this method clears the buffer
    pub fn take_unprocessed(&self) -> Vec<RawNetMessage> {
        self.raw.filter.unprocessed()
    }
}

/// Registration of a multi-response job; dropping it (the stream finished,
/// failed, timed out or was dropped early) unregisters the job id.
struct JobMultiGuard {
    filter: MessageFilter,
    job_id: JobId,
    recv: tokio::sync::mpsc::Receiver<RawNetMessage>,
}

impl JobMultiGuard {
    fn register(filter: &MessageFilter, job_id: JobId) -> Self {
        JobMultiGuard {
            recv: filter.on_job_id_multi(job_id),
            filter: filter.clone(),
            job_id,
        }
    }
}

impl Drop for JobMultiGuard {
    fn drop(&mut self) {
        self.filter.complete_job_id_multi(self.job_id);
    }
}

/// Send the request, then yield responses until one reports completion.
fn job_multi_stream<Rsp: ReceivableMessage + JobMultiple>(
    job: JobMultiGuard,
    send: impl Future<Output = Result<()>> + Send,
    response_timeout: Duration,
) -> impl Stream<Item = Result<Rsp>> + Send {
    enum State<S, R> {
        Send(S, R),
        Receive(R),
        Done,
    }
    futures::stream::unfold(State::Send(Box::pin(send), job), move |state| async move {
        let mut job = match state {
            State::Send(send, job) => match send.await {
                Ok(()) => job,
                Err(e) => return Some((Err(e), State::Done)),
            },
            State::Receive(job) => job,
            State::Done => return None,
        };
        let next = timeout(response_timeout, job.recv.recv())
            .await
            .map_err(|_| NetworkError::Timeout)
            .and_then(|raw| raw.ok_or(NetworkError::EOF))
            .and_then(RawNetMessage::into_message::<Rsp>);
        Some(match next {
            Ok(msg) if msg.completed() => (Ok(msg), State::Done),
            Ok(msg) => (Ok(msg), State::Receive(job)),
            Err(e) => (Err(e), State::Done),
        })
    })
}

pub(crate) trait ConnectionImpl: Sync + Debug {
    fn timeout(&self) -> Duration;
    fn filter(&self) -> &MessageFilter;
    fn raw_session(&self) -> &RawSession;
    fn auth_details(&self) -> Option<&SessionAuthenticationDetails>;

    fn generate_header(&self, job: bool) -> NetMessageHeader {
        NetMessageHeader {
            session_id: self.raw_session().session_id,
            source_job_id: if job {
                self.raw_session().job_id.next()
            } else {
                JobId::NONE
            },
            target_job_id: JobId::NONE,
            steam_id: RawSteamId::new(
                self.auth_details()
                    .map(|auth| auth.steam_id.steam64())
                    .unwrap_or_default(),
            ),
            source_app_id: self.auth_details().and_then(|auth| auth.app_id),
            ..NetMessageHeader::default()
        }
    }

    fn one_with_header<T: ReceivableMessage + 'static>(
        &self,
    ) -> impl Future<Output = Result<(NetMessageHeader, T)>> + 'static {
        // async block instead of async fn, so we don't have to tie the lifetime of the returned future
        // to the lifetime of &self
        let fut = self.filter().one_kind(T::KIND);
        async move {
            let raw = fut.await.map_err(|_| NetworkError::EOF)?;
            raw.into_header_and_message()
        }
    }

    fn on_with_header<T: ReceivableMessage + 'static>(
        &self,
    ) -> impl Stream<Item = Result<(NetMessageHeader, T)>> + 'static {
        BroadcastStream::new(self.filter().on_kind(T::KIND)).map(|raw| {
            let raw = raw.map_err(|_| NetworkError::EOF)?;
            raw.into_header_and_message()
        })
    }

    fn raw_send<Msg: SendableMessage>(
        &self,
        header: NetMessageHeader,
        msg: Msg,
    ) -> impl Future<Output = Result<()>> + Send {
        let kind = msg.kind();
        let is_protobuf = msg.is_protobuf();
        self.raw_send_with_kind(header, msg, kind, is_protobuf)
    }

    fn raw_send_with_kind<Msg: EncodableMessage, K: MsgKindEnum>(
        &self,
        header: NetMessageHeader,
        msg: Msg,
        kind: K,
        is_protobuf: bool,
    ) -> impl Future<Output = Result<()>> + Send;
}

impl ConnectionImpl for Connection {
    fn raw_session(&self) -> &RawSession {
        &self.raw.session
    }

    fn auth_details(&self) -> Option<&SessionAuthenticationDetails> {
        Some(&self.auth)
    }

    fn timeout(&self) -> Duration {
        self.raw.timeout()
    }

    fn filter(&self) -> &MessageFilter {
        self.raw.filter()
    }

    async fn raw_send_with_kind<Msg: EncodableMessage, K: MsgKindEnum>(
        &self,
        header: NetMessageHeader,
        msg: Msg,
        kind: K,
        is_protobuf: bool,
    ) -> Result<()> {
        <RawConnection as ConnectionImpl>::raw_send_with_kind(
            &self.raw,
            header,
            msg,
            kind,
            is_protobuf,
        )
        .await
    }
}

macro_rules! impl_connection {
    ($con:path) => {
        impl ConnectionTrait for $con {
            type Error = NetworkError;

            fn on_notification<T: ServiceNotification>(
                &self,
            ) -> impl Stream<Item = Result<T>> + 'static {
                BroadcastStream::new(self.filter().on_notification(T::NOTIFICATION_NAME))
                    .filter_map(|res| res.ok())
                    .map(|raw| raw.into_notification())
            }

            fn one<T: ReceivableMessage + 'static>(
                &self,
            ) -> impl Future<Output = Result<T>> + 'static {
                self.one_with_header::<T>()
                    .map(|res| res.map(|(_, msg)| msg))
            }

            fn on<T: ReceivableMessage + 'static>(
                &self,
            ) -> impl Stream<Item = Result<T>> + 'static {
                self.on_with_header::<T>()
                    .map(|res| res.map(|(_, msg)| msg))
            }

            async fn service_method<Msg: ServiceMethodRequest>(
                &self,
                msg: Msg,
            ) -> Result<Msg::Response> {
                let header = self.generate_header(true);
                let recv = self.filter().on_job_id(header.source_job_id);
                self.raw_send(header, ServiceMethodMessage(msg)).await?;
                let message = timeout(self.timeout(), recv)
                    .await
                    .map_err(|_| NetworkError::Timeout)?
                    .map_err(|_| NetworkError::EOF)?
                    .into_message::<ServiceMethodResponseMessage>()?;
                message.into_response::<Msg>()
            }

            async fn job<Req: SendableMessage, Rsp: ReceivableMessage>(
                &self,
                msg: Req,
            ) -> Result<Rsp> {
                let header = self.generate_header(true);
                let recv = self.filter().on_job_id(header.source_job_id);
                self.raw_send(header, msg).await?;
                timeout(self.timeout(), recv)
                    .await
                    .map_err(|_| NetworkError::Timeout)?
                    .map_err(|_| NetworkError::EOF)?
                    .into_message()
            }

            fn job_multi<Req: SendableMessage, Rsp: ReceivableMessage + JobMultiple>(
                &self,
                msg: Req,
            ) -> impl Stream<Item = Result<Rsp>> + Send {
                let header = self.generate_header(true);
                let job = JobMultiGuard::register(self.filter(), header.source_job_id);
                let send = self.raw_send(header, msg);
                job_multi_stream(job, send, self.timeout())
            }

            #[instrument(skip(msg), fields(kind = ?msg.kind()))]
            fn send<Msg: SendableMessage>(
                &self,
                msg: Msg,
            ) -> impl Future<Output = Result<()>> + Send {
                self.raw_send(self.generate_header(false), msg)
            }
        }
    };
}

impl_connection!(RawConnection);
impl_connection!(Connection);

#[cfg(test)]
mod tests;
