use super::*;

fn manifest(depot_id: u32, branch: &str, manifest_id: u64) -> DepotManifestInfo {
    DepotManifestInfo {
        depot_id,
        depot_name: None,
        branch: branch.to_string(),
        manifest_id,
        size: 0,
    }
}

#[test]
fn manifest_ref_from_options() {
    assert_eq!(
        ManifestRef::from_options(None, None),
        ManifestRef::default()
    );
    assert_eq!(
        ManifestRef::from_options(None, None),
        ManifestRef::Branch("public".into())
    );
    assert_eq!(
        ManifestRef::from_options(None, Some("beta")),
        ManifestRef::Branch("beta".into())
    );
    // An explicit id wins over a branch.
    assert_eq!(
        ManifestRef::from_options(Some(42), Some("beta")),
        ManifestRef::Id(42)
    );
}

#[test]
fn pick_manifest_by_branch_and_id() {
    let list = [
        manifest(11, "public", 100),
        manifest(11, "beta", 101),
        manifest(12, "public", 200),
    ];
    assert_eq!(
        pick_manifest(&list, 11, &ManifestRef::default()).unwrap(),
        100
    );
    assert_eq!(
        pick_manifest(&list, 11, &ManifestRef::Branch("BETA".into())).unwrap(),
        101
    );
    assert_eq!(
        pick_manifest(&list, 12, &ManifestRef::default()).unwrap(),
        200
    );
    // An id needs no lookup, even for a depot PICS doesn't list.
    assert_eq!(pick_manifest(&[], 99, &ManifestRef::Id(7)).unwrap(), 7);
}

#[test]
fn pick_manifest_errors_name_the_choices() {
    let list = [manifest(11, "public", 100), manifest(11, "beta", 101)];
    let err = pick_manifest(&list, 11, &ManifestRef::Branch("nope".into()))
        .unwrap_err()
        .to_string();
    assert!(err.contains("public, beta"), "{err}");
    let err = pick_manifest(&list, 99, &ManifestRef::default())
        .unwrap_err()
        .to_string();
    assert!(err.contains("depot 99"), "{err}");
}

#[test]
fn glob_matching() {
    assert!(glob_match("*", "anything/at/all"));
    assert!(glob_match("*.exe", "bin/Game.EXE"));
    assert!(glob_match("bin/*", "bin/x64/game.dll"));
    assert!(glob_match("bin/game.???", "bin/game.exe"));
    assert!(glob_match("*x64*dll", "bin/x64/game.dll"));
    assert!(glob_match("readme.txt", "README.TXT"));
    assert!(!glob_match("*.exe", "bin/game.dll"));
    assert!(!glob_match("bin/game.??", "bin/game.exe"));
    assert!(!glob_match("data/*", "bin/data/x"));
    assert!(glob_match("", ""));
    assert!(!glob_match("", "x"));
}

#[test]
fn total_size_counts_files_only() {
    let entry = |path: &str, kind, size| ManifestFileEntry {
        path: path.to_string(),
        kind,
        size,
        sha1: String::new(),
        chunks: 0,
        executable: false,
        link_target: None,
    };
    let files = DepotFiles {
        app_id: 1,
        depot_id: 2,
        manifest_id: 3,
        files: vec![
            entry("bin", EntryKind::Directory, 0),
            entry("bin/a", EntryKind::File, 10),
            entry("bin/b", EntryKind::File, 5),
            entry("bin/l", EntryKind::Symlink, 3),
        ],
    };
    assert_eq!(files.total_size(), 15);
}
