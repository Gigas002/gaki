use super::*;

fn game(app_id: u32, name: &str, owned: bool) -> LibraryGame {
    LibraryGame {
        app_id,
        name: name.to_string(),
        playtime_forever_minutes: None,
        playtime_2weeks_minutes: None,
        last_played: None,
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

#[test]
fn install_targets_list_the_configured_library_first_even_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let configured = root.join("games"); // nothing installed: no steamapps/
    std::fs::create_dir_all(&configured).unwrap();
    let other = root.join("Steam");
    std::fs::create_dir_all(other.join("steamapps")).unwrap();
    let stale = root.join("old"); // not a library
    std::fs::create_dir_all(&stale).unwrap();

    let targets = install_targets(
        Some(configured.clone()),
        vec![other.clone(), stale, configured.clone()],
    );
    assert_eq!(targets, [configured, other.clone()]);
    assert_eq!(install_targets(None, vec![other.clone()]), [other]);
}

fn played(name: &str, minutes: Option<u32>, last: Option<u64>) -> LibraryGame {
    LibraryGame {
        playtime_forever_minutes: minutes,
        last_played: last,
        ..game(1, name, true)
    }
}

fn names(games: &[LibraryGame]) -> Vec<&str> {
    games.iter().map(|g| g.name.as_str()).collect()
}

#[test]
fn sort_by_playtime_puts_most_played_first_and_breaks_ties_by_name() {
    let mut games = vec![
        played("b", Some(10), None),
        played("C", None, None),
        played("a", Some(10), None),
        played("d", Some(300), None),
    ];
    sort_games(&mut games, SortBy::Playtime);
    assert_eq!(names(&games), ["d", "a", "b", "C"]);
}

#[test]
fn sort_by_last_played_puts_never_played_last() {
    let mut games = vec![
        played("never", Some(0), None),
        played("old", Some(5), Some(100)),
        played("new", Some(1), Some(200)),
    ];
    sort_games(&mut games, SortBy::LastPlayed);
    assert_eq!(names(&games), ["new", "old", "never"]);
}

#[test]
fn listing_totals_playtime_of_licensed_games() {
    let listing = Listing {
        games: vec![
            played("a", Some(90), None),
            played("b", None, None),
            played("c", Some(30), None),
        ],
        ..Default::default()
    };
    assert_eq!(listing.total_playtime_minutes(), 120);
}
