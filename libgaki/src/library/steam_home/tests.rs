use std::fs;

use super::*;

/// A Steam folder, XDG folders and gaki's records base under one tempdir.
struct Fx {
    dir: tempfile::TempDir,
    host: Host,
}

impl Fx {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Steam");
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let host = Host {
            root,
            cache: dir.path().join("cache/steam"),
            state: dir.path().join("state/steam"),
            records_base: dir.path().join("state/gaki/libraries"),
        };
        Self { dir, host }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.p(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// An appmanifest for `app` installed in `dir` in `records`, and the game.
    fn game(&self, records: &str, games: &str, app: u32, dir: &str) {
        self.write(
            &format!("{records}/appmanifest_{app}.acf"),
            &format!(
                "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"
            ),
        );
        self.write(&format!("{games}/{dir}/game.bin"), "bits");
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.p(rel)).unwrap()
    }

    fn records(&self, root: &str) -> PathBuf {
        self.host.records_of(&self.p(root))
    }

    /// Moves the library to `to`, then the caches and logs.
    fn relocate(&self, to: &Library) {
        let from = self.host.library().unwrap();
        let plan = self.host.plan_library(&from, to).unwrap();
        self.host.execute(&plan, |_| {}).unwrap();
        let places = self.host.plan_places(false).unwrap();
        self.host.execute(&places, |_| {}).unwrap();
    }

    fn undo(&self) {
        let places = self.host.plan_places(true).unwrap();
        self.host.execute(&places, |_| {}).unwrap();
        let from = self.host.library().unwrap();
        let plan = self.host.plan_library(&from, &Library::InPlace).unwrap();
        self.host.execute(&plan, |_| {}).unwrap();
    }

    fn link_of(&self, rel: &str) -> PathBuf {
        fs::read_link(self.p(rel)).unwrap()
    }

    /// Nothing more to do for `to`.
    fn settled(&self, to: &Library) -> bool {
        let from = self.host.library().unwrap();
        self.host.plan_library(&from, to).unwrap().is_empty()
            && self.host.plan_places(false).unwrap().is_empty()
    }
}

/// A Steam folder as Steam leaves it: a game, a prefix, caches and logs.
fn steam_in_place(fx: &Fx) {
    fx.game("Steam/steamapps", "Steam/steamapps/common", 10, "Game A");
    fx.write("Steam/steamapps/compatdata/10/pfx/user.reg", "reg");
    fx.write("Steam/steamapps/libraryfolders.vdf", "vdf");
    fx.write("Steam/steamapps/shadercache/10/cache", "shaders");
    fx.write("Steam/appcache/appinfo.vdf", "appinfo");
    fx.write("Steam/config/htmlcache/x", "html");
    fx.write("Steam/config/config.vdf", "keep");
    fx.write("Steam/logs/console_log.txt", "log");
}

