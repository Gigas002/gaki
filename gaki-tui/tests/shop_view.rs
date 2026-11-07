//! Renders the Steam Store overlay (`J`) into an off-screen `TestBackend`.
#![cfg(feature = "store")]

use gaki_tui::app::browser::job::Load;
use gaki_tui::app::browser::shop::{ShopSection, ShopView, WishChange};
use gaki_tui::ui;
use libgaki::ops::store::Listing;
use libgaki::steam_client::{StoreAppInfo, StoreEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(view: &ShopView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::shop::draw(frame, frame.area(), view))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn results() -> Listing {
    Listing {
        total: 1,
        suggestions: Vec::new(),
        records: vec![StoreAppInfo {
            app_id: 620,
            name: "Portal 2".into(),
            price: Some("$2.49".into()),
            discount_pct: 75,
            review_summary: Some("Overwhelmingly Positive".into()),
            ..Default::default()
        }],
    }
}

#[test]
fn a_fresh_overlay_asks_for_a_title() {
    let screen = render(&ShopView::default(), 100, 12);
    assert!(
        screen.contains("Steam Store · [Search] Deals Events Wishlist"),
        "{screen}"
    );
    assert!(screen.contains("Type a title and press Enter."), "{screen}");
    assert!(screen.contains("Search: _"), "{screen}");
}

#[test]
fn search_results_show_price_discount_and_reviews() {
    let view = ShopView {
        query: "portal".into(),
        search_input: None,
        search: Some(Load::Ready(results())),
        ..ShopView::default()
    };
    let screen = render(&view, 110, 12);
    assert!(screen.contains("Results for “portal”"), "{screen}");
    assert!(
        screen.contains("▶ 620      Portal 2  $2.49 -75%  Overwhelmingly Positive"),
        "{screen}"
    );
    assert!(screen.contains("[w] wishlist"), "{screen}");
}

#[test]
fn the_wishlist_confirm_replaces_the_hints() {
    let view = ShopView {
        section: ShopSection::Deals,
        search_input: None,
        deals: Some(Load::Ready(results())),
        confirm: Some(WishChange {
            app_id: 620,
            name: "Portal 2".into(),
            add: true,
        }),
        ..ShopView::default()
    };
    let screen = render(&view, 100, 12);
    assert!(screen.contains("discounted top sellers"), "{screen}");
    assert!(
        screen.contains("Add Portal 2 to your wishlist?"),
        "{screen}"
    );
    assert!(screen.contains("[y] yes  [n] no"), "{screen}");
    assert!(!screen.contains("[Esc] close"), "{screen}");
}

#[test]
fn events_and_failures() {
    let view = ShopView {
        section: ShopSection::Events,
        search_input: None,
        events: Some(Load::Ready(vec![StoreEvent {
            title: "Autumn Sale".into(),
            kind: "Sale".into(),
            ..Default::default()
        }])),
        ..ShopView::default()
    };
    let screen = render(&view, 100, 12);
    assert!(screen.contains("▶ Autumn Sale  Sale"), "{screen}");
    assert!(!screen.contains("[w] wishlist"), "{screen}");

    let view = ShopView {
        section: ShopSection::Wishlist,
        search_input: None,
        wishlist: Some(Load::Failed("private".into())),
        ..ShopView::default()
    };
    let screen = render(&view, 100, 12);
    assert!(screen.contains("Couldn't load: private"), "{screen}");
}
