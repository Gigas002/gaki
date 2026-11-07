// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use std::io::Cursor;
use zip::ZipArchive;

use crate::{
    Error,
    crypto::aes256::{self, IV_LENGTH},
    utils::lzma,
};

/// Decrypt a downloaded chunk and unpack it to at most `max_len` bytes (the
/// manifest's size for it).
pub async fn decrypt_and_decompress(
    data: &mut [u8],
    key: [u8; 32],
    max_len: usize,
) -> Result<Vec<u8>, Error> {
    if data.len() <= IV_LENGTH {
        return Err(Error::Eof("data is too small".to_string()));
    }

    let decrypted = aes256::decrypt_cbc_with_iv_extraction(data, key)?;
    if lzma::is_vz(&decrypted) {
        // Legacy LZMA container.
        Ok(lzma::decompress(&decrypted, max_len).await?)
    } else if lzma::is_vsz(&decrypted) {
        // Newer Zstandard container.
        Ok(lzma::decompress_vsz(&decrypted, max_len)?)
    } else {
        // Otherwise the payload is a single-entry zip.
        let cursor = Cursor::new(decrypted);
        let mut archive = ZipArchive::new(cursor)?;
        lzma::read_bounded(archive.by_index(0)?, max_len)
    }
}

#[cfg(test)]
mod tests;
