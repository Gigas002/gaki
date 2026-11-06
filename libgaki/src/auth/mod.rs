//! Where gaki keeps the account it is signed in as.
//!
//! The secrets (the Steam refresh token and a browser web token) live in a
//! [`store::SecretStore`], by default the OS keyring, one item per secret;
//! they are never written to a file. What is not secret (account and persona
//! name, SteamID, which secrets are stored) is in `account.toml` under the
//! state directory.

pub mod session;
pub mod store;

pub use session::{
    StoredAccount, account_path, delete_session, load_session, save_session, stored_account,
};

#[cfg(test)]
mod tests;
