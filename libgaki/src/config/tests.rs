use std::{fs, path::Path};

use super::*;

const HOME: &str = "/home/user";

fn ctx() -> ValidationContext<'static> {
    ValidationContext {
        home: Path::new(HOME),
    }
}

fn messages(issues: &[Issue]) -> Vec<String> {
    issues.iter().map(ToString::to_string).collect()
}

fn has(issues: &[Issue], severity: Severity, needle: &str) -> bool {
    issues
        .iter()
        .any(|issue| issue.severity == severity && issue.message.contains(needle))
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn names_follow_the_lootrs_rule() {
    for ok in ["proton", "proton-cachyos", "a.b_c", "9"] {
        assert!(validate_name(ok).is_ok(), "{ok}");
    }
    for bad in ["", ".", "..", "-x", "a b", "a/b", "ü"] {
        assert!(validate_name(bad).is_err(), "{bad}");
    }
}

#[test]
fn tilde_expands_only_as_a_leading_component() {
    let home = Path::new(HOME);
    assert_eq!(
        expand_tilde(Path::new("~/games"), home),
        Path::new("/home/user/games")
    );
    assert_eq!(expand_tilde(Path::new("~"), home), home);
    assert_eq!(expand_tilde(Path::new("/x/~"), home), Path::new("/x/~"));
    assert_eq!(expand_tilde(Path::new("~bob/x"), home), Path::new("~bob/x"));
}

#[test]
fn empty_config_is_the_default() {
    let config: Config = toml::from_str("").unwrap();
    assert_eq!(config, Config::default());
    assert!(config.cloud_sync);
    assert!(config.validate(&ctx()).is_empty());
}

#[test]
fn config_rejects_unknown_keys() {
    let err = toml::from_str::<Config>("steam_library_path = \"/x\"").unwrap_err();
    assert!(err.to_string().contains("steam_library_path"), "{err}");
}

#[test]
fn config_paths_must_be_absolute() {
    let config: Config = toml::from_str(
        r#"
        steam_library = "games"
        profiles_dir = "~/.config/lootrs/profiles"
        proxy = "localhost:8080"
        "#,
    )
    .unwrap();
    let issues = config.validate(&ctx());
    assert!(
        has(&issues, Severity::Error, "`steam_library`"),
        "{:?}",
        messages(&issues)
    );
    assert!(!has(&issues, Severity::Error, "`profiles_dir`"));
    assert!(has(&issues, Severity::Error, "`proxy`"));
}

#[test]
fn proton_profile_parses_and_validates() {
    let profile: Profile = toml::from_str(
        r#"
        name = "proton"
        runner = "proton"
        prefix = "~/prefixes/proton"
        wrappers = ["mangohud"]
        [proton]
        build = "experimental"
        [env]
        PROTON_ENABLE_WAYLAND = "1"
        [dll_overrides]
        winhttp = "n,b"
        "#,
    )
    .unwrap();
    assert_eq!(profile.runner, Runner::Proton);
    assert!(!profile.proton.as_ref().unwrap().build_is_path());
    assert!(profile.validate(&ctx()).is_empty());
}

#[test]
fn proton_build_path_must_be_absolute() {
    let mut profile = Profile::new("p", Runner::Proton);
    profile.proton = Some(ProtonSection {
        build: Some("compat/proton-ge".into()),
    });
    assert!(has(
        &profile.validate(&ctx()),
        Severity::Error,
        "`proton.build`"
    ));
    profile.proton = Some(ProtonSection {
        build: Some("~/compat/proton-ge".into()),
    });
    assert!(profile.validate(&ctx()).is_empty());
}

#[test]
fn lootrs_profiles_load_and_lootrs_runners_are_reported() {
    // A lootrs wine profile, with fields gaki doesn't model.
    let profile: Profile = toml::from_str(
        r#"
        name = "wine"
        runner = "wine"
        prefix = "~/prefixes/wine"
        dxvk = "/opt/dxvk"
        [wine]
        path = "/usr/bin/wine"
        "#,
    )
    .unwrap();
    let issues = profile.validate(&ctx());
    assert!(
        has(&issues, Severity::Error, "lootrs runner"),
        "{:?}",
        messages(&issues)
    );
    assert!(has(&issues, Severity::Warning, "`prefix` is ignored"));

    let native: Profile = toml::from_str("name = \"native\"\nrunner = \"native\"").unwrap();
    assert!(native.validate(&ctx()).is_empty());
}

#[test]
fn native_profile_warns_about_wine_only_fields() {
    let mut profile = Profile::new("n", Runner::Native);
    profile.dll_overrides.insert("d3d11".into(), "n".into());
    profile.anticheat.eac = true;
    let issues = profile.validate(&ctx());
    assert!(has(
        &issues,
        Severity::Warning,
        "`dll_overrides` is ignored"
    ));
    assert!(has(&issues, Severity::Warning, "`[anticheat]` is ignored"));
    assert!(!issues.iter().any(|i| i.severity == Severity::Error));
}

#[test]
fn dll_override_modes() {
    for ok in ["", "n", "b", "n,b", "b,n", "native", "builtin,native"] {
        assert!(validate_dll_override(ok).is_ok(), "{ok}");
    }
    for bad in ["x", "n,,b", "disabled"] {
        assert!(validate_dll_override(bad).is_err(), "{bad}");
    }
}

#[test]
fn games_file_parses_game_tables() {
    let file: GamesFile = toml::from_str(
        r#"
        [[game]]
        app_id = 292030
        name = "witcher3"
        platform = "windows"
        args = ["-skipIntro"]
        [game.env]
        DXVK_HUD = "fps"

        [[game]]
        app_id = 570
        "#,
    )
    .unwrap();
    assert_eq!(file.games.len(), 2);
    assert_eq!(file.games[0].platform, Some(Platform::Windows));
    assert_eq!(file.games[0].env["DXVK_HUD"], "fps");
    assert!(toml::from_str::<GamesFile>("[[game]]\napp_id = 1\npinned = true").is_err());
}

#[test]
fn numeric_alias_is_rejected() {
    let mut game = Game::new(10);
    game.name = Some("570".into());
    assert!(has(&game.validate(&ctx()), Severity::Error, "all digits"));
}

#[test]
fn catalog_checks_references_and_duplicates() {
    let mut first = Game::new(1);
    first.name = Some("alias".into());
    first.profile = Some("missing".into());
    let mut second = Game::new(1);
    second.name = Some("alias".into());
    let catalog = Catalog {
        config: Config {
            default_profile: Some("gone".into()),
            ..Config::default()
        },
        profiles: vec![Profile::new("native", Runner::Native)],
        games: vec![first, second],
        load_errors: Vec::new(),
    };
    let issues = catalog.validate(&ctx());
    let text = messages(&issues).join("\n");
    assert!(
        text.contains("`default_profile` names profile `gone`"),
        "{text}"
    );
    assert!(text.contains("`profile` names profile `missing`"), "{text}");
    assert!(text.contains("more than one entry"), "{text}");
    assert!(text.contains("already used by app id 1"), "{text}");
}

#[test]
fn load_reads_every_file_and_keeps_going_past_bad_ones() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, CONFIG_FILE, "default_profile = \"native\"\n");
    write(
        root,
        "profiles/native.toml",
        "name = \"native\"\nrunner = \"native\"\n",
    );
    write(
        root,
        "profiles/b.toml",
        "name = \"a\"\nrunner = \"native\"\n",
    );
    write(root, "profiles/broken.toml", "name = ");
    write(root, "profiles/notes.txt", "ignored");
    write(
        root,
        GAMES_FILE,
        "[[game]]\napp_id = 570\nname = \"dota\"\n",
    );

    let catalog = Catalog::load(root, Path::new(HOME));
    assert_eq!(catalog.config.default_profile.as_deref(), Some("native"));
    assert_eq!(catalog.profiles.len(), 1);
    assert!(catalog.profile("native").is_some());
    assert_eq!(catalog.game_by_alias("dota").map(|g| g.app_id), Some(570));
    assert_eq!(
        catalog.game(570).and_then(|g| g.name.as_deref()),
        Some("dota")
    );

    assert_eq!(catalog.load_errors.len(), 2, "{:?}", catalog.load_errors);
    assert!(
        catalog
            .load_errors
            .iter()
            .any(|e| matches!(e, LoadError::NameMismatch { stem, .. } if stem == "b"))
    );
    assert!(
        catalog
            .load_errors
            .iter()
            .any(|e| matches!(e, LoadError::Parse { .. }) && e.path().ends_with("broken.toml"))
    );
}

