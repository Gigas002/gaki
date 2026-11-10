use crate::app::client::Client;
use crate::utils::error::TuiError;

#[test]
fn the_games_cache_round_trips_through_toml() {
    use super::GamesCache;
    use crate::app::game::Game;
    use crate::app::steam::LibraryGame;
    let game = Game::from_library(LibraryGame {
        app_id: 570,
        name: "Dota 2".into(),
        is_installed: true,
        install_path: Some("/games/dota".into()),
        update_available: false,
        is_owned: true,
        is_family_shared: false,
        platform: None,
        active_branch: None,
        assets: None,
        store_url: None,
        playtime_minutes: None,
        playtime_2weeks_minutes: None,
        last_played: None,
    });
    let text = toml::to_string(&GamesCache { games: vec![game] }).unwrap();
    let back: GamesCache = toml::from_str(&text).unwrap();
    assert_eq!(back.games[0].id, 570);
    assert!(back.games[0].installed);
    assert!(
        toml::from_str::<GamesCache>("").is_err(),
        "an empty cache file means the library still has to load"
    );
}

#[test]
fn test_blank_login() {
    let client = Client::new();
    let result = client.login("");
    if let Err(TuiError::Problem(expected)) = result {
        assert!(expected.contains(&"Blank".to_string()));
        return;
    }
    panic!("Failed to unwrap")
}
