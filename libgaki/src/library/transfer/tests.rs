use super::*;

fn game(app_id: u32, name: &str, platform: Option<&str>, branch: &str) -> LibraryGame {
    LibraryGame {
        app_id,
        name: name.to_string(),
        playtime_forever_minutes: None,
        playtime_2weeks_minutes: None,
        last_played: None,
        is_installed: platform.is_some(),
        install_path: None,
        local_manifest_ids: Default::default(),
        update_available: false,
        update_queued: false,
        active_branch: branch.to_string(),
        is_owned: true,
        is_family_shared: false,
        online_required: None,
        platform: platform.map(str::to_string),
    }
}

fn entry(app_id: u32) -> LibraryEntry {
    LibraryEntry {
        app_id,
        name: String::new(),
        platform: None,
        branch: None,
    }
}

fn library() -> Vec<LibraryGame> {
    vec![
        game(10, "zeta", Some("windows"), "public"),
        game(20, "Alpha", Some("linux"), "beta"),
        game(30, "Mid", None, "public"),
        game(40, "Mac", Some("macos"), ""),
    ]
}

#[test]
fn export_lists_installed_games_by_name() {
    let file = export(&library(), false);
    let ids: Vec<u32> = file.games.iter().map(|e| e.app_id).collect();
    assert_eq!(ids, [20, 40, 10], "sorted by name, ignoring case");

    let alpha = &file.games[0];
    assert_eq!(alpha.platform, Some(Platform::Linux));
    assert_eq!(alpha.branch.as_deref(), Some("beta"));
    let mac = &file.games[1];
    assert_eq!(mac.platform, None, "gaki doesn't install macOS builds");
    assert_eq!(mac.branch, None, "an empty branch is the default one");
    let zeta = &file.games[2];
    assert_eq!(zeta.platform, Some(Platform::Windows));
    assert_eq!(zeta.branch, None, "public is the default branch");
}

#[test]
fn export_all_adds_games_that_arent_installed() {
    let file = export(&library(), true);
    let mid = file.games.iter().find(|e| e.app_id == 30).unwrap();
    assert_eq!(mid.name, "Mid");
    assert_eq!((mid.platform, mid.branch.as_deref()), (None, None));
    assert_eq!(file.games.len(), 4);
}

#[test]
fn an_export_reads_back() {
    let file = export(&library(), true);
    let text = to_toml(&file).unwrap();
    assert!(text.starts_with("# gaki library"), "{text}");
    assert!(
        text.contains(
            "[[game]]\napp_id = 20\nname = \"Alpha\"\nplatform = \"linux\"\nbranch = \"beta\""
        ),
        "{text}"
    );
    assert!(!text.contains("branch = \"public\""), "{text}");
    assert_eq!(parse(&text).unwrap(), file);
}

#[test]
fn only_the_app_id_is_required() {
    let file = parse("[[game]]\napp_id = 570\n\n[[game]]\napp_id = 620\nplatform = \"windows\"\n")
        .unwrap();
    assert_eq!(file.games.len(), 2);
    assert_eq!(file.games[0], entry(570));
    assert_eq!(file.games[1].platform, Some(Platform::Windows));
    assert!(parse("").unwrap().games.is_empty());
}

#[test]
fn bad_files_are_errors() {
    let twice = parse("[[game]]\napp_id = 570\n[[game]]\napp_id = 570\n").unwrap_err();
    assert_eq!(twice.to_string(), "app 570 is listed twice");
    assert!(parse("[[game]]\napp_id = 0\n").is_err());
    assert!(parse("[[game]]\nname = \"no id\"\n").is_err());
    assert!(parse("[[game]]\napp_id = 570\nplatform = \"macos\"\n").is_err());
    assert!(parse("[[game]]\napp_id = -1\n").is_err());
}

#[test]
fn read_names_the_file_and_types_the_error() {
    let kind = |e: &anyhow::Error| e.downcast_ref::<TypedError>().unwrap().kind;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("library.toml");
    let err = read(&path).unwrap_err();
    assert!(err.to_string().contains("library.toml"), "{err}");
    assert_eq!(kind(&err), ErrorKind::NotFound);
    std::fs::write(&path, "[[game]]\napp_id = \"x\"\n").unwrap();
    let err = read(&path).unwrap_err();
    assert!(err.to_string().starts_with("failed parsing"), "{err}");
    assert_eq!(kind(&err), ErrorKind::InvalidInput);
    std::fs::write(&path, "[[game]]\napp_id = 570\n").unwrap();
    assert_eq!(read(&path).unwrap().games, [entry(570)]);
}

#[test]
fn import_installs_what_is_owned_and_missing() {
    let mut on_beta = entry(30);
    on_beta.branch = Some("beta".into());
    let file = LibraryFile {
        games: vec![entry(10), on_beta, entry(99)],
    };
    let plan = plan(&file, &library());
    assert_eq!(
        plan.installed.iter().map(|e| e.app_id).collect::<Vec<_>>(),
        [10]
    );
    assert_eq!(
        plan.not_owned.iter().map(|e| e.app_id).collect::<Vec<_>>(),
        [99]
    );
    let [mid] = &plan.install[..] else {
        panic!("expected one install: {:?}", plan.install);
    };
    assert_eq!(mid.name, "Mid", "a nameless entry takes the library's name");
    assert_eq!(
        plan.on_a_branch().map(|e| e.app_id).collect::<Vec<_>>(),
        [30]
    );
}

#[test]
fn an_entry_keeps_its_own_name() {
    let mut named = entry(30);
    named.name = "mine".into();
    let plan = plan(&LibraryFile { games: vec![named] }, &library());
    assert_eq!(plan.install[0].name, "mine");
}
