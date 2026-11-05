use super::*;

#[test]
fn the_runtime_dir_must_be_set_and_absolute() {
    assert!(runtime_dir(None).is_err());
    assert!(runtime_dir(Some("".into())).is_err());
    assert!(runtime_dir(Some("relative/run".into())).is_err());
    assert_eq!(
        runtime_dir(Some("/run/user/1000".into())).unwrap(),
        PathBuf::from("/run/user/1000")
    );
}

#[test]
fn the_socket_dir_is_made_private() {
    use std::os::unix::fs::MetadataExt;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("gaki");
    ensure_private_dir(&dir).unwrap();
    assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);

    // An existing, too-open directory we own is tightened.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    ensure_private_dir(&dir).unwrap();
    assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);

    // A file in the way is refused.
    let file = tmp.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(ensure_private_dir(&file).is_err());
}
