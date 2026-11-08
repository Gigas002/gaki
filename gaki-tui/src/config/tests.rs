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
    assert!(config.check_updates, "updates are checked by default");
    assert!(
        !Config::parse("check_updates = false\n")
            .unwrap()
            .check_updates
    );
    assert!(Config::parse("").is_ok(), "an empty file is the defaults");
    assert!(Config::parse("favorite_games = 3").is_err());
}

#[test]
fn store_regions_default_to_none_and_parse() {
    assert!(Config::default().store_regions.is_empty());
    let config = Config::parse("store_regions = [\"US\", \"DE\"]\n").unwrap();
    assert_eq!(config.store_regions, ["US", "DE"]);
}

#[test]
fn a_commented_file_parses_and_keeps_its_comments_on_save() {
    let text = "# settings\n\n# types\nallowed_games = [\"Game\", \"DLC\"]\n\n# colour\n\
                highlight = \"#66C0F4\"\n\n# updates\ncheck_updates = true\n\n# regions\n\
                store_regions = []\n";
    let mut config = Config::parse(text).unwrap();
    assert_eq!(config.highlight, theme::ACCENT);
    config.check_updates = false;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).unwrap();
    libgaki::config::edit::write_serialized(&path, &config).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        saved,
        text.replace("check_updates = true", "check_updates = false")
    );
}