#[test]
fn load_of_an_empty_dir_is_all_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::load(dir.path(), Path::new(HOME));
    assert_eq!(catalog.config, Config::default());
    assert!(catalog.profiles.is_empty() && catalog.games.is_empty());
    assert!(catalog.load_errors.is_empty());
}

#[test]
fn profiles_dir_redirects_profile_loading() {
    let dir = tempfile::tempdir().unwrap();
    let shared = dir.path().join("lootrs");
    write(
        &shared,
        "native.toml",
        "name = \"native\"\nrunner = \"native\"\n",
    );
    let config_dir = dir.path().join("gaki");
    write(
        &config_dir,
        CONFIG_FILE,
        &format!("profiles_dir = \"{}\"\n", shared.display()),
    );
    let catalog = Catalog::load(&config_dir, Path::new(HOME));
    assert!(
        catalog.profile("native").is_some(),
        "{:?}",
        catalog.load_errors
    );
}

#[test]
fn round_trip_keeps_values() {
    let mut profile = Profile::new("p", Runner::Proton);
    profile.prefix = Some("~/pfx".into());
    profile.proton = Some(ProtonSection {
        build: Some("experimental".into()),
    });
    let text = toml::to_string(&profile).unwrap();
    assert_eq!(toml::from_str::<Profile>(&text).unwrap(), profile);

    let config = Config {
        default_platform: Some(Platform::Linux),
        ..Config::default()
    };
    let text = toml::to_string(&config).unwrap();
    assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
}

