//! Steam session acquisition: restoring a stored login, requiring an
//! authenticated client, and the web-session fallback.

use crate::core::config::load_session;
use crate::core::error::{ErrorKind, TypedError, classify};
use crate::steam_client::{Friend, SteamClient};
use crate::web::web_session::WebSession;
use anyhow::{Context, Result};
use std::time::Duration;

/// Why restoring the stored session failed, as recorded by whoever tried.
#[derive(Debug, Clone)]
pub struct RestoreFailure {
    pub kind: ErrorKind,
    pub retry_after: Option<Duration>,
    pub reason: String,
}

/// Build a client and restore the stored session, if there is one.
///
/// A failed restore is not an error here: the client is returned logged out
/// with the failure recorded (`SteamClient::restore_error`), so commands that
/// work offline still run. Only an unreadable session file is an error.
pub async fn restore_client() -> Result<SteamClient> {
    let mut client = SteamClient::new()?;
    let saved = load_session()
        .await
        .context("could not read the stored session")?;
    if saved.refresh_token.is_some() && saved.account_name.is_some() {
        tracing::info!("Restoring Steam session (connecting to Steam) ...");
        match client.restore_session().await {
            Ok(_) => tracing::info!("Restored Steam session from refresh token"),
            Err(e) => {
                tracing::warn!("Stored refresh token failed ({e:#})");
                client.set_restore_error(e);
            }
        }
    }
    Ok(client)
}

/// Return `client` if it is logged in, otherwise a typed error explaining why.
///
/// `prior` is a restore failure recorded elsewhere (e.g. by a long-lived
/// session holder); it explains the problem better than "not logged in", so it
/// wins over the client's own restore error.
pub fn require_authenticated(
    client: SteamClient,
    prior: Option<RestoreFailure>,
) -> Result<SteamClient> {
    if client.is_authenticated() {
        return Ok(client);
    }
    if let Some(f) = prior {
        return Err(restore_failed(f.kind, f.retry_after, &f.reason));
    }
    if let Some(e) = client.restore_error() {
        let c = classify(e);
        return Err(restore_failed(c.kind, c.retry_after, &format!("{e:#}")));
    }
    Err(TypedError::new(ErrorKind::AuthRequired, "not logged in").into())
}

/// Typed restore failure; an unknown kind means the user has to log in again.
fn restore_failed(kind: ErrorKind, retry_after: Option<Duration>, reason: &str) -> anyhow::Error {
    let kind = if kind == ErrorKind::Unknown {
        ErrorKind::AuthRequired
    } else {
        kind
    };
    TypedError::new(
        kind,
        format!("could not restore the stored session: {reason}"),
    )
    .with_retry_after(retry_after)
    .into()
}

/// A web session for the web-only surfaces (inventory, wallet, market).
///
/// Minted off `client` when it is logged in, otherwise built from the browser
/// web token stored by a web-token login, if it hasn't expired.
pub async fn web_session(client: &SteamClient) -> Result<WebSession> {
    if client.is_authenticated() {
        return client.web_session().await;
    }
    let session = load_session().await.unwrap_or_default();
    let Some(token) = session
        .web_token
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    else {
        return Err(TypedError::new(
            ErrorKind::AuthRequired,
            "not logged in, and no stored web token",
        )
        .into());
    };
    let steam_id = session
        .steam_id
        .context("the stored session has a web token but no SteamID")?;
    // 60 s of leeway so a token about to lapse doesn't fail mid-command. Opaque
    // tokens carry no readable expiry; Steam rejects those server-side instead.
    crate::web::web_token::check_stored_expiry(token, crate::web::web_token::now_unix(), 60)?;
    WebSession::new(steam_id, token, None)
}

/// Resolve a user argument (SteamID64, profile URL, `me`, or a friend's name)
/// to a SteamID64.
///
/// Names are matched against `roster` when given (a cached friends list);
/// otherwise the friends list is fetched from Steam.
pub async fn resolve_user_ident(
    client: &SteamClient,
    input: &str,
    roster: Option<Vec<Friend>>,
) -> Result<u64> {
    use crate::steam_client::ident::{Ident, match_friend_name, parse_ident};
    match parse_ident(input)? {
        Ident::SteamId(id) => Ok(id),
        Ident::Me => client.steam_id().ok_or_else(|| {
            TypedError::new(ErrorKind::AuthRequired, "`me` needs a logged-in session").into()
        }),
        Ident::Name(name) => {
            let friends = match roster {
                Some(friends) => friends,
                None => {
                    crate::ops::social::friends(client, crate::ops::social::FRIENDS_WAIT).await?
                }
            };
            Ok(match_friend_name(&name, &friends)?.steam_id)
        }
    }
}

#[cfg(test)]
mod tests;
