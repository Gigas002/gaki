use crate::auth::{ConfirmationError, ConfirmationMethod, RefreshToken, RefreshTokenError};
use crate::base::{JobId, NetMessageHeader, RawSteamId, ReceivableMessage};
use crate::connection::ConnectionImpl;
use crate::connection::raw::RawConnection;
use crate::eresult::EResult;
use crate::net::NetworkError;
use gaki_steam_vent_crypto::CryptoError;
use gaki_steam_vent_proto::protobuf::MessageField;
use gaki_steam_vent_proto::steammessages_base::CMsgIPAddress;
use gaki_steam_vent_proto::steammessages_base::cmsg_ipaddress;
use gaki_steam_vent_proto::steammessages_clientserver_login::{
    CMsgClientHello, CMsgClientLogon, CMsgClientLogonResponse,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use steamid_ng::{
    AccountType, Instance, InstanceFlags, InstanceType, SteamID, SteamIDParseError, Universe,
};
use thiserror::Error;
use tracing::debug;

type Result<T, E = ConnectionError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConnectionError {
    #[error("Network error: {0:#}")]
    Network(#[from] NetworkError),
    #[error("Login failed: {0:#}")]
    LoginError(#[from] LoginError),
    #[error("Aborted")]
    Aborted,
    #[error("Unsupported confirmation action")]
    UnsupportedConfirmationAction(Vec<ConfirmationMethod>),
}

impl From<ConfirmationError> for ConnectionError {
    fn from(value: ConfirmationError) -> Self {
        match value {
            ConfirmationError::Network(err) => err.into(),
            ConfirmationError::Aborted => ConnectionError::Aborted,
        }
    }
}

impl From<RefreshTokenError> for ConnectionError {
    fn from(value: RefreshTokenError) -> Self {
        ConnectionError::LoginError(value.into())
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LoginError {
    #[error("Access token error: {0:#}")]
    AccessToken(#[from] RefreshTokenError),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("unknown error {0:?}")]
    Unknown(EResult),
    #[error("steam guard required")]
    SteamGuardRequired,
    #[error("steam returned an invalid public key: {0:#}")]
    InvalidPubKey(CryptoError),
    #[error("account not available")]
    UnavailableAccount,
    #[error("rate limited")]
    RateLimited,
    #[error("invalid steam id")]
    InvalidSteamId,
    #[error("steam didn't return a refresh token")]
    NoToken,
}

impl From<EResult> for LoginError {
    fn from(value: EResult) -> Self {
        match value {
            EResult::InvalidPassword => LoginError::InvalidCredentials,
            EResult::AccountDisabled
            | EResult::AccountLockedDown
            | EResult::AccountHasBeenDeleted
            | EResult::AccountNotFound => LoginError::InvalidCredentials,
            EResult::RateLimitExceeded
            | EResult::AccountActivityLimitExceeded
            | EResult::LimitExceeded
            | EResult::AccountLimitExceeded => LoginError::RateLimited,
            EResult::AccountLoginDeniedNeedTwoFactor => LoginError::SteamGuardRequired,
            EResult::InvalidSteamID => LoginError::InvalidSteamId,
            value => LoginError::Unknown(value),
        }
    }
}

impl From<SteamIDParseError> for LoginError {
    fn from(_: SteamIDParseError) -> Self {
        LoginError::InvalidSteamId
    }
}

#[derive(Debug, Clone)]
pub struct JobIdCounter(Arc<AtomicU64>);

impl JobIdCounter {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&self) -> JobId {
        JobId::new(self.0.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for JobIdCounter {
    fn default() -> Self {
        Self(Arc::new(AtomicU64::new(1)))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Session {
    pub session: RawSession,
    pub auth: SessionAuthenticationDetails,
}

/// A session that might not have authenticated yet
#[derive(Debug, Clone)]
pub(crate) struct RawSession {
    pub session_id: i32,
    pub cell_id: u32,
    pub job_id: JobIdCounter,
    pub heartbeat_interval: Duration,
}

/// Details for authenticated sessions
#[derive(Debug, Clone)]
pub(crate) struct SessionAuthenticationDetails {
    pub public_ip: IpAddr,
    pub ip_country_code: String,
    pub steam_id: SteamID,
    pub app_id: Option<u32>,
    pub refresh_token: RefreshToken,
}

impl Default for RawSession {
    fn default() -> Self {
        RawSession {
            session_id: 0,
            cell_id: 0,
            job_id: JobIdCounter::default(),
            heartbeat_interval: Duration::from_secs(15),
        }
    }
}

pub async fn anonymous(connection: &RawConnection, account_type: AccountType) -> Result<Session> {
    let mut ip = CMsgIPAddress::new();
    ip.set_v4(0);

    let logon = CMsgClientLogon {
        protocol_version: Some(65580),
        client_os_type: Some(203),
        anon_user_target_account_name: Some(String::from("anonymous")),
        account_name: Some(String::from("anonymous")),
        supports_rate_limit_response: Some(false),
        obfuscated_private_ip: MessageField::some(ip),
        client_language: Some(String::new()),
        chat_mode: Some(2),
        client_package_version: Some(1771),
        ..CMsgClientLogon::default()
    };

    let steam_id = SteamID::new(
        0,
        Instance::new(InstanceType::All, InstanceFlags::None),
        account_type,
        Universe::Public,
    );
    // Anonymous logons carry no token.
    send_logon(
        connection,
        logon,
        steam_id,
        RefreshToken::anonymous(steam_id),
    )
    .await
}

pub async fn login(
    connection: &mut RawConnection,
    account: Option<&str>,
    steam_id: SteamID,
    refresh_token: &str,
    logon_id: u32,
) -> Result<Session> {
    let token = RefreshToken::new(refresh_token.to_string())?;
    let logon = logon_message(account, refresh_token, logon_id);
    send_logon(connection, logon, steam_id, token).await
}

/// Build the client logon message.
///
/// Private IP doubles as logon ID.
fn logon_message(account: Option<&str>, refresh_token: &str, logon_id: u32) -> CMsgClientLogon {
    let mut ip = CMsgIPAddress::new();
    ip.set_v4(logon_id);

    CMsgClientLogon {
        protocol_version: Some(65580),
        client_os_type: Some(203),
        account_name: account.map(String::from),
        supports_rate_limit_response: Some(false),
        obfuscated_private_ip: MessageField::some(ip),
        client_language: Some(String::new()),
        machine_name: Some(String::new()),
        steamguard_dont_remember_computer: Some(false),
        chat_mode: Some(2),
        access_token: Some(refresh_token.into()),
        client_package_version: Some(1771),
        ..CMsgClientLogon::default()
    }
}

async fn send_logon(
    connection: &RawConnection,
    logon: CMsgClientLogon,
    steam_id: SteamID,
    refresh_token: RefreshToken,
) -> Result<Session> {
    let header = NetMessageHeader {
        source_job_id: JobId::NONE,
        target_job_id: JobId::NONE,
        steam_id: RawSteamId::new(steam_id.steam64()),
        ..NetMessageHeader::default()
    };

    let filter = connection.filter();
    let fut = filter.one_kind(CMsgClientLogonResponse::KIND);
    connection.raw_send(header, logon).await?;

    debug!("waiting for login response");
    let raw_response = fut.await.map_err(|_| NetworkError::EOF)?;
    let (header, response) = raw_response.into_header_and_message::<CMsgClientLogonResponse>()?;
    EResult::from_result(response.eresult()).map_err(LoginError::from)?;

    let assigned_steam_id = if response.has_client_supplied_steamid() {
        let raw = response.client_supplied_steamid();
        SteamID::try_from(raw).unwrap_or(steam_id)
    } else if header.steam_id != RawSteamId::NONE {
        SteamID::from_steam64(header.steam_id.id()).unwrap_or(steam_id)
    } else {
        steam_id
    };

    debug!(steam_id = %u64::from(assigned_steam_id), "session started");
    Ok(Session {
        session: RawSession {
            session_id: header.session_id,
            cell_id: response.cell_id(),
            job_id: JobIdCounter::default(),
            heartbeat_interval: heartbeat_interval(response.heartbeat_seconds()),
        },
        auth: SessionAuthenticationDetails {
            public_ip: response
                .public_ip
                .ip
                .as_ref()
                .and_then(|ip| match &ip {
                    cmsg_ipaddress::Ip::V4(bits) => Some(IpAddr::V4(Ipv4Addr::from(*bits))),
                    cmsg_ipaddress::Ip::V6(bytes) if bytes.len() == 16 => {
                        let mut bits = [0u8; 16];
                        bits.copy_from_slice(&bytes[..]);
                        Some(IpAddr::V6(Ipv6Addr::from(bits)))
                    }
                    _ => None,
                })
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            ip_country_code: response.ip_country_code.clone().unwrap_or_default(),
            steam_id: assigned_steam_id,
            app_id: None,
            refresh_token,
        },
    })
}

/// The heartbeat period Steam asked for. A missing or non-positive value
/// falls back to the default instead of becoming a zero-length busy loop.
fn heartbeat_interval(seconds: i32) -> Duration {
    match u64::try_from(seconds) {
        Ok(secs) if secs > 0 => Duration::from_secs(secs),
        _ => RawSession::default().heartbeat_interval,
    }
}

pub async fn hello<C: ConnectionImpl>(conn: &mut C) -> std::result::Result<(), NetworkError> {
    const PROTOCOL_VERSION: u32 = 65580;
    let req = CMsgClientHello {
        protocol_version: Some(PROTOCOL_VERSION),
        ..CMsgClientHello::default()
    };

    let header = NetMessageHeader {
        session_id: 0,
        source_job_id: JobId::NONE,
        target_job_id: JobId::NONE,
        steam_id: RawSteamId::NONE,
        ..NetMessageHeader::default()
    };

    conn.raw_send(header, req).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
