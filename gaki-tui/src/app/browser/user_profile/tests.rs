use super::*;

fn browser() -> Browser {
    Browser::new(vec![])
}

#[test]
fn the_lookup_prompt_opens_empty_and_esc_closes_it() {
    let mut browser = browser();
    browser.open_profile_lookup();
    assert!(browser.profile_input_open());
    browser.profile_input_push('m');
    browser.profile_input_push('x');
    browser.profile_input_pop();
    browser.profile_input_push('e');
    assert_eq!(
        browser.user_profile.as_ref().unwrap().input.as_deref(),
        Some("me")
    );
    // Nothing looked up yet, so Esc closes the overlay.
    browser.cancel_profile_input();
    assert!(browser.user_profile.is_none());
}

#[test]
fn an_empty_lookup_keeps_the_prompt() {
    let mut browser = browser();
    browser.open_profile_lookup();
    browser.profile_input_push(' ');
    browser.commit_profile_input();
    let view = browser.user_profile.as_ref().unwrap();
    assert_eq!(view.input.as_deref(), Some(""));
    assert!(view.profile.is_none());
}

#[test]
fn esc_over_a_shown_profile_keeps_it() {
    let mut browser = browser();
    browser.user_profile = Some(ProfileView::with_profile(UserProfile {
        steam_id: 76561197960287930,
        ..UserProfile::default()
    }));
    browser.begin_profile_lookup();
    assert!(browser.profile_input_open());
    browser.cancel_profile_input();
    let view = browser.user_profile.as_ref().unwrap();
    assert!(view.input.is_none());
    assert_eq!(view.loaded().unwrap().steam_id, 76561197960287930);
}

#[test]
fn v_without_a_friend_does_nothing() {
    let mut browser = browser();
    browser.open_friend_profile();
    assert!(browser.user_profile.is_none());
}

#[test]
fn the_page_key_needs_a_url() {
    let mut browser = browser();
    browser.user_profile = Some(ProfileView::with_profile(UserProfile::default()));
    browser.open_profile_page();
    assert!(browser.notice.is_none());
}
