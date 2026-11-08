use super::*;

fn library() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

#[test]
fn escape_round_trips() {
    for path in [
        "/home/me/games",
        "/mnt/my-games/.lib",
        "/media/ü drive/x\\y",
    ] {
        let escaped = escape(Path::new(path));
        assert!(!escaped.contains('/'), "{escaped}");
        assert_eq!(unescape(&escaped), Path::new(path));
    }
    assert_eq!(
        escape(Path::new("/home/me/my-games")),
        "home-me-my\\x2dgames"
    );
}

#[test]
fn a_library_is_steams_until_an_install_says_otherwise() {
    let (_dir, root) = library();
    let layout = Layout::of(&root);
    assert!(!layout.is_flat());
    assert_eq!(layout.game_dir("Game"), root.join("steamapps/common/Game"));
    assert_eq!(
        layout.manifest(10),
        root.join("steamapps/appmanifest_10.acf")
    );
}

#[test]
fn the_configured_library_without_steamapps_gets_the_flat_layout() {
    let (_dir, root) = library();
    let layout = Layout::for_install(&root, &root).unwrap();
    assert!(layout.is_flat());
    assert_eq!(layout.game_dir("Game"), root.join("Game"));
    assert!(layout.steamapps().is_dir(), "the records dir remembers it");
    assert!(!root.join("steamapps").exists());
    assert_eq!(Layout::of(&root), layout);

    let manifest = layout.manifest(10);
    assert_eq!(
        Layout::from_steamapps(manifest.parent().unwrap()),
        Some(layout.clone())
    );
    assert_eq!(Layout::from_game_dir(&root.join("Game")), Some(layout));
}

#[test]
fn steam_libraries_and_other_folders_keep_steams_layout() {
    let (_dir, root) = library();
    let steam = root.join("Steam");
    std::fs::create_dir_all(steam.join("steamapps")).unwrap();
    assert!(!Layout::for_install(&steam, &steam).unwrap().is_flat());

    let other = root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    assert!(!Layout::for_install(&other, &root).unwrap().is_flat());

    let layout = Layout::of(&steam);
    assert_eq!(
        Layout::from_steamapps(&steam.join("steamapps")),
        Some(layout.clone())
    );
    assert_eq!(
        Layout::from_game_dir(&steam.join("steamapps/common/Game")),
        Some(layout)
    );
}
