use std::fs;

use super::*;

const CONFIG: &str = "\"InstallConfigStore\"
{
\t\"Software\"
\t{
\t\t\"Valve\"
\t\t{
\t\t\t\"Steam\"
\t\t\t{
\t\t\t\t\"BaseInstallFolder_1\"\t\t\"C:\\\\Games\"
\t\t\t\t\"CompatToolMapping\"
\t\t\t\t{
\t\t\t\t\t\"0\"
\t\t\t\t\t{
\t\t\t\t\t\t\"name\"\t\t\"proton_experimental\"
\t\t\t\t\t\t\"config\"\t\t\"\"
\t\t\t\t\t\t\"priority\"\t\t\"75\"
\t\t\t\t\t}
\t\t\t\t\t\"774171\"
\t\t\t\t\t{
\t\t\t\t\t\t\"name\"\t\t\"dwproton\"
\t\t\t\t\t\t\"config\"\t\t\"\"
\t\t\t\t\t\t\"priority\"\t\t\"250\"
\t\t\t\t\t}
\t\t\t\t}
\t\t\t\t\"depots\"
\t\t\t\t{
\t\t\t\t\t\"774171\"
\t\t\t\t\t{
\t\t\t\t\t\t\"DecryptionKey\"\t\t\"abc\"
\t\t\t\t\t}
\t\t\t\t}
\t\t\t}
\t\t}
\t}
}
";

#[test]
fn tool_mapping_reads_games_not_the_default() {
    let map = tool_mapping(CONFIG);
    assert_eq!(map, BTreeMap::from([(774171, "dwproton".to_owned())]));
}

#[test]
fn set_tool_mapping_changes_only_the_named_entries() {
    let unchanged = set_tool_mapping(CONFIG, &BTreeMap::new()).unwrap();
    assert_eq!(unchanged, CONFIG);

    let changes = BTreeMap::from([
        (774171, Some("proton-cachyos-native".to_owned())),
        (2835570, Some("proton-cachyos-native".to_owned())),
    ]);
    let edited = set_tool_mapping(CONFIG, &changes).unwrap();
    let map = tool_mapping(&edited);
    assert_eq!(map[&774171], "proton-cachyos-native");
    assert_eq!(map[&2835570], "proton-cachyos-native");
    assert!(!edited.contains("dwproton"));
    // Everything else is still there, byte for byte: the default tool, the
    // depot keys of the same app id, the escaped path.
    assert!(edited.contains("\"proton_experimental\""));
    assert!(edited.contains("\"DecryptionKey\"\t\t\"abc\""));
    assert!(edited.contains("\"C:\\\\Games\""));
    assert_eq!(edited.matches('{').count(), edited.matches('}').count());

    let removed = set_tool_mapping(&edited, &BTreeMap::from([(774171, None)])).unwrap();
    assert_eq!(
        tool_mapping(&removed),
        BTreeMap::from([(2835570, "proton-cachyos-native".to_owned())])
    );
    // Both removed again: the original text, but for the dropped entry.
    let both = BTreeMap::from([(774171, None), (2835570, None)]);
    let back = set_tool_mapping(&edited, &both).unwrap();
    assert!(tool_mapping(&back).is_empty());
    assert!(back.contains("\"0\""));
}

#[test]
fn set_tool_mapping_adds_the_block_when_steam_has_none() {
    let start = CONFIG.find("\t\t\t\t\"CompatToolMapping\"").unwrap();
    let end = CONFIG.find("\t\t\t\t\"depots\"").unwrap();
    let bare = format!("{}{}", &CONFIG[..start], &CONFIG[end..]);
    assert!(tool_mapping(&bare).is_empty());
    let edited = set_tool_mapping(
        &bare,
        &BTreeMap::from([(42, Some("proton_experimental".to_owned()))]),
    )
    .unwrap();
    assert_eq!(
        tool_mapping(&edited),
        BTreeMap::from([(42, "proton_experimental".to_owned())])
    );
    assert!(edited.contains("\"DecryptionKey\""));

    let err = set_tool_mapping(
        "\"Other\"\n{\n}\n",
        &BTreeMap::from([(1, Some("x".into()))]),
    );
    assert!(err.is_err());
}