#[test]
fn steam_layout_moves_the_library_caches_and_logs() {
    let fx = Fx::new();
    steam_in_place(&fx);
    let lib = fx.p("games");
    let to = Library::Steam(lib.clone());
    fx.relocate(&to);

    assert_eq!(fx.link_of("Steam/steamapps"), lib.join("steamapps"));
    assert_eq!(fx.read("games/steamapps/common/Game A/game.bin"), "bits");
    assert_eq!(fx.read("games/steamapps/compatdata/10/pfx/user.reg"), "reg");
    assert_eq!(fx.read("games/steamapps/libraryfolders.vdf"), "vdf");
    assert_eq!(fx.link_of("Steam/appcache"), fx.p("cache/steam/appcache"));
    assert_eq!(fx.read("cache/steam/shadercache/10/cache"), "shaders");
    assert_eq!(fx.read("cache/steam/htmlcache/x"), "html");
    assert_eq!(fx.read("Steam/config/config.vdf"), "keep");
    assert_eq!(fx.read("state/steam/logs/console_log.txt"), "log");
    // Steam's folder is private; what left it stays so.
    let mode = fs::metadata(fx.p("games/steamapps"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
    assert!(fx.settled(&to));
}

#[test]
fn gaki_layout_puts_games_in_the_library_and_records_in_state() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.write("games/Not Steam/run.sh", "mine");
    let to = Library::Gaki(fx.p("games"));
    fx.relocate(&to);

    let records = fx.records("games");
    assert_eq!(fx.link_of("Steam/steamapps"), records);
    assert_eq!(
        fs::read_link(records.join("common")).unwrap(),
        fx.p("games")
    );
    assert_eq!(fx.read("games/Game A/game.bin"), "bits");
    assert!(records.join("appmanifest_10.acf").is_file());
    assert!(records.join("compatdata/10/pfx/user.reg").is_file());
    // Steam reaches the game through its own path.
    assert_eq!(fx.read("Steam/steamapps/common/Game A/game.bin"), "bits");
    assert_eq!(fx.read("games/Not Steam/run.sh"), "mine");
    assert_eq!(fx.host.library().unwrap(), to);
    assert_eq!(fx.host.layout_of(&fx.p("games")), Some(Choice::Gaki));
    assert!(fx.settled(&to));
}

/// `sruman steam relocate` left Steam's library in Steam's layout at
/// `games/steam`, and the caches where gaki puts them; the configured
/// library `games` is gaki's own and has a game of its own.
#[test]
fn catches_up_with_a_library_sruman_moved() {
    let fx = Fx::new();
    fx.game(
        "games/steam/steamapps",
        "games/steam/steamapps/common",
        10,
        "Game A",
    );
    fx.write(
        "games/steam/steamapps/compatdata/20/pfx/user.reg",
        "steam's prefix for 20",
    );
    fs::create_dir_all(fx.p("cache/steam/shadercache")).unwrap();
    symlink(
        fx.p("cache/steam/shadercache"),
        fx.p("games/steam/steamapps/shadercache"),
    )
    .unwrap();
    symlink(fx.p("games/steam/steamapps"), fx.p("Steam/steamapps")).unwrap();
    for (path, name) in CACHES.iter().take(5) {
        fs::create_dir_all(fx.p(&format!("cache/steam/{name}"))).unwrap();
        fs::create_dir_all(fx.p("Steam/config")).unwrap();
        symlink(
            fx.p(&format!("cache/steam/{name}")),
            fx.p(&format!("Steam/{path}")),
        )
        .unwrap();
    }
    fs::create_dir_all(fx.p("state/steam/logs")).unwrap();
    symlink(fx.p("state/steam/logs"), fx.p("Steam/logs")).unwrap();
    // gaki's own library, with a game Steam doesn't have.
    let records = fx.records("games");
    let rel = records
        .strip_prefix(fx.dir.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    fx.game(&rel, "games", 20, "Game B");
    fx.write("games/HoYoPlay/launcher.exe", "lootrs'");

    assert_eq!(
        fx.host.library().unwrap(),
        Library::Steam(fx.p("games/steam"))
    );
    assert_eq!(fx.host.layout_of(&fx.p("games")), Some(Choice::Gaki));
    let to = Library::Gaki(fx.p("games"));
    fx.relocate(&to);

    assert_eq!(fx.link_of("Steam/steamapps"), records);
    assert_eq!(fx.read("games/Game A/game.bin"), "bits");
    assert_eq!(fx.read("games/Game B/game.bin"), "bits");
    assert!(records.join("appmanifest_10.acf").is_file());
    assert!(records.join("appmanifest_20.acf").is_file());
    assert!(records.join("compatdata/20/pfx/user.reg").is_file());
    assert!(
        fs::symlink_metadata(records.join("shadercache"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !fx.p("games/steam").exists(),
        "the old library folder is gone"
    );
    assert_eq!(fx.read("games/HoYoPlay/launcher.exe"), "lootrs'");
    assert!(fx.settled(&to));
}

#[test]
fn a_clash_changes_nothing() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.write("games/steamapps/common/Game A/other.bin", "other copy");
    let from = fx.host.library().unwrap();
    let err = fx
        .host
        .plan_library(&from, &Library::Steam(fx.p("games")))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Game A"), "{err}");
    assert!(fx.p("Steam/steamapps/common/Game A/game.bin").is_file());
}

#[test]
fn folders_on_both_sides_merge() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.write(
        "games/steamapps/compatdata/99/pfx/user.reg",
        "other game's prefix",
    );
    fx.relocate(&Library::Steam(fx.p("games")));
    assert!(fx.p("games/steamapps/compatdata/10/pfx/user.reg").is_file());
    assert!(fx.p("games/steamapps/compatdata/99/pfx/user.reg").is_file());
}

#[test]
fn undo_from_gaki_layout_takes_back_only_steams_games() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.write("games/Not Steam/run.sh", "mine");
    fx.relocate(&Library::Gaki(fx.p("games")));
    fx.undo();

    assert!(
        !fs::symlink_metadata(fx.p("Steam/steamapps"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fx.read("Steam/steamapps/common/Game A/game.bin"), "bits");
    assert_eq!(fx.read("Steam/steamapps/compatdata/10/pfx/user.reg"), "reg");
    assert_eq!(fx.read("Steam/steamapps/shadercache/10/cache"), "shaders");
    assert_eq!(fx.read("Steam/appcache/appinfo.vdf"), "appinfo");
    assert_eq!(fx.read("Steam/logs/console_log.txt"), "log");
    assert_eq!(fx.read("games/Not Steam/run.sh"), "mine");
    assert!(!fx.records("games").exists(), "gaki's records are gone");
    assert!(!fx.p(&format!("Steam/{STAGING}")).exists());
    assert!(fx.settled(&Library::InPlace) || fx.host.plan_places(false).is_ok());
    let again = fx
        .host
        .plan_library(&Library::InPlace, &Library::InPlace)
        .unwrap();
    assert!(again.is_empty());
}

#[test]
fn undo_from_steam_layout() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.relocate(&Library::Steam(fx.p("games")));
    fx.undo();
    assert_eq!(fx.read("Steam/steamapps/common/Game A/game.bin"), "bits");
    assert_eq!(fx.read("Steam/steamapps/libraryfolders.vdf"), "vdf");
    assert!(!fx.p("games/steamapps").exists());
}

#[test]
fn switches_layout_in_the_same_library() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.write("games/Not Steam/run.sh", "mine");
    fx.relocate(&Library::Steam(fx.p("games")));
    let gaki = Library::Gaki(fx.p("games"));
    fx.relocate(&gaki);
    assert_eq!(fx.read("games/Game A/game.bin"), "bits");
    assert!(!fx.p("games/steamapps").exists());
    assert!(fx.settled(&gaki));

    let steam = Library::Steam(fx.p("games"));
    fx.relocate(&steam);
    assert_eq!(fx.read("games/steamapps/common/Game A/game.bin"), "bits");
    assert_eq!(fx.read("games/Not Steam/run.sh"), "mine");
    assert!(!fx.records("games").exists());
    assert_eq!(fx.host.layout_of(&fx.p("games")), Some(Choice::Steam));
    assert!(fx.settled(&steam));
}

#[test]
fn an_interrupted_undo_is_finished() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.relocate(&Library::Steam(fx.p("games")));
    fx.undo();
    // As if the swap happened but removing the old link didn't.
    symlink(fx.p("games/steamapps"), fx.p(&format!("Steam/{STAGING}"))).unwrap();
    let plan = fx
        .host
        .plan_library(&Library::InPlace, &Library::InPlace)
        .unwrap();
    fx.host.execute(&plan, |_| {}).unwrap();
    assert!(!fs::symlink_metadata(fx.p(&format!("Steam/{STAGING}"))).is_ok());
}

