use super::*;
use libgaki::ops::store::{Listing, OwnReview, PriceQuote, Prices, ReviewWritten};
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

fn reviews_view(mine: Option<OwnReview>) -> StoreView {
    let mut view = loaded_view();
    view.section = Section::Reviews;
    view.mine = Some(Load::Ready(mine));
    view
}

fn own_review() -> OwnReview {
    OwnReview {
        id: 9,
        app_id: 620,
        text: "Short.".into(),
        voted_up: false,
        is_public: false,
        language: "english".into(),
        received_compensation: false,
        comments_disabled: true,
        created: 1_700_000_000,
        updated: 1_700_000_000,
        votes_up: 3,
        votes_down: 0,
        votes_funny: 1,
        comments: 0,
    }
}

fn screen(view: &StoreView) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::store::draw(frame, frame.area(), view))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_composer_starts_from_your_review_and_edits_it() {
    let mut browser = browser_with(reviews_view(Some(own_review())));
    let text = screen(browser.store.as_ref().unwrap());
    assert!(text.contains("Your review"), "{text}");
    assert!(text.contains("Short."), "{text}");
    assert!(text.contains("[w] write your review"), "{text}");

    browser.begin_review();
    assert!(browser.review_composer_open());
    for c in " Fun.".chars() {
        browser.review_push(c);
    }
    browser.review_push('\n');
    browser.review_push('x');
    browser.review_pop();
    browser.review_toggle(ReviewToggle::Verdict);
    browser.review_toggle(ReviewToggle::Visibility);
    browser.review_toggle(ReviewToggle::Comments);
    let composer = browser.store.as_ref().unwrap().composer.clone().unwrap();
    assert!(composer.replaces);
    assert_eq!(composer.draft.text, "Short. Fun.\n");
    assert!(composer.draft.voted_up && composer.draft.is_public);
    assert!(!composer.draft.comments_disabled);
    let text = screen(browser.store.as_ref().unwrap());
    assert!(text.contains("Changing your review"), "{text}");
    assert!(text.contains("▲ Recommended"), "{text}");
    assert!(text.contains("[ctrl+s] post"), "{text}");

    // The y/n: no goes back to the draft, typing is ignored meanwhile.
    browser.submit_review();
    assert!(browser.review_confirming());
    browser.review_push('z');
    let text = screen(browser.store.as_ref().unwrap());
    assert!(
        text.contains("Replace your review on Steam (public)?"),
        "{text}"
    );
    browser.answer_review(false);
    assert!(!browser.review_confirming());
    assert_eq!(
        browser
            .store
            .as_ref()
            .unwrap()
            .composer
            .as_ref()
            .unwrap()
            .draft
            .text,
        "Short. Fun.\n"
    );
    browser.cancel_review();
    assert!(!browser.review_composer_open());
}

#[test]
fn a_new_review_needs_text_and_a_failed_post_keeps_the_draft() {
    let mut browser = browser_with(reviews_view(None));
    let text = screen(browser.store.as_ref().unwrap());
    assert!(text.contains("You haven't reviewed it"), "{text}");
    browser.begin_review();
    let composer = browser.store.as_ref().unwrap().composer.clone().unwrap();
    assert!(!composer.replaces && composer.draft.voted_up && composer.draft.is_public);
    browser.submit_review();
    assert!(!browser.review_confirming());
    assert!(
        browser
            .store
            .as_ref()
            .unwrap()
            .review_status
            .contains("needs some text"),
        "{}",
        browser.store.as_ref().unwrap().review_status
    );

    browser.review_push('k');
    browser.submit_review();
    assert!(browser.review_confirming());
    // A post that fails comes back to the draft.
    let view = browser.store.as_mut().unwrap();
    view.posting = Some(Load::Loading(std::sync::Arc::new(std::sync::Mutex::new(
        Some(Err("refused".to_string())),
    ))));
    browser.poll_store();
    let view = browser.store.as_ref().unwrap();
    assert!(
        view.review_status.contains("refused"),
        "{}",
        view.review_status
    );
    assert!(!view.composer.as_ref().unwrap().confirming);

    // One that works closes the composer and reloads yours.
    let view = browser.store.as_mut().unwrap();
    view.section = Section::Price;
    view.posting = Some(Load::Loading(std::sync::Arc::new(std::sync::Mutex::new(
        Some(Ok(ReviewWritten::Created)),
    ))));
    browser.poll_store();
    let view = browser.store.as_ref().unwrap();
    assert_eq!(view.review_status, "Posted your review.");
    assert!(view.composer.is_none() && view.mine.is_none());
}

#[test]
fn write_review_is_for_the_reviews_section_once_yours_loaded() {
    let mut browser = browser_with(loaded_view());
    browser.begin_review();
    assert!(!browser.review_composer_open());
    let mut view = loaded_view();
    view.section = Section::Reviews;
    view.mine = Some(Load::Loading(std::sync::Arc::new(std::sync::Mutex::new(
        None,
    ))));
    let mut browser = browser_with(view);
    browser.begin_review();
    assert!(!browser.review_composer_open());
    assert!(
        browser
            .store
            .as_ref()
            .unwrap()
            .review_status
            .contains("Still loading")
    );
}
