use thiserror::Error;

/// Why a key, secret or encryption was rejected.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// The RSA modulus or exponent isn't hex, or isn't a usable public key.
    #[error("invalid RSA public key: {0}")]
    InvalidPublicKey(String),
    /// RSA encryption failed (e.g. the input is too long for the key).
    #[error("RSA encryption failed: {0}")]
    Encryption(String),
    /// The shared secret is neither hex nor base64, or is empty.
    #[error("invalid Steam Guard shared secret")]
    InvalidSharedSecret,
}
