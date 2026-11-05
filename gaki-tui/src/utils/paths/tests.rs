use super::*;

#[test]
fn adopts_the_legacy_config_once() {
    let tmp = tempfile::tempdir().unwrap();
    let (legacy, dir) = (tmp.path().join("aurelia-tui"), tmp.path().join("tui"));
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&dir).unwrap();
    fs::write(legacy.join(CONFIG_FILE), r#"{"favorite_games":[400]}"#).unwrap();

    assert!(adopt_legacy_config(&legacy, &dir).unwrap());
    assert_eq!(
        fs::read_to_string(dir.join(CONFIG_FILE)).unwrap(),
        r#"{"favorite_games":[400]}"#
    );
    // The old file stays, and a second run doesn't overwrite the new one.
    fs::write(dir.join(CONFIG_FILE), "{}").unwrap();
    assert!(!adopt_legacy_config(&legacy, &dir).unwrap());
    assert_eq!(fs::read_to_string(dir.join(CONFIG_FILE)).unwrap(), "{}");
    assert!(legacy.join(CONFIG_FILE).exists());
}

#[test]
fn nothing_to_adopt_without_a_legacy_config() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(!adopt_legacy_config(&tmp.path().join("missing"), tmp.path()).unwrap());
    assert!(!tmp.path().join(CONFIG_FILE).exists());
}
