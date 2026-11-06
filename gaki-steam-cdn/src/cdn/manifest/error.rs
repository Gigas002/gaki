// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use aes::cipher::block_padding;
use std::{io, str};
use thiserror::Error;
use zip::result::ZipError;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("lack of data: {0}")]
    Eof(String),
    #[error("decompress: {0}")]
    Decompress(String),
    #[error("magic mismatch: {0}")]
    MagicMismatch(String),
    #[error("protobuf parsing: {0}")]
    Protobuf(String),
    #[error("fail decrypt names: {0}")]
    DecryptFilename(String),
}

impl From<ZipError> for ManifestError {
    fn from(err: ZipError) -> Self {
        Self::Decompress(err.to_string())
    }
}

impl From<io::Error> for ManifestError {
    fn from(err: io::Error) -> Self {
        Self::Decompress(err.to_string())
    }
}

impl From<protobuf::Error> for ManifestError {
    fn from(err: protobuf::Error) -> Self {
        Self::Protobuf(err.to_string())
    }
}

impl From<base64::DecodeError> for ManifestError {
    fn from(err: base64::DecodeError) -> Self {
        Self::DecryptFilename(err.to_string())
    }
}

impl From<block_padding::Error> for ManifestError {
    fn from(err: block_padding::Error) -> Self {
        Self::DecryptFilename(err.to_string())
    }
}

impl From<str::Utf8Error> for ManifestError {
    fn from(err: str::Utf8Error) -> Self {
        Self::DecryptFilename(err.to_string())
    }
}