#[test]
fn steam_tool_name_reads_the_build() {
    let dir = tempfile::tempdir().unwrap();
    let custom = dir.path().join("proton-cachyos-native");
    fs::create_dir_all(&custom).unwrap();
    fs::write(
        custom.join("compatibilitytool.vdf"),
        "\"compatibilitytools\"\n{\n  \"compat_tools\"\n  {\n    \"proton-cachyos-native\"\n    {\n      \"display_name\" \"x\"\n    }\n  }\n}\n",
    )
    .unwrap();
    assert_eq!(
        steam_tool_name(&custom).as_deref(),
        Some("proton-cachyos-native")
    );
    let experimental = dir.path().join("Proton - Experimental");
    fs::create_dir_all(&experimental).unwrap();
    assert_eq!(
        steam_tool_name(&experimental).as_deref(),
        Some("proton_experimental")
    );
    let other = dir.path().join("Proton 9.0");
    fs::create_dir_all(&other).unwrap();
    assert_eq!(steam_tool_name(&other), None);
}

/// A Steam folder with a library of games, and gaki's state, in a tempdir.
struct Fx {
    dir: tempfile::TempDir,
    host: Host,
}

impl Fx {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        fs::create_dir_all(root.join("steamapps/common")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(root.join("config/config.vdf"), CONFIG).unwrap();
        let host = Host {
            root,
            state: dir.path().join("state/shared-prefix"),
        };
        Self { dir, host }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// A game `app` whose install holds `file`.
    fn game(&self, app: u32, name: &str, file: &str, bytes: &[u8]) {
        let steamapps = self.host.root.join("steamapps");
        fs::write(
            steamapps.join(format!("appmanifest_{app}.acf")),
            format!(
                "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app}\"\n\t\"name\"\t\t\"{name}\"\n\t\"installdir\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n}}\n"
            ),
        )
        .unwrap();
        let dir = steamapps.join("common").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file), bytes).unwrap();
    }

    fn compat(&self, app: u32) -> PathBuf {
        self.host
            .root
            .join("steamapps/compatdata")
            .join(app.to_string())
    }

    fn shared(&self) -> Shared {
        Shared {
            profile: "proton_cachyos".into(),
            prefix: self.p("prefixes/proton_cachyos"),
            tool: Some("proton-cachyos-native".into()),
        }
    }
}

/// Four games: Windows-only without a prefix, Windows-only with its own,
/// one Steam maps to dwproton (with a Linux build), and a Linux one; plus
/// a Steam runtime.
fn four_games(fx: &Fx) {
    fx.game(10, "Fresh", "game.exe", b"MZ");
    fx.game(20, "Played", "game.exe", b"MZ");
    fs::create_dir_all(fx.compat(20).join("pfx/drive_c")).unwrap();
    fs::write(fx.compat(20).join("pfx/drive_c/save"), "progress").unwrap();
    fx.game(774171, "Mapped", "game.x86_64", b"\x7fELF");
    fx.game(30, "Native", "game.x86_64", b"\x7fELF");
    fx.game(
        1391110,
        "Steam Linux Runtime 2.0 (soldier)",
        "run",
        b"\x7fELF",
    );
}

#[test]
fn games_are_the_ones_steam_runs_through_proton() {
    let fx = Fx::new();
    four_games(&fx);
    let games = fx.host.games().unwrap();
    let ids: Vec<u32> = games.iter().map(|game| game.app_id).collect();
    assert_eq!(ids, [10, 20, 774171]);
    assert_eq!(games[2].tool.as_deref(), Some("dwproton"));
}

/// Every game shares `shared`.
fn all(shared: &Shared) -> impl Fn(&Game) -> Option<Want> + '_ {
    move |_| Some(Want::Shared(shared.clone()))
}

