use super::ensure_steam_appid;

#[test]
fn writes_one_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(ensure_steam_appid(dir.path(), 1030300).unwrap());
    let text = std::fs::read_to_string(dir.path().join("steam_appid.txt")).unwrap();
    assert_eq!(text, "1030300");
    // The next launch keeps it.
    assert!(!ensure_steam_appid(dir.path(), 1030300).unwrap());
}

#[test]
fn keeps_the_developers_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("steam_appid.txt");
    // A demo sharing the full game's files, say.
    std::fs::write(&path, "480\n").unwrap();
    assert!(!ensure_steam_appid(dir.path(), 1030300).unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "480\n");
}

#[test]
fn reports_a_folder_it_cannot_write() {
    let dir = tempfile::tempdir().unwrap();
    assert!(ensure_steam_appid(&dir.path().join("missing"), 1).is_err());
}
