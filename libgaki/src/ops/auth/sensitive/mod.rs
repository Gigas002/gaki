//! `reauth_sensitive`: a fresh login before sensitive reads.
//!
//! The inventory, the wallet and the market listings take a
//! [`SensitiveGrant`]. With `reauth_sensitive` off, [`sensitive_grant`] hands
//! one out freely. With it on, a grant exists only after a password or QR
//! login in this process ([`super::login`], [`super::login_qr`]), within
//! [`REAUTH_WINDOW`], and each login is good for one action. A stored or
//! restored session never counts: the point is that someone who can only use
//! the saved token (a process running as you, a copied token) can't read
//! these without the password or the phone.

use crate::core::error::{ErrorKind, TypedError};
use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a fresh login stays good for the sensitive action it was for.
pub const REAUTH_WINDOW: Duration = Duration::from_secs(5 * 60);

/// When this process last finished a password or QR login that no sensitive
/// action has used yet.
static FRESH_LOGIN: Mutex<Option<Instant>> = Mutex::new(None);

/// Permission to run one sensitive action. Only [`sensitive_grant`] makes
/// one, and the sensitive ops take it by value.
#[derive(Debug)]
#[must_use]
pub struct SensitiveGrant(());

/// Whether `reauth_sensitive` is on in `config.toml`.
pub fn reauth_sensitive() -> bool {
    crate::core::config::load_runtime_settings_blocking().reauth_sensitive
}

/// Record a finished password or QR login.
pub(crate) fn record_fresh_login() {
    *FRESH_LOGIN.lock().unwrap() = Some(Instant::now());
}

/// Whether a sensitive action would need a login first: `reauth_sensitive`
/// is on and no unused fresh login is at hand.
pub fn reauth_needed() -> bool {
    reauth_sensitive() && !fresh_login_at_hand(*FRESH_LOGIN.lock().unwrap(), Instant::now())
}

fn fresh_login_at_hand(login: Option<Instant>, now: Instant) -> bool {
    login.is_some_and(|at| now.saturating_duration_since(at) <= REAUTH_WINDOW)
}

/// Permission for one sensitive action. With `reauth_sensitive` on this
/// uses up the last fresh login, and fails (`AuthRequired`) without one.
pub fn sensitive_grant() -> Result<SensitiveGrant> {
    grant(reauth_sensitive(), &FRESH_LOGIN, Instant::now())
}

fn grant(required: bool, fresh: &Mutex<Option<Instant>>, now: Instant) -> Result<SensitiveGrant> {
    if !required {
        return Ok(SensitiveGrant(()));
    }
    let login = fresh.lock().unwrap().take();
    if fresh_login_at_hand(login, now) {
        return Ok(SensitiveGrant(()));
    }
    Err(TypedError::new(
        ErrorKind::AuthRequired,
        "reauth_sensitive is on: sign in again (password or QR code) to read the inventory, \
         wallet or market listings",
    )
    .into())
}

#[cfg(test)]
mod tests;
