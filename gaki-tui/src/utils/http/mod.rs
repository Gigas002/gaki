//! Plain HTTP GETs for artwork and ProtonDB summaries, run on the shared
//! worker runtime ([`crate::app::worker`]) with one reused client.
//!
//! Callers are background threads, so blocking on the worker here never
//! stalls the UI.

use crate::app::worker;
use std::sync::OnceLock;
use std::time::Duration;

/// How long one request may take before it counts as failed.
const TIMEOUT: Duration = Duration::from_secs(20);

/// The shared client (gaki user agent, [`TIMEOUT`]); `None` if it couldn't be
/// built, which makes every fetch a miss.
fn client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| libgaki::core::net::http_client(TIMEOUT).ok())
        .as_ref()
}

/// The body of a successful GET of `url`, or `None` on any failure
/// (network error, timeout, or a non-2xx status).
pub fn get(url: &str) -> Option<Vec<u8>> {
    let client = client()?;
    worker::global().block_on(async {
        let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
        response.bytes().await.ok().map(|b| b.to_vec())
    })
}

#[cfg(test)]
mod tests;
