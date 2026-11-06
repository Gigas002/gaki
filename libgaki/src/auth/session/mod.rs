//! The stored session: `account.toml` plus the secrets in the
//! [`super::store::session_store`].

use super::store::{
    LEGACY_SESSION_KEY, SecretStore, refresh_token_key, session_store, web_token_key,
};
use crate::core::models::SessionState;
use anyhow::{Context, Result, anyhow};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const ACCOUNT_FILE: &str = "account.toml";
/// Where gaki kept the whole session, tokens included, before 5.9.
const LEGACY_SESSION_FILE: &str = "session.json";

/// `account.toml`: who is signed in, and which secrets the store holds for
/// them. Nothing in it is secret.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AccountFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    persona_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    steam_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_instance_id: Option<u64>,
    /// A refresh token is in the secret store.
    #[serde(default)]
    refresh_token: bool,
    /// A browser web token is in the secret store.
    #[serde(default)]
    web_token: bool,
}

impl AccountFile {
    fn name(&self) -> Option<&str> {
        [&self.persona_name, &self.account_name]
            .into_iter()
            .flatten()
            .map(|n| n.trim())
            .find(|n| !n.is_empty())
    }
}

/// `account.toml`: under `$XDG_STATE_HOME/gaki`, or in the config dir when one
/// is set for the process (`--config-dir`, tests), so an isolated config dir
/// has its own account.
pub fn account_path() -> Result<PathBuf> {
    use crate::core::config::{config_dir, config_dir_override, state_dir};
    let dir = if config_dir_override().is_some() || cfg!(test) {
        config_dir()?
    } else {
        state_dir()?
    };
    Ok(dir.join(ACCOUNT_FILE))
}

fn legacy_session_path() -> Result<PathBuf> {
    Ok(crate::core::config::config_dir()?.join(LEGACY_SESSION_FILE))
}

fn read_account() -> Result<Option<AccountFile>> {
    crate::core::store::read(&account_path()?)
}

/// Run a blocking secret-store call off the async runtime.
async fn with_store<T: Send + 'static>(
    f: impl FnOnce(&'static dyn SecretStore) -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || f(session_store()))
        .await
        .context("the secret store task failed")?
}

/// The stored session: `account.toml` and its secrets. No session is
/// `SessionState::default()`. An error means one is stored but can't be
/// read: a broken `account.toml`, an unreachable keyring, a token the store
/// lost, or a `session.json` from before the keyring.
pub async fn load_session() -> Result<SessionState> {
    let Some(account) = read_account()? else {
        if legacy_session_path()?.exists() {
            return Err(anyhow!(
                "session.json is from an older gaki, which kept the token in a file; \
                 tokens now live in the OS keyring. Run `gaki-cli login` once"
            ));
        }
        return Ok(SessionState::default());
    };
    let wanted = [account.refresh_token, account.web_token];
    let secrets = match account.steam_id {
        Some(id) if wanted.contains(&true) => {
            with_store(move |store| {
                let get = |want: bool, key: String| -> Result<Option<String>> {
                    if !want {
                        return Ok(None);
                    }
                    store.get(&key)?.map(Some).ok_or_else(|| {
                        anyhow!(
                            "the stored {} is missing from {}; run `gaki-cli login` again",
                            key.split(':').next().unwrap_or("token"),
                            store.describe()
                        )
                    })
                };
                Ok((
                    get(wanted[0], refresh_token_key(id))?,
                    get(wanted[1], web_token_key(id))?,
                ))
            })
            .await?
        }
        _ => (None, None),
    };
    Ok(SessionState {
        account_name: account.account_name,
        persona_name: account.persona_name,
        steam_id: account.steam_id,
        refresh_token: secrets.0.map(SecretString::from),
        client_instance_id: account.client_instance_id,
        web_token: secrets.1.map(SecretString::from),
    })
}

/// Store `session`: its tokens in the secret store, the rest in
/// `account.toml`. Never writes a token to a file; without a working store
/// this fails. Also removes what an older gaki left (`session.json` and its
/// keyring key) and the tokens of an account this one replaces.
pub async fn save_session(session: &SessionState) -> Result<()> {
    let present = |t: &Option<SecretString>| {
        t.as_ref()
            .is_some_and(|t| !t.expose_secret().trim().is_empty())
    };
    let (refresh, web) = (present(&session.refresh_token), present(&session.web_token));
    let steam_id = session.steam_id;
    if (refresh || web) && steam_id.is_none() {
        return Err(anyhow!("a token without a SteamID can't be stored"));
    }
    let previous = read_account().ok().flatten().and_then(|a| a.steam_id);
    let tokens = (
        session.refresh_token.clone().filter(|_| refresh),
        session.web_token.clone().filter(|_| web),
    );
    with_store(move |store| {
        if let Some(id) = steam_id {
            match &tokens.0 {
                Some(token) => store.set(&refresh_token_key(id), token.expose_secret())?,
                None => store.delete(&refresh_token_key(id))?,
            }
            match &tokens.1 {
                Some(token) => store.set(&web_token_key(id), token.expose_secret())?,
                None => store.delete(&web_token_key(id))?,
            }
        }
        if let Some(old) = previous.filter(|old| Some(*old) != steam_id) {
            delete_tokens(store, old);
        }
        if let Err(e) = store.delete(LEGACY_SESSION_KEY) {
            tracing::debug!("could not remove the old session key: {e:#}");
        }
        Ok(())
    })
    .await?;
    crate::core::store::write(
        &account_path()?,
        &AccountFile {
            account_name: session.account_name.clone(),
            persona_name: session.persona_name.clone(),
            steam_id,
            client_instance_id: session.client_instance_id,
            refresh_token: refresh,
            web_token: web,
        },
    )?;
    remove_legacy_file();
    Ok(())
}

fn delete_tokens(store: &dyn SecretStore, steam_id: u64) {
    for key in [refresh_token_key(steam_id), web_token_key(steam_id)] {
        if let Err(e) = store.delete(&key) {
            tracing::warn!("could not remove {key}: {e:#}");
        }
    }
}

fn remove_legacy_file() {
    if let Ok(path) = legacy_session_path()
        && path.exists()
    {
        match std::fs::remove_file(&path) {
            Ok(()) => tracing::info!("removed the old {}", path.display()),
            Err(e) => tracing::warn!("could not remove {}: {e}", path.display()),
        }
    }
}

/// Forget the stored session: its secrets, `account.toml`, and an old
/// `session.json`.
pub async fn delete_session() -> Result<()> {
    if let Some(id) = read_account().ok().flatten().and_then(|a| a.steam_id) {
        with_store(move |store| {
            for key in [refresh_token_key(id), web_token_key(id)] {
                store.delete(&key)?;
            }
            Ok(())
        })
        .await?;
    }
    let path = account_path()?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("could not remove {}", path.display()))?;
    }
    remove_legacy_file();
    Ok(())
}

/// The stored session, as seen from `account.toml` alone (no secret store).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredAccount {
    /// None stored.
    None,
    /// One is stored; its account is unknown.
    Unknown,
    /// One is stored for this account (persona or account name).
    Account(String),
}

/// Who `account.toml` says is signed in, without touching the keyring.
pub fn stored_account() -> StoredAccount {
    classify(read_account().ok().flatten())
}

fn classify(account: Option<AccountFile>) -> StoredAccount {
    let Some(account) = account else {
        return StoredAccount::None;
    };
    match account.name() {
        Some(name) => StoredAccount::Account(name.to_owned()),
        None if account.refresh_token || account.web_token => StoredAccount::Unknown,
        None => StoredAccount::None,
    }
}

#[cfg(test)]
mod tests;
