use super::*;
use libgaki::ops::store::{Listing, PriceQuote, Prices};
use libgaki::web::discovery::ReviewPage;

fn quote(country: &str) -> PriceQuote {
    PriceQuote {
        country: country.to_string(),
        available: true,
        is_free: false,
        price: Some("$9.99".into()),
        price_cents: Some(999),
        original_price: None,
        original_price_cents: None,
        discount_pct: 0,
        discount_end: None,
        discount_end_date: None,
        purchase_options: Vec::new(),
        error: None,
    }
}

/// A view with every section already loaded, so nothing starts a fetch.
fn loaded_view() -> StoreView {
    let mut view = StoreView::new(620, "Portal 2".into(), vec!["US".into()]);
    view.price = Some(Load::Ready(Prices {
        name: "Portal 2".into(),
        regions: vec![quote("US")],
    }));
    view.players = Some(Load::Ready(1234));
    view.news = Some(Load::Ready(Vec::new()));
    view.reviews = Some(Load::Ready(ReviewPage::default()));
    view.similar = Some(Load::Ready(Listing {
        total: 0,
        suggestions: Vec::new(),
        records: Vec::new(),
    }));
    view
}

fn browser_with(view: StoreView) -> Browser {
    let mut browser = Browser::new(vec![]);
    browser.store = Some(view);
    browser
}

#[test]
fn sections_cycle_both_ways_and_wrap() {
    assert_eq!(Section::Price.cycle(1), Section::Players);
    assert_eq!(Section::Price.cycle(-1), Section::Similar);
    assert_eq!(Section::Similar.cycle(1), Section::Price);
    assert_eq!(Section::News.cycle(5), Section::News);
}

#[test]
fn switching_sections_resets_the_scroll() {
    let mut browser = browser_with(loaded_view());
    browser.store.as_mut().unwrap().scroll = 3;
    browser.store_cycle(1);
    let view = browser.store.as_ref().unwrap();
    assert_eq!(view.section, Section::Players);
    assert_eq!(view.scroll, 0);
}

#[test]
fn scrolling_stays_inside_the_body() {
    let mut browser = browser_with(loaded_view());
    // The price body is a header and one row.
    browser.store_scroll(10);
    assert_eq!(browser.store.as_ref().unwrap().scroll, 1);
    browser.store_scroll(-10);
    assert_eq!(browser.store.as_ref().unwrap().scroll, 0);
}

#[test]
fn regions_prompt_is_price_only_and_prefilled() {
    let mut browser = browser_with(loaded_view());
    browser.begin_store_regions();
    assert_eq!(
        browser.store.as_ref().unwrap().regions_input.as_deref(),
        Some("US")
    );
    browser.store_input_pop();
    browser.store_input_pop();
    for c in "us".chars() {
        browser.store_input_push(c);
    }
    // The same regions keep the loaded prices.
    browser.submit_store_input();
    let view = browser.store.as_ref().unwrap();
    assert!(view.regions_input.is_none());
    assert!(view.price.as_ref().unwrap().ready().is_some());

    browser.store_cycle(1);
    browser.begin_store_regions();
    assert!(!browser.store_input_open());
}

#[test]
fn cancelling_the_prompt_keeps_the_regions() {
    let mut browser = browser_with(loaded_view());
    browser.begin_store_regions();
    browser.store_input_push('x');
    browser.cancel_store_input();
    let view = browser.store.as_ref().unwrap();
    assert!(view.regions_input.is_none());
    assert_eq!(view.regions, ["US"]);
}

#[test]
fn split_regions_accepts_commas_and_spaces() {
    assert_eq!(split_regions("us, de jp"), ["US", "DE", "JP"]);
    assert_eq!(split_regions(" , "), Vec::<String>::new());
}

#[test]
fn polling_adopts_finished_loads() {
    let mut view = loaded_view();
    view.players = Some(Load::Loading(std::sync::Arc::new(std::sync::Mutex::new(
        Some(Ok(42)),
    ))));
    let mut browser = browser_with(view);
    browser.poll_store();
    let players = browser.store.as_ref().unwrap().players.as_ref().unwrap();
    assert_eq!(players.ready(), Some(&42));
}

#[test]
fn close_drops_the_view() {
    let mut browser = browser_with(loaded_view());
    assert!(browser.overlay_open());
    browser.close_store();
    assert!(browser.store.is_none());
    assert!(!browser.overlay_open());
}
