// Based on steam-vent-crypto, another-steam-totp and steam-machine-id (MIT);
// see README.md and LICENSE-MIT.
//! The cryptography a Steam client login needs:
//!
//! - [`PasswordKey`]: RSA-encrypt the password with the per-account key
//!   `Authentication.GetPasswordRSAPublicKey` returns.
//! - [`SharedSecret`]: Steam Guard mobile-authenticator (TOTP) codes.
//! - [`MachineId`]: the machine-ID message object sent with device details.

mod error;
mod machine_id;
mod password;
mod totp;

pub use error::CryptoError;
pub use machine_id::MachineId;
pub use password::PasswordKey;
pub use totp::SharedSecret;