/// Every game its own prefix: undoing.
fn own(_: &Game) -> Option<Want> {
    Some(Want::Own)
}

fn run(fx: &Fx, want: impl Fn(&Game) -> Option<Want>, replace: bool) -> Plan {
    let games = fx.host.games().unwrap();
    let plan = fx.host.plan(&games, want, replace).unwrap();
    fx.host.execute(&plan, |_| {}).unwrap();
    plan
}

fn prefix_of(fx: &Fx, app: u32) -> Prefix {
    let games = fx.host.games().unwrap();
    let game = games.iter().find(|game| game.app_id == app).unwrap();
    fx.host.prefix(game).unwrap()
}

fn linked_to(target: &Path) -> Prefix {
    Prefix::Linked {
        target: target.to_path_buf(),
        ours: true,
    }
}

#[test]
fn sharing_links_new_games_and_keeps_own_prefixes() {
    let fx = Fx::new();
    four_games(&fx);
    let shared = fx.shared();
    let plan = run(&fx, all(&shared), false);
    assert_eq!(
        plan.kept,
        [("Played".to_owned(), Kept::Own(fx.compat(20)))],
        "a prefix of its own may hold saves"
    );
    assert_eq!(plan.moving().count(), 0);

    for app in [10, 774171] {
        assert_eq!(prefix_of(&fx, app), linked_to(&shared.prefix));
    }
    assert_eq!(prefix_of(&fx, 20), Prefix::Own);
    let map = fx.host.tool_mapping().unwrap();
    assert_eq!(map[&10], "proton-cachyos-native");
    assert_eq!(map[&774171], "proton-cachyos-native");
    assert!(!map.contains_key(&20));
    assert!(fx.host.root.join("config/config.vdf.gaki-backup").is_file());
    assert_eq!(
        fx.host.linked().unwrap(),
        BTreeMap::from([(10, String::new()), (774171, "dwproton".to_owned())])
    );

    // Again: nothing left to do.
    let games = fx.host.games().unwrap();
    assert!(
        fx.host
            .plan(&games, all(&shared), false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn replace_moves_own_prefixes_aside_and_own_restores_everything() {
    let fx = Fx::new();
    four_games(&fx);
    let shared = fx.shared();
    fs::create_dir_all(shared.prefix.join("pfx")).unwrap();
    fs::write(shared.prefix.join("pfx/keep"), "shared").unwrap();

    let plan = run(&fx, all(&shared), true);
    assert_eq!(plan.kept, []);
    assert_eq!(prefix_of(&fx, 20), linked_to(&shared.prefix));
    let aside = fx.host.state.join("prefixes/20");
    assert_eq!(
        fs::read_to_string(aside.join("pfx/drive_c/save")).unwrap(),
        "progress"
    );

    run(&fx, own, false);
    assert_eq!(prefix_of(&fx, 10), Prefix::None);
    assert_eq!(prefix_of(&fx, 774171), Prefix::None);
    assert_eq!(
        fs::read_to_string(fx.compat(20).join("pfx/drive_c/save")).unwrap(),
        "progress"
    );
    assert!(!aside.exists());
    // The shared prefix itself is never touched.
    assert_eq!(
        fs::read_to_string(shared.prefix.join("pfx/keep")).unwrap(),
        "shared"
    );
    // The tools are what they were: dwproton back, none for the others.
    assert_eq!(
        fx.host.tool_mapping().unwrap(),
        BTreeMap::from([(774171, "dwproton".to_owned())])
    );
    assert!(!fx.host.state.exists(), "nothing left in gaki's state");
    let games = fx.host.games().unwrap();
    assert!(fx.host.plan(&games, own, false).unwrap().is_empty());
}

#[test]
fn each_game_gets_the_prefix_it_wants() {
    let fx = Fx::new();
    four_games(&fx);
    let shared = fx.shared();
    let other = Shared {
        profile: "dwproton".into(),
        prefix: fx.p("prefixes/dwproton"),
        tool: Some("dwproton".into()),
    };
    run(&fx, all(&shared), false);

    // Then: app 10 on another profile, 774171 its own again, the rest as
    // they are.
    let want = |game: &Game| match game.app_id {
        10 => Some(Want::Shared(other.clone())),
        774171 => Some(Want::Own),
        _ => None,
    };
    run(&fx, want, false);
    assert_eq!(prefix_of(&fx, 10), linked_to(&other.prefix));
    assert_eq!(prefix_of(&fx, 774171), Prefix::None);
    let map = fx.host.tool_mapping().unwrap();
    assert_eq!(map[&10], "dwproton");
    assert_eq!(map[&774171], "dwproton", "its own tool from before");
    // App 10 is still gaki's, with its first tool (none) to restore.
    assert_eq!(
        fx.host.linked().unwrap(),
        BTreeMap::from([(10, String::new())])
    );
    run(&fx, own, false);
    assert!(!fx.host.tool_mapping().unwrap().contains_key(&10));
}

#[test]
fn a_rerun_keeps_the_first_tool_to_restore() {
    let fx = Fx::new();
    four_games(&fx);
    run(&fx, all(&fx.shared()), false);
    // Another build for the same profile later.
    let other = Shared {
        tool: Some("proton_experimental".into()),
        ..fx.shared()
    };
    run(&fx, all(&other), false);
    assert_eq!(fx.host.linked().unwrap()[&774171], "dwproton");
}

#[test]
fn links_gaki_didnt_make_are_left_and_unlink_never_follows() {
    let fx = Fx::new();
    four_games(&fx);
    let shared = fx.shared();
    let other = fx.p("other-prefix");
    fs::create_dir_all(&other).unwrap();
    fs::create_dir_all(fx.compat(10).parent().unwrap()).unwrap();
    symlink(&other, fx.compat(10)).unwrap();
    let plan = run(&fx, all(&shared), true);
    assert!(
        plan.kept
            .contains(&("Fresh".to_owned(), Kept::Elsewhere(other.clone())))
    );
    let compat = fx.compat(10);
    assert!(!plan.steps.iter().any(|step| match step {
        Step::Tool { app_id, .. } => *app_id == 10,
        Step::Link { path, .. } => *path == compat,
        _ => false,
    }));
    // Undoing leaves it too: gaki didn't make it.
    run(&fx, own, false);
    assert_eq!(
        prefix_of(&fx, 10),
        Prefix::Linked {
            target: other,
            ours: false
        }
    );

    // A directory where a link was expected stays.
    let fake = Plan {
        steps: vec![Step::Unlink {
            app_id: 20,
            path: fx.compat(20),
        }],
        kept: vec![],
    };
    assert!(fx.host.execute(&fake, |_| {}).is_err());
    assert!(fx.compat(20).join("pfx/drive_c/save").is_file());
}

#[test]
fn a_relative_prefix_is_refused() {
    let fx = Fx::new();
    four_games(&fx);
    let shared = Shared {
        prefix: PathBuf::from("prefixes/x"),
        ..fx.shared()
    };
    let games = fx.host.games().unwrap();
    assert!(fx.host.plan(&games, all(&shared), false).is_err());
}

#[test]
fn removing_a_linked_prefix_removes_only_the_link() {
    let fx = Fx::new();
    let shared = fx.p("shared");
    fs::create_dir_all(shared.join("pfx")).unwrap();
    fs::write(shared.join("pfx/save"), "progress").unwrap();
    let linked = fx.compat(1);
    fs::create_dir_all(linked.parent().unwrap()).unwrap();
    symlink(&shared, &linked).unwrap();
    remove_compat(&linked).unwrap();
    assert!(fs::symlink_metadata(&linked).is_err());
    assert_eq!(
        fs::read_to_string(shared.join("pfx/save")).unwrap(),
        "progress"
    );

    let own = fx.compat(2);
    fs::create_dir_all(own.join("pfx")).unwrap();
    remove_compat(&own).unwrap();
    assert!(!own.exists());
    remove_compat(&own).unwrap();
}
