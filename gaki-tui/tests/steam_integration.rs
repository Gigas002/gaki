//! End-to-end checks against real Steam, run in-process through `libgaki`.
//! Ignored by default because they need network access and, for most, a
//! logged-in session in the default config dir (`gaki-cli login`). Run
//! explicitly with:
//!
//! ```text
//! cargo test -p gaki-tui --test steam_integration -- --ignored
//! ```

use gaki_tui::app::game;
use gaki_tui::app::steam;

#[test]
#[ignore]
fn health_reports_session() {
    let health = steam::health().expect("health check should parse");
    assert!(health.logged_in, "expected an authenticated session");
    assert!(health.account.is_some(), "expected an account name");
}

#[test]
#[ignore]
fn library_loads_and_maps_to_games() {
    // A single fetch, mapped 1:1 (a second library fetch can legitimately
    // return a different count as family-shared/online data resolves).
    let raw = steam::fetch_library().expect("list --json should parse");
    assert!(!raw.is_empty(), "expected a non-empty library");

    let games: Vec<_> = raw.iter().cloned().map(game::Game::from_library).collect();
    assert_eq!(games.len(), raw.len());

    // The mapped Game should preserve identity and install state.
    let first = &raw[0];
    let mapped = &games[0];
    assert_eq!(mapped.id, first.app_id as i32);
    assert_eq!(mapped.installed, first.is_installed);
    // store_url is carried over as the homepage.
    if let Some(url) = &first.store_url {
        assert_eq!(&mapped.homepage, url);
    }

    // load_library() does its own fetch+map; just confirm it succeeds and is
    // non-empty (its count can differ from `raw` between invocations).
    let loaded = game::load_library().expect("library should map into Games");
    assert!(!loaded.is_empty());
}

#[test]
#[ignore]
fn https_proton_lookup_does_not_abort() {
    // This HTTPS request must complete (returning Some/None) without crashing
    // the process.
    let _ = gaki_tui::app::game::protondb::ProtonData::get(1888160);
}

#[test]
#[ignore]
fn classic_login_rejects_bad_credentials() {
    use gaki_tui::app::steam::LoginPhase;
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    let phase = Arc::new(Mutex::new(LoginPhase::Idle));
    let (_tx, rx) = channel();
    let result = steam::login_classic(
        "not_a_real_user_xyz",
        "definitely-wrong-password",
        &phase,
        rx,
    );

    assert!(result.is_err(), "bad credentials should not log in");
    let final_phase = phase.lock().unwrap().clone();
    match final_phase {
        LoginPhase::Failed(msg) => assert!(!msg.is_empty(), "expected a failure message"),
        other => panic!("expected Failed phase, got {:?}", other),
    }
}

#[test]
#[ignore]
fn friends_search_resolves_a_known_account() {
    // `friends search` is read-only and needs no login: it resolves a SteamID64
    // / profile URL / vanity to a concrete account. Gabe Newell's well-known
    // SteamID64 is a stable fixture.
    let found = steam::friends_search("76561197960287930").expect("friends search --json");
    assert_eq!(found.steam_id, 76561197960287930);
    assert!(!found.display_name().is_empty(), "expected a display name");
}

#[test]
#[ignore]
fn info_parses_developers_and_publishers() {
    let raw = steam::fetch_library().expect("list");
    let app_id = raw[0].app_id as i32;
    let info = steam::fetch_info(app_id).expect("info --json should parse");
    assert_eq!(info.app_id, app_id as u32);
    assert!(!info.name.is_empty());
}

#[test]
#[ignore]
fn market_search_then_price_round_trips() {
    use std::sync::{Arc, Mutex};
    use steam::{MarketPriceState, MarketSearchState};

    // Both calls are read-only and need no login. Search for a well-known TF2
    // item, then price the first result by its (app_id, market hash name).
    let search = Arc::new(Mutex::new((0, MarketSearchState::Idle)));
    steam::market_search_async("Mann Co".to_string(), 7, search.clone());
    let (generation, state) = search.lock().unwrap().clone();
    assert_eq!(generation, 7, "the result carries the search generation");
    let results = match state {
        MarketSearchState::Ready(results) => results,
        other => panic!("market search failed: {other:?}"),
    };
    assert!(!results.is_empty(), "expected at least one market result");

    let first = &results[0];
    assert!(!first.market_hash_name.is_empty(), "result has a hash name");

    let price = Arc::new(Mutex::new(MarketPriceState::Idle));
    steam::market_price_async(first.app_id, first.market_hash_name.clone(), price.clone());
    let price = match price.lock().unwrap().clone() {
        MarketPriceState::Ready(price) => price,
        other => panic!("market price failed: {other:?}"),
    };
    assert_eq!(price.market_hash_name, first.market_hash_name);
    assert!(
        price.summary().is_some(),
        "expected some price/volume data for a popular item"
    );
}
