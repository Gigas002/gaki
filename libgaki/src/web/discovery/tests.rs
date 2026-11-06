use super::*;
use serde_json::json;

fn reviews_body(cursor: &str) -> Value {
    json!({
        "success": 1,
        "query_summary": {
            "review_score": 8, "review_score_desc": "Very Positive",
            "total_positive": 90, "total_negative": 10, "total_reviews": 100
        },
        "reviews": [{
            "recommendationid": "555",
            "author": {"steamid": "7656", "personaname": "Ann",
                       "playtime_forever": 90, "playtime_at_review": 30},
            "language": "english", "voted_up": true, "votes_up": 4, "votes_funny": 1,
            "timestamp_created": 1700000000, "steam_purchase": true,
            "received_for_free": false, "written_during_early_access": true,
            "review": "Good."
        }, {}],
        "cursor": cursor
    })
}

#[test]
fn reviews_read_the_summary_and_each_review() {
    let page = parse_reviews(&reviews_body("AoJ"), 10, None).unwrap();
    let s = &page.summary;
    assert_eq!((s.score, s.label.as_str()), (8, "Very Positive"));
    assert_eq!(
        (s.total_positive, s.total_negative, s.total_reviews),
        (90, 10, 100)
    );

    let [review, empty] = &page.reviews[..] else {
        panic!("expected two reviews: {:?}", page.reviews);
    };
    assert_eq!(review.id, "555");
    assert_eq!(
        (review.author.as_str(), review.steam_id.as_str()),
        ("Ann", "7656")
    );
    assert_eq!(review.language, "english");
    assert!(review.voted_up && review.steam_purchase && review.early_access);
    assert!(!review.received_for_free);
    assert_eq!((review.votes_up, review.votes_funny), (4, 1));
    assert_eq!(review.playtime_hours, 1.5);
    assert_eq!(review.playtime_at_review_hours, 0.5);
    assert_eq!(review.created, 1_700_000_000);
    assert_eq!(review.text, "Good.");

    assert_eq!(empty.id, "", "missing fields default");
    assert!(!empty.voted_up);
    assert_eq!(page.next_cursor.as_deref(), Some("AoJ"));
}

#[test]
fn a_repeated_or_empty_cursor_ends_the_list() {
    let page = parse_reviews(&reviews_body("AoJ"), 10, Some("AoJ")).unwrap();
    assert_eq!(page.next_cursor, None);
    let page = parse_reviews(&reviews_body(""), 10, None).unwrap();
    assert_eq!(page.next_cursor, None);
}

#[test]
fn unsuccessful_reviews_are_not_found() {
    let err = parse_reviews(&json!({"success": 2}), 10, None).unwrap_err();
    let typed = err.downcast_ref::<TypedError>().unwrap();
    assert_eq!(typed.kind, ErrorKind::NotFound);
}

#[test]
fn news_items_are_read_and_stripped() {
    let body = json!({"appnews": {"newsitems": [{
        "gid": 42, "title": "Patch", "url": "https://x", "author": "Dev",
        "feedlabel": "Announcements", "date": 1700000000,
        "contents": "<p>Fixed &amp; improved</p>"
    }]}});
    let [item] = &parse_news(&body, 10).unwrap()[..] else {
        panic!("expected one item");
    };
    assert_eq!(item.gid, "42", "a numeric id is kept as text");
    assert_eq!(
        (item.title.as_str(), item.url.as_str()),
        ("Patch", "https://x")
    );
    assert_eq!(
        (item.author.as_str(), item.feed.as_str()),
        ("Dev", "Announcements")
    );
    assert_eq!(item.date, 1_700_000_000);
    assert_eq!(item.contents, "Fixed & improved");
}

#[test]
fn a_news_response_without_items_has_changed_shape() {
    let err = parse_news(&json!({"appnews": {}}), 10).unwrap_err();
    let typed = err.downcast_ref::<TypedError>().unwrap();
    assert_eq!(typed.kind, ErrorKind::SourceChanged);
}

#[test]
fn negative_timestamps_clamp_to_zero() {
    let body = json!({"appnews": {"newsitems": [{"date": -5}]}});
    assert_eq!(parse_news(&body, 10).unwrap()[0].date, 0);
}
