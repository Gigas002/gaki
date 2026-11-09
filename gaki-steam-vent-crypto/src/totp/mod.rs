use crate::CryptoError;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use std::time::{SystemTime, UNIX_EPOCH};

/// Steam Guard's code alphabet: digits and consonants that can't be misread.
const ALPHABET: &[u8; 26] = b"23456789BCDFGHJKMNPQRTVWXY";
/// Codes roll over every 30 seconds.
const PERIOD_SECS: u64 = 30;

/// A Steam Guard mobile-authenticator shared secret.
///
/// Produces the 5-character login codes the Steam app shows.
#[derive(Clone)]
pub struct SharedSecret(Vec<u8>);

impl std::fmt::Debug for SharedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedSecret(..)")
    }
}

impl SharedSecret {
    /// Decode a secret as found in `maFile`s: base64, or hex. (The base32
    /// secret of `otpauth://` URLs is a different encoding.)
    pub fn decode(secret: &str) -> Result<Self, CryptoError> {
        let secret = secret.trim();
        let bytes = if secret.len().is_multiple_of(2)
            && !secret.is_empty()
            && secret.bytes().all(|b| b.is_ascii_hexdigit())
        {
            hex::decode(secret).ok()
        } else {
            STANDARD.decode(secret).ok()
        };
        match bytes {
            Some(bytes) if !bytes.is_empty() => Ok(Self(bytes)),
            _ => Err(CryptoError::InvalidSharedSecret),
        }
    }

    /// The code for the current time.
    pub fn auth_code(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        self.auth_code_at(now)
    }

    /// The code valid at `unix_secs`.
    pub fn auth_code_at(&self, unix_secs: u64) -> String {
        let mut mac =
            <Hmac<Sha1> as KeyInit>::new_from_slice(&self.0).expect("HMAC takes any key length");
        mac.update(&(unix_secs / PERIOD_SECS).to_be_bytes());
        let digest = mac.finalize().into_bytes();
        // RFC 4226 dynamic truncation, then base-26 into Steam's alphabet.
        let offset = usize::from(digest[19] & 0x0f);
        let mut code = u32::from_be_bytes([
            digest[offset],
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]) & 0x7fff_ffff;
        (0..5)
            .map(|_| {
                let c = ALPHABET[(code % 26) as usize] as char;
                code /= 26;
                c
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
