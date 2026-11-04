//! Steam store helpers.

use crate::core::error::{ErrorKind, TypedError};
use anyhow::Result;

/// Price region used when none is given.
pub const DEFAULT_COUNTRY: &str = "US";

/// Resolve the store country: `requested` (normalized) or [`DEFAULT_COUNTRY`].
pub fn resolve_country(requested: Option<&str>) -> Result<String> {
    let Some(raw) = requested else {
        return Ok(DEFAULT_COUNTRY.to_string());
    };
    crate::core::utils::normalize_country(raw).ok_or_else(|| {
        TypedError::new(
            ErrorKind::InvalidInput,
            format!("invalid country code `{raw}`; use a two-letter ISO code like US or DE"),
        )
        .into()
    })
}

#[cfg(test)]
mod tests;