#[test]
fn app_references_are_ids_or_aliases() {
    let mut game = Game::new(292030);
    game.name = Some("witcher3".into());
    let catalog = Catalog {
        games: vec![game],
        ..Catalog::default()
    };
    assert_eq!(catalog.app_id("570"), Ok(570));
    assert_eq!(catalog.app_id("witcher3"), Ok(292030));
    let err = catalog.app_id("witcher").unwrap_err();
    assert!(err.contains("games.toml"), "{err}");
}

mod editing {
    use super::super::edit::{self, TomlFile};
    use super::*;

    const CONFIG: &str = "# my settings\ndefault_profile = \"proton\" # the usual\n\n# proxy for work\n# proxy = \"socks5://x\"\n";

    #[test]
    fn set_keeps_comments_and_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        fs::write(&path, CONFIG).unwrap();

        let mut file = TomlFile::open(&path).unwrap();
        assert!(file.set("default_profile", Some("native".into())));
        assert!(file.set("cloud_sync", Some(false.into())));
        assert!(
            !file.set("cloud_sync", Some(false.into())),
            "same value: no change"
        );
        file.parse::<Config>().unwrap();
        file.save().unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# my settings\ndefault_profile = \"native\" # the usual\n"),
            "{text}"
        );
        assert!(
            text.contains("# proxy for work\n# proxy = \"socks5://x\""),
            "{text}"
        );
        assert!(text.contains("cloud_sync = false"), "{text}");

        let mut file = TomlFile::open(&path).unwrap();
        assert!(file.set("cloud_sync", None));
        assert!(!file.set("cloud_sync", None));
    }

    #[test]
    fn a_bad_value_fails_to_parse_before_saving() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        let mut file = TomlFile::open(&path).unwrap();
        file.set("default_platform", Some("macos".into()));
        assert!(file.parse::<Config>().is_err());
        assert!(!path.exists(), "nothing written");
    }

    #[test]
    fn game_tables_are_added_pruned_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(GAMES_FILE);
        fs::write(
            &path,
            "# my games\n[[game]]\napp_id = 570 # dota\nname = \"dota2\"\n",
        )
        .unwrap();

        let mut file = TomlFile::open(&path).unwrap();
        edit::set_in(
            edit::game_mut(&mut file.doc, 570),
            "platform",
            Some("linux".into()),
        );
        edit::set_in(
            edit::game_mut(&mut file.doc, 10),
            "profile",
            Some("native".into()),
        );
        let games: GamesFile = file.parse().unwrap();
        assert_eq!(games.games.len(), 2);
        assert_eq!(games.games[0].platform, Some(Platform::Linux));
        assert_eq!(games.games[1].profile.as_deref(), Some("native"));
        let text = file.doc.to_string();
        assert!(
            text.starts_with("# my games\n[[game]]\napp_id = 570 # dota\n"),
            "{text}"
        );

        edit::set_in(edit::game_mut(&mut file.doc, 10), "profile", None);
        edit::prune_game(&mut file.doc, 10);
        assert_eq!(
            file.parse::<GamesFile>().unwrap().games.len(),
            1,
            "empty entry pruned"
        );

        assert!(edit::remove_game(&mut file.doc, 570));
        assert!(!edit::remove_game(&mut file.doc, 570));
        assert!(file.parse::<GamesFile>().unwrap().games.is_empty());
        assert!(!file.doc.to_string().contains("[[game]]"));
    }
}
