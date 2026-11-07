//! Renders the Store overlay into an off-screen `TestBackend`.
#![cfg(feature = "store")]

use gaki_tui::app::browser::job::Load;
use gaki_tui::app::browser::store::{Section, StoreView};
use gaki_tui::ui;
use libgaki::ops::store::{PriceQuote, Prices};
use libgaki::web::discovery::{Review, ReviewPage, ReviewSummary};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn render(view: &StoreView, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| ui::store::draw(frame, frame.area(), view))
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

fn quote(country: &str, price: &str, discount: i32) -> PriceQuote {
    PriceQuote {
        country: country.to_string(),
        available: true,
        is_free: false,
        price: Some(price.to_string()),
        price_cents: None,
        original_price: Some("$19.99".into()),
        original_price_cents: None,
        discount_pct: discount,
        discount_end: None,
        discount_end_date: Some("2026-11-20".into()),
        purchase_options: Vec::new(),
        error: None,
    }
}

#[test]
fn the_price_section_compares_regions() {
    let mut view = StoreView::new(620, "Portal 2".into(), vec![]);
    let mut failed = quote("JP", "", 0);
    failed.error = Some("timed out".into());
    view.price = Some(Load::Ready(Prices {
        name: "Portal 2".into(),
        regions: vec![quote("US", "$4.99", 75), quote("DE", "4,99€", 0), failed],
    }));
    let screen = render(&view, 100, 16);
    assert!(screen.contains("Store · Portal 2"), "{screen}");
    assert!(screen.contains("Price"), "{screen}");
    assert!(screen.contains("$4.99"), "{screen}");
    assert!(screen.contains("-75%"), "{screen}");
    assert!(screen.contains("2026-11-20"), "{screen}");
    assert!(screen.contains("4,99€"), "{screen}");
    assert!(
        screen.contains("JP      couldn't load: timed out"),
        "{screen}"
    );
    assert!(screen.contains("[r] regions"), "{screen}");
}

#[test]
fn a_loading_section_says_so() {
    let mut view = StoreView::new(620, "Portal 2".into(), vec![]);
    view.section = Section::Players;
    let screen = render(&view, 80, 10);
    assert!(screen.contains("Loading the player count…"), "{screen}");
    assert!(!screen.contains("[r] regions"), "{screen}");
}

#[test]
fn reviews_show_the_summary_and_each_verdict() {
    let mut view = StoreView::new(620, "Portal 2".into(), vec![]);
    view.section = Section::Reviews;
    view.reviews = Some(Load::Ready(ReviewPage {
        summary: ReviewSummary {
            score: 9,
            label: "Overwhelmingly Positive".into(),
            total_positive: 980,
            total_negative: 20,
            total_reviews: 1000,
        },
        reviews: vec![Review {
            author: "someone".into(),
            voted_up: false,
            text: "Too short.".into(),
            ..Default::default()
        }],
        next_cursor: None,
    }));
    let screen = render(&view, 90, 14);
    assert!(
        screen.contains("Overwhelmingly Positive  98% of 1,000 positive"),
        "{screen}"
    );
    assert!(screen.contains("▼ Not recommended"), "{screen}");
    assert!(screen.contains("Too short."), "{screen}");
}

#[test]
fn players_are_grouped_by_thousands() {
    let mut view = StoreView::new(620, "Portal 2".into(), vec![]);
    view.section = Section::Players;
    view.players = Some(Load::Ready(1_234_567));
    let screen = render(&view, 80, 10);
    assert!(screen.contains("1,234,567 playing right now"), "{screen}");
}
