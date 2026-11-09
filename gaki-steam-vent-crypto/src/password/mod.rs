use crate::CryptoError;
use rsa::{BoxedUint, Pkcs1v15Encrypt, PublicKey};

/// The RSA key Steam hands out per account to encrypt the login password.
#[derive(Debug, Clone)]
pub struct PasswordKey(PublicKey);

impl PasswordKey {
    /// Build the key from the hex `publickey_mod` / `publickey_exp` of a
    /// `CAuthentication_GetPasswordRSAPublicKey_Response`.
    pub fn from_hex(modulus: &str, exponent: &str) -> Result<Self, CryptoError> {
        let n = BoxedUint::from_be_slice_vartime(&decode_hex(modulus)?);
        let e = BoxedUint::from_be_slice_vartime(&decode_hex(exponent)?);
        PublicKey::new(n, e)
            .map(Self)
            .map_err(|err| CryptoError::InvalidPublicKey(err.to_string()))
    }

    /// PKCS#1 v1.5-encrypt `password`, as `BeginAuthSessionViaCredentials`
    /// expects (base64 it before sending).
    pub fn encrypt(&self, password: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.0
            .encrypt(&mut rand::rng(), Pkcs1v15Encrypt, password)
            .map_err(|err| CryptoError::Encryption(err.to_string()))
    }
}

/// Hex to bytes; an odd digit count gets the leading zero Steam leaves off.
fn decode_hex(digits: &str) -> Result<Vec<u8>, CryptoError> {
    let digits = digits.trim();
    if digits.is_empty() {
        return Err(CryptoError::InvalidPublicKey("empty hex value".into()));
    }
    let padded;
    let digits = if digits.len() % 2 == 1 {
        padded = format!("0{digits}");
        &padded
    } else {
        digits
    };
    hex::decode(digits).map_err(|err| CryptoError::InvalidPublicKey(err.to_string()))
}

#[cfg(test)]
mod tests;
