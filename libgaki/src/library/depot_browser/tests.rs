use super::*;

#[test]
fn entry_kind_from_flags() {
    assert_eq!(entry_kind(0, ""), EntryKind::File);
    assert_eq!(entry_kind(FLAG_EXECUTABLE, ""), EntryKind::File);
    assert_eq!(entry_kind(FLAG_DIRECTORY, ""), EntryKind::Directory);
    assert_eq!(entry_kind(FLAG_SYMLINK, ""), EntryKind::Symlink);
    // A link target alone marks a symlink, as in an install.
    assert_eq!(entry_kind(0, "lib.so.1"), EntryKind::Symlink);
}

#[test]
fn normalize_request_strips_leading_separators() {
    assert_eq!(normalize_request("bin/game.exe"), "bin/game.exe");
    assert_eq!(normalize_request("/bin/game.exe"), "bin/game.exe");
    assert_eq!(normalize_request("./bin/game.exe"), "bin/game.exe");
    assert_eq!(normalize_request(" .//bin\\game.exe "), "bin/game.exe");
}

#[test]
fn download_target_is_the_file_name_under_out_dir() {
    let out = Path::new("/downloads");
    assert_eq!(
        download_target(out, "bin/x64/game.exe").unwrap(),
        PathBuf::from("/downloads/game.exe")
    );
    assert_eq!(
        download_target(out, "readme.txt").unwrap(),
        PathBuf::from("/downloads/readme.txt")
    );
    assert_eq!(
        download_target(out, "/abs/data.pak").unwrap(),
        PathBuf::from("/downloads/data.pak")
    );
}

#[test]
fn download_target_refuses_unsafe_paths() {
    let out = Path::new("/downloads");
    assert!(download_target(out, "../.bashrc").is_err());
    assert!(download_target(out, "bin/../../x").is_err());
    assert!(download_target(out, "").is_err());
    assert!(download_target(out, "/").is_err());
}

#[test]
fn part_path_is_hidden_next_to_dest() {
    assert_eq!(
        part_path(Path::new("/d/game.exe")),
        PathBuf::from("/d/.game.exe.gaki-part")
    );
}

#[test]
fn check_dest_refuses_existing_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("f");
    assert!(check_dest(&dest, false).is_ok());
    std::fs::write(&dest, b"x").unwrap();
    let err = check_dest(&dest, false).unwrap_err().to_string();
    assert!(err.contains("--force"), "{err}");
    assert!(check_dest(&dest, true).is_ok());
    // A dangling symlink still counts as existing.
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path().join("missing"), &link).unwrap();
    assert!(check_dest(&link, false).is_err());
    // Never over a directory, even with overwrite.
    assert!(check_dest(dir.path(), true).is_err());
}

#[test]
fn place_without_overwrite_keeps_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let part = dir.path().join(".f.gaki-part");
    let dest = dir.path().join("f");
    std::fs::write(&part, b"new").unwrap();
    std::fs::write(&dest, b"old").unwrap();
    assert!(place(&part, &dest, false).is_err());
    assert_eq!(std::fs::read(&dest).unwrap(), b"old");

    std::fs::remove_file(&dest).unwrap();
    place(&part, &dest, false).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), b"new");
    assert!(!part.exists());
}

#[test]
fn place_with_overwrite_replaces_a_symlink_not_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    std::fs::write(&target, b"keep").unwrap();
    let dest = dir.path().join("f");
    std::os::unix::fs::symlink(&target, &dest).unwrap();
    let part = dir.path().join(".f.gaki-part");
    std::fs::write(&part, b"new").unwrap();

    place(&part, &dest, true).unwrap();
    assert!(!dest.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&dest).unwrap(), b"new");
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");
}

#[test]
fn sha1_file_matches_known_digest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(
        sha1_file(&path).unwrap(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
}
