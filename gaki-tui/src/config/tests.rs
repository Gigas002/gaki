use super::*;

#[test]
fn configs_with_the_dropped_hltb_switch_still_load() {
    let mut legacy = serde_json::to_value(Config::default()).unwrap();
    legacy["hltb_enabled"] = serde_json::json!(true);
    legacy["favorite_games"] = serde_json::json!([400]);
    let config: Config = serde_json::from_value(legacy).unwrap();
    assert_eq!(config.favorite_games, [400]);
}
