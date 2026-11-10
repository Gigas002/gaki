use super::*;
use libgaki::ops::store::{Listing, Wishlist};
use libgaki::steam_client::{StoreAppInfo, StoreEvent, WishlistEntry};
use std::sync::{Arc, Mutex};

fn record(app_id: u32, name: &str) -> StoreAppInfo {
    StoreAppInfo {
        app_id,
        name: name.to_string(),
        price: Some("$9.99".into()),
        ..Default::default()
    }
}

fn listing(records: Vec<StoreAppInfo>) -> Listing {
    Listing {
        total: records.len() as i32,
        suggestions: Vec::new(),
        records,
    }
}

fn wishlist(ids: &[u32]) -> Wishlist {
    Wishlist {
        steam_id: 1,
        total: ids.len(),
        items: ids
            .iter()
            .map(|&id| {
                (
                    WishlistEntry {
                        app_id: id,
                        priority: 0,
                        date_added: 0,
                    },
                    (id != 999).then(|| record(id, &format!("Game {id}"))),
                )
            })
            .collect(),
    }
}

/// A view with every section loaded, so switching starts no fetch.
fn loaded() -> ShopView {
    ShopView {
        query: "portal".into(),
        search_input: None,
        search: Some(Load::Ready(listing(vec![
            record(620, "Portal 2"),
            record(400, "Portal"),
        ]))),
        deals: Some(Load::Ready(listing(vec![record(10, "Deal")]))),
        events: Some(Load::Ready(vec![StoreEvent {
            title: "Autumn Sale".into(),
            ..Default::default()
        }])),
        wishlist: Some(Load::Ready(wishlist(&[400, 999]))),
        ..ShopView::default()
    }
}

fn browser_with(view: ShopView) -> Browser {
    let mut browser = Browser::new(vec![]);
    browser.shop = Some(view);
    browser
}

fn view(browser: &Browser) -> &ShopView {
    browser.shop.as_ref().unwrap()
}

#[test]
fn opens_on_search_ready_to_type() {
    let mut browser = Browser::new(vec![]);
    browser.open_shop();
    assert!(browser.shop_input_open());
    assert_eq!(view(&browser).section, ShopSection::Search);
    assert!(browser.overlay_open());
    // An empty search only closes the prompt.
    browser.submit_shop_input();
    assert!(!browser.shop_input_open());
    assert!(view(&browser).search.is_none());
}

#[test]
fn sections_cycle_and_reset_the_highlight() {
    assert_eq!(ShopSection::Search.cycle(-1), ShopSection::Wishlist);
    assert_eq!(ShopSection::Wishlist.cycle(1), ShopSection::Search);
    let mut browser = browser_with(loaded());
    browser.shop_move(1);
    assert_eq!(view(&browser).selected, 1);
    browser.shop_cycle(1);
    assert_eq!(view(&browser).section, ShopSection::Deals);
    assert_eq!(view(&browser).selected, 0);
}

#[test]
fn rows_per_section() {
    let mut browser = browser_with(loaded());
    assert_eq!(view(&browser).row_count(), 2);
    browser.shop_move(5);
    assert_eq!(view(&browser).selected_game().unwrap().app_id, 400);
    browser.shop_cycle(2);
    // Events have rows but no games.
    assert_eq!(view(&browser).row_count(), 1);
    assert!(view(&browser).selected_game().is_none());
    browser.shop_cycle(1);
    // A wishlist item without a store record still gets a row.
    let rows = view(&browser).game_rows();
    assert_eq!(rows[1].name, "App 999");
}

#[test]
fn deals_cycle_through_the_three_lists() {
    use libgaki::steam_client::DealsScope;
    assert_eq!(
        next_scope(DealsScope::DiscountedTopSellers),
        DealsScope::TopSellers
    );
    assert_eq!(next_scope(DealsScope::TopSellers), DealsScope::Specials);
    assert_eq!(
        next_scope(DealsScope::Specials),
        DealsScope::DiscountedTopSellers
    );
    // `s` does nothing outside Deals.
    let mut browser = browser_with(loaded());
    browser.shop_next_deals();
    assert_eq!(view(&browser).deals_scope, DealsScope::DiscountedTopSellers);
}

