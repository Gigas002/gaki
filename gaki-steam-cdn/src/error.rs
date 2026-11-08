// Based on steam-cdn (Apache-2.0), modified by Aurelia and gaki; see README.md.
use aes::cipher::block_padding;
use reqwest::StatusCode;
use steam_vent::NetworkError;
use tokio::{sync::AcquireError, task::JoinError};
use zip::result::ZipError;

use thiserror::Error;

use crate::cdn::manifest::error::ManifestError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("lack of data: {0}")]
    Eof(String),
    #[error("decompress: {0}")]
    Decompress(String),
    #[error("unexpected: {0}")]
    Unexpected(String),
    #[error("web request - {0}")]
    Request(String),
    #[error("http status - {0}")]
    HttpStatus(StatusCode),
    #[error("{0}")]
    Network(String),
    #[error("malformed vdf - {0}")]
    InvalidVDF(String),
    #[error("manifest {0}")]
    Manifest(#[from] ManifestError),
    #[error("unexpected none")]
    NoneOption,
    /// A manifest entry would land outside the install directory.
    #[error("unsafe depot entry: {0}")]
    UnsafePath(String),
}

impl From<JoinError> for Error {
    fn from(err: JoinError) -> Self {
        Self::Unexpected(err.to_string())
    }
}

impl From<ZipError> for Error {
    fn from(err: ZipError) -> Self {
        Self::Decompress(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        // Keep causes (DNS, OS) for callers.
        let mut text = err.to_string();
        let mut source = std::error::Error::source(&err);
        while let Some(cause) = source {
            text.push_str(": ");
            text.push_str(&cause.to_string());
            source = cause.source();
        }
        Self::Request(text)
    }
}

impl From<NetworkError> for Error {
    fn from(err: NetworkError) -> Self {
        Self::Network(err.to_string())
    }
}

impl From<keyvalues_parser::error::Error> for Error {
    fn from(err: keyvalues_parser::error::Error) -> Self {
        Self::InvalidVDF(err.to_string())
    }
}

impl From<block_padding::Error> for Error {
    fn from(err: block_padding::Error) -> Self {
        Self::Unexpected(err.to_string())
    }
}

impl From<AcquireError> for Error {
    fn from(err: AcquireError) -> Self {
        Self::Unexpected(err.to_string())
    }
}
