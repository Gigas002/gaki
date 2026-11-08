//! Keeping market traffic gentle (STEAM-RULES.md §3, item B).
//!
//! The market, inventory and wallet pages are undocumented web endpoints
//! that Steam rate-limits hard, so each answer is kept for a while and the
//! requests one process makes are spaced out. The cache lives in memory
//! only, never on disk: inventory, wallet and listings are account data.
//! Commands run through the session daemon share its cache.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;

/// The shortest gap between two market requests from one process.
pub const MIN_INTERVAL: Duration = Duration::from_secs(3);

/// How long a price or a search result is reused.
pub const PUBLIC_TTL: Duration = Duration::from_secs(5 * 60);

/// How long the account's inventory, wallet and listings are reused.
pub const ACCOUNT_TTL: Duration = Duration::from_secs(60);

static LAST_REQUEST: tokio::sync::Mutex<Option<Instant>> = tokio::sync::Mutex::const_new(None);

/// How long to wait before the next request, if the last one went out at
/// `last`.
pub(crate) fn wait_needed(last: Option<Instant>, now: Instant, interval: Duration) -> Duration {
    last.map_or(Duration::ZERO, |t| {
        (t + interval).saturating_duration_since(now)
    })
}

/// Wait for this process's turn to send a market request. Callers queue in
/// order, each at least [`MIN_INTERVAL`] after the one before.
async fn turn() {
    let mut last = LAST_REQUEST.lock().await;
    let wait = wait_needed(*last, Instant::now(), MIN_INTERVAL);
    if !wait.is_zero() {
        tracing::debug!(
            wait_ms = wait.as_millis() as u64,
            "spacing out market requests"
        );
        tokio::time::sleep(wait).await;
    }
    *last = Some(Instant::now());
}

/// Answers kept for `ttl`, by key.
pub(crate) struct Cache<K, V> {
    ttl: Duration,
    entries: Mutex<HashMap<K, (Instant, V)>>,
}

impl<K: Eq + Hash, V: Clone> Cache<K, V> {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The answer stored for `key`, if it's younger than the TTL at `now`.
    pub(crate) fn get(&self, key: &K, now: Instant) -> Option<V> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let (at, value) = entries.get(key)?;
        (now.saturating_duration_since(*at) < self.ttl).then(|| value.clone())
    }

    /// Store `value` for `key`, fetched at `now`, and drop expired entries.
    pub(crate) fn put(&self, key: K, value: V, now: Instant) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries.retain(|_, (at, _)| now.saturating_duration_since(*at) < self.ttl);
        entries.insert(key, (now, value));
    }
}

/// The cached answer for `key`, or `fetch`'s, sent in turn. Failures aren't
/// cached, so a retry asks Steam again.
pub(crate) async fn cached<K, V, F>(cache: &Cache<K, V>, key: K, fetch: F) -> Result<V>
where
    K: Eq + Hash,
    V: Clone,
    F: Future<Output = Result<V>>,
{
    if let Some(value) = cache.get(&key, Instant::now()) {
        tracing::debug!("market answer from cache");
        return Ok(value);
    }
    turn().await;
    let value = fetch.await?;
    cache.put(key, value.clone(), Instant::now());
    Ok(value)
}
