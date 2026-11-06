//! Secret storage: the OS keyring, or memory.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The Secret Service "service" every gaki item is stored under.
pub const SERVICE: &str = "gaki";

/// Holds secrets by key. Calls may block (D-Bus), so async callers use
/// `spawn_blocking`.
pub trait SecretStore: Send + Sync {
    /// The secret for `key`; `None` when there is none.
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, secret: &str) -> Result<()>;
    /// Remove `key`; no error when it was not there.
    fn delete(&self, key: &str) -> Result<()>;
    /// For messages: where the secrets are.
    fn describe(&self) -> &'static str;
}

/// The OS keyring (Secret Service): one item per secret, service `gaki`,
/// user `<key>`.
pub struct Keyring;

impl Keyring {
    fn entry(key: &str) -> Result<keyring_core::Entry> {
        install_store()?;
        keyring_core::Entry::new(SERVICE, key).map_err(|e| unavailable(&e))
    }
}

/// Make the Secret Service the keyring's store, once per process. A failure
/// (no D-Bus session, no Secret Service) is remembered and reported on every
/// use, so `secret_store = "memory"` keeps working without one.
fn install_store() -> Result<()> {
    static INSTALLED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    INSTALLED
        .get_or_init(|| {
            dbus_secret_service_keyring_store::Store::new()
                .map(|store| keyring_core::set_default_store(store))
                .map_err(|e| unavailable(&e).to_string())
        })
        .clone()
        .map_err(|e| anyhow::anyhow!(e))
}

fn unavailable(e: &keyring_core::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "the OS keyring is unavailable ({e}); gaki keeps tokens only there, never in a \
         file. Start a Secret Service keyring (e.g. gnome-keyring or KeePassXC), or run \
         `gaki-cli config set secret_store memory` to keep them in the running daemon only"
    )
}

impl SecretStore for Keyring {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(unavailable(&e)),
        }
    }

    fn set(&self, key: &str, secret: &str) -> Result<()> {
        Self::entry(key)?
            .set_password(secret)
            .map_err(|e| unavailable(&e))
            .with_context(|| format!("could not store {key}"))
    }

    fn delete(&self, key: &str) -> Result<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(unavailable(&e)),
        }
    }

    fn describe(&self) -> &'static str {
        "the OS keyring"
    }
}

/// Secrets kept in this process only: gone when it exits.
#[derive(Default)]
pub struct Memory(Mutex<HashMap<String, String>>);

impl SecretStore for Memory {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.lock().get(key).cloned())
    }

    fn set(&self, key: &str, secret: &str) -> Result<()> {
        self.lock().insert(key.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.lock().remove(key);
        Ok(())
    }

    fn describe(&self) -> &'static str {
        "memory (`secret_store = \"memory\"`)"
    }
}

impl Memory {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }
}

static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();

/// Use `store` for the rest of the process. Call before anything reads a
/// secret; a later call is ignored and returns `false`.
pub fn set_session_store(store: Box<dyn SecretStore>) -> bool {
    STORE.set(store).is_ok()
}

/// The process's secret store: the one set with [`set_session_store`], else
/// [`Memory`] when `GAKI_SECRET_STORE=memory`, `secret_store = "memory"` in
/// `config.toml` (or in unit tests), else the [`Keyring`].
pub fn session_store() -> &'static dyn SecretStore {
    STORE
        .get_or_init(|| {
            let memory = cfg!(test)
                || std::env::var("GAKI_SECRET_STORE")
                    .is_ok_and(|v| v.eq_ignore_ascii_case("memory"))
                || crate::core::config::read_config_toml_lenient().secret_store
                    == crate::config::SecretStoreKind::Memory;
            if memory {
                Box::new(Memory::default())
            } else {
                Box::new(Keyring)
            }
        })
        .as_ref()
}

/// The key of the Steam refresh token for `steam_id`.
pub fn refresh_token_key(steam_id: u64) -> String {
    format!("refresh-token:{steam_id}")
}

/// The key of the browser web token for `steam_id`.
pub fn web_token_key(steam_id: u64) -> String {
    format!("web-token:{steam_id}")
}

/// The keyring item older gaki versions kept the `session.json` key in.
pub const LEGACY_SESSION_KEY: &str = "session-key";

#[cfg(test)]
mod tests;
