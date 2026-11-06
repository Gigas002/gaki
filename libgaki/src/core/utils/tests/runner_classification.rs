use super::*;

/// Create `dir/rel` as an empty marker file, creating parent dirs.
fn touch(dir: &Path, rel: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, b"").unwrap();
}

#[test]
fn classify_proton_by_script() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Proton - Experimental");
    std::fs::create_dir_all(&root).unwrap();
    touch(&root, "proton");
    touch(&root, "files/bin/wine64");
    assert_eq!(classify_runner(&root), RunnerKind::Proton);
}

#[test]
fn classify_proton_by_files_bin_wine_without_script() {
    // A Proton-style tree missing the top-level script is still Proton by layout.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("GE-Proton9-20");
    std::fs::create_dir_all(&root).unwrap();
    touch(&root, "files/bin/wine");
    assert_eq!(classify_runner(&root), RunnerKind::Proton);
}

#[test]
fn classify_plain_wine() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wine-9.0");
    std::fs::create_dir_all(&root).unwrap();
    touch(&root, "bin/wine");
    touch(&root, "bin/wine64");
    assert_eq!(classify_runner(&root), RunnerKind::PlainWine);
}

#[test]
fn classify_wine_tkg_by_name() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wine-tkg-staging-9.0");
    std::fs::create_dir_all(&root).unwrap();
    touch(&root, "bin/wine64");
    assert_eq!(classify_runner(&root), RunnerKind::WineTkg);
}

#[test]
fn classify_unknown_empty_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("not-a-runner");
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(classify_runner(&root), RunnerKind::Unknown);
}
