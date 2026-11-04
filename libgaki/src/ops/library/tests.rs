use super::*;

fn game(app_id: u32, name: &str, owned: bool) -> LibraryGame {
    LibraryGame {
        app_id,
        name: name.to_string(),
        playtime_forever_minutes: None,
        is_installed: true,
        install_path: None,
        local_manifest_ids: Default::default(),
        update_available: false,
        update_queued: false,
        active_branch: "public".to_string(),
        is_owned: owned,
        is_family_shared: false,
        online_required: None,
        platform: None,
    }
}

fn shared(app_id: u32, name: &str) -> SharedApp {
    SharedApp {
        app_id,
        name: name.to_string(),
        owner_steamid: Some(1),
    }
}

#[test]
fn new_shared_apps_are_added_as_not_installed() {
    let mut games = vec![];
    merge_family_shared(&mut games, vec![shared(10, "Borrowed")]);
    assert_eq!(games.len(), 1);
    assert!(games[0].is_family_shared);
    assert!(!games[0].is_owned);
    assert!(!games[0].is_installed);
}

#[test]
fn installed_unowned_app_is_flagged_and_renamed() {
    let mut games = vec![game(10, "App 10", false)];
    merge_family_shared(&mut games, vec![shared(10, "Borrowed")]);
    assert_eq!(games.len(), 1);
    assert!(games[0].is_family_shared);
    assert_eq!(games[0].name, "Borrowed");
}

#[test]
fn owned_app_stays_owned() {
    let mut games = vec![game(10, "Mine", true)];
    merge_family_shared(&mut games, vec![shared(10, "Mine")]);
    assert!(!games[0].is_family_shared);
    assert!(games[0].is_owned);
}
