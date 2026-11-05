//! Logging in and out, and inspecting the stored session.

use crate::auth::{load_session, save_session};
use crate::core::error::{ErrorKind, TypedError};
use crate::core::models::{QrEvent, SessionState, SteamGuardReq};
use crate::steam_client::SteamClient;
use anyhow::{Context, Result};
use std::time::Duration;

/// Default wait for a QR login to be approved.
pub const QR_DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Default bound on one password-login attempt, which can block while Steam
/// waits for a Guard code or a Mobile-app approval.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);

/// Which Steam Guard code Steam asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardCode {
    /// A code mailed to the account's address.
    Email,
    /// A code from the Steam Mobile authenticator.
    Device,
}

impl GuardCode {
    pub fn as_str(self) -> &'static str {
        match self {
            GuardCode::Email => "email",
            GuardCode::Device => "device",
        }
    }
}

/// Where a password-login attempt ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStep {
    /// Logged in; the session is saved and secured.
    LoggedIn,
    /// Steam wants a Guard code. Ask for it and call [`login`] again with it.
    CodeRequired(GuardCode),
    /// Steam wants the login approved in the Mobile app; try again afterwards.
    ApprovalRequired,
}

/// Credentials and options for one password-login attempt.
#[derive(Debug, Clone)]
pub struct PasswordLogin {
    pub username: String,
    /// Redacted in `Debug`, zeroed on drop.
    pub password: secrecy::SecretString,
    /// A Steam Guard code, when the caller already has one.
    pub guard_code: Option<String>,
    /// Let the Steam client read a Guard code from stdin itself.
    pub read_code_from_stdin: bool,
    /// Bound on the attempt; `None` waits as long as Steam does.
    pub timeout: Option<Duration>,
}

/// One password-login attempt.
///
/// When Steam needs more from the user, this returns the next [`LoginStep`]
/// instead of an error; a code is only asked for when none was given.
pub async fn login(client: &mut SteamClient, request: &PasswordLogin) -> Result<LoginStep> {
    let attempt = client.login(
        request.username.clone(),
        secrecy::ExposeSecret::expose_secret(&request.password).to_owned(),
        request.guard_code.clone(),
        request.read_code_from_stdin,
    );
    let result = match request.timeout {
        Some(limit) => tokio::time::timeout(limit, attempt).await.map_err(|_| {
            anyhow::Error::from(TypedError::new(
                ErrorKind::Timeout,
                format!(
                    "login timed out after {}s waiting for a Steam Guard code or Steam Mobile \
                     app approval",
                    limit.as_secs()
                ),
            ))
        })?,
        None => attempt.await,
    };
    match result {
        Ok(_) => {
            secure_session(client).await;
            Ok(LoginStep::LoggedIn)
        }
        Err(err) => {
            let pending = client.pending_confirmations();
            let code = request
                .guard_code
                .is_none()
                .then(|| {
                    pending.iter().find_map(|p| match p.requirement {
                        SteamGuardReq::EmailCode { .. } => Some(GuardCode::Email),
                        SteamGuardReq::DeviceCode => Some(GuardCode::Device),
                        SteamGuardReq::DeviceConfirmation => None,
                    })
                })
                .flatten();
            if let Some(code) = code {
                Ok(LoginStep::CodeRequired(code))
            } else if pending
                .iter()
                .any(|p| matches!(p.requirement, SteamGuardReq::DeviceConfirmation))
            {
                Ok(LoginStep::ApprovalRequired)
            } else {
                Err(err).context("login failed")
            }
        }
    }
}

/// Log in by QR code. `on_event` receives each challenge URL (re-sent when
/// Steam rotates it) and the "scanned" notice. Secures the session on success.
pub async fn login_qr(
    client: &mut SteamClient,
    timeout: Duration,
    on_event: impl FnMut(QrEvent),
) -> Result<SessionState> {
    let session = client
        .login_qr(timeout, on_event)
        .await
        .context("QR login failed")?;
    secure_session(client).await;
    Ok(session)
}

/// After a successful login: store the persona name. Best-effort, since the
/// login itself already succeeded.
pub async fn secure_session(client: &SteamClient) {
    let persona = client.own_persona_name().await;
    let result = async {
        let mut session = load_session().await?;
        if persona.is_some() {
            session.persona_name = persona;
        }
        save_session(&session).await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!("could not store the persona name: {e:#}");
    }
}

/// How long [`logout`] waits for Steam to revoke the token.
pub const REVOKE_TIMEOUT: Duration = Duration::from_secs(20);

/// What [`logout`] did about the refresh token at Steam.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "revocation", content = "reason", rename_all = "snake_case")]
pub enum Revocation {
    /// Steam revoked it: copies of it no longer work.
    Revoked,
    /// No refresh token was stored (logged out, or a web token only).
    NoToken,
    /// Steam couldn't be asked; the token may work until it expires.
    Failed(String),
}

/// Revoke the stored refresh token at Steam, then drop the connection and
/// delete the stored session. The local copy is deleted even when the
/// revocation fails; the returned [`Revocation`] says which happened. Only
/// failing to delete the local copy is an error. A browser web token can't
/// be revoked; it expires on its own.
pub async fn logout(client: &mut SteamClient) -> Result<Revocation> {
    let revocation = revoke(client).await;
    if let Revocation::Failed(reason) = &revocation {
        tracing::warn!("could not revoke the refresh token at Steam: {reason}");
    }
    client.logout().await?;
    Ok(revocation)
}

