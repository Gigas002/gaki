use super::*;

#[test]
fn official_lookup_is_case_and_punctuation_insensitive() {
    assert_eq!(find_official("proton 9.0").unwrap().app_id, 2805730);
    assert_eq!(
        find_official("Proton Experimental").unwrap().app_id,
        1493710
    );
    assert_eq!(
        find_official("proton-experimental").unwrap().name,
        "Proton - Experimental"
    );
}

#[test]
fn custom_builds_are_not_installable() {
    assert!(find_official("GE-Proton9-20").is_none());
    assert!(find_official("proton-cachyos-9.0").is_none());
}

#[test]
fn search_order_is_system_then_user_then_steam() {
    let tmp = tempfile::tempdir().unwrap();
    let system = tmp.path().join("system");
    let user = tmp.path().join("user");
    let library = tmp.path().join("library");
    let dirs = search_dirs_from(&system, std::slice::from_ref(&user), &library);
    let labels: Vec<&str> = dirs.iter().map(|(_, l)| *l).collect();
    assert_eq!(labels, ["system", "user", "steam"]);
    assert_eq!(dirs[2].0, library.join("steamapps/common"));
}

#[test]
fn duplicate_user_dirs_are_collapsed() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("Steam/compatibilitytools.d");
    std::fs::create_dir_all(&real).unwrap();
    let link = tmp.path().join("root");
    std::os::unix::fs::symlink(tmp.path().join("Steam"), &link).unwrap();
    let dirs = search_dirs_from(
        &tmp.path().join("system"),
        &[link.join("compatibilitytools.d"), real],
        &tmp.path().join("library"),
    );
    assert_eq!(dirs.iter().filter(|(_, l)| *l == "user").count(), 1);
}

#[test]
fn earlier_search_dir_shadows_same_name() {
    let tmp = tempfile::tempdir().unwrap();
    let system = tmp.path().join("system");
    let user = tmp.path().join("user");
    let library = tmp.path().join("library");
    std::fs::create_dir_all(system.join("GE-Proton9-20")).unwrap();
    std::fs::create_dir_all(user.join("GE-Proton9-20")).unwrap();
    std::fs::create_dir_all(user.join("proton-cachyos")).unwrap();
    std::fs::create_dir_all(library.join("steamapps/common/Proton 9.0")).unwrap();
    // Not a Proton: ignored under steamapps/common.
    std::fs::create_dir_all(library.join("steamapps/common/Half-Life")).unwrap();

    let found = list_installed_in(&search_dirs_from(&system, &[user], &library));
    let got: Vec<(&str, &str)> = found
        .iter()
        .map(|p| (p.name.as_str(), p.location))
        .collect();
    assert_eq!(
        got,
        [
            ("GE-Proton9-20", "system"),
            ("Proton 9.0", "steam"),
            ("proton-cachyos", "user"),
        ]
    );
}
