use super::*;
use serde_json::json;

#[test]
fn inventory_assets_take_their_descriptions() {
    let body = json!({
        "success": 1,
        "assets": [
            {"assetid": "11", "classid": "100", "instanceid": "0", "amount": "3"},
            {"assetid": "12", "classid": "200", "instanceid": "7"},
            {"assetid": "13", "classid": "999", "instanceid": "0"}
        ],
        "descriptions": [
            {"classid": "100", "instanceid": "0", "name": "Card", "market_hash_name": "123-Card",
             "type": "Trading Card", "tradable": 1, "marketable": 1, "icon_url": "abc"},
            {"classid": "200", "instanceid": "7", "name": "Gem", "tradable": 0, "icon_url": ""}
        ]
    })
    .to_string();
    let items = parse_inventory(&body).unwrap();
    assert_eq!(items.len(), 3);

    let card = &items[0];
    assert_eq!(
        (card.asset_id.as_str(), card.class_id.as_str()),
        ("11", "100")
    );
    assert_eq!(card.name, "Card");
    assert_eq!(card.market_hash_name, "123-Card");
    assert_eq!(card.item_type, "Trading Card");
    assert_eq!(card.amount, 3, "a string amount is read");
    assert!(card.tradable && card.marketable);
    assert_eq!(
        card.icon_url.as_deref(),
        Some(&*format!("{ECON_IMAGE_BASE}abc"))
    );

    let gem = &items[1];
    assert_eq!(gem.name, "Gem", "the instance id picks the description");
    assert_eq!(gem.amount, 1, "a missing amount means one");
    assert!(!gem.tradable && !gem.marketable);
    assert_eq!(gem.icon_url, None, "an empty icon is no icon");

    let unknown = &items[2];
    assert_eq!(
        unknown.name, "",
        "an asset without a description keeps its ids"
    );
    assert_eq!(unknown.asset_id, "13");
}

#[test]
fn an_inventory_without_assets_is_empty() {
    assert!(parse_inventory(r#"{"success":1}"#).unwrap().is_empty());
}

#[test]
fn an_inventory_failure_carries_steams_message() {
    let err = parse_inventory(r#"{"success":0,"error":"This profile is private."}"#).unwrap_err();
    assert_eq!(err.to_string(), "This profile is private.");
    let err = parse_inventory(r#"{"success":0}"#).unwrap_err();
    assert_eq!(err.to_string(), "inventory unavailable");
    let err = parse_inventory("<html>").unwrap_err();
    assert!(err.to_string().contains("login --reconnect"), "{err}");
}

#[test]
fn my_listings_read_listings_and_buy_orders() {
    let body = json!({
        "success": true,
        "listings": [
            {"listingid": "1", "price": 150, "asset": {"market_hash_name": "Hat"}},
            {"listingid": "2", "price": "75", "asset": {"name": "Plain name"}},
            {"price": 1}
        ],
        "buy_orders": [
            {"buy_orderid": "9", "hash_name": "Key", "price": "250", "quantity": 2},
            {"hash_name": "no id"}
        ]
    })
    .to_string();
    let state = parse_my_listings(&body).unwrap();

    let listings: Vec<_> = state
        .listings
        .iter()
        .map(|l| (l.listing_id.as_str(), l.market_hash_name.as_str(), l.price))
        .collect();
    assert_eq!(
        listings,
        [("1", "Hat", 150), ("2", "Plain name", 75)],
        "a listing without an id is skipped"
    );

    let [order] = &state.buy_orders[..] else {
        panic!("expected one buy order: {:?}", state.buy_orders);
    };
    assert_eq!(order.buy_order_id, "9");
    assert_eq!(order.market_hash_name, "Key");
    assert_eq!((order.price, order.quantity), (250, 2));
}

#[test]
fn an_empty_market_account_has_no_listings() {
    let state = parse_my_listings(r#"{"success":true}"#).unwrap();
    assert!(state.listings.is_empty() && state.buy_orders.is_empty());
}

#[test]
fn a_rejected_market_session_is_an_error() {
    for body in [r#"{"success":false}"#, r#"{"success":0}"#] {
        let err = parse_my_listings(body).unwrap_err();
        assert!(err.to_string().contains("rejected"), "{body}: {err}");
    }
    assert!(parse_my_listings("not json").is_err());
}

#[test]
fn search_results_need_a_name() {
    let v = json!({"results": [
        {"name": "Hat", "hash_name": "440-Hat", "app_name": "TF2", "sell_listings": 4,
         "sell_price": 12, "sell_price_text": "$0.12", "asset_description": {"appid": 440}},
        {"name": "Bare"},
        {"hash_name": "nameless"}
    ]});
    let results = parse_json_array(&v, "results", parse_search_result);
    assert_eq!(results.len(), 2);
    let hat = &results[0];
    assert_eq!(
        (hat.name.as_str(), hat.market_hash_name.as_str()),
        ("Hat", "440-Hat")
    );
    assert_eq!((hat.app_id, hat.app_name.as_str()), (440, "TF2"));
    assert_eq!((hat.sell_listings, hat.sell_price), (4, 12));
    assert_eq!(hat.sell_price_text, "$0.12");
    let bare = &results[1];
    assert_eq!((bare.app_id, bare.sell_price), (0, 0));
    assert!(parse_json_array(&v, "missing", parse_search_result).is_empty());
}

#[test]
fn loose_numbers_accept_strings() {
    assert_eq!(as_i64_loose(&json!(-5)), Some(-5));
    assert_eq!(as_i64_loose(&json!("-5")), Some(-5));
    assert_eq!(as_i64_loose(&json!("five")), None);
    assert_eq!(as_u64_loose(&json!(7)), Some(7));
    assert_eq!(as_u64_loose(&json!("7")), Some(7));
    assert_eq!(as_u64_loose(&json!("-7")), None);
    assert_eq!(as_u64_loose(&json!(null)), None);
}

#[test]
fn the_wallet_is_read_from_the_market_page() {
    let html = r#"<script>var g_rgWalletInfo = {"wallet_currency":3,"wallet_country":"DE","wallet_balance":"1234"};</script>"#;
    let wallet = parse_wallet(html).unwrap();
    assert_eq!(wallet.balance_cents, 1234);
    assert_eq!(wallet.currency, 3);
    assert_eq!(wallet.country, "DE");
    assert_eq!(wallet.formatted, "12.34 EUR");

    let err = parse_wallet("<html></html>").unwrap_err();
    assert!(err.to_string().contains("g_rgWalletInfo"), "{err}");
    assert!(parse_wallet("g_rgWalletInfo = {nope};").is_err());
}

#[test]
fn currencies_format_with_their_code() {
    assert_eq!(format_currency(500, 1), "5.00 USD");
    assert_eq!(format_currency(-250, 5), "-2.50 RUB");
    assert_eq!(format_currency(1, 999), "0.01 (currency 999)");
}