async fn revoke(client: &mut SteamClient) -> Revocation {
    use secrecy::ExposeSecret;
    let session = match load_session().await {
        Ok(session) => session,
        Err(e) => return Revocation::Failed(format!("{e:#}")),
    };
    let Some(token) = session
        .refresh_token
        .filter(|t| !t.expose_secret().trim().is_empty())
    else {
        return Revocation::NoToken;
    };
    let attempt = async {
        if !client.is_authenticated() {
            client.restore_session().await?;
        }
        client.revoke_refresh_token(token.expose_secret()).await
    };
    match tokio::time::timeout(REVOKE_TIMEOUT, attempt).await {
        Ok(Ok(())) => Revocation::Revoked,
        Ok(Err(e)) => Revocation::Failed(format!("{e:#}")),
        Err(_) => Revocation::Failed(format!(
            "Steam did not answer within {}s",
            REVOKE_TIMEOUT.as_secs()
        )),
    }
}

pub use crate::auth::StoredAccount as StoredSession;

/// Who the stored session is for, read without the keyring.
pub async fn stored_session() -> StoredSession {
    crate::auth::stored_account()
}

/// Who is signed in, and how.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SessionStatus {
    /// A live, logged-in client session exists.
    pub authenticated: bool,
    pub account: Option<String>,
    pub steam_id: Option<u64>,
    /// A browser web token is stored, so the web-only commands work even
    /// when `authenticated` is false.
    pub web_token: bool,
}

/// Session status for `client` (or no client at all). Identity falls back to
/// the stored session, so a web-token-only sign-in still reports who it is.
pub async fn session_status(client: Option<&SteamClient>) -> SessionStatus {
    let stored = load_session().await.ok();
    SessionStatus {
        authenticated: client.is_some_and(|c| c.is_authenticated()),
        account: stored.as_ref().and_then(|s| s.account_name.clone()),
        steam_id: client
            .and_then(|c| c.steam_id())
            .or(stored.as_ref().and_then(|s| s.steam_id)),
        web_token: stored.as_ref().is_some_and(|s| {
            s.web_token
                .as_ref()
                .is_some_and(|t| !secrecy::ExposeSecret::expose_secret(t).is_empty())
        }),
    }
}

/// The result of a browser (OpenID) identity check.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct OpenIdIdentity {
    pub steam_id: u64,
    /// Whether it is the stored session's account; `None` with no stored session.
    pub matches_stored_session: Option<bool>,
}

/// Verify the user's identity on Steam's own sign-in page (OpenID 2.0).
/// `on_url` receives the sign-in URL to open. Proves identity only: Steam
/// issues no session over OpenID, and the stored session is left alone.
pub async fn verify_openid(on_url: impl FnMut(&str)) -> Result<OpenIdIdentity> {
    let steam_id = crate::web::openid::verify_identity_via_browser(on_url)
        .await
        .context("browser (OpenID) sign-in failed")?;
    let matches_stored_session = load_session()
        .await
        .ok()
        .and_then(|s| s.steam_id)
        .map(|stored| stored == steam_id);
    Ok(OpenIdIdentity {
        steam_id,
        matches_stored_session,
    })
}

/// A web token that was stored.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SavedWebToken {
    pub steam_id: u64,
    pub account: Option<String>,
    /// Unix expiry; `None` for opaque tokens, whose expiry only Steam knows.
    pub expires_at: Option<u64>,
    /// Whether a full client session is stored too.
    pub full_session: bool,
}

/// Parse and store a browser web token (the JSON from the clientjstoken page,
/// or a bare token). Rejects expired tokens and tokens for a different account
/// than the stored session.
pub async fn store_web_token(raw: &str) -> Result<SavedWebToken> {
    use crate::web::web_token::{now_unix, parse_web_token};
    // The stored SteamID doubles as the identity of a bare opaque token.
    let mut session = load_session().await.unwrap_or_default();
    let info = parse_web_token(raw, session.steam_id)?;
    if info.expires_at.is_some_and(|exp| exp <= now_unix()) {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            "this web token already expired; reload the clientjstoken page in a signed-in \
             browser and use the fresh JSON",
        )
        .into());
    }
    if let Some(stored) = session.steam_id
        && stored != info.steam_id
    {
        return Err(TypedError::new(
            ErrorKind::InvalidInput,
            format!(
                "this web token belongs to SteamID {} but the stored session is for {stored}; \
                 log out first to switch accounts",
                info.steam_id
            ),
        )
        .into());
    }
    session.web_token = Some(info.token.clone());
    session.steam_id.get_or_insert(info.steam_id);
    if session.account_name.is_none() {
        session.account_name = info.account_name.clone();
    }
    let full_session = session.refresh_token.is_some();
    save_session(&session).await?;
    Ok(SavedWebToken {
        steam_id: info.steam_id,
        account: info.account_name,
        expires_at: info.expires_at,
        full_session,
    })
}

#[cfg(test)]
mod tests;