#[test]
fn wish_adds_unless_the_game_is_already_listed() {
    let mut browser = browser_with(loaded());
    browser.begin_shop_wish();
    let change = view(&browser).confirm.clone().unwrap();
    assert_eq!(change.app_id, 620);
    assert!(change.add);
    browser.answer_shop_confirm(false);
    assert!(view(&browser).confirm.is_none());
    assert!(view(&browser).wish_op.is_none());

    // Portal is on the loaded wishlist, so `w` offers to remove it.
    browser.shop_move(1);
    browser.begin_shop_wish();
    assert!(!view(&browser).confirm.as_ref().unwrap().add);
}

#[test]
fn wish_in_the_wishlist_section_removes() {
    let mut browser = browser_with(loaded());
    browser.shop_cycle(-1);
    browser.begin_shop_wish();
    let change = view(&browser).confirm.clone().unwrap();
    assert_eq!(change.app_id, 400);
    assert!(!change.add);
}

#[test]
fn one_wishlist_change_at_a_time() {
    let mut shop = loaded();
    let change = WishChange {
        app_id: 1,
        name: "x".into(),
        add: true,
    };
    shop.wish_op = Some((change, Arc::new(Mutex::new(None))));
    let mut browser = browser_with(shop);
    browser.begin_shop_wish();
    assert!(view(&browser).confirm.is_none());
    assert!(view(&browser).status.contains("already running"));
}

#[test]
fn a_finished_change_reports_and_drops_the_stale_wishlist() {
    let mut shop = loaded();
    let change = WishChange {
        app_id: 620,
        name: "Portal 2".into(),
        add: true,
    };
    shop.wish_op = Some((change, Arc::new(Mutex::new(Some(Ok(3))))));
    let mut browser = browser_with(shop);
    browser.poll_shop();
    let message = "Added Portal 2 (3 on your wishlist).";
    assert_eq!(browser.notice.as_deref(), Some(message));
    assert_eq!(view(&browser).status, message);
    assert!(view(&browser).wish_op.is_none());
    // Not shown, so not reloaded yet.
    assert!(view(&browser).wishlist.is_none());
}

#[test]
fn search_prompt_is_search_only_and_prefilled() {
    let mut browser = browser_with(loaded());
    browser.begin_shop_search();
    assert_eq!(view(&browser).search_input.as_deref(), Some("portal"));
    browser.cancel_shop_input();
    browser.shop_cycle(1);
    browser.begin_shop_search();
    assert!(!browser.shop_input_open());
}

/// [`loaded`], plus a friend's loaded wishlist, shown.
fn with_friend() -> ShopView {
    ShopView {
        section: ShopSection::Friend,
        friend: Some(FriendWishlist {
            steam_id: 76561197960287930,
            name: "Gabe".into(),
            load: Some(Load::Ready(wishlist(&[400, 730]))),
        }),
        ..loaded()
    }
}

#[test]
fn the_friend_section_joins_the_tab_ring_only_with_a_friend() {
    assert_eq!(loaded().sections(), ShopSection::ALL);
    let shop = with_friend();
    assert_eq!(shop.sections().last(), Some(&ShopSection::Friend));
    assert_eq!(shop.section_label(ShopSection::Friend), "Gabe's wishlist");
    assert_eq!(shop.section_label(ShopSection::Wishlist), "Wishlist");

    let mut browser = browser_with(with_friend());
    browser.shop_cycle(1);
    assert_eq!(view(&browser).section, ShopSection::Search, "wraps forward");
    browser.shop_cycle(-1);
    assert_eq!(view(&browser).section, ShopSection::Friend, "and back");
    browser.shop_cycle(-1);
    assert_eq!(view(&browser).section, ShopSection::Wishlist);

    let mut browser = browser_with(loaded());
    browser.shop_cycle(-1);
    assert_eq!(
        view(&browser).section,
        ShopSection::Wishlist,
        "no friend, no Friend section"
    );
}

#[test]
fn wish_on_a_friends_game_checks_your_own_wishlist() {
    let mut browser = browser_with(with_friend());
    assert_eq!(view(&browser).row_count(), 2);
    // Portal (400) is on yours too: `w` removes it from yours.
    browser.begin_shop_wish();
    let change = view(&browser).confirm.clone().unwrap();
    assert_eq!(change.app_id, 400);
    assert!(!change.add);
    browser.answer_shop_confirm(false);
    // 730 isn't: `w` adds it to yours.
    browser.shop_move(1);
    browser.begin_shop_wish();
    let change = view(&browser).confirm.clone().unwrap();
    assert_eq!(change.app_id, 730);
    assert!(change.add);
}
