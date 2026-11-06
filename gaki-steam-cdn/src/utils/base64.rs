// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use base64::{
    DecodeError, Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};

pub fn base64_decode<T: AsRef<[u8]>>(input: T) -> Result<Vec<u8>, DecodeError> {
    let alphabet = alphabet::STANDARD;
    let acceptable = alphabet.as_str().as_bytes();
    let mut filtered = input.as_ref().to_vec();
    filtered.retain(|i| acceptable.contains(i));

    let mut decoded = Vec::<u8>::new();
    GeneralPurpose::new(
        &alphabet,
        GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true)
            .with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
    .decode_vec(filtered, &mut decoded)?;
    Ok(decoded)
}

#[cfg(test)]
#[path = "base64_tests.rs"]
mod tests;
