// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use aes::{
    Aes256, Aes256Dec, Block,
    cipher::{
        BlockCipherDecrypt, BlockModeDecrypt, KeyInit, KeyIvInit,
        block_padding::{self, Pkcs7},
    },
};

pub const IV_LENGTH: usize = 16;

/// Decrypt Steam's AES-256 format: the first block is the IV, encrypted with
/// the key in ECB mode, and the rest is CBC with PKCS#7 padding.
pub fn decrypt_cbc_with_iv_extraction(
    data: &mut [u8],
    key: [u8; 32],
) -> Result<Vec<u8>, block_padding::Error> {
    if data.len() < IV_LENGTH {
        return Err(block_padding::Error);
    }
    let key = key.into();
    let mut iv = Block::try_from(&data[..IV_LENGTH]).map_err(|_| block_padding::Error)?;
    Aes256Dec::new(&key).decrypt_block(&mut iv);

    Ok(cbc::Decryptor::<Aes256>::new(&key, &iv)
        .decrypt_padded::<Pkcs7>(&mut data[IV_LENGTH..])?
        .to_vec())
}

/// Encrypt the way Steam does, for tests: an ECB-encrypted IV block, then CBC.
#[cfg(test)]
pub(crate) fn encrypt_for_test(plain: &[u8], key: [u8; 32], iv: [u8; IV_LENGTH]) -> Vec<u8> {
    use aes::cipher::{BlockCipherEncrypt, BlockModeEncrypt};
    let key = key.into();
    let mut iv_block = Block::from(iv);
    let mut buf = plain.to_vec();
    buf.resize(plain.len() + IV_LENGTH, 0);
    let ciphertext = cbc::Encryptor::<Aes256>::new(&key, &iv_block)
        .encrypt_padded::<Pkcs7>(&mut buf, plain.len())
        .unwrap()
        .to_vec();
    aes::Aes256Enc::new(&key).encrypt_block(&mut iv_block);
    [iv_block.as_slice(), &ciphertext].concat()
}

#[cfg(test)]
#[path = "aes256_tests.rs"]
mod tests;
