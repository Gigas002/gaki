use super::*;

#[test]
fn settings_round_trip_through_toml() {
    let config = Config {
        default_user: "me".into(),
        hidden_games: vec![10],
        favorite_games: vec![400, 570],
        ..Config::default()
    };
    let text = toml::to_string(&config).unwrap();
    let back = Config::parse(&text).unwrap();
    assert_eq!(back.favorite_games, [400, 570]);
    assert_eq!(back.hidden_games, [10]);
    assert_eq!(back.highlight, config.highlight);
    assert_eq!(back.allowed_games.len(), config.allowed_games.len());
}

#[test]
fn missing_keys_take_the_defaults() {
    let config = Config::parse("favorite_games = [400]\n").unwrap();
    assert_eq!(config.favorite_games, [400]);
    assert_eq!(config.highlight, theme::ACCENT);
    assert!(Config::parse("").is_ok(), "an empty file is the defaults");
    assert!(Config::parse("favorite_games = 3").is_err());
}
