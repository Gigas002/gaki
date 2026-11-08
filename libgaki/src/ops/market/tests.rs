use std::cell::Cell;
use std::time::{Duration, Instant};

use super::pace::*;

#[test]
fn spacing_waits_out_the_interval() {
    let t = Instant::now();
    let gap = Duration::from_secs(3);
    assert_eq!(wait_needed(None, t, gap), Duration::ZERO);
    assert_eq!(
        wait_needed(Some(t), t + Duration::from_secs(1), gap),
        Duration::from_secs(2)
    );
    assert_eq!(
        wait_needed(Some(t), t + Duration::from_secs(5), gap),
        Duration::ZERO
    );
}

#[test]
fn cache_answers_until_the_ttl() {
    let t = Instant::now();
    let cache = Cache::new(Duration::from_secs(60));
    cache.put(1, "a", t);
    assert_eq!(cache.get(&1, t + Duration::from_secs(59)), Some("a"));
    assert_eq!(cache.get(&1, t + Duration::from_secs(60)), None);
    assert_eq!(cache.get(&2, t), None);
    // A later put drops what has expired.
    cache.put(2, "b", t + Duration::from_secs(61));
    cache.put(1, "c", t + Duration::from_secs(61));
    assert_eq!(cache.get(&1, t + Duration::from_secs(62)), Some("c"));
}

#[tokio::test]
async fn cached_fetches_once_and_never_keeps_failures() {
    let cache = Cache::new(Duration::from_secs(60));
    let calls = Cell::new(0);
    let fetch = |ok: bool| {
        calls.set(calls.get() + 1);
        async move {
            if ok {
                Ok(7)
            } else {
                Err(anyhow::anyhow!("rate limited"))
            }
        }
    };
    assert!(cached(&cache, "k", fetch(false)).await.is_err());
    assert_eq!(cached(&cache, "k", fetch(true)).await.unwrap(), 7);
    assert_eq!(cache.get(&"k", Instant::now()), Some(7));
    assert_eq!(calls.get(), 2);
}