#[test]
fn refuses_a_library_inside_steams_folder() {
    let fx = Fx::new();
    steam_in_place(&fx);
    let inside = Library::Steam(fx.p("Steam/games"));
    assert!(fx.host.plan_library(&Library::InPlace, &inside).is_err());
}

#[test]
fn reads_where_an_unknown_link_leads() {
    let fx = Fx::new();
    fs::create_dir_all(fx.p("elsewhere")).unwrap();
    symlink(fx.p("elsewhere"), fx.p("Steam/steamapps")).unwrap();
    assert!(fx.host.library().is_err());
}

#[test]
fn copies_keep_links_and_modes() {
    let fx = Fx::new();
    fx.write("src/dir/file", "x");
    fs::set_permissions(fx.p("src/dir/file"), fs::Permissions::from_mode(0o751)).unwrap();
    symlink("dir/file", fx.p("src/link")).unwrap();
    copy_any(&fx.p("src"), &fx.p("dst")).unwrap();
    assert_eq!(
        fs::read_link(fx.p("dst/link")).unwrap(),
        Path::new("dir/file")
    );
    let mode = fs::metadata(fx.p("dst/dir/file"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o751);
}

#[test]
fn a_linked_twin_library_is_dropped() {
    let fx = Fx::new();
    steam_in_place(&fx);
    fx.relocate(&Library::Steam(fx.p("games")));
    let mut libraries = vec![fx.host.root.clone(), fx.p("games")];
    crate::library::drop_linked_twins(&mut libraries);
    assert_eq!(libraries, [fx.p("games")]);
}

#[test]
fn gaki_layout_needs_a_path_steam_can_read() {
    let fx = Fx::new();
    steam_in_place(&fx);
    let dashed = fx.p("my-games");
    assert!(!fx.host.gaki_layout_works(&dashed));
    let err = fx
        .host
        .plan_library(&Library::InPlace, &Library::Gaki(dashed.clone()))
        .unwrap_err()
        .to_string();
    assert!(err.contains("folder separator"), "{err}");
    assert!(
        fx.host
            .plan_library(&Library::InPlace, &Library::Steam(dashed))
            .is_ok()
    );
}
